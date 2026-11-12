//! Debounced, latest-wins translation scheduler with an LRU cache and the free-tier cap.
use crate::core::google::TranslateApi;
use crate::core::types::{Lang, TranslateError, Translation};
use crate::core::usage::{Threshold, UsageMeter};
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

pub const DEBOUNCE: Duration = Duration::from_millis(200);
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
        })
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
                tokio::time::sleep(DEBOUNCE).await;
            }
            if me.latest_seq() != seq {
                return;
            }
            let (result, threshold) = me.translate_inner(&text, &target).await;
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
        self.translate_inner(text, target).await
    }

    async fn translate_inner(&self, text: &str, target: &str) -> (Result<Translation, TranslateError>, Option<Threshold>) {
        if text.trim().is_empty() {
            return (Ok(Translation { text: text.to_string(), detected: None }), None);
        }
        let key = (text.to_string(), target.to_string());
        if let Some(hit) = self.cache.lock().unwrap().get(&key).cloned() {
            return (Ok(hit), None);
        }
        let chars = text.chars().count() as u64;
        if !self.usage.lock().unwrap().can_spend(chars, self.cap_enabled.load(Ordering::SeqCst)) {
            return (Err(TranslateError::CapReached), None);
        }
        match self.api.translate(text, target).await {
            Ok(t) => {
                let threshold = self.usage.lock().unwrap().record(chars, chrono::Local::now().date_naive());
                self.cache.lock().unwrap().put(key, t.clone());
                (Ok(t), threshold)
            }
            Err(e) => (Err(e), None),
        }
    }
}

#[cfg(test)]
mod tests {
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
