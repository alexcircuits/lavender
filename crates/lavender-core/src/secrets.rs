//! Secrets enter the core only through [`SecretStore`] (ADR 0006): environment variables or
//! secret files on the server, the OS keychain on desktop. Values are `SecretString`, so
//! they never appear in `Debug` output or logs.

use std::collections::HashMap;
use std::sync::RwLock;

use secrecy::SecretString;

/// Every secret Lavender knows about. There are no free-form secret names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretKey {
    FirmsMapKey,
    AisStreamApiKey,
    OpenSkyClientId,
    OpenSkyClientSecret,
    Ll2ApiToken,
    MapillaryClientToken,
    TomTomApiKey,
}

impl SecretKey {
    pub const ALL: [Self; 7] = [
        Self::FirmsMapKey,
        Self::AisStreamApiKey,
        Self::OpenSkyClientId,
        Self::OpenSkyClientSecret,
        Self::Ll2ApiToken,
        Self::MapillaryClientToken,
        Self::TomTomApiKey,
    ];

    /// The environment variable a server operator sets.
    #[must_use]
    pub const fn env_var(self) -> &'static str {
        match self {
            Self::FirmsMapKey => "LAVENDER_FIRMS_MAP_KEY",
            Self::AisStreamApiKey => "LAVENDER_AISSTREAM_API_KEY",
            Self::OpenSkyClientId => "LAVENDER_OPENSKY_CLIENT_ID",
            Self::OpenSkyClientSecret => "LAVENDER_OPENSKY_CLIENT_SECRET",
            Self::Ll2ApiToken => "LAVENDER_LL2_API_TOKEN",
            Self::MapillaryClientToken => "LAVENDER_MAPILLARY_CLIENT_TOKEN",
            Self::TomTomApiKey => "LAVENDER_TOMTOM_API_KEY",
        }
    }

    /// A short name for configuration messages ("add a FIRMS map key").
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::FirmsMapKey => "NASA FIRMS map key",
            Self::AisStreamApiKey => "AISStream API key",
            Self::OpenSkyClientId => "OpenSky client id",
            Self::OpenSkyClientSecret => "OpenSky client secret",
            Self::Ll2ApiToken => "Launch Library 2 API token",
            Self::MapillaryClientToken => "Mapillary client token",
            Self::TomTomApiKey => "TomTom API key",
        }
    }

    /// Stable identifier used by frontend and desktop adapters.
    #[must_use]
    pub const fn as_id(self) -> &'static str {
        match self {
            Self::FirmsMapKey => "firms",
            Self::AisStreamApiKey => "aisstream",
            Self::OpenSkyClientId => "opensky_id",
            Self::OpenSkyClientSecret => "opensky_secret",
            Self::Ll2ApiToken => "ll2",
            Self::MapillaryClientToken => "mapillary",
            Self::TomTomApiKey => "tomtom",
        }
    }

    /// Resolves an identifier or environment variable name to a `SecretKey`.
    #[must_use]
    pub fn from_identifier(id: &str) -> Option<Self> {
        let trimmed = id.trim();
        Self::ALL.into_iter().find(|&key| {
            trimmed.eq_ignore_ascii_case(key.as_id()) || trimmed.eq_ignore_ascii_case(key.env_var())
        })
    }
}

/// Errors occurring during secret mutation.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("Secret store is read-only in this runtime")]
    ReadOnly,
    #[error("Secret store is temporarily unavailable")]
    Unavailable,
}

/// Read access to secrets, plus optional writes for adapters that support them (desktop).
pub trait SecretStore: Send + Sync + std::fmt::Debug {
    fn get(&self, key: SecretKey) -> Option<SecretString>;

    fn is_set(&self, key: SecretKey) -> bool {
        self.get(key).is_some()
    }

    /// # Errors
    /// Returns `SecretError::ReadOnly` if this store is not writable.
    fn set(&self, _key: SecretKey, _value: String) -> Result<(), SecretError> {
        Err(SecretError::ReadOnly)
    }

    /// # Errors
    /// Returns `SecretError::ReadOnly` if this store is not writable.
    fn clear(&self, _key: SecretKey) -> Result<(), SecretError> {
        Err(SecretError::ReadOnly)
    }

    fn status(&self) -> HashMap<SecretKey, bool> {
        let mut map = HashMap::new();
        for key in SecretKey::ALL {
            map.insert(key, self.is_set(key));
        }
        map
    }
}

/// Reads `LAVENDER_*` environment variables, or `LAVENDER_*_FILE` paths (container secrets),
/// once at construction.
#[derive(Debug, Default)]
pub struct EnvSecretStore {
    values: HashMap<SecretKey, SecretString>,
}

impl EnvSecretStore {
    #[must_use]
    pub fn from_env() -> Self {
        let mut values = HashMap::new();
        for key in SecretKey::ALL {
            let legacy = match key {
                SecretKey::Ll2ApiToken => std::env::var("LL2_API_TOKEN").ok(),
                SecretKey::AisStreamApiKey => std::env::var("AISSTREAM_API_KEY").ok(),
                SecretKey::MapillaryClientToken => std::env::var("MAPILLARY_CLIENT_TOKEN").ok(),
                SecretKey::TomTomApiKey => std::env::var("TOMTOM_API_KEY").ok(),
                _ => None,
            };
            let direct = std::env::var(key.env_var()).ok().or(legacy);
            let from_file = std::env::var(format!("{}_FILE", key.env_var()))
                .ok()
                .and_then(|path| std::fs::read_to_string(path).ok());
            if let Some(v) =
                direct.or(from_file).map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
            {
                values.insert(key, SecretString::from(v));
            }
        }
        Self { values }
    }
}

impl SecretStore for EnvSecretStore {
    fn get(&self, key: SecretKey) -> Option<SecretString> {
        self.values.get(&key).cloned()
    }
}

/// In-memory secrets for tests and session-only desktop keys.
#[derive(Debug, Default)]
pub struct MemorySecretStore {
    values: RwLock<HashMap<SecretKey, SecretString>>,
}

impl MemorySecretStore {
    #[must_use]
    pub fn with(entries: impl IntoIterator<Item = (SecretKey, &'static str)>) -> Self {
        let store = Self::default();
        for (k, v) in entries {
            let _ = store.set(k, v.to_owned());
        }
        store
    }

    /// # Errors
    /// Returns `SecretError::Unavailable` if the write lock cannot be acquired.
    pub fn set(&self, key: SecretKey, value: String) -> Result<(), SecretError> {
        if let Ok(mut map) = self.values.write() {
            map.insert(key, SecretString::from(value));
            Ok(())
        } else {
            Err(SecretError::Unavailable)
        }
    }

    /// # Errors
    /// Returns `SecretError::Unavailable` if the write lock cannot be acquired.
    pub fn clear(&self, key: SecretKey) -> Result<(), SecretError> {
        if let Ok(mut map) = self.values.write() {
            map.remove(&key);
            Ok(())
        } else {
            Err(SecretError::Unavailable)
        }
    }
}

impl SecretStore for MemorySecretStore {
    fn get(&self, key: SecretKey) -> Option<SecretString> {
        self.values.read().ok().and_then(|m| m.get(&key).cloned())
    }

    fn set(&self, key: SecretKey, value: String) -> Result<(), SecretError> {
        self.set(key, value)
    }

    fn clear(&self, key: SecretKey) -> Result<(), SecretError> {
        self.clear(key)
    }
}

/// Secrets for fixture mode, where recorded documents replace every provider.
///
/// No provider contacts its upstream there, so no key is ever read or sent. Every key counts
/// as configured, so key-gated layers (fires, vessels) replay their fixtures like the others,
/// but no key has a value: a live provider that reached this store would still find none.
#[derive(Debug, Default)]
pub struct FixtureSecretStore;

impl SecretStore for FixtureSecretStore {
    fn get(&self, _key: SecretKey) -> Option<SecretString> {
        None
    }

    fn is_set(&self, _key: SecretKey) -> bool {
        true
    }
}

/// Persistent secrets backed by the operating system keychain (macOS Keychain,
/// Windows Credential Manager, Linux Secret Service). Falls back to in-memory
/// session storage if the system service is unavailable.
#[cfg(feature = "keyring")]
#[derive(Debug)]
pub struct KeyringSecretStore {
    service: String,
    fallback: MemorySecretStore,
}

#[cfg(feature = "keyring")]
impl KeyringSecretStore {
    #[must_use]
    pub fn new(service: impl Into<String>) -> Self {
        Self { service: service.into(), fallback: MemorySecretStore::default() }
    }
}

#[cfg(feature = "keyring")]
impl SecretStore for KeyringSecretStore {
    fn get(&self, key: SecretKey) -> Option<SecretString> {
        let entry = keyring::Entry::new(&self.service, key.as_id()).ok()?;
        entry.get_password().ok().map(SecretString::from).or_else(|| self.fallback.get(key))
    }

    fn set(&self, key: SecretKey, value: String) -> Result<(), SecretError> {
        if let Ok(entry) = keyring::Entry::new(&self.service, key.as_id())
            && entry.set_password(&value).is_ok()
        {
            let _ = self.fallback.set(key, value);
            return Ok(());
        }
        self.fallback.set(key, value)
    }

    fn clear(&self, key: SecretKey) -> Result<(), SecretError> {
        let _ = self.fallback.clear(key);
        if let Ok(entry) = keyring::Entry::new(&self.service, key.as_id()) {
            let _ = entry.delete_credential();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;

    #[test]
    fn fixture_store_configures_every_key_without_holding_a_value() {
        let store = FixtureSecretStore;
        for key in SecretKey::ALL {
            assert!(store.is_set(key), "{key:?}");
            assert!(store.get(key).is_none(), "{key:?}");
        }
    }

    #[test]
    fn debug_output_never_contains_the_value() {
        let store = MemorySecretStore::with([(SecretKey::FirmsMapKey, "super-secret-value")]);
        assert_eq!(
            store.get(SecretKey::FirmsMapKey).unwrap().expose_secret(),
            "super-secret-value"
        );
        let debug = format!("{store:?} {:?}", store.get(SecretKey::FirmsMapKey));
        assert!(!debug.contains("super-secret-value"), "{debug}");
        assert!(!store.is_set(SecretKey::AisStreamApiKey));
    }

    #[test]
    fn memory_secret_store_mutation_and_status() {
        let store = MemorySecretStore::default();
        assert!(!store.is_set(SecretKey::TomTomApiKey));
        assert!(store.set(SecretKey::TomTomApiKey, "tomtom-123".into()).is_ok());
        assert!(store.is_set(SecretKey::TomTomApiKey));

        let status = store.status();
        assert_eq!(status.get(&SecretKey::TomTomApiKey), Some(&true));
        assert_eq!(status.get(&SecretKey::FirmsMapKey), Some(&false));

        assert!(store.clear(SecretKey::TomTomApiKey).is_ok());
        assert!(!store.is_set(SecretKey::TomTomApiKey));
    }

    #[test]
    fn secret_key_identifier_resolution() {
        assert_eq!(SecretKey::from_identifier("firms"), Some(SecretKey::FirmsMapKey));
        assert_eq!(
            SecretKey::from_identifier("LAVENDER_AISSTREAM_API_KEY"),
            Some(SecretKey::AisStreamApiKey)
        );
        assert_eq!(SecretKey::from_identifier("tomtom"), Some(SecretKey::TomTomApiKey));
        assert_eq!(SecretKey::from_identifier("unknown_key"), None);
    }
}
