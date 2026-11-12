//! Controller tests against an in-memory FakeField (included from controller.rs).
use super::*;
use crate::core::test_support::{upper, FakeApi};
use crate::core::usage::UsageMeter;
use std::sync::Mutex as StdMutex;
use std::time::Duration;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Default)]
struct FieldState {
    text: String,
    caret: usize,
    clipboard: Option<String>,
    writes: Vec<ReplacePlan>,
}

struct FakeField {
    st: StdMutex<FieldState>,
    password: bool,
    write_fails: bool,
    selection: Option<(String, Option<Rect>)>,
}

impl FakeField {
    fn new(initial: &str) -> Self {
        Self {
            st: StdMutex::new(FieldState { text: initial.into(), caret: initial.len(), ..Default::default() }),
            password: false,
            write_fails: false,
            selection: None,
        }
    }
    fn text(&self) -> String {
        self.st.lock().unwrap().text.clone()
    }
}

impl Field for FakeField {
    fn focus(&self) -> Option<FocusInfo> {
        Some(FocusInfo {
            hwnd: 1,
            is_password: self.password,
            unit: LengthUnit::Graphemes,
            caret: Some(Rect { x: 100, y: 500, w: 2, h: 20 }),
            work_area: Rect { x: 0, y: 0, w: 1920, h: 1040 },
        })
    }
    fn write(&self, plan: &ReplacePlan, _expect: &str) -> WriteOutcome {
        if self.write_fails {
            return WriteOutcome::Failed;
        }
        let mut s = self.st.lock().unwrap();
        s.writes.push(plan.clone());
        let before = s.text[..s.caret].to_string();
        let mut start = s.caret;
        for g in before.graphemes(true).rev().take(plan.select_left) {
            start -= g.len();
        }
        let caret = s.caret;
        s.text.replace_range(start..caret, &plan.type_text);
        s.caret = start + plan.type_text.len();
        WriteOutcome::Ok
    }
    fn send_enter(&self) {
        let mut s = self.st.lock().unwrap();
        let c = s.caret;
        s.text.insert_str(c, "⏎");
        s.caret += "⏎".len();
    }
    fn set_clipboard(&self, text: &str) {
        self.st.lock().unwrap().clipboard = Some(text.into());
    }
    fn clipboard_text(&self) -> Option<String> {
        self.st.lock().unwrap().clipboard.clone()
    }
    fn read_selection(&self) -> Option<(String, Option<Rect>)> {
        self.selection.clone()
    }
}

#[derive(Default)]
struct RecordingUi {
    strip: StdMutex<Option<StripState>>,
    popup: StdMutex<Option<PopupState>>,
    notes: StdMutex<Vec<String>>,
}

impl UiSink for RecordingUi {
    fn strip(&self, s: &StripState) {
        *self.strip.lock().unwrap() = Some(s.clone());
    }
    fn popup(&self, p: &PopupState) {
        *self.popup.lock().unwrap() = Some(p.clone());
    }
    fn notify(&self, title: &str, _body: &str) {
        self.notes.lock().unwrap().push(title.into());
    }
}

struct Rig {
    h: ControllerHandle,
    field: Arc<FakeField>,
    ui: Arc<RecordingUi>,
    api: Arc<FakeApi>,
    settings: Arc<RwLock<Settings>>,
    dir: tempfile::TempDir,
}

impl Rig {
    fn strip(&self) -> StripState {
        self.ui.strip.lock().unwrap().clone().expect("strip state")
    }
    fn send(&self, ev: InputEvent) {
        self.h.input.send(ev).unwrap();
    }
    fn key(&self, k: KeyInput) {
        self.send(InputEvent::Key(k));
    }
    fn toggle(&self) {
        self.key(KeyInput::Hotkey(HotkeyId::Toggle));
    }
    fn typ(&self, s: &str) {
        for g in s.graphemes(true) {
            self.key(KeyInput::Text(g.into()));
        }
    }
    fn edit(&self, op: EditOp, n: usize) {
        for _ in 0..n {
            self.key(KeyInput::Edit(op.clone()));
        }
    }
}

fn rig_with(field: FakeField, preload: u64) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let today = chrono::Local::now().date_naive();
    let mut meter = UsageMeter::load(dir.path().join("usage.json"), today);
    if preload > 0 {
        meter.record(preload, today);
    }
    let api = Arc::new(FakeApi::new(upper));
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let translator = Translator::new(api.clone(), Arc::new(StdMutex::new(meter)), Arc::new(AtomicBool::new(true)), tx);
    let field = Arc::new(field);
    let ui = Arc::new(RecordingUi::default());
    let settings = Arc::new(RwLock::new(Settings::default()));
    let h = Controller::spawn(field.clone(), ui.clone(), translator, rx, settings.clone(), dir.path().join("settings.json"));
    Rig { h, field, ui, api, settings, dir }
}

fn rig() -> Rig {
    rig_with(FakeField::new(""), 0)
}

async fn wait(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

#[tokio::test(start_paused = true)]
async fn typing_shows_translation_only() {
    let r = rig();
    r.toggle();
    r.typ("bonjour");
    wait(300).await;
    assert_eq!(r.field.text(), "BONJOUR");
    let s = r.strip();
    assert!(s.visible);
    assert_eq!(s.source, "bonjour");
    assert_eq!(s.status, StripStatus::Ok);
    assert_eq!(s.detected.as_deref(), Some("fr"));
    assert_eq!(s.pos, Some(Point { x: 100, y: 500 - STRIP_SIZE.h - 8 }));
}

#[tokio::test(start_paused = true)]
async fn edit_mid_sentence_rewrites() {
    let r = rig();
    r.toggle();
    r.typ("hello world");
    wait(300).await;
    r.edit(EditOp::Left, 6);
    r.edit(EditOp::Backspace, 5);
    r.typ("bye");
    wait(300).await;
    assert_eq!(r.field.text(), "BYE WORLD");
}

#[tokio::test(start_paused = true)]
async fn preexisting_text_untouched() {
    let r = rig_with(FakeField::new("Hi: "), 0);
    r.toggle();
    r.typ("ok");
    wait(300).await;
    assert_eq!(r.field.text(), "Hi: OK");
}

#[tokio::test(start_paused = true)]
async fn sentence_lock_then_new_sentence() {
    let r = rig();
    r.toggle();
    r.typ("one. ");
    wait(50).await;
    r.typ("two");
    wait(300).await;
    assert_eq!(r.field.text(), "ONE. TWO");
    r.edit(EditOp::Backspace, 3);
    wait(300).await;
    assert_eq!(r.field.text(), "ONE. ");
    assert_eq!(r.strip().source, "");
}

#[tokio::test(start_paused = true)]
async fn enter_flushes_pending_translation_before_sending() {
    let r = rig();
    r.api.push_delays(&[100]);
    r.toggle();
    r.typ("abc");
    r.key(KeyInput::Enter);
    wait(400).await;
    assert_eq!(r.field.text(), "ABC⏎");
}

#[tokio::test(start_paused = true)]
async fn enter_with_current_translation_sends_once() {
    let r = rig();
    r.toggle();
    r.typ("abc");
    wait(300).await;
    r.key(KeyInput::Enter);
    wait(10).await;
    assert_eq!(r.field.text(), "ABC⏎");
    assert_eq!(r.api.call_count(), 1);
}

#[tokio::test(start_paused = true)]
async fn backspace_with_empty_tail_reaches_app() {
    let r = rig();
    r.toggle();
    r.typ("abc");
    wait(300).await;
    assert!(!r.h.state.tail_empty.load(Ordering::SeqCst));
    r.key(KeyInput::Enter);
    wait(10).await;
    assert!(r.h.state.active.load(Ordering::SeqCst));
    assert!(r.h.state.tail_empty.load(Ordering::SeqCst));
    assert_eq!(r.field.text(), "ABC⏎");
}

#[tokio::test(start_paused = true)]
async fn passthrough_shortcut_locks_tail() {
    let r = rig();
    r.toggle();
    r.typ("ab");
    wait(300).await;
    r.key(KeyInput::Other);
    r.typ("c");
    wait(300).await;
    assert_eq!(r.field.text(), "ABC");
    assert_eq!(r.field.st.lock().unwrap().writes.last().unwrap().select_left, 0);
}

#[tokio::test(start_paused = true)]
async fn stale_output_ignored() {
    let r = rig();
    r.api.push_delays(&[300, 10]);
    r.toggle();
    r.typ("a");
    wait(250).await;
    r.typ("b");
    wait(600).await;
    assert_eq!(r.field.text(), "AB");
}

#[tokio::test(start_paused = true)]
async fn password_refused() {
    let mut f = FakeField::new("");
    f.password = true;
    let r = rig_with(f, 0);
    r.toggle();
    wait(10).await;
    assert!(!r.h.state.active.load(Ordering::SeqCst));
    assert_eq!(r.strip().status, StripStatus::Password);
    wait(2000).await;
    assert!(!r.strip().visible);
}

#[tokio::test(start_paused = true)]
async fn focus_change_ends_session_without_writing() {
    let r = rig();
    r.api.push_delays(&[200]);
    r.toggle();
    r.typ("x");
    wait(1).await;
    r.send(InputEvent::FocusChanged);
    wait(600).await;
    assert_eq!(r.field.text(), "");
    assert!(!r.strip().visible);
    assert!(!r.h.state.active.load(Ordering::SeqCst));
}

#[tokio::test(start_paused = true)]
async fn offline_offers_original_on_exit() {
    let r = rig();
    r.api.fail_always(Some(TranslateError::Offline));
    r.toggle();
    r.typ("salut");
    wait(300).await;
    assert_eq!(r.strip().status, StripStatus::Offline);
    r.key(KeyInput::Escape);
    wait(10).await;
    assert!(r.strip().offer_original);
    assert!(!r.h.state.active.load(Ordering::SeqCst));
    r.h.offline_choice(true);
    wait(10).await;
    assert_eq!(r.field.text(), "salut");
    assert!(!r.strip().visible);
}

#[tokio::test(start_paused = true)]
async fn offline_retries_automatically() {
    let r = rig();
    r.api.push_errors(&[Some(TranslateError::Offline)]);
    r.toggle();
    r.typ("salut");
    wait(300).await;
    assert_eq!(r.strip().status, StripStatus::Offline);
    wait(2500).await;
    assert_eq!(r.field.text(), "SALUT");
    assert_eq!(r.strip().status, StripStatus::Ok);
}

#[tokio::test(start_paused = true)]
async fn write_failure_uses_clipboard() {
    let mut f = FakeField::new("");
    f.write_fails = true;
    let r = rig_with(f, 0);
    r.toggle();
    r.typ("hi");
    wait(300).await;
    assert_eq!(r.field.clipboard_text().as_deref(), Some("HI"));
    assert_eq!(r.strip().status, StripStatus::PasteFallback);
}

#[tokio::test(start_paused = true)]
async fn cycle_target_resubmits() {
    let r = rig();
    r.toggle();
    r.typ("ab");
    wait(300).await;
    assert_eq!(r.api.call_count(), 1);
    r.key(KeyInput::Hotkey(HotkeyId::CycleTarget));
    wait(10).await;
    assert_eq!(r.settings.read().unwrap().target, "fr");
    assert_eq!(Settings::load(&r.dir.path().join("settings.json")).target, "fr");
    assert_eq!(r.strip().target, "fr");
    assert_eq!(r.api.call_count(), 2);
}

#[tokio::test(start_paused = true)]
async fn threshold_notifies() {
    let r = rig_with(FakeField::new(""), 399_999);
    r.toggle();
    r.typ("ab");
    wait(300).await;
    assert_eq!(r.ui.notes.lock().unwrap().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn paste_inserts_clipboard_into_source() {
    let r = rig();
    r.field.set_clipboard("coucou");
    r.toggle();
    r.key(KeyInput::Paste);
    wait(300).await;
    assert_eq!(r.strip().source, "coucou");
    assert_eq!(r.field.text(), "COUCOU");
}

#[tokio::test(start_paused = true)]
async fn read_selection_shows_popup() {
    let mut f = FakeField::new("");
    f.selection = Some(("salut".into(), Some(Rect { x: 50, y: 400, w: 80, h: 20 })));
    let r = rig_with(f, 0);
    r.key(KeyInput::Hotkey(HotkeyId::ReadSelection));
    wait(10).await;
    let p = r.ui.popup.lock().unwrap().clone().unwrap();
    assert!(p.visible);
    assert_eq!(p.translation.as_deref(), Some("SALUT"));
    assert_eq!(p.detected.as_deref(), Some("fr"));
    assert_eq!(p.target, "en");
    r.send(InputEvent::MouseDown { x: 1500, y: 10 });
    wait(10).await;
    assert!(!r.ui.popup.lock().unwrap().clone().unwrap().visible);
}

#[tokio::test(start_paused = true)]
async fn reply_in_starts_session_with_language() {
    let r = rig();
    r.h.reply_in("fr".into());
    wait(10).await;
    assert!(r.h.state.active.load(Ordering::SeqCst));
    assert_eq!(r.strip().target, "fr");
}
