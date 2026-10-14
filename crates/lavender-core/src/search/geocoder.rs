//! Operator-configured place lookup. All traffic goes through the shared Fetcher;
//! response caching, duplicate requests and process-wide host pacing are shared here.

mod parsing;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use lavender_domain::{CommercialUse, Licence, PlaceHit, Privacy, SearchQuery, SourceInfo};
use moka::future::Cache;
use tokio::sync::{Mutex as AsyncMutex, MutexGuard, Semaphore};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::config::{GeocoderConfig, GeocoderFormat};
use crate::error::CoreError;
use crate::fetch::{FetchError, FetchOptions, Fetcher, HostPolicy};

const MAX_BODY_BYTES: usize = 1024 * 1024;
const OSM_CREDIT: &str = "© OpenStreetMap contributors (ODbL 1.0)";
const OSM_COPYRIGHT: &str = "https://www.openstreetmap.org/copyright";

type HostGate = Arc<AsyncMutex<Option<Instant>>>;

// Trusted configuration supplies these host keys, never user queries. Retain the
// send history even when a core is rebuilt so a second core cannot reset the quota.
static HOST_GATES: OnceLock<Mutex<HashMap<String, HostGate>>> = OnceLock::new();

fn host_gate(base: &Url) -> HostGate {
    let mut gates = HOST_GATES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    gates
        .entry(base.host_str().unwrap_or_default().to_ascii_lowercase())
        .or_insert_with(|| Arc::new(AsyncMutex::new(None)))
        .clone()
}

/// Records completion on drop too, including cancellation during a Fetcher request.
/// Spacing from completion is conservative and also covers followed redirects.
struct RequestLease<'a> {
    next_request: MutexGuard<'a, Option<Instant>>,
    interval: Duration,
}

impl Drop for RequestLease<'_> {
    fn drop(&mut self) {
        *self.next_request = Some(Instant::now() + self.interval);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GeocoderError {
    #[error("Place search is not configured on this server.")]
    Disabled,
    #[error("Place search is busy; try again shortly.")]
    Busy,
    #[error("Place search was cancelled.")]
    Cancelled,
    #[error("Place search is temporarily unavailable.")]
    Unavailable,
    #[error("The place service returned an unreadable response.")]
    Malformed,
}

impl From<FetchError> for GeocoderError {
    fn from(error: FetchError) -> Self {
        match error {
            FetchError::Cancelled => Self::Cancelled,
            FetchError::Status { status: 420 | 429, .. } => Self::Busy,
            _ => Self::Unavailable,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GeocodedPlaces {
    pub places: Vec<PlaceHit>,
    pub notices: Vec<String>,
    fetched_at: Instant,
}

impl GeocodedPlaces {
    #[must_use]
    pub fn new(places: Vec<PlaceHit>, notices: Vec<String>) -> Self {
        Self { places, notices, fetched_at: Instant::now() }
    }
}

/// Replaceable place lookup; server and desktop use the same operation and credits.
#[async_trait]
pub trait Geocoder: Send + Sync + std::fmt::Debug {
    fn source_info(&self) -> SourceInfo;

    /// # Errors
    /// Returns a safe failure category, without query text, URLs or upstream bodies.
    async fn geocode(
        &self,
        query: &SearchQuery,
        cancel: CancellationToken,
    ) -> Result<GeocodedPlaces, GeocoderError>;
}

#[derive(Debug)]
pub struct ConfiguredGeocoder {
    config: GeocoderConfig,
    fetcher: Arc<Fetcher>,
    cache: Cache<String, GeocodedPlaces>,
    pending: Semaphore,
    gate: Option<HostGate>,
}

impl ConfiguredGeocoder {
    /// The application validates endpoint transport policy in `CoreConfig::validate`.
    /// Disabled configurations never contact an upstream, even if a URL is supplied.
    ///
    /// # Errors
    /// Returns invalid configuration for missing enabled endpoints or unsafe limits.
    pub fn new(config: GeocoderConfig, fetcher: Arc<Fetcher>) -> Result<Self, CoreError> {
        if !config.validation_errors().is_empty() {
            return Err(CoreError::BadRequest("invalid place geocoder configuration".to_owned()));
        }
        let gate = config.base_url.as_ref().map(host_gate);
        Ok(Self {
            cache: Cache::builder()
                .max_capacity(config.cache_max_entries)
                .time_to_live(Duration::from_secs(config.cache_ttl_s))
                .build(),
            pending: Semaphore::new(config.max_pending),
            gate,
            fetcher,
            config,
        })
    }

    fn source_id(&self) -> &'static str {
        match self.config.format {
            GeocoderFormat::Nominatim => "osm-geocoder-nominatim",
            GeocoderFormat::Photon => "osm-geocoder-photon",
        }
    }

    fn url(&self, query: &SearchQuery) -> Result<Url, GeocoderError> {
        let base = self.config.base_url.as_ref().ok_or(GeocoderError::Disabled)?;
        let endpoint = match self.config.format {
            GeocoderFormat::Nominatim => "search",
            GeocoderFormat::Photon => "api/",
        };
        let mut url = base.join(endpoint).map_err(|_| GeocoderError::Unavailable)?;
        let limit = query.limit.unwrap_or(8).clamp(1, 25);
        {
            let mut params = url.query_pairs_mut();
            params.append_pair("q", query.q.trim());
            params.append_pair("limit", &limit.to_string());
            match self.config.format {
                GeocoderFormat::Nominatim => {
                    params.append_pair("format", "jsonv2");
                    params.append_pair("addressdetails", "1");
                    if let Some(near) = query.near {
                        // A viewbox boosts nearby matches; bounded=0 keeps worldwide
                        // results eligible. Clip at poles and the antimeridian.
                        let west = (near.lon - 1.0).max(-180.0);
                        let east = (near.lon + 1.0).min(180.0);
                        let south = (near.lat - 1.0).max(-90.0);
                        let north = (near.lat + 1.0).min(90.0);
                        params.append_pair("viewbox", &format!("{west},{north},{east},{south}"));
                        params.append_pair("bounded", "0");
                    }
                }
                GeocoderFormat::Photon => {
                    if let Some(near) = query.near {
                        params.append_pair("lat", &near.lat.to_string());
                        params.append_pair("lon", &near.lon.to_string());
                    }
                }
            }
        }
        Ok(url)
    }

    async fn fetch(
        &self,
        url: &Url,
        limit: usize,
        cancel: CancellationToken,
    ) -> Result<GeocodedPlaces, GeocoderError> {
        let _pending = self.pending.try_acquire().map_err(|_| GeocoderError::Busy)?;
        let gate = self.gate.as_ref().ok_or(GeocoderError::Disabled)?;
        let interval = Duration::from_millis(self.config.min_request_interval_ms);
        let mut guard = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(GeocoderError::Cancelled),
            guard = gate.lock() => guard,
        };
        if let Some(next_request) = *guard {
            tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(GeocoderError::Cancelled),
                () = tokio::time::sleep_until(tokio::time::Instant::from_std(next_request)) => {}
            }
        }
        if cancel.is_cancelled() {
            return Err(GeocoderError::Cancelled);
        }
        *guard = Some(Instant::now() + interval);
        let _lease = RequestLease { next_request: guard, interval };
        let response = self
            .fetcher
            .get(
                url,
                &HostPolicy::for_url(url).with_min_interval(interval),
                &FetchOptions {
                    max_body_bytes: Some(MAX_BODY_BYTES),
                    cancel: Some(cancel),
                    // Interactive searches do not turn a quota/error response into
                    // repeated queued requests. The analyst may retry explicitly.
                    retries: Some(0),
                    ..FetchOptions::default()
                },
            )
            .await?;
        parsing::parse(&response.body, self.config.format, self.source_id(), limit)
    }
}

#[async_trait]
impl Geocoder for ConfiguredGeocoder {
    fn source_info(&self) -> SourceInfo {
        let service = match self.config.format {
            GeocoderFormat::Nominatim => "Nominatim-compatible place service",
            GeocoderFormat::Photon => "Photon-compatible place service",
        };
        let attribution = self
            .config
            .attribution
            .as_ref()
            .map_or_else(|| OSM_CREDIT.to_owned(), |credit| format!("{credit} · {OSM_CREDIT}"));
        SourceInfo {
            id: self.source_id().to_owned(),
            name: service.to_owned(),
            homepage: self.config.base_url.as_ref().map_or_else(
                || OSM_COPYRIGHT.to_owned(),
                |url| url.as_str().to_owned(),
            ),
            attribution,
            licence: Licence {
                name: "Open Database License 1.0".to_owned(),
                url: "https://opendatacommons.org/licenses/odbl/1-0/".to_owned(),
            },
            // The data licence does not establish commercial service permission.
            commercial: CommercialUse::Unknown,
            requires_key: false,
            privacy: Privacy::Normal,
            notes: Some("Configured place queries and optional camera-centre bias are sent to this service. The operator must clear its usage terms (ENG-938); public Nominatim forbids autocomplete.".to_owned()),
        }
    }

    async fn geocode(
        &self,
        query: &SearchQuery,
        cancel: CancellationToken,
    ) -> Result<GeocodedPlaces, GeocoderError> {
        if cancel.is_cancelled() {
            return Err(GeocoderError::Cancelled);
        }
        if !self.config.enabled {
            return Err(GeocoderError::Disabled);
        }
        let url = self.url(query)?;
        // The complete encoded request includes format, bias and limit. Different
        // limits or centres never borrow a cache entry with different semantics.
        let key = url.as_str().to_owned();
        let limit = usize::try_from(query.limit.unwrap_or(8).clamp(1, 25)).unwrap_or(8);
        let lookup = self.cache.try_get_with(key, self.fetch(&url, limit, cancel.clone()));
        let mut answer = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(GeocoderError::Cancelled),
            result = lookup => result.map_err(|error| (*error).clone())?,
        };
        // Moka performs capacity eviction asynchronously; finish that work before
        // returning so the configured count remains an observable bound.
        self.cache.run_pending_tasks().await;
        let age = answer.fetched_at.elapsed().as_secs();
        if age > 0 {
            answer.notices.push(format!("Place results were retrieved {age} seconds ago."));
        }
        Ok(answer)
    }
}
