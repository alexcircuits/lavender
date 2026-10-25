//! Typed core configuration. Adapters build it (server: TOML + `LAVENDER__*` env; desktop:
//! app config dir) and hand it in; the core never reads files or the environment for
//! configuration itself. [`CoreConfig::validate`] reports every problem at once.

use std::time::Duration;

use lavender_domain::{BBox, BikeshareSystem, LicenceMode};
use serde::Deserialize;
use url::Url;

use crate::sources;

#[allow(clippy::expect_used)] // Only called with constant literals; `defaults_are_valid` covers them.
fn url(s: &str) -> Url {
    Url::parse(s).expect("constant URL literal")
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct CoreConfig {
    /// `evaluation` (default) or `commercial` (ADR 0008).
    pub licence_mode: LicenceMode,
    /// Source ids an operator has cleared for commercial use by agreement.
    pub licence_acknowledged: Vec<String>,
    pub fetch: FetchConfig,
    pub hub: HubConfig,
    pub providers: ProvidersConfig,
    pub geocoder: GeocoderConfig,
    pub routing: RoutingConfig,
    pub bikeshare: BikeshareConfig,
    pub transit: TransitConfig,
    pub terrain: TerrainConfig,
    pub traffic: TrafficConfig,
    pub alerting: AlertingConfig,
    pub resolution: ResolutionConfig,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            licence_mode: LicenceMode::Evaluation,
            licence_acknowledged: Vec::new(),
            fetch: FetchConfig::default(),
            hub: HubConfig::default(),
            providers: ProvidersConfig::default(),
            geocoder: GeocoderConfig::default(),
            routing: RoutingConfig::default(),
            bikeshare: BikeshareConfig::default(),
            transit: TransitConfig::default(),
            terrain: TerrainConfig::default(),
            traffic: TrafficConfig::default(),
            alerting: AlertingConfig::default(),
            resolution: ResolutionConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FetchConfig {
    /// Sent with every upstream request. Operators should add a contact address.
    pub user_agent: String,
    pub connect_timeout_s: u64,
    pub request_timeout_s: u64,
    /// Body cap for requests whose provider sets none. Providers' own caps (USGS 32 MiB,
    /// CelesTrak 20 MiB, adsb.lol 12 MiB) apply instead of it; every cap is bounded by
    /// [`crate::fetch::MAX_BODY_CEILING`].
    pub max_body_bytes: usize,
    pub max_redirects: u8,
    pub retries: u8,
    pub per_host_concurrency: usize,
    /// Hosts that may be contacted over plain HTTP. Empty in production; tests use it
    /// for local mock servers.
    /// Explicit development/internal-host exception to HTTPS and public-address checks.
    /// Production public providers should leave this empty.
    pub allow_http_hosts: Vec<String>,
}

impl Default for FetchConfig {
    fn default() -> Self {
        Self {
            user_agent: format!(
                "Lavender/{} (NoirSystems geospatial workspace)",
                env!("CARGO_PKG_VERSION")
            ),
            connect_timeout_s: 5,
            request_timeout_s: 20,
            max_body_bytes: 16 * 1024 * 1024,
            max_redirects: 3,
            retries: 2,
            per_host_concurrency: 4,
            allow_http_hosts: Vec::new(),
        }
    }
}

impl FetchConfig {
    #[must_use]
    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(self.connect_timeout_s)
    }

    #[must_use]
    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_s)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct HubConfig {
    /// Keep a refresh task alive this long after its last subscriber leaves, so repeated
    /// one-shot snapshots and quick viewport changes do not refetch.
    pub linger_s: u64,
    /// Upper bound on concurrently refreshed (feed, region) keys.
    pub max_slots: usize,
    /// Items per snapshot before truncation (with a notice).
    pub max_items: usize,
    /// How long a one-shot snapshot waits for the first upstream result.
    pub first_result_timeout_s: u64,
}

impl Default for HubConfig {
    fn default() -> Self {
        Self { linger_s: 60, max_slots: 256, max_items: 20_000, first_result_timeout_s: 25 }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
pub struct ProvidersConfig {
    pub usgs: UsgsConfig,
    pub adsb_lol: AdsbLolConfig,
    /// Synthetic aircraft: replaces the live adsb.lol source for the aircraft
    /// feed when enabled. Default off — an owner decision, like every optional
    /// source. No network is touched.
    pub simulation: SimulationConfig,
    pub opensky: OpenSkyConfig,
    pub adsbdb: AdsbdbConfig,
    pub celestrak: CelestrakConfig,
    pub cyclones: CyclonesConfig,
    pub firms: FirmsConfig,
    pub launches: LaunchesConfig,
    pub wind: WindConfig,
    pub aisstream: AisStreamConfig,
    pub perimeters: PerimetersConfig,
}

/// The wire format of an operator-selected OSM geocoding service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GeocoderFormat {
    #[default]
    Nominatim,
    Photon,
}

/// Place lookup is opt-in. No public endpoint is selected implicitly: choosing a
/// self-hosted or contracted service remains an operator decision (ENG-938).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct GeocoderConfig {
    pub enabled: bool,
    pub format: GeocoderFormat,
    /// Service root, ending in '/'. Search uses `search` or `api/` below this root.
    pub base_url: Option<Url>,
    pub cache_max_entries: u64,
    pub cache_ttl_s: u64,
    /// Applies across geocoder instances in this process, with a 1000 ms floor.
    pub min_request_interval_ms: u64,
    /// Distinct cache misses admitted while waiting or fetching. Duplicates coalesce.
    pub max_pending: usize,
    /// Optional additional service credit, displayed alongside mandatory OSM credit.
    pub attribution: Option<String>,
}

impl Default for GeocoderConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            format: GeocoderFormat::Nominatim,
            base_url: None,
            cache_max_entries: 128,
            cache_ttl_s: 300,
            min_request_interval_ms: 1100,
            max_pending: 4,
            attribution: None,
        }
    }
}

/// Routing service configuration with pacing and caching.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct RoutingConfig {
    pub enabled: bool,
    pub car_endpoint: Option<String>,
    pub bike_endpoint: Option<String>,
    pub foot_endpoint: Option<String>,
    pub cache_max_entries: u64,
    pub cache_ttl_s: u64,
    pub min_request_interval_ms: u64,
    pub max_queue_depth: usize,
    pub request_timeout_s: u64,
    pub max_response_bytes: usize,
    pub max_leg_km: f64,
    pub max_total_km: f64,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            car_endpoint: None,
            bike_endpoint: None,
            foot_endpoint: None,
            cache_max_entries: 200,
            cache_ttl_s: 600,
            min_request_interval_ms: 1000,
            max_queue_depth: 8,
            request_timeout_s: 12,
            max_response_bytes: 8 * 1024 * 1024,
            max_leg_km: 600.0,
            max_total_km: 2500.0,
        }
    }
}

impl RoutingConfig {
    pub(crate) fn validation_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let validate_ep = |name: &str, ep: &Option<String>, errs: &mut Vec<String>| {
            if let Some(s) = ep {
                match Url::parse(s) {
                    Ok(u) => {
                        if !matches!(u.scheme(), "https" | "http")
                            || u.host_str().is_none()
                            || !u.username().is_empty()
                            || u.password().is_some()
                        {
                            errs.push(format!(
                                "{name}: must have an HTTP(S) host and no credentials"
                            ));
                        }
                    }
                    Err(_) => errs.push(format!("{name}: must be a valid URL")),
                }
            }
        };
        validate_ep("routing.car_endpoint", &self.car_endpoint, &mut errors);
        validate_ep("routing.bike_endpoint", &self.bike_endpoint, &mut errors);
        validate_ep("routing.foot_endpoint", &self.foot_endpoint, &mut errors);

        if self.min_request_interval_ms < 50 {
            errors.push("routing.min_request_interval_ms: must be at least 50 ms".to_owned());
        }
        if self.max_queue_depth == 0 || self.max_queue_depth > 256 {
            errors.push("routing.max_queue_depth: must be between 1 and 256".to_owned());
        }
        if self.max_leg_km <= 0.0 {
            errors.push("routing.max_leg_km: must be positive".to_owned());
        }
        if self.max_total_km < self.max_leg_km {
            errors.push("routing.max_total_km: must be at least max_leg_km".to_owned());
        }
        errors
    }
}

/// Bike-share (GBFS) configuration.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct BikeshareConfig {
    pub enabled: bool,
    pub cache_ttl_s: u64,
    pub discovery_ttl_s: u64,
    pub systems: Vec<BikeshareSystem>,
}

impl Default for BikeshareConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_ttl_s: 60,
            discovery_ttl_s: 300,
            systems: crate::providers::bikeshare::default_approved_systems(),
        }
    }
}

impl BikeshareConfig {
    pub(crate) fn validation_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.cache_ttl_s < 10 || self.cache_ttl_s > 86400 {
            errors.push("bikeshare.cache_ttl_s: must be between 10 and 86400".to_string());
        }
        if self.discovery_ttl_s < 60 || self.discovery_ttl_s > 86400 {
            errors.push("bikeshare.discovery_ttl_s: must be between 60 and 86400".to_string());
        }
        for system in &self.systems {
            if system.id.trim().is_empty() {
                errors.push("bikeshare.systems: system id cannot be empty".to_string());
            }
            if let Err(e) =
                crate::providers::bikeshare::BikeshareEngine::validate_no_arbitrary_url(&system.id)
            {
                errors.push(format!("bikeshare.systems: invalid system id: {e}"));
            }
            if let Ok(u) = Url::parse(&system.gbfs_url) {
                if !matches!(u.scheme(), "https" | "http") || u.host_str().is_none() {
                    errors.push(format!(
                        "bikeshare.systems.{}: gbfs_url must be a valid HTTP(S) URL",
                        system.id
                    ));
                }
            } else {
                errors
                    .push(format!("bikeshare.systems.{}: gbfs_url must be a valid URL", system.id));
            }
        }
        errors
    }
}

/// GTFS-Realtime transit configuration.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TransitConfig {
    pub enabled: bool,
    pub feeds: Vec<lavender_domain::TransitFeed>,
    pub feed_bytes_limit: usize,
    pub process_bytes_limit: usize,
    pub max_keys: usize,
}

impl Default for TransitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            feeds: crate::providers::transit::default_approved_feeds(),
            feed_bytes_limit: crate::providers::transit::history::DEFAULT_FEED_BYTES_LIMIT,
            process_bytes_limit: crate::providers::transit::history::DEFAULT_PROCESS_BYTES_LIMIT,
            max_keys: crate::providers::transit::history::DEFAULT_MAX_KEYS,
        }
    }
}

impl TransitConfig {
    pub(crate) fn validation_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();
        for feed in &self.feeds {
            if !crate::providers::transit::valid_transit_feed_id(&feed.id) {
                errors.push(format!("transit.feeds: invalid feed id '{}'", feed.id));
            }
        }
        errors
    }
}

/// Re:Earth terrain point-heights configuration.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TerrainConfig {
    pub enabled: bool,
    pub upstream_url: String,
    pub max_points: usize,
    pub chunk_size: usize,
    pub cache_capacity: usize,
    pub cache_ttl_days: u64,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            upstream_url: "https://terrain.reearth.land/heights.json".to_string(),
            max_points: crate::providers::terrain::client::TERRAIN_DEFAULT_MAX_POINTS,
            chunk_size: crate::providers::terrain::client::TERRAIN_DEFAULT_CHUNK_SIZE,
            cache_capacity: crate::providers::terrain::client::TERRAIN_DEFAULT_CACHE_CAPACITY,
            cache_ttl_days: crate::providers::terrain::client::TERRAIN_DEFAULT_TTL_DAYS,
        }
    }
}

impl TerrainConfig {
    pub(crate) fn validation_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.max_points == 0 {
            errors.push("terrain.max_points must be greater than 0".to_string());
        }
        if self.chunk_size == 0 {
            errors.push("terrain.chunk_size must be greater than 0".to_string());
        }
        if self.cache_capacity == 0 {
            errors.push("terrain.cache_capacity must be greater than 0".to_string());
        }
        errors
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TrafficConfig {
    pub enabled: bool,
    pub daily_budget: u64,
    pub cache_dir: std::path::PathBuf,
    pub max_disk_bytes: u64,
    pub tomtom_base_url: String,
    pub openfreemap_tile_url: String,
}

impl Default for TrafficConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            daily_budget: 6000,
            cache_dir: std::path::PathBuf::from(".lavender-cache/tomtom"),
            max_disk_bytes: 500 * 1024 * 1024,
            tomtom_base_url: "https://api.tomtom.com/traffic/map/4/tile/flow/relative".to_string(),
            openfreemap_tile_url: "https://tiles.openfreemap.org/planet".to_string(),
        }
    }
}

impl TrafficConfig {
    pub(crate) fn validation_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.daily_budget == 0 {
            errors.push("traffic.daily_budget must be greater than 0".to_string());
        }
        if self.max_disk_bytes == 0 {
            errors.push("traffic.max_disk_bytes must be greater than 0".to_string());
        }
        errors
    }
}

/// Configuration for the geofence alerting rule engine.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct AlertingConfig {
    /// Whether geofence alerting is active (default false).
    pub enabled: bool,
    /// Maximum number of active rules.
    pub max_rules: usize,
    /// Maximum number of alert events retained in the inbox buffer.
    pub inbox_capacity: usize,
    /// Maximum number of entity containment states tracked in memory.
    pub state_capacity: usize,
}

impl Default for AlertingConfig {
    fn default() -> Self {
        Self { enabled: false, max_rules: 100, inbox_capacity: 500, state_capacity: 10_000 }
    }
}

/// Configuration for the cross-feed entity resolution engine.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ResolutionConfig {
    /// Maximum number of canonical entities retained in memory before LRU eviction.
    pub max_entities: usize,
    /// Maximum number of relation links retained in memory.
    pub max_links: usize,
}

impl Default for ResolutionConfig {
    fn default() -> Self {
        Self { max_entities: 50_000, max_links: 100_000 }
    }
}

impl GeocoderConfig {
    /// Transport allowlists are checked separately by the core and Fetcher.
    pub(crate) fn validation_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if let Some(base) = &self.base_url {
            if base.host_str().is_none()
                || !matches!(base.scheme(), "https" | "http")
                || !base.username().is_empty()
                || base.password().is_some()
                || base.query().is_some()
                || base.fragment().is_some()
            {
                errors.push("geocoder.base_url: must have an HTTP(S) host and no credentials, query or fragment".to_owned());
            }
        } else if self.enabled {
            errors.push("geocoder.base_url: required when place lookup is enabled".to_owned());
        }
        if !(1..=4096).contains(&self.cache_max_entries) {
            errors.push("geocoder.cache_max_entries: must be between 1 and 4096".to_owned());
        }
        if !(1..=86400).contains(&self.cache_ttl_s) {
            errors.push("geocoder.cache_ttl_s: must be between 1 and 86400".to_owned());
        }
        if !(1000..=60000).contains(&self.min_request_interval_ms) {
            errors.push(
                "geocoder.min_request_interval_ms: must be between 1000 and 60000".to_owned(),
            );
        }
        if !(1..=16).contains(&self.max_pending) {
            errors.push("geocoder.max_pending: must be between 1 and 16".to_owned());
        }
        if self.attribution.as_ref().is_some_and(|text| {
            text.trim().is_empty()
                || text.chars().count() > 200
                || text.chars().any(char::is_control)
        }) {
            errors.push(
                "geocoder.attribution: must be 1–200 characters without control characters"
                    .to_owned(),
            );
        }
        errors
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct UsgsConfig {
    pub enabled: bool,
    pub base_url: Url,
    /// One of the USGS summary feeds, for example `2.5_day`, `all_hour`, `4.5_week`.
    pub summary: String,
    pub cadence_s: u64,
}

impl Default for UsgsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_url: url("https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/"),
            summary: "2.5_day".to_owned(),
            cadence_s: 60,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct CyclonesConfig {
    pub enabled: bool,
    /// Root containing CurrentStorms.json; only fixed relative requests are used.
    pub status_base_url: Url,
    /// NHC/CPHC summary `MapServer` root ending with '/'.
    pub gis_base_url: Url,
    pub cadence_s: u64,
}

impl Default for CyclonesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            status_base_url: url("https://www.nhc.noaa.gov/"),
            gis_base_url: url(
                "https://mapservices.weather.noaa.gov/tropical/rest/services/tropical/NHC_tropical_weather_summary/MapServer/",
            ),
            cadence_s: 600,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct AdsbLolConfig {
    pub enabled: bool,
    pub base_url: Url,
    pub cadence_s: u64,
    /// Grid cell size for regional point queries (5° fits the 250 nm radius limit).
    pub cell_deg: f64,
    /// Cells per subscription before falling back to the worldwide military feed.
    pub max_cells: u32,
    /// Minimum spacing between requests to adsb.lol (it rejects bursts with 420/429).
    pub min_request_interval_ms: u64,
}

impl Default for AdsbLolConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_url: url("https://api.adsb.lol/"),
            cadence_s: 20,
            cell_deg: 5.0,
            max_cells: 4,
            min_request_interval_ms: 2_500,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct OpenSkyConfig {
    pub enabled: bool,
    pub base_url: Url,
    pub auth_url: Url,
    pub cadence_s: u64,
    /// Grid cell size for regional bounding box queries.
    pub cell_deg: f64,
    /// Cells per subscription before falling back to the worldwide feed.
    pub max_cells: u32,
    /// Minimum spacing between requests to OpenSky.
    pub min_request_interval_ms: u64,
}

impl Default for OpenSkyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: url("https://opensky-network.org/api/"),
            auth_url: url(
                "https://auth.opensky-network.org/auth/realms/opensky-network/protocol/openid-connect/token",
            ),
            cadence_s: 30,
            cell_deg: 5.0,
            max_cells: 4,
            min_request_interval_ms: 5_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct AdsbdbConfig {
    pub enabled: bool,
    pub base_url: Url,
    pub cache_max_entries: u64,
    pub cache_ttl_s: u64,
    pub min_request_interval_ms: u64,
}

impl Default for AdsbdbConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: url("https://api.adsbdb.com/v0/"),
            cache_max_entries: 2048,
            cache_ttl_s: 86400,
            min_request_interval_ms: 200,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct CelestrakConfig {
    pub enabled: bool,
    pub base_url: Url,
    /// CelesTrak GP groups to load.
    pub groups: Vec<String>,
    /// CelesTrak asks clients not to refresh a group more often than every two hours.
    pub cadence_s: u64,
}

impl Default for CelestrakConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_url: url("https://celestrak.org/NORAD/elements/"),
            // `last-30-days` holds every object from the last month's launches, which the
            // operational groups do not list yet: launches are matched to their satellites
            // from it (ENG-957). It comes last so an object also in a named group keeps
            // that group.
            groups: [
                "stations",
                "visual",
                "weather",
                "gps-ops",
                "galileo",
                "science",
                "last-30-days",
            ]
            .map(str::to_owned)
            .to_vec(),
            cadence_s: 7_200,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FirmsConfig {
    pub enabled: bool,
    pub base_url: Url,
    pub sources: Vec<String>,
    pub cadence_s: u64,
    /// Time allowed for each source's download, body included (two days of a VIIRS
    /// source are tens of megabytes). Replaces `fetch.request_timeout_s` for FIRMS.
    pub request_timeout_s: u64,
}

impl Default for FirmsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_url: url("https://firms.modaps.eosdis.nasa.gov/api/area/csv/"),
            sources: vec![
                "VIIRS_NOAA20_NRT".to_owned(),
                "VIIRS_NOAA21_NRT".to_owned(),
                "VIIRS_SNPP_NRT".to_owned(),
                "MODIS_NRT".to_owned(),
            ],
            cadence_s: 600,
            request_timeout_s: 120,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LaunchesConfig {
    pub enabled: bool,
    pub base_url: Url,
    /// Refresh interval (default 900 s = 15 minutes).
    pub cadence_s: u64,
    /// Maximum allowed upstream requests per hour (default 15).
    pub max_requests_per_hour: u32,
    /// Minimum retry floor / spacing after failure or 429 (default 240 s).
    pub retry_floor_s: u64,
}

impl Default for LaunchesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_url: url("https://ll.thespacedevs.com/2.3.0/"),
            cadence_s: 900,
            max_requests_per_hour: 15,
            retry_floor_s: 240,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct WindConfig {
    pub enabled: bool,
    pub gfs_base_url: Url,
    pub ifs_base_url: Url,
    pub ttl_s: u64,
    pub timeout_s: u64,
    pub scalar_timeout_s: u64,
    pub failure_backoff_s: u64,
    pub target_dx: f64,
}

impl Default for WindConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            gfs_base_url: url("https://noaa-gfs-bdp-pds.s3.amazonaws.com/"),
            ifs_base_url: url("https://data.ecmwf.int/forecasts/"),
            ttl_s: 3600,
            timeout_s: 40,
            scalar_timeout_s: 12,
            failure_backoff_s: 60,
            target_dx: 1.0,
        }
    }
}

/// Live vessels from AISStream. Needs `LAVENDER_AISSTREAM_API_KEY`; without it the layer
/// is unconfigured and no connection is made.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct AisStreamConfig {
    pub enabled: bool,
    /// The WebSocket endpoint: `wss://`, or `ws://` only for `fetch.allow_http_hosts`.
    pub url: Url,
    /// Areas AISStream sends reports for (at most 16); the default is the whole world.
    pub bounding_boxes: Vec<BBox>,
    /// Seconds between snapshots for each subscriber (and re-reads of each map cell).
    pub update_interval_s: u64,
    /// Grid cell size for area queries; a view needing more than `max_cells` cells
    /// shows the most recently heard vessels worldwide instead.
    pub cell_deg: f64,
    pub max_cells: u32,
    pub max_vessels: usize,
    pub max_track_points: usize,
    pub max_total_track_points: usize,
    /// Vessels not heard from for this long are removed; tracks keep this much history.
    pub retention_s: u64,
    /// A connection that sends nothing for this long, not even the answer to the pings
    /// sent every quarter of it, is replaced. With a worldwide subscription, so is one
    /// that delivers no AIS messages for this long; narrower areas may be quiet.
    pub read_timeout_s: u64,
    /// First wait after a failed connection; it doubles up to `reconnect_max_s`.
    pub reconnect_min_ms: u64,
    pub reconnect_max_s: u64,
    /// Disconnect after this long without anyone asking for vessels (0 = stay connected).
    pub idle_disconnect_s: u64,
}

impl Default for AisStreamConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            url: url("wss://stream.aisstream.io/v0/stream"),
            bounding_boxes: vec![BBox::WORLD],
            update_interval_s: 5,
            cell_deg: 10.0,
            max_cells: 9,
            max_vessels: 50_000,
            max_track_points: 64,
            max_total_track_points: 500_000,
            retention_s: 1_800,
            read_timeout_s: 120,
            reconnect_min_ms: 5_000,
            reconnect_max_s: 900,
            idle_disconnect_s: 600,
        }
    }
}

impl AisStreamConfig {
    /// Every problem with this section; `allow_http_hosts` permits `ws://` test servers.
    pub(crate) fn validation_errors(&self, allow_http_hosts: &[String]) -> Vec<String> {
        let mut errors = Vec::new();
        let mut check = |ok: bool, message: &str| {
            if !ok {
                errors.push(format!("providers.aisstream.{message}"));
            }
        };
        let host = self.url.host_str().unwrap_or_default();
        let plain_allowed = allow_http_hosts.iter().any(|h| h.eq_ignore_ascii_case(host));
        check(
            self.url.scheme() == "wss" || (self.url.scheme() == "ws" && plain_allowed),
            "url: must be a wss:// URL",
        );
        check(
            !host.is_empty()
                && self.url.username().is_empty()
                && self.url.password().is_none()
                && self.url.query().is_none()
                && self.url.fragment().is_none(),
            "url: requires a host and no credentials, query or fragment",
        );
        check((1..=16).contains(&self.bounding_boxes.len()), "bounding_boxes: list 1–16 areas");
        check((1..=60).contains(&self.update_interval_s), "update_interval_s: must be 1–60");
        check((1.0..=45.0).contains(&self.cell_deg), "cell_deg: must be 1–45");
        check((1..=64).contains(&self.max_cells), "max_cells: must be 1–64");
        check((1..=200_000).contains(&self.max_vessels), "max_vessels: must be 1–200000");
        check((1..=1_000).contains(&self.max_track_points), "max_track_points: must be 1–1000");
        check(
            (self.max_track_points..=5_000_000).contains(&self.max_total_track_points),
            "max_total_track_points: must be max_track_points–5000000",
        );
        check((60..=86_400).contains(&self.retention_s), "retention_s: must be 60–86400");
        check((10..=3_600).contains(&self.read_timeout_s), "read_timeout_s: must be 10–3600");
        check(
            (1_000..=60_000).contains(&self.reconnect_min_ms),
            "reconnect_min_ms: must be 1000–60000",
        );
        check(
            (self.reconnect_min_ms.div_ceil(1_000)..=86_400).contains(&self.reconnect_max_s),
            "reconnect_max_s: must be at least reconnect_min_ms and at most 86400",
        );
        check(
            self.idle_disconnect_s == 0 || (60..=86_400).contains(&self.idle_disconnect_s),
            "idle_disconnect_s: must be 0 or 60–86400",
        );
        errors
    }
}

/// NIFC WFIGS current fire perimeters, joined with InciWeb incident pages.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PerimetersConfig {
    pub enabled: bool,
    /// The WFIGS current perimeters feature layer, ending with '/'; only its fixed
    /// `query` resource is requested.
    pub wfigs_base_url: Url,
    /// WFIGS republishes every 5 minutes; refreshing faster only repeats the same data.
    pub cadence_s: u64,
    /// Whether perimeters are linked to their InciWeb pages. Off means no InciWeb requests.
    pub inciweb_enabled: bool,
    /// InciWeb site root, ending with '/'.
    pub inciweb_base_url: Url,
    /// How long the InciWeb incident index is reused before it is fetched again.
    pub inciweb_index_ttl_s: u64,
    /// Most InciWeb incident pages read per refresh (at most 8; a refresh's few seconds of
    /// InciWeb time allow about four); links for the rest follow later.
    pub inciweb_pages_per_refresh: usize,
}

impl Default for PerimetersConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            wfigs_base_url: url(
                "https://services3.arcgis.com/T4QMspbfLg3qTGWY/arcgis/rest/services/WFIGS_Interagency_Perimeters_Current/FeatureServer/0/",
            ),
            cadence_s: 300,
            inciweb_enabled: true,
            inciweb_base_url: url("https://inciweb.wildfire.gov/"),
            inciweb_index_ttl_s: 21_600,
            inciweb_pages_per_refresh: 4,
        }
    }
}

impl PerimetersConfig {
    fn validation_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();
        for (name, endpoint) in
            [("wfigs_base_url", &self.wfigs_base_url), ("inciweb_base_url", &self.inciweb_base_url)]
        {
            if endpoint.host_str().is_none()
                || !endpoint.username().is_empty()
                || endpoint.password().is_some()
                || endpoint.query().is_some()
                || endpoint.fragment().is_some()
            {
                errors.push(format!("providers.perimeters.{name}: requires a host and no credentials, query or fragment"));
            }
        }
        if self.cadence_s < 300 {
            errors.push("providers.perimeters.cadence_s: must be at least 300".to_owned());
        }
        if !(3_600..=86_400).contains(&self.inciweb_index_ttl_s) {
            errors.push(
                "providers.perimeters.inciweb_index_ttl_s: must be between 3600 and 86400"
                    .to_owned(),
            );
        }
        if self.inciweb_pages_per_refresh > 8 {
            errors.push(
                "providers.perimeters.inciweb_pages_per_refresh: must be at most 8".to_owned(),
            );
        }
        errors
    }
}

const USGS_SUMMARIES: [&str; 20] = [
    "significant_hour",
    "4.5_hour",
    "2.5_hour",
    "1.0_hour",
    "all_hour",
    "significant_day",
    "4.5_day",
    "2.5_day",
    "1.0_day",
    "all_day",
    "significant_week",
    "4.5_week",
    "2.5_week",
    "1.0_week",
    "all_week",
    "significant_month",
    "4.5_month",
    "2.5_month",
    "1.0_month",
    "all_month",
];

impl CoreConfig {
    /// Checks every setting and returns all problems found, so an operator fixes them in
    /// one pass.
    ///
    /// # Errors
    /// Returns the list of problems when any setting is invalid.
    #[allow(clippy::too_many_lines)]
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let mut https = |name: &str, u: &Url| {
            let host = u.host_str().unwrap_or_default();
            if u.scheme() != "https" && !self.fetch.allow_http_hosts.iter().any(|h| h == host) {
                errors.push(format!("{name}: must be an https:// URL (got {})", u.scheme()));
            }
            if !u.path().ends_with('/') {
                errors.push(format!("{name}: must end with '/'"));
            }
        };
        https("providers.usgs.base_url", &self.providers.usgs.base_url);
        https("providers.adsb_lol.base_url", &self.providers.adsb_lol.base_url);
        https("providers.celestrak.base_url", &self.providers.celestrak.base_url);
        https("providers.cyclones.status_base_url", &self.providers.cyclones.status_base_url);
        https("providers.cyclones.gis_base_url", &self.providers.cyclones.gis_base_url);
        https("providers.firms.base_url", &self.providers.firms.base_url);
        https("providers.launches.base_url", &self.providers.launches.base_url);
        https("providers.wind.gfs_base_url", &self.providers.wind.gfs_base_url);
        https("providers.wind.ifs_base_url", &self.providers.wind.ifs_base_url);
        https("providers.perimeters.wfigs_base_url", &self.providers.perimeters.wfigs_base_url);
        https("providers.perimeters.inciweb_base_url", &self.providers.perimeters.inciweb_base_url);
        if self.providers.opensky.enabled {
            https("providers.opensky.base_url", &self.providers.opensky.base_url);
        }
        if self.providers.adsbdb.enabled {
            https("providers.adsbdb.base_url", &self.providers.adsbdb.base_url);
        }
        if let Some(base) = &self.geocoder.base_url {
            https("geocoder.base_url", base);
        }

        if self.providers.opensky.enabled {
            let auth = &self.providers.opensky.auth_url;
            let host = auth.host_str().unwrap_or_default();
            if auth.scheme() != "https" && !self.fetch.allow_http_hosts.iter().any(|h| h == host) {
                errors.push(format!(
                    "providers.opensky.auth_url: must be an https:// URL (got {})",
                    auth.scheme()
                ));
            }
            if self.licence_mode == LicenceMode::Commercial
                && !self.licence_acknowledged.iter().any(|s| s == sources::OPENSKY)
            {
                errors.push(
                    "providers.opensky: OpenSky is licensed for non-commercial/evaluation use only; commercial use requires explicit licence acknowledgement".to_owned(),
                );
            }
        }
        if self.providers.adsbdb.enabled
            && self.licence_mode == LicenceMode::Commercial
            && !self.licence_acknowledged.iter().any(|s| s == sources::ADS_B_DB)
        {
            errors.push(
                "providers.adsbdb: adsbdb route enrichment requires permission for commercial redistribution; commercial use requires explicit licence acknowledgement".to_owned(),
            );
        }
        errors.extend(self.geocoder.validation_errors());
        errors.extend(self.providers.aisstream.validation_errors(&self.fetch.allow_http_hosts));
        errors.extend(self.providers.perimeters.validation_errors());
        errors.extend(self.routing.validation_errors());
        errors.extend(self.bikeshare.validation_errors());
        errors.extend(self.transit.validation_errors());
        errors.extend(self.terrain.validation_errors());
        errors.extend(self.traffic.validation_errors());

        let wind = &self.providers.wind;
        if wind.ttl_s < 60 {
            errors.push("providers.wind.ttl_s: must be at least 60".to_owned());
        }
        if wind.timeout_s == 0 || wind.timeout_s > 300 {
            errors.push("providers.wind.timeout_s: must be between 1 and 300".to_owned());
        }
        if wind.scalar_timeout_s == 0 || wind.scalar_timeout_s > 300 {
            errors.push("providers.wind.scalar_timeout_s: must be between 1 and 300".to_owned());
        }
        if !(0.25..=180.0).contains(&wind.target_dx) {
            errors.push("providers.wind.target_dx: must be between 0.25 and 180.0".to_owned());
        }

        let launches = &self.providers.launches;
        if launches.cadence_s < 60 {
            errors.push("providers.launches.cadence_s: must be at least 60".to_owned());
        }
        if launches.max_requests_per_hour == 0 || launches.max_requests_per_hour > 1000 {
            errors.push(
                "providers.launches.max_requests_per_hour: must be between 1 and 1000".to_owned(),
            );
        }
        if launches.base_url.host_str().is_none()
            || !launches.base_url.username().is_empty()
            || launches.base_url.password().is_some()
            || launches.base_url.query().is_some()
            || launches.base_url.fragment().is_some()
        {
            errors.push("providers.launches.base_url: requires a host and no credentials, query or fragment".to_owned());
        }

        let firms = &self.providers.firms;
        if firms.cadence_s < 600 {
            errors.push("providers.firms.cadence_s: must be at least 600".to_owned());
        }
        if firms.request_timeout_s == 0 || firms.request_timeout_s > 600 {
            errors.push("providers.firms.request_timeout_s: must be between 1 and 600".to_owned());
        }
        if firms.base_url.host_str().is_none()
            || !firms.base_url.username().is_empty()
            || firms.base_url.password().is_some()
            || firms.base_url.query().is_some()
            || firms.base_url.fragment().is_some()
        {
            errors.push(
                "providers.firms.base_url: requires a host and no credentials, query or fragment"
                    .to_owned(),
            );
        }
        if firms.sources.is_empty() || firms.sources.len() > 10 {
            errors.push("providers.firms.sources: list 1–10 sources".to_owned());
        }
        for (i, s) in firms.sources.iter().enumerate() {
            if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                errors.push(format!("providers.firms.sources: '{s}' is not a valid source name"));
            } else if firms.sources[..i].iter().any(|earlier| earlier.eq_ignore_ascii_case(s)) {
                // Each source is one download per cadence against the shared key quota.
                errors.push(format!("providers.firms.sources: '{s}' is listed more than once"));
            }
        }

        let cyclones = &self.providers.cyclones;
        if cyclones.cadence_s < 600 {
            errors.push("providers.cyclones.cadence_s: must be at least 600".to_owned());
        }
        for (name, endpoint) in
            [("status", &cyclones.status_base_url), ("gis", &cyclones.gis_base_url)]
        {
            if endpoint.host_str().is_none()
                || !endpoint.username().is_empty()
                || endpoint.password().is_some()
                || endpoint.query().is_some()
                || endpoint.fragment().is_some()
            {
                errors.push(format!("providers.cyclones.{name}_base_url: requires a host and no credentials, query or fragment"));
            }
        }
        let usgs = &self.providers.usgs;
        if !USGS_SUMMARIES.contains(&usgs.summary.as_str()) {
            errors.push(format!(
                "providers.usgs.summary: '{}' is not a USGS summary feed",
                usgs.summary
            ));
        }
        if usgs.cadence_s < 30 {
            errors.push("providers.usgs.cadence_s: must be at least 30".to_owned());
        }
        let adsb = &self.providers.adsb_lol;
        if adsb.cadence_s < 5 {
            errors.push("providers.adsb_lol.cadence_s: must be at least 5".to_owned());
        }
        if !(1.0..=5.0).contains(&adsb.cell_deg) {
            errors.push(
                "providers.adsb_lol.cell_deg: must be between 1 and 5 (250 nm radius limit)"
                    .to_owned(),
            );
        }
        if !(1..=25).contains(&adsb.max_cells) {
            errors.push("providers.adsb_lol.max_cells: must be between 1 and 25".to_owned());
        }
        let cel = &self.providers.celestrak;
        if cel.cadence_s < 7_200 {
            errors.push("providers.celestrak.cadence_s: CelesTrak asks for at most one refresh per 2 hours (7200)".to_owned());
        }
        if cel.groups.is_empty() || cel.groups.len() > 20 {
            errors.push("providers.celestrak.groups: list 1–20 groups".to_owned());
        }
        for g in &cel.groups {
            if g.is_empty()
                || !g.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            {
                errors.push(format!("providers.celestrak.groups: '{g}' is not a valid group name"));
            }
        }
        if !(1024..=crate::fetch::MAX_BODY_CEILING).contains(&self.fetch.max_body_bytes)
            || self.fetch.request_timeout_s == 0
        {
            errors.push(format!(
                "fetch: max_body_bytes must be 1024–{} and request_timeout_s > 0",
                crate::fetch::MAX_BODY_CEILING
            ));
        }
        if self.hub.max_slots == 0 || self.hub.max_items == 0 {
            errors.push("hub: max_slots and max_items must be positive".to_owned());
        }
        if self.alerting.max_rules == 0
            || self.alerting.inbox_capacity == 0
            || self.alerting.state_capacity == 0
        {
            errors.push(
                "alerting: max_rules, inbox_capacity and state_capacity must be positive"
                    .to_owned(),
            );
        }
        if self.resolution.max_entities == 0 || self.resolution.max_links == 0 {
            errors.push("resolution: max_entities and max_links must be positive".to_owned());
        }
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        CoreConfig::default().validate().unwrap();
    }

    #[test]
    fn cyclone_roots_are_fixed_resources_with_a_ten_minute_floor() {
        let mut config = CoreConfig::default();
        assert_eq!(config.providers.cyclones.cadence_s, 600);
        config.providers.cyclones.cadence_s = 599;
        config.providers.cyclones.status_base_url =
            Url::parse("https://user:password@www.nhc.noaa.gov/?token=secret").unwrap();
        let errors = config.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("cadence_s")));
        assert!(errors.iter().any(|e| e.contains("no credentials")));
        assert!(errors.iter().all(|e| !e.contains("secret") && !e.contains("password")));
    }

    #[test]
    fn firms_downloads_each_of_at_most_ten_distinct_sources_per_ten_minutes() {
        let mut config = CoreConfig::default();
        config.providers.firms.cadence_s = 599;
        config.providers.firms.sources = vec!["VIIRS_SNPP_NRT".into(), "viirs_snpp_nrt".into()];
        let errors = config.validate().unwrap_err();
        assert_eq!(errors.len(), 2, "{errors:#?}");
        assert!(errors.iter().any(|e| e.contains("cadence_s")));
        assert!(errors.iter().any(|e| e.contains("more than once")));
        config.providers.firms.cadence_s = 600;
        config.providers.firms.sources = (0..11).map(|i| format!("SOURCE_{i}")).collect();
        assert_eq!(config.validate().unwrap_err().len(), 1);
    }

    #[test]
    fn firms_download_timeout_is_bounded() {
        let mut config = CoreConfig::default();
        assert_eq!(config.providers.firms.request_timeout_s, 120);
        for invalid in [0, 601] {
            config.providers.firms.request_timeout_s = invalid;
            let errors = config.validate().unwrap_err();
            assert_eq!(errors.len(), 1, "{errors:#?}");
            assert!(errors[0].contains("providers.firms.request_timeout_s"));
        }
        config.providers.firms.request_timeout_s = 600;
        config.validate().unwrap();
    }

    #[test]
    fn place_lookup_requires_an_explicit_endpoint_and_pacing_floor() {
        let mut config = CoreConfig::default();
        assert!(!config.geocoder.enabled);
        assert!(config.geocoder.base_url.is_none());
        config.geocoder.enabled = true;
        config.geocoder.min_request_interval_ms = 999;
        config.geocoder.cache_max_entries = 0;
        config.geocoder.cache_ttl_s = 0;
        config.geocoder.max_pending = 0;
        assert_eq!(config.validate().unwrap_err().len(), 5);
    }

    #[test]
    fn geocoder_urls_and_attribution_cannot_carry_credentials_or_controls() {
        for endpoint in [
            "https://private:secret@example.com/",
            "https://example.com/?token=secret",
            "https://example.com/#secret",
        ] {
            let mut config = CoreConfig::default();
            config.geocoder.base_url = Some(Url::parse(endpoint).unwrap());
            config.geocoder.attribution = Some("private\ncredit".to_owned());
            let errors = config.validate().unwrap_err();
            assert_eq!(errors.len(), 2);
            assert!(!errors.join(" ").contains("secret"));
        }
    }

    #[test]
    fn reports_every_problem_at_once() {
        let mut c = CoreConfig::default();
        c.providers.usgs.base_url = Url::parse("http://example.com/feed/").unwrap();
        c.providers.usgs.summary = "everything".into();
        c.providers.celestrak.cadence_s = 60;
        c.providers.celestrak.groups = vec!["Bad Group".into()];
        let errors = c.validate().unwrap_err();
        assert_eq!(errors.len(), 4, "{errors:#?}");
    }

    #[test]
    fn http_is_allowed_only_for_listed_hosts() {
        let mut c = CoreConfig::default();
        c.providers.usgs.base_url = Url::parse("http://127.0.0.1:9/feed/").unwrap();
        assert!(c.validate().is_err());
        c.fetch.allow_http_hosts = vec!["127.0.0.1".into()];
        c.validate().unwrap();
    }

    #[test]
    fn body_cap_is_bounded_by_the_ceiling() {
        let mut c = CoreConfig::default();
        c.fetch.max_body_bytes = crate::fetch::MAX_BODY_CEILING + 1;
        assert_eq!(c.validate().unwrap_err().len(), 1);
    }

    #[test]
    fn wind_spacing_must_be_a_number_between_a_quarter_degree_and_180() {
        for valid in [0.25, 1.0, 180.0] {
            let mut c = CoreConfig::default();
            c.providers.wind.target_dx = valid;
            c.validate().unwrap();
        }
        for invalid in [f64::NAN, f64::INFINITY, 0.2, 181.0] {
            let mut c = CoreConfig::default();
            c.providers.wind.target_dx = invalid;
            let errors = c.validate().unwrap_err();
            assert!(errors.iter().any(|e| e.contains("target_dx")), "{invalid}: {errors:?}");
        }
    }

    #[test]
    fn aisstream_requires_a_secure_bounded_configuration() {
        let mut c = CoreConfig::default();
        assert_eq!(c.providers.aisstream.url.as_str(), "wss://stream.aisstream.io/v0/stream");
        c.providers.aisstream.url =
            Url::parse("ws://user:secret@stream.example/v0?key=secret").unwrap();
        c.providers.aisstream.bounding_boxes.clear();
        c.providers.aisstream.max_vessels = 0;
        c.providers.aisstream.reconnect_min_ms = 10;
        c.providers.aisstream.idle_disconnect_s = 5;
        let errors = c.validate().unwrap_err();
        assert_eq!(errors.len(), 6, "{errors:#?}");
        assert!(errors.iter().all(|e| !e.contains("secret")));
        let mut c = CoreConfig::default();
        c.providers.aisstream.url = Url::parse("ws://127.0.0.1:9/v0/stream").unwrap();
        c.fetch.allow_http_hosts = vec!["127.0.0.1".into()];
        c.validate().unwrap();
    }

    #[test]
    fn perimeter_endpoints_are_fixed_resources_with_bounded_inciweb_use() {
        let mut config = CoreConfig::default();
        assert_eq!(config.providers.perimeters.cadence_s, 300);
        let p = &mut config.providers.perimeters;
        p.cadence_s = 299;
        p.inciweb_index_ttl_s = 60;
        p.inciweb_pages_per_refresh = 9;
        p.wfigs_base_url =
            Url::parse("https://user:secret@services3.arcgis.com/layer/?token=secret").unwrap();
        p.inciweb_base_url = Url::parse("http://inciweb.wildfire.gov/").unwrap();
        let errors = config.validate().unwrap_err();
        assert_eq!(errors.len(), 5, "{errors:#?}");
        assert!(errors.iter().all(|e| !e.contains("secret")));
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let parsed: Result<CoreConfig, _> = serde_json::from_str(r#"{"licenceMod":"commercial"}"#);
        assert!(parsed.is_err());
    }
}

/// The synthetic aircraft simulation (LB-151): a deterministic seeded fleet
/// flown by `crate::simulation::AircraftSimulator`, served as the aircraft
/// feed's source instead of adsb.lol when enabled. Never the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationConfig {
    /// Whether the aircraft feed runs on simulation instead of the live source.
    pub enabled: bool,
    /// Seed: the same seed always produces the same fleet and flight.
    pub seed: u64,
    /// How many simulated aircraft to fly.
    pub fleet_size: usize,
    /// Simulator (and hub refresh) step in seconds.
    pub step_s: u64,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self { enabled: false, seed: 42, fleet_size: 24, step_s: 2 }
    }
}
