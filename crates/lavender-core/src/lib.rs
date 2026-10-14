//! Lavender's application core (ADR 0002).
//!
//! Everything a Lavender backend does lives here, behind [`LavenderCore`]: provider
//! adapters, the egress policy, the feed hub that shares one upstream refresh among all
//! subscribers, the source registry with its licence gate, health tracking and search.
//! Nothing in this crate knows about HTTP servers or Tauri; `lavender-server` and the
//! desktop app are thin adapters around it.

pub mod aircraft_history;
pub mod audit;
pub mod config;
pub mod core;
pub mod diagnostics;
pub mod error;
pub mod fetch;
pub mod health;
pub mod hub;
pub mod providers;
pub mod registry;
pub mod search;
pub mod secrets;
pub mod settings;
pub mod sources;

pub use crate::core::{LavenderCore, LavenderCoreBuilder};
pub use config::CoreConfig;
pub use error::CoreError;
pub use hub::{FeedSubscription, RegionBudget, SubscriptionEvent};
pub use secrets::{EnvSecretStore, MemorySecretStore, SecretKey, SecretStore};
pub use settings::{FileSettingsStore, MemorySettingsStore, SettingsStore};
