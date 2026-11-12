//! Google Cloud Translation v2 (Basic) client, authenticated with an API key.
pub use crate::core::types::{TranslateError, Translation};
use async_trait::async_trait;
use serde::Deserialize;
use std::time::Duration;

pub const GOOGLE_BASE: &str = "https://translation.googleapis.com";

#[async_trait]
pub trait TranslateApi: Send + Sync {
    async fn translate(&self, text: &str, target: &str) -> Result<Translation, TranslateError>;
}

/// The API in use, replaceable at runtime when the owner changes the key.
/// With no key yet, every call fails with `InvalidKey`.
pub struct SwappableApi {
    inner: std::sync::RwLock<Option<std::sync::Arc<dyn TranslateApi>>>,
}

impl SwappableApi {
    pub fn new(api: Option<std::sync::Arc<dyn TranslateApi>>) -> Self {
        Self { inner: std::sync::RwLock::new(api) }
    }
    pub fn set(&self, api: std::sync::Arc<dyn TranslateApi>) {
        *self.inner.write().unwrap() = Some(api);
    }
    /// Replace or clear (no key for the chosen service yet).
    pub fn set_opt(&self, api: Option<std::sync::Arc<dyn TranslateApi>>) {
        *self.inner.write().unwrap() = api;
    }
}

#[async_trait]
impl TranslateApi for SwappableApi {
    async fn translate(&self, text: &str, target: &str) -> Result<Translation, TranslateError> {
        let api = self.inner.read().unwrap().clone();
        match api {
            Some(a) => a.translate(text, target).await,
            None => Err(TranslateError::InvalidKey),
        }
    }
}

pub struct GoogleV2 {
    client: reqwest::Client,
    base: String,
    key: String,
}

#[derive(Deserialize)]
struct Resp {
    data: RespData,
}
#[derive(Deserialize)]
struct RespData {
    translations: Vec<RespItem>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RespItem {
    translated_text: String,
    detected_source_language: Option<String>,
}

impl GoogleV2 {
    pub fn new(key: String) -> Self {
        Self::with_base(key, GOOGLE_BASE.into())
    }

    pub fn with_base(key: String, base: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .expect("reqwest client");
        Self { client, base, key }
    }
}

/// Google sometimes returns HTML entities even with format=text; decode the common ones.
fn unescape(s: &str) -> String {
    s.replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn map_error(status: u16, body: &str) -> TranslateError {
    if body.contains("API_KEY_INVALID") || body.contains("API key not valid") || status == 401 {
        return TranslateError::InvalidKey;
    }
    let quota = ["dailyLimitExceeded", "userRateLimitExceeded", "rateLimitExceeded", "quotaExceeded", "RESOURCE_EXHAUSTED"];
    if status == 429 || (status == 403 && quota.iter().any(|q| body.contains(q))) {
        return TranslateError::QuotaExceeded;
    }
    let msg = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("HTTP {status}"));
    TranslateError::Other(msg)
}

#[async_trait]
impl TranslateApi for GoogleV2 {
    async fn translate(&self, text: &str, target: &str) -> Result<Translation, TranslateError> {
        let url = format!("{}/language/translate/v2", self.base);
        let form = [("q", text), ("target", target), ("format", "text"), ("key", self.key.as_str())];
        let resp = self
            .client
            .post(url)
            .form(&form)
            .send()
            .await
            .map_err(|_| TranslateError::Offline)?;
        let status = resp.status().as_u16();
        let body = resp.text().await.map_err(|_| TranslateError::Offline)?;
        if !(200..300).contains(&status) {
            return Err(map_error(status, &body));
        }
        let parsed: Resp = serde_json::from_str(&body).map_err(|e| TranslateError::Other(e.to_string()))?;
        let item = parsed
            .data
            .translations
            .into_iter()
            .next()
            .ok_or_else(|| TranslateError::Other("empty response".into()))?;
        Ok(Translation { text: unescape(&item.translated_text), detected: item.detected_source_language })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ok_body(text: &str) -> serde_json::Value {
        serde_json::json!({"data":{"translations":[{"translatedText":text,"detectedSourceLanguage":"ar"}]}})
    }

    #[tokio::test]
    async fn sends_expected_form_and_parses() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/language/translate/v2"))
            .and(body_string_contains("format=text"))
            .and(body_string_contains("target=en"))
            .and(body_string_contains("key=k"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_body("Hello")))
            .expect(1)
            .mount(&server)
            .await;
        let api = GoogleV2::with_base("k".into(), server.uri());
        let t = api.translate("مرحبا", "en").await.unwrap();
        assert_eq!(t, Translation { text: "Hello".into(), detected: Some("ar".into()) });
    }

    #[tokio::test]
    async fn format_text_and_entities_decoded() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_body("l&#39;heure &lt;3 &amp;lt;")))
            .mount(&server)
            .await;
        let api = GoogleV2::with_base("k".into(), server.uri());
        assert_eq!(api.translate("x", "fr").await.unwrap().text, "l'heure <3 &lt;");
    }

    #[tokio::test]
    async fn invalid_key_maps() {
        let server = MockServer::start().await;
        let body = serde_json::json!({"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","status":"INVALID_ARGUMENT","details":[{"reason":"API_KEY_INVALID"}]}});
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_json(body))
            .mount(&server)
            .await;
        let api = GoogleV2::with_base("bad".into(), server.uri());
        assert_eq!(api.translate("x", "en").await, Err(TranslateError::InvalidKey));
    }

    #[tokio::test]
    async fn quota_maps() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(429)).mount(&server).await;
        let api = GoogleV2::with_base("k".into(), server.uri());
        assert_eq!(api.translate("x", "en").await, Err(TranslateError::QuotaExceeded));
    }

    #[tokio::test]
    async fn other_error_keeps_message() {
        let server = MockServer::start().await;
        let body = serde_json::json!({"error":{"code":400,"message":"Invalid Value","status":"INVALID_ARGUMENT"}});
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(400).set_body_json(body)).mount(&server).await;
        let api = GoogleV2::with_base("k".into(), server.uri());
        assert_eq!(api.translate("x", "zz").await, Err(TranslateError::Other("Invalid Value".into())));
    }

    #[tokio::test]
    async fn swappable_without_key_is_invalid_key() {
        let api = SwappableApi::new(None);
        assert_eq!(api.translate("x", "en").await, Err(TranslateError::InvalidKey));
    }

    #[tokio::test]
    async fn swappable_cleared_is_invalid_key() {
        let api = SwappableApi::new(Some(Arc::new(GoogleV2::new("k".into()))));
        api.set_opt(None);
        assert_eq!(api.translate("x", "en").await, Err(TranslateError::InvalidKey));
    }

    #[tokio::test]
    async fn swappable_uses_new_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_string_contains("key=new"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_body("ok")))
            .mount(&server)
            .await;
        let api = SwappableApi::new(None);
        api.set(Arc::new(GoogleV2::with_base("new".into(), server.uri())));
        assert_eq!(api.translate("x", "en").await.unwrap().text, "ok");
    }

    #[tokio::test]
    async fn offline_maps() {
        let api = GoogleV2::with_base("k".into(), "http://127.0.0.1:1".into());
        assert_eq!(api.translate("x", "en").await, Err(TranslateError::Offline));
    }

    #[tokio::test]
    async fn timeout_maps_offline() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_body("late")).set_delay(Duration::from_secs(4)))
            .mount(&server)
            .await;
        let api = GoogleV2::with_base("k".into(), server.uri());
        let start = std::time::Instant::now();
        assert_eq!(api.translate("x", "en").await, Err(TranslateError::Offline));
        assert!(start.elapsed() < Duration::from_millis(3800));
    }
}
