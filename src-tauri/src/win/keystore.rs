//! Google API key storage in Windows Credential Manager.
const SERVICE: &str = "LiveTranslator";
const USER: &str = "google-api-key";

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, USER).map_err(|e| e.to_string())
}

pub fn get_api_key() -> Option<String> {
    entry().ok()?.get_password().ok().filter(|k| !k.is_empty())
}

pub fn set_api_key(key: &str) -> Result<(), String> {
    entry()?.set_password(key).map_err(|e| e.to_string())
}

pub fn delete_api_key() -> Result<(), String> {
    match entry()?.delete_credential() {
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
        set_api_key("test-key-123").unwrap();
        assert_eq!(get_api_key().as_deref(), Some("test-key-123"));
        delete_api_key().unwrap();
        assert_eq!(get_api_key(), None);
    }
}
