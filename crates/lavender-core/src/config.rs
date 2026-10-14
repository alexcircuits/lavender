//! Typed core configuration. Adapters build it (server: TOML + `LAVENDER__*` env; desktop:
//! app config dir) and hand it in; the core never reads files or the environment for
//! configuration itself. [`CoreConfig::validate`] reports every problem at once.

use std::time::Duration;

use lavender_domain::LicenceMode;
use serde::Deserialize;
use url::Url;

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
    pub celestrak: CelestrakConfig,
    pub cyclones: CyclonesConfig,
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
            groups: ["stations", "visual", "weather", "gps-ops", "galileo", "science"]
                .map(str::to_owned)
                .to_vec(),
            cadence_s: 7_200,
        }
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
        if let Some(base) = &self.geocoder.base_url {
            https("geocoder.base_url", base);
        }
        errors.extend(self.geocoder.validation_errors());

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
    fn unknown_keys_are_rejected() {
        let parsed: Result<CoreConfig, _> = serde_json::from_str(r#"{"licenceMod":"commercial"}"#);
        assert!(parsed.is_err());
    }
}
