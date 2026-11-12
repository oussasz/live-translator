//! The typing-flow brain. Talks to Windows only through `Field` and to the UI only
//! through `UiSink`, so the whole flow is testable with fakes.
use crate::core::compose::{Applied, ComposeSession, EditOp};
use crate::core::placement::place_near;
use crate::core::replace::{plan_replace, ReplacePlan};
use crate::core::settings::Settings;
use crate::core::translator::{Translator, TranslatorOutput};
use crate::core::types::{Lang, LengthUnit, Point, Rect, Size, TranslateError, Translation};
use crate::core::usage::Threshold;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

pub const STRIP_SIZE: Size = Size { w: 420, h: 44 };
pub const POPUP_SIZE: Size = Size { w: 360, h: 180 };
const GAP: i32 = 8;
const PASSWORD_NOTICE: Duration = Duration::from_millis(1500);
const OFFLINE_RETRY: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyId {
    Toggle,
    CycleTarget,
    ReadSelection,
}

/// What the hook saw. The hook sends a key's real meaning only when it swallowed it;
/// a key it let through to the app during a session arrives as `Other`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyInput {
    Text(String),
    Edit(EditOp),
    Enter,
    Escape,
    Paste,
    Hotkey(HotkeyId),
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputEvent {
    Key(KeyInput),
    MouseDown { x: i32, y: i32 },
    FocusChanged,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FocusInfo {
    pub hwnd: isize,
    pub is_password: bool,
    pub unit: LengthUnit,
    pub caret: Option<Rect>,
    pub work_area: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteOutcome {
    Ok,
    Failed,
}

pub trait Field: Send + Sync {
    fn focus(&self) -> Option<FocusInfo>;
    fn write(&self, plan: &ReplacePlan, expect_suffix: &str) -> WriteOutcome;
    fn send_enter(&self);
    fn set_clipboard(&self, text: &str);
    fn clipboard_text(&self) -> Option<String>;
    fn read_selection(&self) -> Option<(String, Option<Rect>)>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StripStatus {
    Idle,
    Translating,
    Ok,
    Offline,
    KeyError,
    Cap,
    QuotaError,
    Error,
    PasteFallback,
    Password,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StripState {
    pub visible: bool,
    pub source: String,
    pub caret: usize,
    pub target: Lang,
    pub detected: Option<Lang>,
    pub status: StripStatus,
    pub pos: Option<Point>,
    pub offer_original: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PopupState {
    pub visible: bool,
    pub source: String,
    pub translation: Option<String>,
    pub detected: Option<Lang>,
    pub target: Lang,
    pub error: Option<TranslateError>,
    pub pos: Option<Point>,
}

pub trait UiSink: Send + Sync {
    fn strip(&self, s: &StripState);
    fn popup(&self, p: &PopupState);
    fn notify(&self, title: &str, body: &str);
    /// Translate mode switched on/off (tray label). Optional.
    fn mode(&self, _active: bool) {}
}

/// Shared with the hook thread so it can decide to swallow keys without a round trip.
#[derive(Default)]
pub struct HookState {
    pub active: AtomicBool,
    pub tail_empty: AtomicBool,
    pub paused: AtomicBool,
}

enum Cmd {
    OfflineChoice(bool),
    SetTarget(Lang),
    ReplyIn(Lang),
    ClosePopup,
    HidePasswordNotice,
    Retry,
}

#[derive(Clone)]
pub struct ControllerHandle {
    pub input: UnboundedSender<InputEvent>,
    pub state: Arc<HookState>,
    cmd: UnboundedSender<Cmd>,
}

impl ControllerHandle {
    pub fn offline_choice(&self, insert_original: bool) {
        let _ = self.cmd.send(Cmd::OfflineChoice(insert_original));
    }
    pub fn set_target(&self, t: Lang) {
        let _ = self.cmd.send(Cmd::SetTarget(t));
    }
    pub fn reply_in(&self, t: Lang) {
        let _ = self.cmd.send(Cmd::ReplyIn(t));
    }
    pub fn close_popup(&self) {
        let _ = self.cmd.send(Cmd::ClosePopup);
    }
}

struct Active {
    compose: ComposeSession,
    focus: FocusInfo,
    /// Tail changed since the last translation was written.
    dirty: bool,
    last_submitted: u64,
    pending_lock: Option<u64>,
}

/// Text the owner can still insert after leaving translate mode while offline.
struct Leftover {
    original: String,
    live_output: String,
    unit: LengthUnit,
}

pub struct Controller {
    field: Arc<dyn Field>,
    ui: Arc<dyn UiSink>,
    translator: Arc<Translator>,
    settings: Arc<RwLock<Settings>>,
    settings_path: PathBuf,
    state: Arc<HookState>,
    cmd_tx: UnboundedSender<Cmd>,
    session: Option<Active>,
    leftover: Option<Leftover>,
    strip: StripState,
    popup: PopupState,
}

fn status_for(e: &TranslateError) -> StripStatus {
    match e {
        TranslateError::Offline => StripStatus::Offline,
        TranslateError::InvalidKey => StripStatus::KeyError,
        TranslateError::QuotaExceeded => StripStatus::QuotaError,
        TranslateError::CapReached => StripStatus::Cap,
        TranslateError::Other(_) => StripStatus::Error,
    }
}

fn default_anchor(work: Rect) -> Rect {
    Rect { x: work.x + work.w / 2 - STRIP_SIZE.w / 2, y: work.y + work.h - 120, w: 1, h: 20 }
}

impl Controller {
    pub fn spawn(
        field: Arc<dyn Field>,
        ui: Arc<dyn UiSink>,
        translator: Arc<Translator>,
        outputs: UnboundedReceiver<TranslatorOutput>,
        settings: Arc<RwLock<Settings>>,
        settings_path: PathBuf,
    ) -> ControllerHandle {
        let (input_tx, input_rx) = unbounded_channel();
        let (cmd_tx, cmd_rx) = unbounded_channel();
        let state = Arc::new(HookState::default());
        state.tail_empty.store(true, Ordering::SeqCst);
        let target = settings.read().unwrap().target.clone();
        let me = Controller {
            field,
            ui,
            translator,
            settings,
            settings_path,
            state: state.clone(),
            cmd_tx: cmd_tx.clone(),
            session: None,
            leftover: None,
            strip: StripState {
                visible: false,
                source: String::new(),
                caret: 0,
                target: target.clone(),
                detected: None,
                status: StripStatus::Idle,
                pos: None,
                offer_original: false,
            },
            popup: PopupState {
                visible: false,
                source: String::new(),
                translation: None,
                detected: None,
                target,
                error: None,
                pos: None,
            },
        };
        tokio::spawn(me.run(input_rx, cmd_rx, outputs));
        ControllerHandle { input: input_tx, state, cmd: cmd_tx }
    }

    async fn run(
        mut self,
        mut input: UnboundedReceiver<InputEvent>,
        mut cmds: UnboundedReceiver<Cmd>,
        mut outputs: UnboundedReceiver<TranslatorOutput>,
    ) {
        loop {
            tokio::select! {
                Some(ev) = input.recv() => self.on_input(ev).await,
                Some(c) = cmds.recv() => self.on_cmd(c).await,
                Some(o) = outputs.recv() => self.on_output(o),
                else => break,
            }
        }
    }

    // ---------- state publishing ----------

    fn sync_hook_state(&self) {
        let (active, empty) = match &self.session {
            Some(s) => (true, s.compose.is_tail_empty() && s.compose.live_output().is_empty() && !s.dirty),
            None => (false, true),
        };
        self.state.active.store(active, Ordering::SeqCst);
        self.state.tail_empty.store(empty, Ordering::SeqCst);
    }

    fn push_strip(&mut self) {
        if let Some(s) = &self.session {
            self.strip.source = s.compose.tail().to_string();
            self.strip.caret = s.compose.tail_caret();
            self.strip.target = s.compose.target().to_string();
        }
        self.sync_hook_state();
        self.ui.strip(&self.strip);
    }

    fn notify_threshold(&self, t: Option<Threshold>) {
        match t {
            Some(Threshold::Warn80) => self.ui.notify(
                "80% of free translations used",
                "You've used 80% of this month's free Google translations.",
            ),
            Some(Threshold::Reached100) => self.ui.notify(
                "Free translation limit reached",
                "Translation pauses until next month. You can turn off the limit in settings.",
            ),
            None => {}
        }
    }

    fn current_target(&self) -> Lang {
        self.settings.read().unwrap().target.clone()
    }

    fn save_target(&self, t: &Lang) {
        let mut s = self.settings.write().unwrap();
        s.target = t.clone();
        let _ = s.save(&self.settings_path);
    }

    // ---------- events ----------

    async fn on_input(&mut self, ev: InputEvent) {
        match ev {
            InputEvent::Key(KeyInput::Hotkey(HotkeyId::Toggle)) => {
                if self.session.is_some() {
                    self.end_session();
                } else {
                    self.start_session();
                }
            }
            InputEvent::Key(KeyInput::Hotkey(HotkeyId::CycleTarget)) => self.cycle_target(),
            InputEvent::Key(KeyInput::Hotkey(HotkeyId::ReadSelection)) => self.read_selection().await,
            InputEvent::Key(KeyInput::Escape) => {
                if self.session.is_some() {
                    self.end_session();
                }
            }
            InputEvent::Key(KeyInput::Text(t)) => self.edit(EditOp::Insert(t)),
            InputEvent::Key(KeyInput::Paste) => {
                if let Some(t) = self.field.clipboard_text() {
                    self.edit(EditOp::Insert(t));
                }
            }
            InputEvent::Key(KeyInput::Edit(op)) => self.edit(op),
            InputEvent::Key(KeyInput::Enter) => self.enter().await,
            InputEvent::Key(KeyInput::Other) => self.lock_tail(),
            InputEvent::MouseDown { x, y } => {
                self.lock_tail();
                if self.popup.visible && !self.point_in_popup(x, y) {
                    self.hide_popup();
                }
            }
            InputEvent::FocusChanged => {
                if self.session.is_some() {
                    self.session = None;
                    self.strip.visible = false;
                    self.ui.mode(false);
                    self.push_strip();
                }
            }
        }
    }

    async fn on_cmd(&mut self, c: Cmd) {
        match c {
            Cmd::OfflineChoice(insert) => {
                if let Some(l) = self.leftover.take() {
                    if insert {
                        let plan = plan_replace(&l.live_output, &l.original, l.unit);
                        if !plan.is_noop() && self.field.write(&plan, &l.original) == WriteOutcome::Failed {
                            self.field.set_clipboard(&l.original);
                        }
                    }
                }
                self.strip.offer_original = false;
                self.strip.visible = false;
                self.push_strip();
            }
            Cmd::SetTarget(t) => self.apply_target(t),
            Cmd::ReplyIn(t) => {
                self.hide_popup();
                self.save_target(&t);
                if self.session.is_none() {
                    self.start_session();
                } else {
                    self.apply_target(t);
                }
            }
            Cmd::ClosePopup => self.hide_popup(),
            Cmd::HidePasswordNotice => {
                if self.session.is_none() && self.strip.status == StripStatus::Password {
                    self.strip.visible = false;
                    self.strip.status = StripStatus::Idle;
                    self.push_strip();
                }
            }
            Cmd::Retry => {
                let resubmit = match &self.session {
                    Some(s) => s.dirty && self.strip.status == StripStatus::Offline,
                    None => false,
                };
                if resubmit {
                    self.submit(true, false);
                }
            }
        }
    }

    fn on_output(&mut self, o: TranslatorOutput) {
        self.notify_threshold(o.threshold);
        let Some(s) = &self.session else { return };
        if o.seq != self.translator.latest_seq() || o.seq != s.last_submitted {
            return;
        }
        match o.result {
            Ok(t) => {
                self.write_translation(&t);
                if let Some(s) = self.session.as_mut() {
                    s.dirty = false;
                    if s.pending_lock == Some(o.seq) {
                        s.pending_lock = None;
                        s.compose.lock_tail();
                    }
                }
            }
            Err(e) => {
                self.strip.status = status_for(&e);
                if e == TranslateError::Offline {
                    let tx = self.cmd_tx.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(OFFLINE_RETRY).await;
                        let _ = tx.send(Cmd::Retry);
                    });
                }
            }
        }
        self.push_strip();
    }

    // ---------- session ----------

    fn start_session(&mut self) {
        let Some(focus) = self.field.focus() else { return };
        self.leftover = None;
        self.strip.offer_original = false;
        let anchor = focus.caret.unwrap_or_else(|| default_anchor(focus.work_area));
        self.strip.pos = Some(place_near(anchor, STRIP_SIZE, focus.work_area, GAP));
        self.strip.visible = true;
        self.strip.detected = None;
        if focus.is_password {
            self.strip.status = StripStatus::Password;
            self.strip.source.clear();
            self.push_strip();
            let tx = self.cmd_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(PASSWORD_NOTICE).await;
                let _ = tx.send(Cmd::HidePasswordNotice);
            });
            return;
        }
        self.strip.status = StripStatus::Idle;
        self.session = Some(Active {
            compose: ComposeSession::new(self.current_target()),
            focus,
            dirty: false,
            last_submitted: 0,
            pending_lock: None,
        });
        self.ui.mode(true);
        self.push_strip();
    }

    fn end_session(&mut self) {
        let Some(s) = self.session.take() else { return };
        let failed = matches!(
            self.strip.status,
            StripStatus::Offline | StripStatus::KeyError | StripStatus::QuotaError | StripStatus::Cap | StripStatus::Error
        );
        if failed && s.dirty && !s.compose.is_tail_empty() {
            self.leftover = Some(Leftover {
                original: s.compose.tail().to_string(),
                live_output: s.compose.live_output().to_string(),
                unit: s.focus.unit,
            });
            self.strip.offer_original = true;
        } else {
            self.strip.visible = false;
        }
        self.ui.mode(false);
        self.push_strip();
    }

    fn lock_tail(&mut self) {
        if let Some(s) = self.session.as_mut() {
            s.compose.lock_tail();
            s.dirty = false;
            s.pending_lock = None;
            self.push_strip();
        }
    }

    fn edit(&mut self, op: EditOp) {
        let Some(s) = self.session.as_mut() else { return };
        match s.compose.apply(op) {
            Applied::PassThrough => {}
            Applied::CaretOnly => self.push_strip(),
            Applied::Changed { lock_now, immediate } => self.submit(immediate, lock_now),
        }
    }

    fn submit(&mut self, immediate: bool, lock_now: bool) {
        let Some(s) = self.session.as_mut() else { return };
        s.dirty = true;
        let seq = self.translator.submit(s.compose.tail().to_string(), s.compose.target().to_string(), immediate);
        s.last_submitted = seq;
        s.pending_lock = lock_now.then_some(seq);
        self.strip.status = StripStatus::Translating;
        self.push_strip();
    }

    /// Writes `t` over the current live output. Returns false if the app refused the write.
    fn write_translation(&mut self, t: &Translation) -> bool {
        let Some(s) = self.session.as_mut() else { return false };
        self.strip.detected = t.detected.clone();
        let plan = plan_replace(s.compose.live_output(), &t.text, s.focus.unit);
        if plan.is_noop() {
            self.strip.status = StripStatus::Ok;
            return true;
        }
        match self.field.write(&plan, &t.text) {
            WriteOutcome::Ok => {
                s.compose.set_live_output(t.text.clone());
                self.strip.status = StripStatus::Ok;
                true
            }
            WriteOutcome::Failed => {
                self.field.set_clipboard(&t.text);
                s.compose.lock_tail();
                s.dirty = false;
                self.strip.status = StripStatus::PasteFallback;
                false
            }
        }
    }

    async fn enter(&mut self) {
        let Some(s) = &self.session else { return };
        if s.dirty && !s.compose.is_tail_empty() {
            let (tail, target) = (s.compose.tail().to_string(), s.compose.target().to_string());
            let (res, threshold) = self.translator.translate_now(&tail, &target).await;
            self.notify_threshold(threshold);
            match res {
                Ok(t) => {
                    self.write_translation(&t);
                }
                Err(e) => self.strip.status = status_for(&e),
            }
        }
        if let Some(s) = self.session.as_mut() {
            s.compose.lock_tail();
            s.dirty = false;
            s.pending_lock = None;
        }
        self.field.send_enter();
        self.push_strip();
    }

    // ---------- targets ----------

    fn cycle_target(&mut self) {
        let favs = self.settings.read().unwrap().favorites.clone();
        if favs.is_empty() {
            return;
        }
        let cur = self.session.as_ref().map(|s| s.compose.target().to_string()).unwrap_or_else(|| self.current_target());
        let i = favs.iter().position(|f| *f == cur).map(|i| (i + 1) % favs.len()).unwrap_or(0);
        self.apply_target(favs[i].clone());
    }

    fn apply_target(&mut self, t: Lang) {
        self.save_target(&t);
        self.strip.target = t.clone();
        self.popup.target = t.clone();
        let resubmit = match self.session.as_mut() {
            Some(s) => {
                s.compose.set_target(t);
                !s.compose.is_tail_empty()
            }
            None => false,
        };
        if resubmit {
            self.submit(true, false);
        } else {
            self.push_strip();
        }
    }

    // ---------- read popup ----------

    async fn read_selection(&mut self) {
        let Some((text, rect)) = self.field.read_selection() else { return };
        if text.trim().is_empty() {
            return;
        }
        let work = self.field.focus().map(|f| f.work_area).unwrap_or(Rect { x: 0, y: 0, w: 1920, h: 1040 });
        let anchor = rect.unwrap_or_else(|| default_anchor(work));
        let mut target = self.current_target();
        let (mut res, threshold) = self.translator.translate_now(&text, &target).await;
        self.notify_threshold(threshold);
        // Text already in the target language: show it in the next favourite instead.
        if let Ok(t) = &res {
            if t.detected.as_deref() == Some(target.as_str()) {
                let favs = self.settings.read().unwrap().favorites.clone();
                if let Some(alt) = favs.iter().find(|f| **f != target).cloned() {
                    let (r2, th2) = self.translator.translate_now(&text, &alt).await;
                    self.notify_threshold(th2);
                    res = r2.map(|mut t2| {
                        t2.detected = t2.detected.or(Some(target.clone()));
                        t2
                    });
                    target = alt;
                }
            }
        }
        let (translation, detected, error) = match res {
            Ok(t) => (Some(t.text), t.detected, None),
            Err(e) => (None, None, Some(e)),
        };
        self.popup = PopupState {
            visible: true,
            source: text,
            translation,
            detected,
            target,
            error,
            pos: Some(place_near(anchor, POPUP_SIZE, work, GAP)),
        };
        self.ui.popup(&self.popup);
    }

    fn point_in_popup(&self, x: i32, y: i32) -> bool {
        match self.popup.pos {
            Some(p) => x >= p.x && x < p.x + POPUP_SIZE.w && y >= p.y && y < p.y + POPUP_SIZE.h,
            None => false,
        }
    }

    fn hide_popup(&mut self) {
        if self.popup.visible {
            self.popup.visible = false;
            self.ui.popup(&self.popup);
        }
    }
}

#[cfg(test)]
#[path = "controller_tests.rs"]
mod tests;
