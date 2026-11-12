//! API keys (one per translation service) in Windows Credential Manager.
use crate::core::settings::Provider;

const SERVICE: &str = "LiveTranslator";

fn user(p: Provider) -> &'static str {
    match p {
        Provider::Google => "google-api-key",
        Provider::Groq => "groq-api-key",
    }
}

fn entry(p: Provider) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, user(p)).map_err(|e| e.to_string())
}

pub fn get_api_key(p: Provider) -> Option<String> {
    entry(p).ok()?.get_password().ok().filter(|k| !k.is_empty())
}

pub fn set_api_key(p: Provider, key: &str) -> Result<(), String> {
    entry(p)?.set_password(key).map_err(|e| e.to_string())
}

pub fn delete_api_key(p: Provider) -> Result<(), String> {
    match entry(p)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "touches the real Windows Credential Manager"]
    fn set_get_delete_roundtrip() {
        let had = get_api_key(Provider::Groq);
        set_api_key(Provider::Groq, "test-key-123").unwrap();
        assert_eq!(get_api_key(Provider::Groq).as_deref(), Some("test-key-123"));
        delete_api_key(Provider::Groq).unwrap();
        assert_eq!(get_api_key(Provider::Groq), None);
        if let Some(k) = had {
            set_api_key(Provider::Groq, &k).unwrap();
        }
    }
}
