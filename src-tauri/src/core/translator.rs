//! Debounced, latest-wins translation scheduler with an LRU cache and the free-tier cap.
use crate::core::google::TranslateApi;
use crate::core::types::{Lang, TranslateError, Translation};
use crate::core::usage::{Threshold, UsageMeter};
use lru::LruCache;
use std::num::NonZeroUsize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

pub const DEBOUNCE: Duration = Duration::from_millis(200);
/// Groq free plan allows 30 requests/minute; stay under it.
pub const GROQ_DEBOUNCE: Duration = Duration::from_millis(600);
pub const GROQ_RPM_LIMIT: usize = 28;
const RPM_WINDOW: Duration = Duration::from_secs(60);

/// How requests are paced and counted for the active service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pacing {
    /// 200 ms debounce, monthly character count with the free-tier cap.
    Google,
    /// 600 ms debounce, at most 28 requests/minute, daily request count.
    Groq,
}
const CACHE_SIZE: usize = 512;

#[derive(Debug)]
pub struct TranslatorOutput {
    pub seq: u64,
    pub result: Result<Translation, TranslateError>,
    pub threshold: Option<Threshold>,
}

pub struct Translator {
    api: Arc<dyn TranslateApi>,
    usage: Arc<Mutex<UsageMeter>>,
    cap_enabled: Arc<AtomicBool>,
    out: mpsc::UnboundedSender<TranslatorOutput>,
    seq: AtomicU64,
    cache: Mutex<LruCache<(String, Lang), Translation>>,
    pacing: AtomicU8,
    window: Mutex<VecDeque<tokio::time::Instant>>,
}

impl Translator {
    pub fn new(
        api: Arc<dyn TranslateApi>,
        usage: Arc<Mutex<UsageMeter>>,
        cap_enabled: Arc<AtomicBool>,
        out: mpsc::UnboundedSender<TranslatorOutput>,
    ) -> Arc<Self> {
        Arc::new(Self {
            api,
            usage,
            cap_enabled,
            out,
            seq: AtomicU64::new(0),
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(CACHE_SIZE).unwrap())),
            pacing: AtomicU8::new(0),
            window: Mutex::new(VecDeque::new()),
        })
    }

    pub fn set_pacing(&self, p: Pacing) {
        self.pacing.store(if p == Pacing::Groq { 1 } else { 0 }, Ordering::SeqCst);
        // A different service has its own translations; don't reuse cached ones.
        self.cache.lock().unwrap().clear();
    }

    fn is_groq(&self) -> bool {
        self.pacing.load(Ordering::SeqCst) == 1
    }

    /// Takes a request slot in the per-minute window, or says how long to wait for one.
    fn try_slot(&self) -> Option<Duration> {
        let now = tokio::time::Instant::now();
        let mut w = self.window.lock().unwrap();
        while w.front().is_some_and(|t| now.duration_since(*t) >= RPM_WINDOW) {
            w.pop_front();
        }
        if w.len() < GROQ_RPM_LIMIT {
            w.push_back(now);
            None
        } else {
            Some(RPM_WINDOW - now.duration_since(*w.front().unwrap()))
        }
    }

    pub fn latest_seq(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }

    /// Schedules a translation; only the newest submission ever produces an output.
    pub fn submit(self: &Arc<Self>, text: String, target: Lang, immediate: bool) -> u64 {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let me = Arc::clone(self);
        tokio::spawn(async move {
            if !immediate {
                tokio::time::sleep(if me.is_groq() { GROQ_DEBOUNCE } else { DEBOUNCE }).await;
            }
            if me.latest_seq() != seq {
                return;
            }
            let Some((result, threshold)) = me.translate_inner(&text, &target, Some(seq)).await else { return };
            if me.latest_seq() != seq {
                return;
            }
            let _ = me.out.send(TranslatorOutput { seq, result, threshold });
        });
        seq
    }

    /// Translates right away (Enter flush, read popup); supersedes pending submits.
    pub async fn translate_now(&self, text: &str, target: &str) -> (Result<Translation, TranslateError>, Option<Threshold>) {
        self.seq.fetch_add(1, Ordering::SeqCst);
        self.translate_inner(text, target, None).await.expect("no seq: never superseded")
    }

    /// `seq`: when set, gives up (None) if a newer submission arrives while waiting for a Groq slot.
    async fn translate_inner(
        &self,
        text: &str,
        target: &str,
        seq: Option<u64>,
    ) -> Option<(Result<Translation, TranslateError>, Option<Threshold>)> {
        if text.trim().is_empty() {
            return Some((Ok(Translation { text: text.to_string(), detected: None }), None));
        }
        let key = (text.to_string(), target.to_string());
        if let Some(hit) = self.cache.lock().unwrap().get(&key).cloned() {
            return Some((Ok(hit), None));
        }
        let groq = self.is_groq();
        let chars = text.chars().count() as u64;
        if groq {
            while let Some(wait) = self.try_slot() {
                if seq.is_some_and(|s| self.latest_seq() != s) {
                    return None;
                }
                tokio::time::sleep(wait.min(Duration::from_millis(500))).await;
            }
        } else if !self.usage.lock().unwrap().can_spend(chars, self.cap_enabled.load(Ordering::SeqCst)) {
            return Some((Err(TranslateError::CapReached), None));
        }
        match self.api.translate(text, target).await {
            Ok(t) => {
                let today = chrono::Local::now().date_naive();
                let threshold = {
                    let mut u = self.usage.lock().unwrap();
                    if groq { u.record_request(today) } else { u.record(chars, today) }
                };
                self.cache.lock().unwrap().put(key, t.clone());
                Some((Ok(t), threshold))
            }
            Err(e) => Some((Err(e), None)),
        }
    }
}

#[cfg(test)]
mod tests {

    #[tokio::test(start_paused = true)]
    async fn groq_debounce_is_600ms() {
        let mut r = rig(0);
        r.tr.set_pacing(Pacing::Groq);
        r.tr.submit("a".into(), "en".into(), false);
        advance(300).await;
        assert_eq!(r.api.call_count(), 0);
        advance(400).await;
        assert_eq!(r.api.call_count(), 1);
        assert!(r.rx.try_recv().is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn groq_limits_requests_per_minute() {
        let r = rig(0);
        r.tr.set_pacing(Pacing::Groq);
        let start = tokio::time::Instant::now();
        for i in 0..GROQ_RPM_LIMIT {
            r.tr.translate_now(&format!("t{i}"), "en").await.0.unwrap();
        }
        assert!(start.elapsed() < Duration::from_secs(1));
        r.tr.translate_now("one more", "en").await.0.unwrap();
        assert!(start.elapsed() >= Duration::from_secs(60), "waited only {:?}", start.elapsed());
        assert_eq!(r.api.call_count(), GROQ_RPM_LIMIT + 1);
    }

    #[tokio::test(start_paused = true)]
    async fn groq_counts_requests_not_chars() {
        let r = rig(0);
        r.tr.set_pacing(Pacing::Groq);
        r.tr.translate_now("hello", "en").await.0.unwrap();
        let s = r.usage.lock().unwrap().snapshot();
        assert_eq!(s.requests_today, 1);
        assert_eq!(s.chars, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn groq_ignores_google_char_cap() {
        let r = rig(500_000);
        r.tr.set_pacing(Pacing::Groq);
        assert!(r.tr.translate_now("x", "en").await.0.is_ok());
    }
    use super::*;
    use crate::core::test_support::{upper, FakeApi};
    use chrono::NaiveDate;
    use std::time::Duration;
    use tokio::sync::mpsc::UnboundedReceiver;

    struct Rig {
        api: Arc<FakeApi>,
        tr: Arc<Translator>,
        rx: UnboundedReceiver<TranslatorOutput>,
        usage: Arc<Mutex<UsageMeter>>,
        _dir: tempfile::TempDir,
    }

    fn rig(preload: u64) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let today: NaiveDate = chrono::Local::now().date_naive();
        let mut meter = UsageMeter::load(dir.path().join("usage.json"), today);
        if preload > 0 {
            meter.record(preload, today);
        }
        let usage = Arc::new(Mutex::new(meter));
        let api = Arc::new(FakeApi::new(upper));
        let (tx, rx) = mpsc::unbounded_channel();
        let tr = Translator::new(api.clone(), usage.clone(), Arc::new(AtomicBool::new(true)), tx);
        Rig { api, tr, rx, usage, _dir: dir }
    }

    async fn advance(ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }

    #[tokio::test(start_paused = true)]
    async fn debounces_rapid_submits() {
        let mut r = rig(0);
        r.tr.submit("a".into(), "en".into(), false);
        advance(50).await;
        r.tr.submit("ab".into(), "en".into(), false);
        advance(50).await;
        let last = r.tr.submit("abc".into(), "en".into(), false);
        advance(250).await;
        assert_eq!(*r.api.calls.lock().unwrap(), vec!["abc".to_string()]);
        let out = r.rx.try_recv().unwrap();
        assert_eq!(out.seq, last);
        assert_eq!(out.result.unwrap().text, "ABC");
        assert!(r.rx.try_recv().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn immediate_skips_delay() {
        let mut r = rig(0);
        r.tr.submit("hi".into(), "en".into(), true);
        advance(1).await;
        assert_eq!(r.rx.try_recv().unwrap().result.unwrap().text, "HI");
    }

    #[tokio::test(start_paused = true)]
    async fn stale_response_dropped() {
        let mut r = rig(0);
        r.api.push_delays(&[300, 0]);
        r.tr.submit("a".into(), "en".into(), true);
        advance(100).await;
        r.tr.submit("ab".into(), "en".into(), true);
        advance(400).await;
        assert_eq!(r.rx.try_recv().unwrap().result.unwrap().text, "AB");
        assert!(r.rx.try_recv().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn cache_hit_no_call_no_usage() {
        let r = rig(0);
        r.tr.translate_now("x", "en").await.0.unwrap();
        r.tr.translate_now("x", "en").await.0.unwrap();
        assert_eq!(r.api.call_count(), 1);
        assert_eq!(r.usage.lock().unwrap().snapshot().chars, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn cap_reached_blocks_call() {
        let r = rig(500_000);
        assert_eq!(r.tr.translate_now("x", "en").await.0, Err(TranslateError::CapReached));
        assert_eq!(r.api.call_count(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn whitespace_only_no_call() {
        let r = rig(0);
        assert_eq!(r.tr.translate_now("  ", "en").await.0.unwrap().text, "  ");
        assert_eq!(r.api.call_count(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn threshold_reported_once() {
        let r = rig(399_999);
        assert_eq!(r.tr.translate_now("ab", "en").await.1, Some(Threshold::Warn80));
        assert_eq!(r.tr.translate_now("cd", "en").await.1, None);
    }
}
