//! UI Automation + Win32 queries about the focused text field.
use crate::controller::{FocusInfo, InputEvent};
use crate::core::types::{LengthUnit, Rect};
use std::cell::RefCell;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{ClientToScreen, GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, SAFEARRAY};
use windows::Win32::System::Ole::{SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetUBound, SafeArrayUnaccessData};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern, IUIAutomationTextPattern2,
    IUIAutomationValuePattern, UIA_TextPattern2Id, UIA_TextPatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO};

thread_local! {
    static UIA: RefCell<Option<IUIAutomation>> = const { RefCell::new(None) };
}

fn uia() -> Option<IUIAutomation> {
    UIA.with(|cell| {
        if cell.borrow().is_none() {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                *cell.borrow_mut() = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok();
            }
        }
        cell.borrow().clone()
    })
}

fn focused_element() -> Option<IUIAutomationElement> {
    unsafe { uia()?.GetFocusedElement().ok() }
}

/// First rectangle (x, y, w, h doubles) from a UIA SAFEARRAY; destroys the array.
unsafe fn first_rect(psa: *mut SAFEARRAY) -> Option<Rect> {
    if psa.is_null() {
        return None;
    }
    let mut out = None;
    unsafe {
        if let Ok(ub) = SafeArrayGetUBound(psa, 1) {
            if ub >= 3 {
                let mut data: *mut core::ffi::c_void = std::ptr::null_mut();
                if SafeArrayAccessData(psa, &mut data).is_ok() {
                    let d = std::slice::from_raw_parts(data as *const f64, 4);
                    out = Some(Rect { x: d[0] as i32, y: d[1] as i32, w: d[2].max(1.0) as i32, h: d[3].max(1.0) as i32 });
                    let _ = SafeArrayUnaccessData(psa);
                }
            }
        }
        let _ = SafeArrayDestroy(psa);
    }
    out
}

fn system_caret() -> Option<Rect> {
    unsafe {
        let fg = GetForegroundWindow();
        let thread = GetWindowThreadProcessId(fg, None);
        let mut gi = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        GetGUIThreadInfo(thread, &mut gi).ok()?;
        if gi.hwndCaret.is_invalid() || gi.rcCaret.bottom <= gi.rcCaret.top {
            return None;
        }
        let mut p = POINT { x: gi.rcCaret.left, y: gi.rcCaret.top };
        let _ = ClientToScreen(gi.hwndCaret, &mut p);
        Some(Rect { x: p.x, y: p.y, w: 2, h: gi.rcCaret.bottom - gi.rcCaret.top })
    }
}

fn uia_caret(el: &IUIAutomationElement) -> Option<Rect> {
    unsafe {
        let tp2: IUIAutomationTextPattern2 = el.GetCurrentPatternAs(UIA_TextPattern2Id).ok()?;
        let mut active = windows::core::BOOL(0);
        let range = tp2.GetCaretRange(&mut active).ok()?;
        first_rect(range.GetBoundingRectangles().ok()?)
    }
}

fn element_anchor(el: &IUIAutomationElement) -> Option<Rect> {
    let r: RECT = unsafe { el.CurrentBoundingRectangle().ok()? };
    Some(Rect { x: r.left, y: r.top, w: 2, h: (r.bottom - r.top).clamp(1, 30) })
}

pub fn work_area_at(x: i32, y: i32) -> Rect {
    unsafe {
        let mon = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(mon, &mut mi).as_bool() {
            let w = mi.rcWork;
            return Rect { x: w.left, y: w.top, w: w.right - w.left, h: w.bottom - w.top };
        }
    }
    Rect { x: 0, y: 0, w: 1920, h: 1040 }
}

pub fn focused() -> Option<FocusInfo> {
    let el = focused_element()?;
    unsafe {
        let is_password = el.CurrentIsPassword().map(|b| b.as_bool()).unwrap_or(false);
        let class = el.CurrentClassName().map(|b| b.to_string()).unwrap_or_default();
        let unit = if class == "Edit" || class.starts_with("RichEdit") { LengthUnit::Utf16 } else { LengthUnit::Graphemes };
        let caret = system_caret().or_else(|| uia_caret(&el)).or_else(|| element_anchor(&el));
        let at = caret.map(|c| (c.x, c.y)).unwrap_or((0, 0));
        Some(FocusInfo { hwnd: GetForegroundWindow().0 as isize, is_password, unit, caret, work_area: work_area_at(at.0, at.1) })
    }
}

/// Current value of the focused control when it exposes a ValuePattern.
pub fn focused_value() -> Option<String> {
    let el = focused_element()?;
    unsafe {
        let vp: IUIAutomationValuePattern = el.GetCurrentPatternAs(UIA_ValuePatternId).ok()?;
        vp.CurrentValue().ok().map(|b| b.to_string())
    }
}

/// Selected text of the focused element (and where it is), via TextPattern.
pub fn selection() -> Option<(String, Option<Rect>)> {
    let el = focused_element()?;
    unsafe {
        let tp: IUIAutomationTextPattern = el.GetCurrentPatternAs(UIA_TextPatternId).ok()?;
        let ranges = tp.GetSelection().ok()?;
        if ranges.Length().ok()? < 1 {
            return None;
        }
        let range = ranges.GetElement(0).ok()?;
        let text = range.GetText(-1).ok()?.to_string();
        if text.trim().is_empty() {
            return None;
        }
        let rect = range.GetBoundingRectangles().ok().and_then(|p| first_rect(p));
        Some((text, rect))
    }
}

fn focus_key() -> (isize, isize) {
    unsafe {
        let fg = GetForegroundWindow();
        let thread = GetWindowThreadProcessId(fg, None);
        let mut gi = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        let focus = if GetGUIThreadInfo(thread, &mut gi).is_ok() { gi.hwndFocus } else { HWND::default() };
        (fg.0 as isize, focus.0 as isize)
    }
}

/// Polls the foreground window and its focused child every 100 ms and reports changes.
pub fn watch_focus(tx: UnboundedSender<InputEvent>) {
    std::thread::Builder::new()
        .name("lt-focus".into())
        .spawn(move || {
            let mut last = focus_key();
            loop {
                std::thread::sleep(Duration::from_millis(100));
                let now = focus_key();
                if now != last {
                    last = now;
                    if tx.send(InputEvent::FocusChanged).is_err() {
                        return;
                    }
                }
            }
        })
        .expect("spawn focus watcher");
}
