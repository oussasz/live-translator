//! AT-SPI focus tracking on its own thread. Other threads ask it questions through a channel.
use crate::controller::{FocusInfo, InputEvent};
use crate::core::types::{LengthUnit, Rect};
use atspi::events::focus::FocusEvent;
use atspi::events::object::StateChangedEvent;
use atspi::events::{Event, FocusEvents, ObjectEvents};
use atspi::proxy::accessible::AccessibleProxy;
use atspi::proxy::text::TextProxy;
use atspi::{AccessibilityConnection, CoordType, Interface, Role, State};
use futures_lite::StreamExt;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;
use zbus::proxy::CacheProperties;

enum Query {
    Focus(std::sync::mpsc::Sender<Option<FocusInfo>>),
    Selection(std::sync::mpsc::Sender<Option<(String, Option<Rect>)>>),
}

static QUERIES: OnceLock<tokio::sync::mpsc::UnboundedSender<Query>> = OnceLock::new();
static CURRENT: Mutex<Option<(String, String)>> = Mutex::new(None);
static CURRENT_ID: AtomicIsize = AtomicIsize::new(0);
static TERMINAL: AtomicBool = AtomicBool::new(false);
static WORK_AREA: Mutex<Rect> = Mutex::new(Rect { x: 0, y: 0, w: 1920, h: 1040 });

pub fn set_work_area(r: Rect) {
    *WORK_AREA.lock().unwrap() = r;
}

fn work_area() -> Rect {
    *WORK_AREA.lock().unwrap()
}

/// A stable id for the focused object (0 = unknown / AT-SPI unavailable).
pub fn object_id(name: &str, path: &str) -> isize {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (name, path).hash(&mut h);
    (h.finish() as isize).max(1)
}

pub fn current_id() -> isize {
    CURRENT_ID.load(Ordering::SeqCst)
}

pub fn is_terminal() -> bool {
    TERMINAL.load(Ordering::SeqCst)
}

fn fallback() -> FocusInfo {
    FocusInfo { hwnd: current_id(), is_password: false, unit: LengthUnit::Graphemes, caret: None, work_area: work_area() }
}

pub fn focused_info() -> Option<FocusInfo> {
    let Some(q) = QUERIES.get() else { return Some(fallback()) };
    let (tx, rx) = std::sync::mpsc::channel();
    if q.send(Query::Focus(tx)).is_err() {
        return Some(fallback());
    }
    Some(rx.recv_timeout(Duration::from_millis(400)).ok().flatten().unwrap_or_else(fallback))
}

pub fn selection() -> Option<(String, Option<Rect>)> {
    let q = QUERIES.get()?;
    let (tx, rx) = std::sync::mpsc::channel();
    q.send(Query::Selection(tx)).ok()?;
    rx.recv_timeout(Duration::from_millis(400)).ok().flatten()
}

async fn proxies<'a>(
    conn: &'a zbus::Connection,
    name: &'a str,
    path: &'a str,
) -> Option<(AccessibleProxy<'a>, Option<TextProxy<'a>>)> {
    let acc = AccessibleProxy::builder(conn)
        .destination(name)
        .ok()?
        .path(path)
        .ok()?
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()?;
    let has_text = acc.get_interfaces().await.map(|i| i.contains(Interface::Text)).unwrap_or(false);
    let text = if has_text {
        TextProxy::builder(conn)
            .destination(name)
            .ok()?
            .path(path)
            .ok()?
            .cache_properties(CacheProperties::No)
            .build()
            .await
            .ok()
    } else {
        None
    };
    Some((acc, text))
}

async fn answer_focus(conn: &zbus::Connection) -> Option<FocusInfo> {
    let (name, path) = CURRENT.lock().unwrap().clone()?;
    let (acc, text) = proxies(conn, &name, &path).await?;
    let role = acc.get_role().await.ok();
    TERMINAL.store(role == Some(Role::Terminal), Ordering::SeqCst);
    let mut caret = None;
    if let Some(t) = &text {
        if let Ok(off) = t.caret_offset().await {
            if let Ok((x, y, w, h)) = t.get_character_extents(off.max(0), CoordType::Screen).await {
                if x != 0 || y != 0 {
                    caret = Some(Rect { x, y, w: w.max(2), h: if h > 0 { h } else { 20 } });
                }
            }
        }
    }
    Some(FocusInfo {
        hwnd: object_id(&name, &path),
        is_password: role == Some(Role::PasswordText),
        unit: LengthUnit::Graphemes,
        caret,
        work_area: work_area(),
    })
}

async fn answer_selection(conn: &zbus::Connection) -> Option<(String, Option<Rect>)> {
    let (name, path) = CURRENT.lock().unwrap().clone()?;
    let (_, text) = proxies(conn, &name, &path).await?;
    let t = text?;
    if t.get_n_selections().await.ok()? < 1 {
        return None;
    }
    let (start, end) = t.get_selection(0).await.ok()?;
    let s = t.get_text(start, end).await.ok()?;
    if s.trim().is_empty() {
        return None;
    }
    let rect = t
        .get_range_extents(start, end, CoordType::Screen)
        .await
        .ok()
        .map(|(x, y, w, h)| Rect { x, y, w: w.max(1), h: h.max(1) });
    Some((s, rect))
}

async fn run(tx: UnboundedSender<InputEvent>, mut queries: tokio::sync::mpsc::UnboundedReceiver<Query>) {
    let Ok(a11y) = AccessibilityConnection::new().await else {
        // No accessibility bus: answer every question with the fallback.
        while let Some(q) = queries.recv().await {
            match q {
                Query::Focus(r) => {
                    let _ = r.send(None);
                }
                Query::Selection(r) => {
                    let _ = r.send(None);
                }
            }
        }
        return;
    };
    let _ = a11y.register_event::<FocusEvent>().await;
    let _ = a11y.register_event::<StateChangedEvent>().await;
    let conn = a11y.connection().clone();
    let mut events = std::pin::pin!(a11y.event_stream());
    loop {
        tokio::select! {
            ev = events.next() => {
                let Some(Ok(ev)) = ev else { continue };
                let item = match ev {
                    Event::Object(ObjectEvents::StateChanged(e)) if e.state == State::Focused && e.enabled => e.item,
                    Event::Focus(FocusEvents::Focus(e)) => e.item,
                    _ => continue,
                };
                let (Some(name), path) = (item.name_as_str(), item.path_as_str()) else { continue };
                let id = object_id(name, path);
                if id != current_id() {
                    *CURRENT.lock().unwrap() = Some((name.to_string(), path.to_string()));
                    CURRENT_ID.store(id, Ordering::SeqCst);
                    let _ = tx.send(InputEvent::FocusChanged);
                }
            }
            q = queries.recv() => {
                let Some(q) = q else { return };
                match q {
                    Query::Focus(r) => { let _ = r.send(answer_focus(&conn).await); }
                    Query::Selection(r) => { let _ = r.send(answer_selection(&conn).await); }
                }
            }
        }
    }
}

pub fn watch_focus(tx: UnboundedSender<InputEvent>) {
    let (qtx, qrx) = tokio::sync::mpsc::unbounded_channel();
    if QUERIES.set(qtx).is_err() {
        return;
    }
    std::thread::Builder::new()
        .name("lt-atspi".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("atspi runtime");
            rt.block_on(run(tx, qrx));
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_ids_are_stable_and_nonzero() {
        assert_eq!(object_id(":1.4", "/a/b"), object_id(":1.4", "/a/b"));
        assert_ne!(object_id(":1.4", "/a/b"), object_id(":1.4", "/a/c"));
        assert!(object_id("", "") != 0);
    }

    #[test]
    fn focus_without_atspi_falls_back() {
        let f = fallback();
        assert!(!f.is_password);
        assert_eq!(f.caret, None);
    }
}
