//! The owner's original (untranslated) text for one translate-mode session.
//! Only the unlocked "tail" (current sentence) is editable and re-translated.
use crate::core::types::Lang;
use unicode_segmentation::UnicodeSegmentation;

pub const SENTENCE_END: [char; 5] = ['.', '!', '?', '؟', '…'];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditOp {
    Insert(String),
    Backspace,
    Delete,
    DeleteWordBack,
    Left,
    Right,
    Home,
    End,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Applied {
    /// Tail text changed; translate it (immediately if asked) and lock afterwards if `lock_now`.
    Changed { lock_now: bool, immediate: bool },
    /// Only the caret moved (or nothing happened); swallow the key, no translation.
    CaretOnly,
    /// The tail is empty: let the key reach the app untouched.
    PassThrough,
}

pub struct ComposeSession {
    tail: String,
    /// Byte offset into `tail`, always on a grapheme boundary.
    caret: usize,
    live_output: String,
    target: Lang,
}

fn is_break_char(c: char) -> bool {
    c.is_whitespace() || c.is_ascii_punctuation() || SENTENCE_END.contains(&c) || matches!(c, '،' | '؛')
}

impl ComposeSession {
    pub fn new(target: Lang) -> Self {
        Self { tail: String::new(), caret: 0, live_output: String::new(), target }
    }

    pub fn tail(&self) -> &str {
        &self.tail
    }

    pub fn tail_caret(&self) -> usize {
        self.tail[..self.caret].graphemes(true).count()
    }

    pub fn is_tail_empty(&self) -> bool {
        self.tail.is_empty()
    }

    pub fn live_output(&self) -> &str {
        &self.live_output
    }

    pub fn set_live_output(&mut self, s: String) {
        self.live_output = s;
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn set_target(&mut self, t: Lang) {
        self.target = t;
    }

    /// Finishes the current sentence: its translation stays in the field as is.
    pub fn lock_tail(&mut self) {
        self.tail.clear();
        self.caret = 0;
        self.live_output.clear();
    }

    fn prev_boundary(&self) -> Option<usize> {
        self.tail[..self.caret].grapheme_indices(true).next_back().map(|(i, _)| i)
    }

    fn next_boundary(&self) -> Option<usize> {
        self.tail[self.caret..].graphemes(true).next().map(|g| self.caret + g.len())
    }

    pub fn apply(&mut self, op: EditOp) -> Applied {
        if self.tail.is_empty() && !matches!(op, EditOp::Insert(_)) {
            return Applied::PassThrough;
        }
        let changed = |immediate| Applied::Changed { lock_now: false, immediate };
        match op {
            EditOp::Insert(s) => {
                if s.is_empty() {
                    return Applied::CaretOnly;
                }
                self.tail.insert_str(self.caret, &s);
                self.caret += s.len();
                let at_end = self.caret == self.tail.len();
                let immediate = s.chars().last().is_some_and(is_break_char);
                let lock_now = at_end
                    && self.tail.ends_with(' ')
                    && self.tail.trim_end().chars().last().is_some_and(|c| SENTENCE_END.contains(&c));
                Applied::Changed { lock_now, immediate }
            }
            EditOp::Backspace => match self.prev_boundary() {
                Some(p) => {
                    self.tail.replace_range(p..self.caret, "");
                    self.caret = p;
                    changed(false)
                }
                None => Applied::CaretOnly,
            },
            EditOp::Delete => match self.next_boundary() {
                Some(n) => {
                    self.tail.replace_range(self.caret..n, "");
                    changed(false)
                }
                None => Applied::CaretOnly,
            },
            EditOp::DeleteWordBack => {
                let before = &self.tail[..self.caret];
                let trimmed = before.trim_end();
                let start = trimmed.rfind(char::is_whitespace).map(|i| i + trimmed[i..].chars().next().unwrap().len_utf8()).unwrap_or(0);
                if start == self.caret {
                    return Applied::CaretOnly;
                }
                self.tail.replace_range(start..self.caret, "");
                self.caret = start;
                changed(false)
            }
            EditOp::Left => {
                if let Some(p) = self.prev_boundary() {
                    self.caret = p;
                }
                Applied::CaretOnly
            }
            EditOp::Right => {
                if let Some(n) = self.next_boundary() {
                    self.caret = n;
                }
                Applied::CaretOnly
            }
            EditOp::Home => {
                self.caret = 0;
                Applied::CaretOnly
            }
            EditOp::End => {
                self.caret = self.tail.len();
                Applied::CaretOnly
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> ComposeSession {
        ComposeSession::new("en".into())
    }
    fn typ(c: &mut ComposeSession, text: &str) -> Applied {
        let mut last = Applied::CaretOnly;
        for g in text.graphemes(true) {
            last = c.apply(EditOp::Insert(g.to_string()));
        }
        last
    }

    #[test]
    fn insert_and_backspace_with_caret() {
        let mut c = s();
        typ(&mut c, "abcde");
        c.apply(EditOp::Left);
        c.apply(EditOp::Left);
        assert_eq!(c.apply(EditOp::Backspace), Applied::Changed { lock_now: false, immediate: false });
        assert_eq!(c.tail(), "abde");
        assert_eq!(c.tail_caret(), 2);
    }

    #[test]
    fn arabic_caret_by_grapheme() {
        let mut c = s();
        typ(&mut c, "مرحبا");
        c.apply(EditOp::Left);
        c.apply(EditOp::Backspace);
        assert_eq!(c.tail(), "مرحا");
    }

    #[test]
    fn ctrl_backspace_deletes_word() {
        let mut c = s();
        typ(&mut c, "bonjour le monde");
        c.apply(EditOp::DeleteWordBack);
        assert_eq!(c.tail(), "bonjour le ");
        c.apply(EditOp::DeleteWordBack);
        assert_eq!(c.tail(), "bonjour ");
    }

    #[test]
    fn delete_home_end() {
        let mut c = s();
        typ(&mut c, "abc");
        c.apply(EditOp::Home);
        assert_eq!(c.tail_caret(), 0);
        c.apply(EditOp::Delete);
        assert_eq!(c.tail(), "bc");
        c.apply(EditOp::End);
        assert_eq!(c.tail_caret(), 2);
        assert_eq!(c.apply(EditOp::Right), Applied::CaretOnly);
        assert_eq!(c.tail_caret(), 2);
    }

    #[test]
    fn sentence_end_requests_lock() {
        let mut c = s();
        typ(&mut c, "Hi.");
        assert_eq!(c.apply(EditOp::Insert(" ".into())), Applied::Changed { lock_now: true, immediate: true });
    }

    #[test]
    fn arabic_question_mark_locks() {
        let mut c = s();
        typ(&mut c, "كيف حالك؟");
        assert_eq!(c.apply(EditOp::Insert(" ".into())), Applied::Changed { lock_now: true, immediate: true });
    }

    #[test]
    fn space_is_immediate_not_lock() {
        let mut c = s();
        typ(&mut c, "hi");
        assert_eq!(c.apply(EditOp::Insert(" ".into())), Applied::Changed { lock_now: false, immediate: true });
        assert_eq!(c.apply(EditOp::Insert("x".into())), Applied::Changed { lock_now: false, immediate: false });
    }

    #[test]
    fn sentence_end_in_middle_does_not_lock() {
        let mut c = s();
        typ(&mut c, "ab");
        c.apply(EditOp::Left);
        assert_eq!(c.apply(EditOp::Insert(". ".into())), Applied::Changed { lock_now: false, immediate: true });
    }

    #[test]
    fn edit_ops_on_empty_tail_pass_through() {
        let mut c = s();
        for op in [EditOp::Backspace, EditOp::Left, EditOp::Delete, EditOp::Right, EditOp::Home, EditOp::End, EditOp::DeleteWordBack] {
            assert_eq!(c.apply(op), Applied::PassThrough);
        }
    }

    #[test]
    fn paste_inserts_at_caret() {
        let mut c = s();
        typ(&mut c, "ac");
        c.apply(EditOp::Left);
        c.apply(EditOp::Insert("b".into()));
        assert_eq!(c.tail(), "abc");
    }

    #[test]
    fn lock_tail_clears_tail_and_live_output() {
        let mut c = s();
        typ(&mut c, "hello");
        c.set_live_output("HELLO".into());
        c.lock_tail();
        assert!(c.is_tail_empty());
        assert_eq!(c.tail_caret(), 0);
        assert_eq!(c.live_output(), "");
    }

    #[test]
    fn emoji_is_one_grapheme() {
        let mut c = s();
        c.apply(EditOp::Insert("a".into()));
        c.apply(EditOp::Insert("👍🏽".into()));
        c.apply(EditOp::Backspace);
        assert_eq!(c.tail(), "a");
    }
}
