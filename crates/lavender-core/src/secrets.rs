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
}

impl SecretKey {
    pub const ALL: [Self; 4] = [
        Self::FirmsMapKey,
        Self::AisStreamApiKey,
        Self::OpenSkyClientId,
        Self::OpenSkyClientSecret,
    ];

    /// The environment variable a server operator sets.
    #[must_use]
    pub const fn env_var(self) -> &'static str {
        match self {
            Self::FirmsMapKey => "LAVENDER_FIRMS_MAP_KEY",
            Self::AisStreamApiKey => "LAVENDER_AISSTREAM_API_KEY",
            Self::OpenSkyClientId => "LAVENDER_OPENSKY_CLIENT_ID",
            Self::OpenSkyClientSecret => "LAVENDER_OPENSKY_CLIENT_SECRET",
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
        }
    }
}

/// Read access to secrets, plus optional writes for adapters that support them (desktop).
pub trait SecretStore: Send + Sync + std::fmt::Debug {
    fn get(&self, key: SecretKey) -> Option<SecretString>;

    fn is_set(&self, key: SecretKey) -> bool {
        self.get(key).is_some()
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
            let direct = std::env::var(key.env_var()).ok();
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
            store.set(k, v.to_owned());
        }
        store
    }

    pub fn set(&self, key: SecretKey, value: String) {
        if let Ok(mut map) = self.values.write() {
            map.insert(key, SecretString::from(value));
        }
    }
}

impl SecretStore for MemorySecretStore {
    fn get(&self, key: SecretKey) -> Option<SecretString> {
        self.values.read().ok().and_then(|m| m.get(&key).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;

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
}
