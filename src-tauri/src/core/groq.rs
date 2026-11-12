//! Groq (OpenAI-compatible chat completions) used as a translation engine.
use crate::core::google::TranslateApi;
pub use crate::core::types::{TranslateError, Translation};
use async_trait::async_trait;
use std::time::Duration;

pub const GROQ_BASE: &str = "https://api.groq.com";

pub struct GroqClient {
    client: reqwest::Client,
    base: String,
    key: String,
    model: String,
}

pub fn lang_name(code: &str) -> String {
    match code {
        "en" => "English".into(),
        "fr" => "French".into(),
        "ar" => "Arabic".into(),
        other => other.to_string(),
    }
}

fn system_prompt(target: &str) -> String {
    let name = lang_name(target);
    format!(
        "You are a translation engine. Translate the user's message into {name}. \
Keep the meaning, tone, names, emoji, numbers and punctuation. \
Do not add explanations, notes, quotes or greetings. \
If the message is already in {name}, return it unchanged. \
Reply with only a JSON object: {{\"detected\": \"<ISO 639-1 code of the message's language>\", \"translation\": \"<the translation>\"}}"
    )
}

/// Reads the model's reply. Tolerates code fences or chatter around the JSON object;
/// anything without a "translation" string is an error (never typed into the field).
pub fn parse_reply(content: &str) -> Result<Translation, TranslateError> {
    let bad = || TranslateError::Other("Unexpected reply from the model".into());
    let start = content.find('{').ok_or_else(bad)?;
    let end = content.rfind('}').ok_or_else(bad)?;
    if end < start {
        return Err(bad());
    }
    let v: serde_json::Value = serde_json::from_str(&content[start..=end]).map_err(|_| bad())?;
    let text = v.get("translation").and_then(|t| t.as_str()).ok_or_else(bad)?;
    let detected = v
        .get("detected")
        .and_then(|d| d.as_str())
        .map(|d| d.trim().to_lowercase())
        .filter(|d| !d.is_empty());
    Ok(Translation { text: text.trim().to_string(), detected })
}

impl GroqClient {
    pub fn new(key: String, model: String) -> Self {
        Self::with_base(key, model, GROQ_BASE.into())
    }

    pub fn with_base(key: String, model: String, base: String) -> Self {
        let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().expect("reqwest client");
        Self { client, base, key, model }
    }
}

#[async_trait]
impl TranslateApi for GroqClient {
    async fn translate(&self, text: &str, target: &str) -> Result<Translation, TranslateError> {
        let mut body = serde_json::json!({
            "model": self.model,
            "temperature": 0,
            "max_completion_tokens": 2048,
            "response_format": {"type": "json_object"},
            "messages": [
                {"role": "system", "content": system_prompt(target)},
                {"role": "user", "content": text}
            ]
        });
        if self.model.starts_with("openai/gpt-oss") {
            body["reasoning_effort"] = serde_json::json!("low");
        }
        let resp = self
            .client
            .post(format!("{}/openai/v1/chat/completions", self.base))
            .bearer_auth(&self.key)
            .json(&body)
            .send()
            .await
            .map_err(|_| TranslateError::Offline)?;
        let status = resp.status().as_u16();
        let raw = resp.text().await.map_err(|_| TranslateError::Offline)?;
        match status {
            200..=299 => {}
            401 | 403 => return Err(TranslateError::InvalidKey),
            429 => return Err(TranslateError::RateLimited),
            500..=599 => return Err(TranslateError::Offline),
            _ => {
                let msg = serde_json::from_str::<serde_json::Value>(&raw)
                    .ok()
                    .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
                    .unwrap_or_else(|| format!("HTTP {status}"));
                return Err(TranslateError::Other(msg));
            }
        }
        let v: serde_json::Value =
            serde_json::from_str(&raw).map_err(|_| TranslateError::Other("Unexpected reply from Groq".into()))?;
        let content = v["choices"][0]["message"]["content"].as_str().unwrap_or("");
        parse_reply(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_partial_json, body_string_contains, header, method, path};
    use wiremock::{Match, Mock, MockServer, Request, ResponseTemplate};

    fn reply(content: &str) -> serde_json::Value {
        serde_json::json!({"choices":[{"message":{"role":"assistant","content":content}}]})
    }

    struct NoReasoningField;
    impl Match for NoReasoningField {
        fn matches(&self, req: &Request) -> bool {
            let v: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or_default();
            v.get("reasoning_effort").is_none()
        }
    }

    #[tokio::test]
    async fn sends_chat_request_with_json_mode() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/openai/v1/chat/completions"))
            .and(header("authorization", "Bearer k"))
            .and(body_partial_json(serde_json::json!({
                "model": "openai/gpt-oss-20b",
                "temperature": 0,
                "response_format": {"type": "json_object"},
                "reasoning_effort": "low"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(reply(r#"{"detected":"ar","translation":"Hello"}"#)))
            .expect(1)
            .mount(&server)
            .await;
        let api = GroqClient::with_base("k".into(), "openai/gpt-oss-20b".into(), server.uri());
        let t = api.translate("\u{645}\u{631}\u{62d}\u{628}\u{627}", "en").await.unwrap();
        assert_eq!(t, Translation { text: "Hello".into(), detected: Some("ar".into()) });
    }

    #[tokio::test]
    async fn llama_has_no_reasoning_effort() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(NoReasoningField)
            .and(body_partial_json(serde_json::json!({"model": "llama-3.3-70b-versatile"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(reply(r#"{"detected":"fr","translation":"Hi"}"#)))
            .expect(1)
            .mount(&server)
            .await;
        let api = GroqClient::with_base("k".into(), "llama-3.3-70b-versatile".into(), server.uri());
        assert_eq!(api.translate("salut", "en").await.unwrap().text, "Hi");
    }

    #[tokio::test]
    async fn system_prompt_names_target_language() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_string_contains("into French"))
            .respond_with(ResponseTemplate::new(200).set_body_json(reply(r#"{"detected":"en","translation":"Bonjour"}"#)))
            .expect(1)
            .mount(&server)
            .await;
        let api = GroqClient::with_base("k".into(), "openai/gpt-oss-20b".into(), server.uri());
        assert_eq!(api.translate("hello", "fr").await.unwrap().text, "Bonjour");
    }

    #[tokio::test]
    async fn invalid_key_maps() {
        let server = MockServer::start().await;
        let body = serde_json::json!({"error":{"message":"Invalid API Key","type":"invalid_request_error","code":"invalid_api_key"}});
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(401).set_body_json(body)).mount(&server).await;
        let api = GroqClient::with_base("bad".into(), "openai/gpt-oss-20b".into(), server.uri());
        assert_eq!(api.translate("x", "en").await, Err(TranslateError::InvalidKey));
    }

    #[tokio::test]
    async fn rate_limit_maps() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "2"))
            .mount(&server)
            .await;
        let api = GroqClient::with_base("k".into(), "openai/gpt-oss-20b".into(), server.uri());
        assert_eq!(api.translate("x", "en").await, Err(TranslateError::RateLimited));
    }

    #[tokio::test]
    async fn server_error_maps_offline() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(503)).mount(&server).await;
        let api = GroqClient::with_base("k".into(), "openai/gpt-oss-20b".into(), server.uri());
        assert_eq!(api.translate("x", "en").await, Err(TranslateError::Offline));
    }

    #[tokio::test]
    async fn model_error_keeps_message() {
        let server = MockServer::start().await;
        let body = serde_json::json!({"error":{"message":"The model `nope` does not exist","type":"invalid_request_error"}});
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(404).set_body_json(body)).mount(&server).await;
        let api = GroqClient::with_base("k".into(), "nope".into(), server.uri());
        assert_eq!(api.translate("x", "en").await, Err(TranslateError::Other("The model `nope` does not exist".into())));
    }

    #[tokio::test]
    async fn offline_maps() {
        let api = GroqClient::with_base("k".into(), "openai/gpt-oss-20b".into(), "http://127.0.0.1:1".into());
        assert_eq!(api.translate("x", "en").await, Err(TranslateError::Offline));
    }

    #[test]
    fn parse_reply_plain_json() {
        let t = parse_reply(r#"{"detected":"FR","translation":"The invoice"}"#).unwrap();
        assert_eq!(t, Translation { text: "The invoice".into(), detected: Some("fr".into()) });
    }

    #[test]
    fn parse_reply_tolerates_fences_and_chatter() {
        let t = parse_reply("Sure!\n```json\n{\"detected\":\"ar\",\"translation\":\"l'heure <3\"}\n```").unwrap();
        assert_eq!(t.text, "l'heure <3");
    }

    #[test]
    fn parse_reply_missing_detected_is_none() {
        assert_eq!(parse_reply(r#"{"translation":"Hi"}"#).unwrap().detected, None);
    }

    #[test]
    fn parse_reply_rejects_junk() {
        assert!(matches!(parse_reply("Hello there"), Err(TranslateError::Other(_))));
        assert!(matches!(parse_reply(r#"{"detected":"en"}"#), Err(TranslateError::Other(_))));
    }

    #[test]
    fn lang_names() {
        assert_eq!(lang_name("ar"), "Arabic");
        assert_eq!(lang_name("fr"), "French");
        assert_eq!(lang_name("en"), "English");
    }
}
