//! Lavender's application core (ADR 0002).
//!
//! Everything a Lavender backend does lives here, behind [`LavenderCore`]: provider
//! adapters, the egress policy, the feed hub that shares one upstream refresh among all
//! subscribers, the source registry with its licence gate, health tracking and search.
//! Nothing in this crate knows about HTTP servers or Tauri; `lavender-server` and the
//! desktop app are thin adapters around it.

pub mod aircraft_history;
pub mod analytics;
pub mod audit;
pub mod config;
pub mod core;
pub mod diagnostics;
pub mod error;
pub mod fetch;
pub mod health;
pub mod hub;
pub mod motion;
pub mod providers;
pub mod registry;
pub mod satellite_catalog;
pub mod search;
pub mod secrets;
pub mod settings;
pub mod simulation;
pub mod sources;
pub mod vessel_store;

pub mod regions {
    pub use crate::providers::regions::*;
}

pub use crate::core::{LavenderCore, LavenderCoreBuilder};
pub use config::CoreConfig;
pub use error::CoreError;
pub use hub::{FeedSubscription, RegionBudget, SubscriptionEvent};
pub use providers::boundaries::{BOUNDARIES_ATTRIBUTION, BoundaryStore};
pub use providers::cables::{CableStore, LandingPoint, SubmarineCable};
pub use providers::cctv::{CctvCatalog, build_synthetic_cctv_svg};
pub use providers::divisions::{DIVISIONS_ATTRIBUTION, DivisionIndex, DivisionRecord};
pub use providers::imagery_search::{
    DEFAULT_CATALOG_DAYS, ImageryAttribution, ImageryCandidate, ImageryError, ImageryErrorDetail,
    ImagerySearchProvider, RecentImageryResponse, ThumbnailProbeResponse, ValidatedBox,
    gibs_template, rank_latest, wvs_snapshot_url,
};
pub use providers::infrastructure::{InfrastructureIndex, InfrastructureItem, InfrastructureKind};
pub use providers::installations::{
    InstallationKind, InstallationStore, MilitaryInstallation, military_installation_cache_key,
    quantize_military_installation_box,
};
pub use providers::overpass::{
    OverpassClient, OverpassError, OverpassPayload, SanitizedQuery, is_overpass_boundary_query,
    overpass_payload_is_data, parse_overpass_upstreams, sanitize_overpass_body,
};
pub use providers::regions::{RegionIndex, RegionRecord};
pub use providers::weather::{
    WeatherBounds, WeatherError, WeatherManifest, WeatherMetadata, WeatherProvider,
};
#[cfg(feature = "keyring")]
pub use secrets::KeyringSecretStore;
pub use secrets::{EnvSecretStore, FixtureSecretStore, MemorySecretStore, SecretKey, SecretStore};
pub use settings::{FileSettingsStore, MemorySettingsStore, SettingsStore};
