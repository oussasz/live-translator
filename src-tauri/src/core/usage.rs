//! Monthly character counter for Google's free tier (500K chars/month).
use crate::core::settings::{load_json_or_default, save_json_atomic};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const FREE_TIER_CHARS: u64 = 500_000;
const WARN_AT: u64 = FREE_TIER_CHARS * 80 / 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Threshold {
    Warn80,
    Reached100,
}

/// Groq free plan: requests per day.
pub const GROQ_FREE_REQUESTS_PER_DAY: u64 = 1000;
const REQ_WARN_AT: u64 = GROQ_FREE_REQUESTS_PER_DAY * 80 / 100;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UsageSnapshot {
    pub month: String,
    pub chars: u64,
    pub limit: u64,
    pub requests_today: u64,
    pub request_limit: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct UsageFile {
    month: String,
    chars: u64,
    day: String,
    requests: u64,
}

pub struct UsageMeter {
    path: PathBuf,
    data: UsageFile,
}

fn month_key(d: NaiveDate) -> String {
    d.format("%Y-%m").to_string()
}

fn day_key(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

impl UsageFile {
    fn roll(&mut self, today: NaiveDate) {
        if self.month != month_key(today) {
            self.month = month_key(today);
            self.chars = 0;
        }
        if self.day != day_key(today) {
            self.day = day_key(today);
            self.requests = 0;
        }
    }
}

impl UsageMeter {
    pub fn load(path: PathBuf, today: NaiveDate) -> Self {
        let mut data: UsageFile = load_json_or_default(&path);
        data.roll(today);
        Self { path, data }
    }

    pub fn can_spend(&self, n: u64, cap_enabled: bool) -> bool {
        !cap_enabled || self.data.chars + n <= FREE_TIER_CHARS
    }

    /// Adds `n` characters, persists, and reports a threshold crossed by this call.
    pub fn record(&mut self, n: u64, today: NaiveDate) -> Option<Threshold> {
        self.data.roll(today);
        let before = self.data.chars;
        self.data.chars += n;
        let after = self.data.chars;
        let _ = save_json_atomic(&self.path, &self.data);
        if before < FREE_TIER_CHARS && after >= FREE_TIER_CHARS {
            Some(Threshold::Reached100)
        } else if before < WARN_AT && after >= WARN_AT {
            Some(Threshold::Warn80)
        } else {
            None
        }
    }

    /// Counts one Groq request for today, persists, and reports a threshold crossed by this call.
    pub fn record_request(&mut self, today: NaiveDate) -> Option<Threshold> {
        self.data.roll(today);
        self.data.requests += 1;
        let _ = save_json_atomic(&self.path, &self.data);
        match self.data.requests {
            GROQ_FREE_REQUESTS_PER_DAY => Some(Threshold::Reached100),
            REQ_WARN_AT => Some(Threshold::Warn80),
            _ => None,
        }
    }

    pub fn snapshot(&self) -> UsageSnapshot {
        UsageSnapshot {
            month: self.data.month.clone(),
            chars: self.data.chars,
            limit: FREE_TIER_CHARS,
            requests_today: self.data.requests,
            request_limit: GROQ_FREE_REQUESTS_PER_DAY,
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn requests_counted_per_day_with_thresholds() {
        let dir = tempfile::tempdir().unwrap();
        let t = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        let mut m = UsageMeter::load(dir.path().join("usage.json"), t);
        for _ in 0..799 {
            assert_eq!(m.record_request(t), None);
        }
        assert_eq!(m.record_request(t), Some(Threshold::Warn80));
        for _ in 0..199 {
            m.record_request(t);
        }
        assert_eq!(m.record_request(t), Some(Threshold::Reached100));
        let s = m.snapshot();
        assert_eq!(s.requests_today, 1000);
        assert_eq!(s.request_limit, 1000);
    }

    #[test]
    fn request_day_rollover_and_persist() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.json");
        let d1 = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2026, 11, 13).unwrap();
        let mut m = UsageMeter::load(path.clone(), d1);
        m.record_request(d1);
        m.record_request(d1);
        assert_eq!(UsageMeter::load(path.clone(), d1).snapshot().requests_today, 2);
        assert_eq!(UsageMeter::load(path, d2).snapshot().requests_today, 0);
    }
    use super::*;
    use chrono::NaiveDate;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn record_accumulates_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.json");
        let mut m = UsageMeter::load(path.clone(), d(2026, 11, 12));
        m.record(100, d(2026, 11, 12));
        m.record(50, d(2026, 11, 12));
        let again = UsageMeter::load(path, d(2026, 11, 12));
        assert_eq!(again.snapshot().chars, 150);
        assert_eq!(again.snapshot().month, "2026-11");
        assert_eq!(again.snapshot().limit, 500_000);
    }

    #[test]
    fn month_rollover_resets() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = UsageMeter::load(dir.path().join("usage.json"), d(2026, 11, 30));
        m.record(100, d(2026, 11, 30));
        m.record(1, d(2026, 12, 1));
        assert_eq!(m.snapshot().chars, 1);
        assert_eq!(m.snapshot().month, "2026-12");
    }

    #[test]
    fn load_in_new_month_starts_at_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.json");
        UsageMeter::load(path.clone(), d(2026, 11, 30)).record(100, d(2026, 11, 30));
        let m = UsageMeter::load(path, d(2026, 12, 2));
        assert_eq!(m.snapshot().chars, 0);
        assert_eq!(m.snapshot().month, "2026-12");
    }

    #[test]
    fn thresholds_fire_once() {
        let dir = tempfile::tempdir().unwrap();
        let t = d(2026, 11, 12);
        let mut m = UsageMeter::load(dir.path().join("usage.json"), t);
        assert_eq!(m.record(399_999, t), None);
        assert_eq!(m.record(1, t), Some(Threshold::Warn80));
        assert_eq!(m.record(10, t), None);
        assert_eq!(m.record(100_000, t), Some(Threshold::Reached100));
    }

    #[test]
    fn cap_blocks_when_enabled_only() {
        let dir = tempfile::tempdir().unwrap();
        let t = d(2026, 11, 12);
        let mut m = UsageMeter::load(dir.path().join("usage.json"), t);
        m.record(499_990, t);
        assert!(!m.can_spend(20, true));
        assert!(m.can_spend(20, false));
        assert!(m.can_spend(10, true));
    }

    #[test]
    fn corrupt_usage_starts_at_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.json");
        std::fs::write(&path, "garbage").unwrap();
        let m = UsageMeter::load(path, d(2026, 11, 12));
        assert_eq!(m.snapshot().chars, 0);
        assert!(dir.path().join("usage.corrupt.json").exists());
    }
}
