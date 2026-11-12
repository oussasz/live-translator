//! Computes the minimal keystrokes to turn the previously written translation into the new one.
pub use crate::core::types::LengthUnit;
use unicode_segmentation::UnicodeSegmentation;

/// Press Shift+Left `select_left` times, then type `type_text` (typing replaces the selection).
/// If `type_text` is empty and `select_left > 0`, press Delete once instead.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ReplacePlan {
    pub select_left: usize,
    pub type_text: String,
}

impl ReplacePlan {
    pub fn is_noop(&self) -> bool {
        self.select_left == 0 && self.type_text.is_empty()
    }
}

pub fn len_in(s: &str, unit: LengthUnit) -> usize {
    match unit {
        LengthUnit::Graphemes => s.graphemes(true).count(),
        LengthUnit::Utf16 => s.encode_utf16().count(),
    }
}

/// Keeps the longest common grapheme prefix (less flicker), replaces the rest.
pub fn plan_replace(prev: &str, next: &str, unit: LengthUnit) -> ReplacePlan {
    let common: usize = prev
        .graphemes(true)
        .zip(next.graphemes(true))
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len())
        .sum();
    ReplacePlan { select_left: len_in(&prev[common..], unit), type_text: next[common..].to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use LengthUnit::{Graphemes as G, Utf16 as U};

    fn p(n: usize, t: &str) -> ReplacePlan {
        ReplacePlan { select_left: n, type_text: t.into() }
    }

    #[test]
    fn identical_is_noop() {
        assert_eq!(plan_replace("Hello", "Hello", G), p(0, ""));
    }
    #[test]
    fn append_only() {
        assert_eq!(plan_replace("Hello", "Hello there", G), p(0, " there"));
    }
    #[test]
    fn rewrite_suffix() {
        assert_eq!(plan_replace("Hello world", "Hello there", G), p(5, "there"));
    }
    #[test]
    fn full_rewrite() {
        assert_eq!(plan_replace("I go", "We went", G), p(4, "We went"));
    }
    #[test]
    fn shrink_to_empty() {
        assert_eq!(plan_replace("abc", "", G), p(3, ""));
    }
    #[test]
    fn emoji_counts_by_unit() {
        assert_eq!(plan_replace("ok 👍🏽", "ok", G).select_left, 2);
        assert_eq!(plan_replace("ok 👍🏽", "ok", U).select_left, 5);
    }
    #[test]
    fn french_accent_combining() {
        assert_eq!(plan_replace("cafe\u{301}", "cafe", G), p(1, "e"));
        assert_eq!(plan_replace("cafe\u{301}", "cafe", U), p(2, "e"));
    }
}
