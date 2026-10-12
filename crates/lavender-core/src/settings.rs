//! Operator/user settings persisted by the adapter (server data dir or desktop app dir).
//! The core only sees the [`SettingsStore`] trait.

use std::sync::RwLock;

use serde::{Deserialize, Serialize};

/// Settings that can change at runtime. Versioned so stored documents can be migrated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CoreSettings {
    pub version: u32,
}

impl Default for CoreSettings {
    fn default() -> Self {
        Self { version: 1 }
    }
}

pub trait SettingsStore: Send + Sync + std::fmt::Debug {
    fn load(&self) -> CoreSettings;
    /// Persists settings.
    ///
    /// # Errors
    /// Returns a message when the settings cannot be written.
    fn save(&self, settings: &CoreSettings) -> Result<(), String>;
}

#[derive(Debug, Default)]
pub struct MemorySettingsStore {
    value: RwLock<CoreSettings>,
}

impl SettingsStore for MemorySettingsStore {
    fn load(&self) -> CoreSettings {
        self.value.read().map(|v| v.clone()).unwrap_or_default()
    }

    fn save(&self, settings: &CoreSettings) -> Result<(), String> {
        let mut guard = self.value.write().map_err(|e| e.to_string())?;
        *guard = settings.clone();
        Ok(())
    }
}

/// Settings as a JSON document on disk, written atomically (temp file + rename). Used by
/// the server (data dir) and the desktop app (app config dir).
#[derive(Debug)]
pub struct FileSettingsStore {
    path: std::path::PathBuf,
}

impl FileSettingsStore {
    #[must_use]
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl SettingsStore for FileSettingsStore {
    fn load(&self) -> CoreSettings {
        std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn save(&self, settings: &CoreSettings) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_store_round_trips_and_defaults_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSettingsStore::new(dir.path().join("nested/settings.json"));
        assert_eq!(store.load(), CoreSettings::default());
        store.save(&CoreSettings { version: 7 }).unwrap();
        assert_eq!(store.load().version, 7);
    }
}
