//! Deterministic fake translation API for unit and integration tests.
use crate::core::google::TranslateApi;
use crate::core::types::{TranslateError, Translation};
use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

pub type Transform = fn(&str) -> String;

pub fn upper(s: &str) -> String {
    s.to_uppercase()
}

/// Reverses word order and uppercases: "le chat noir" -> "NOIR CHAT LE".
pub fn reverse_words_upper(s: &str) -> String {
    s.split(' ').rev().collect::<Vec<_>>().join(" ").to_uppercase()
}

pub struct FakeApi {
    pub calls: Mutex<Vec<String>>,
    delays_ms: Mutex<VecDeque<u64>>,
    errors: Mutex<VecDeque<Option<TranslateError>>>,
    always_error: Mutex<Option<TranslateError>>,
    transform: Transform,
}

impl FakeApi {
    pub fn new(transform: Transform) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            delays_ms: Mutex::new(VecDeque::new()),
            errors: Mutex::new(VecDeque::new()),
            always_error: Mutex::new(None),
            transform,
        }
    }
    /// Delay for the next calls, in order; later calls use 0.
    pub fn push_delays(&self, ms: &[u64]) {
        self.delays_ms.lock().unwrap().extend(ms.iter().copied());
    }
    pub fn fail_always(&self, e: Option<TranslateError>) {
        *self.always_error.lock().unwrap() = e;
    }
    pub fn push_errors(&self, errs: &[Option<TranslateError>]) {
        self.errors.lock().unwrap().extend(errs.iter().cloned());
    }
    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

#[async_trait]
impl TranslateApi for FakeApi {
    async fn translate(&self, text: &str, _target: &str) -> Result<Translation, TranslateError> {
        self.calls.lock().unwrap().push(text.to_string());
        let delay = self.delays_ms.lock().unwrap().pop_front().unwrap_or(0);
        let err = self.errors.lock().unwrap().pop_front().flatten().or_else(|| self.always_error.lock().unwrap().clone());
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        match err {
            Some(e) => Err(e),
            None => Ok(Translation { text: (self.transform)(text), detected: Some("fr".into()) }),
        }
    }
}
