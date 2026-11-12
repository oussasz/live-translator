//! Shared plain data types used across the core, controller and UI layers.
use serde::{Deserialize, Serialize};

/// A Google language code such as "en", "fr", "ar".
pub type Lang = String;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Translation {
    pub text: String,
    pub detected: Option<Lang>,
}

#[derive(Clone, Debug, PartialEq, thiserror::Error, Serialize)]
pub enum TranslateError {
    #[error("offline")]
    Offline,
    #[error("invalid API key")]
    InvalidKey,
    #[error("Google quota exceeded")]
    QuotaExceeded,
    #[error("free monthly limit reached")]
    CapReached,
    #[error("{0}")]
    Other(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Size {
    pub w: i32,
    pub h: i32,
}

/// How the target text control counts "one Shift+Left".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LengthUnit {
    Graphemes,
    Utf16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_error_serializes_as_tag() {
        assert_eq!(serde_json::to_string(&TranslateError::InvalidKey).unwrap(), "\"InvalidKey\"");
    }
}
