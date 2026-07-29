//! API keys, stored in the macOS Keychain.
//!
//! Keys are written once from the Settings screen and read back only when a
//! request is about to go out. They are never returned to the frontend, never
//! written to `config.json`, and never logged — the UI can ask whether a key
//! *exists* (`has`), which is all it needs to render "Configured ✓".

use crate::error::{AppError, Result};

use super::{BUNDLE_ID, LEGACY_BUNDLE_ID};

/// Keychain entries are scoped to the bundle identity, so renaming the app would
/// otherwise strand every saved key.
const SERVICE: &str = BUNDLE_ID;

pub const OPENROUTER_API_KEY: &str = "openrouter_api_key";
pub const OPENAI_API_KEY: &str = "openai_api_key";
pub const CUSTOM_LLM_API_KEY: &str = "custom_llm_api_key";
pub const STT_API_KEY: &str = "stt_api_key";

/// Every key the Settings screen knows how to manage.
pub const SECRET_KEYS: &[&str] = &[
    OPENROUTER_API_KEY,
    OPENAI_API_KEY,
    CUSTOM_LLM_API_KEY,
    STT_API_KEY,
];

#[derive(Clone, Default)]
pub struct SecretStore;

impl SecretStore {
    pub fn new() -> Self {
        Self
    }

    fn entry(&self, key: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(SERVICE, key).map_err(|e| AppError::Keychain(e.to_string()))
    }

    /// Copy any keys left under the app's previous identity.
    ///
    /// Copy rather than move: the old entries are harmless where they are, and
    /// deleting them would make downgrading lossy. Called once at startup, and a
    /// no-op after the first run since `set` writes to the new service.
    ///
    /// Errors are logged, never propagated — a locked Keychain must not stop the
    /// app launching, and the user can always re-paste a key.
    pub fn migrate_from_legacy(&self) -> usize {
        let mut moved = 0;

        for key in SECRET_KEYS {
            if self.has(key) {
                continue;
            }
            let Ok(old) = keyring::Entry::new(LEGACY_BUNDLE_ID, key) else {
                continue;
            };
            match old.get_password() {
                Ok(value) => match self.set(key, &value) {
                    Ok(()) => {
                        moved += 1;
                        tracing::info!(key, "migrated key from the previous app identity");
                    }
                    Err(e) => tracing::warn!(key, error = %e, "could not migrate key"),
                },
                Err(keyring::Error::NoEntry) => {}
                Err(e) => tracing::debug!(key, error = %e, "no legacy key to migrate"),
            }
        }
        moved
    }

    /// Store a key. An empty or whitespace-only value deletes the entry instead,
    /// so clearing a field in the UI actually removes the credential.
    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        if value.trim().is_empty() {
            return self.delete(key);
        }
        self.entry(key)?
            .set_password(value.trim())
            .map_err(|e| AppError::Keychain(e.to_string()))
    }

    /// Read a key. `Ok(None)` means "not set" — distinct from a Keychain failure,
    /// which stays an error so a locked keychain isn't mistaken for a missing key.
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        match self.entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AppError::Keychain(e.to_string())),
        }
    }

    pub fn delete(&self, key: &str) -> Result<()> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::Keychain(e.to_string())),
        }
    }

    /// Whether a key is set. This is the only secret-related fact the frontend
    /// is ever told.
    pub fn has(&self, key: &str) -> bool {
        matches!(self.get(key), Ok(Some(_)))
    }

    /// Read a required key, with an error message that names the missing key and
    /// points at the fix.
    pub fn require(&self, key: &str, provider: &str) -> Result<String> {
        self.get(key)?.ok_or_else(|| {
            AppError::Config(format!(
                "No API key set for {provider}. Add one in Settings → Models."
            ))
        })
    }
}
