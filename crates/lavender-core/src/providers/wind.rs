//! Global wind forecast provider (ADR 0008).
//!
//! Fetches NOAA GFS and ECMWF IFS cycle inventories, issues byte-range requests for
//! 10 m U/V wind and optional companion scalars (temperature at 2 m AGL, pressure MSL),
//! decodes the GRIB2 messages in pure Rust after checking each is the field its inventory
//! entry named ([`decode_forecast_field`]; supported templates and the fallback when an
//! upstream changes packing: [`SUPPORTED_DATA_TEMPLATES`]; memory caps: [`MAX_MESSAGE_BYTES`],
//! [`MAX_GRID_POINTS`], [`MAX_OUTPUT_POINTS`]) and resamples them to a compact equirectangular
//! grid served as little-endian f32 ([`lavender_domain::WindGridDef`]: byte layout).
//!
//! Every upstream attempt is recorded in the shared health tracker under the model's
//! source (`noaa-gfs`, `ecmwf-ifs`); cache hits are not attempts.

mod grib;
mod grid;
mod inventory;

pub use grib::{
    DecodedMessage, ForecastField, GribField, GribHeader, MAX_GRID_POINTS, MAX_MESSAGE_BYTES,
    SUPPORTED_DATA_TEMPLATES, decode_forecast_field, decode_grib_message, read_grib_header,
};
pub use grid::{MAX_OUTPUT_POINTS, ResampledGrid, pack_grid_binary, resample_wind_grid};
pub use inventory::{
    ByteRange, GfsIndexEntry, IfsIndexEntry, WindMessageRanges, gfs_urls, gfs_wind_ranges,
    ifs_urls, ifs_wind_ranges, nearest_gfs_step, nearest_ifs_step, parse_gfs_idx, parse_ifs_index,
    select_latest_gfs_cycle, select_latest_ifs_cycle,
};

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lavender_domain::{
    ApiError, ErrorCode, WeatherOverlay, WeatherScalarMetadata, WindCycle, WindGridDef,
    WindManifest, WindModel,
};
use tokio::sync::Mutex as AsyncMutex;
use url::Url;

use crate::config::WindConfig;
use crate::error::CoreError;
use crate::fetch::{FetchOptions, Fetcher, HostPolicy};
use crate::health::HealthTracker;
use crate::providers::ProviderError;
use crate::registry::Registry;
use crate::sources;

/// Scalar error message when temperature is unavailable.
pub const TEMPERATURE_UNAVAILABLE: &str = "Temperature field unavailable";
/// Scalar error message when pressure is unavailable.
pub const PRESSURE_UNAVAILABLE: &str = "Mean sea level pressure field unavailable";
/// Manifest reason while no current forecast could be fetched.
pub const UPSTREAM_UNAVAILABLE: &str = "Wind upstream unavailable";

const INVENTORY_MAX_BYTES: usize = 2 * 1024 * 1024;
const GFS_PUBLICATION_LAG_MS: i64 = 5 * 3_600_000;
const IFS_PUBLICATION_LAG_MS: i64 = 6 * 3_600_000;
/// Grid generations kept per variant, so a client holding the previous manifest can still
/// fetch its grid after a refresh.
const RETAINED_GENERATIONS: usize = 2;

/// The registered source (`sources.rs`) that supplies a model.
#[must_use]
pub const fn source_id(model: WindModel) -> &'static str {
    match model {
        WindModel::Gfs => sources::NOAA_GFS,
        WindModel::Ifs => sources::ECMWF_IFS,
    }
}

/// Whether a registered source is one of the wind models.
#[must_use]
pub fn is_wind_source(id: &str) -> bool {
    id == sources::NOAA_GFS || id == sources::ECMWF_IFS
}

/// The licence gate (ADR 0008) for a model's source.
///
/// # Errors
/// Returns a `disabledByLicence` error when the registry's licence policy does not permit
/// the model's source.
pub fn licence_gate(registry: &Registry, model: WindModel) -> Result<(), CoreError> {
    let source = source_id(model);
    if registry.licence_permits(source) {
        return Ok(());
    }
    let name = registry.source(source).map_or(source, |def| def.info.name.as_str());
    Err(CoreError::Api(ApiError::new(
        ErrorCode::DisabledByLicence,
        format!("{name} wind forecasts are disabled by the commercial licence policy."),
    )))
}

/// Metadata for companion weather scalar fields.
#[must_use]
pub fn weather_scalar_metadata(overlay: WeatherOverlay) -> WeatherScalarMetadata {
    match overlay {
        WeatherOverlay::Temperature => WeatherScalarMetadata {
            kind: overlay,
            units: "°C".into(),
            level: "2 m above ground".into(),
        },
        WeatherOverlay::Pressure => WeatherScalarMetadata {
            kind: overlay,
            units: "hPa".into(),
            level: "mean sea level".into(),
        },
        WeatherOverlay::None => {
            WeatherScalarMetadata { kind: overlay, units: String::new(), level: String::new() }
        }
    }
}

/// Fallback error string for a missing scalar.
#[must_use]
pub fn weather_scalar_error(overlay: WeatherOverlay) -> String {
    match overlay {
        WeatherOverlay::Temperature => TEMPERATURE_UNAVAILABLE.to_string(),
        WeatherOverlay::Pressure => PRESSURE_UNAVAILABLE.to_string(),
        WeatherOverlay::None => String::new(),
    }
}

fn grid_resolution_label(spacing: f64) -> String {
    if (spacing - spacing.round()).abs() < 1e-4 {
        format!("{spacing:.0}")
    } else {
        format!("{spacing}")
    }
}

/// The model run, step and objects one fetch uses.
struct UpstreamPlan {
    model: WindModel,
    date: String,
    hour: u32,
    forecast_hour: u32,
    run: jiff::Timestamp,
    object_url: Url,
    inventory_url: Url,
    policy: HostPolicy,
}

/// A freshly built forecast variant.
struct FetchedVariant {
    manifest: WindManifest,
    grid: Vec<u8>,
    id: String,
}

#[derive(Debug, Clone)]
struct CachedVariant {
    manifest: WindManifest,
    history: VecDeque<(String, Vec<u8>)>,
    fetched_at_ms: i64,
    last_attempt_ms: i64,
}

type VariantKey = (WindModel, WeatherOverlay);
type VariantGates = Mutex<HashMap<VariantKey, Arc<AsyncMutex<()>>>>;
/// Epoch milliseconds; the wall clock unless a test replaces it.
type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Global wind forecast provider managing upstream fetching, decoding, caching, and generation retention.
pub struct WindProvider {
    state: Arc<SharedState>,
    clock: Clock,
}

/// What refreshes share with the provider. Each refresh runs in its own task holding this,
/// so a caller that gives up (a client disconnect, or the server's request timeout, which
/// is shorter than the wind budget by default) neither cancels the fetch nor loses its
/// result and health record.
struct SharedState {
    fetcher: Arc<Fetcher>,
    config: WindConfig,
    health: Arc<HealthTracker>,
    cache: Mutex<HashMap<VariantKey, CachedVariant>>,
    gates: VariantGates,
}

impl std::fmt::Debug for WindProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindProvider").field("config", &self.state.config).finish_non_exhaustive()
    }
}

impl WindProvider {
    /// A provider that records upstream outcomes in `health` (the core's shared tracker).
    #[must_use]
    pub fn new(fetcher: Arc<Fetcher>, config: WindConfig, health: Arc<HealthTracker>) -> Self {
        Self {
            state: Arc::new(SharedState {
                fetcher,
                config,
                health,
                cache: Mutex::new(HashMap::new()),
                gates: Mutex::new(HashMap::new()),
            }),
            clock: Arc::new(|| jiff::Timestamp::now().as_millisecond()),
        }
    }

    /// Replaces the wall clock (epoch milliseconds) that selects cycles; for tests.
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn Fn() -> i64 + Send + Sync>) -> Self {
        self.clock = clock;
        self
    }

    /// Whether the operator enabled wind forecasts (`providers.wind.enabled`).
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.state.config.enabled
    }

    /// Refuses every call while wind forecasts are disabled, before any network access.
    ///
    /// # Errors
    /// Returns an `unconfigured` error when `providers.wind.enabled` is false.
    pub fn ensure_enabled(&self) -> Result<(), CoreError> {
        if self.state.config.enabled {
            Ok(())
        } else {
            Err(CoreError::Api(ApiError::new(
                ErrorCode::Unconfigured,
                "Wind forecasts are disabled in provider settings.",
            )))
        }
    }

    /// Retrieve the current manifest for a model and overlay variant.
    ///
    /// Upstream failures are not errors: the last good generation is served marked stale,
    /// or an `unavailable` manifest when none exists, and the failure goes to health.
    ///
    /// # Errors
    /// Returns [`CoreError`] if the provider is disabled.
    pub async fn manifest(
        &self,
        model: WindModel,
        overlay: WeatherOverlay,
    ) -> Result<WindManifest, CoreError> {
        self.ensure_enabled()?;
        let key = (model, overlay);
        if let Some(manifest) = self.state.cached_manifest(key, (self.clock)()) {
            return Ok(manifest);
        }
        // Dropping this future stops only the waiting, not the refresh (see `SharedState`).
        let state = Arc::clone(&self.state);
        let clock = Arc::clone(&self.clock);
        tokio::spawn(async move { state.refresh(key, &clock).await })
            .await
            .map_err(|_| CoreError::Upstream("The wind refresh was interrupted.".into()))
    }

    /// Retrieve raw binary grid bytes by grid ID.
    ///
    /// # Errors
    /// Returns [`CoreError::NotFound`] if the grid ID is not in retained history.
    pub async fn grid(
        &self,
        model: WindModel,
        overlay: WeatherOverlay,
        id: &str,
    ) -> Result<Vec<u8>, CoreError> {
        self.ensure_enabled()?;
        let clean_id = id.strip_suffix(".bin").unwrap_or(id);
        let key = (model, overlay);
        if let Some(bytes) = self.state.retained_grid(key, clean_id) {
            return Ok(bytes);
        }
        // A grid from a newer generation than this process has seen: refresh once.
        self.manifest(model, overlay).await?;
        self.state
            .retained_grid(key, clean_id)
            .ok_or_else(|| CoreError::NotFound("unknown_grid".into()))
    }
}

impl SharedState {
    fn gate_for(&self, key: VariantKey) -> Arc<AsyncMutex<()>> {
        let mut map = self.gates.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        map.entry(key).or_insert_with(|| Arc::new(AsyncMutex::new(()))).clone()
    }

    fn lock_cache(&self) -> std::sync::MutexGuard<'_, HashMap<VariantKey, CachedVariant>> {
        self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The cached manifest while it is within its TTL or failure backoff.
    fn cached_manifest(&self, key: VariantKey, now_ms: i64) -> Option<WindManifest> {
        let ttl_ms = i64::try_from(self.config.ttl_s.saturating_mul(1000)).unwrap_or(i64::MAX);
        let backoff_ms =
            i64::try_from(self.config.failure_backoff_s.saturating_mul(1000)).unwrap_or(i64::MAX);
        let cache = self.lock_cache();
        cache
            .get(&key)
            .filter(|cv| {
                now_ms.saturating_sub(cv.fetched_at_ms) < ttl_ms
                    || now_ms.saturating_sub(cv.last_attempt_ms) < backoff_ms
            })
            .map(|cv| cv.manifest.clone())
    }

    fn retained_grid(&self, key: VariantKey, id: &str) -> Option<Vec<u8>> {
        let cache = self.lock_cache();
        cache
            .get(&key)
            .and_then(|cv| cv.history.iter().find(|(known, _)| known == id))
            .map(|(_, bytes)| bytes.clone())
    }

    /// One upstream refresh per variant at a time; waiters reuse its result.
    async fn refresh(&self, key: VariantKey, clock: &Clock) -> WindManifest {
        let gate = self.gate_for(key);
        let _guard = gate.lock().await;
        let now_ms = clock();
        if let Some(manifest) = self.cached_manifest(key, now_ms) {
            return manifest;
        }
        let (model, overlay) = key;
        let started = Instant::now();
        let outcome = tokio::time::timeout(
            Duration::from_secs(self.config.timeout_s),
            self.fetch_variant(model, overlay, now_ms),
        )
        .await
        .unwrap_or_else(|_| Err(ProviderError::Unavailable("Wind upstream timed out.".into())));
        let source = source_id(model);
        match outcome {
            Ok(variant) => {
                match &variant.manifest.scalar_error {
                    Some(reason) => self.health.record_degraded(source, started.elapsed(), reason),
                    None => self.health.record_success(source, started.elapsed()),
                }
                self.store_success(key, variant, now_ms)
            }
            Err(error) => {
                let message = error.public_message();
                tracing::warn!(source, error = %message, "wind fetch failed");
                self.health.record_failure(source, &message);
                self.store_failure(key, now_ms)
            }
        }
    }

    fn store_success(&self, key: VariantKey, variant: FetchedVariant, now_ms: i64) -> WindManifest {
        let FetchedVariant { manifest, grid, id } = variant;
        let mut cache = self.lock_cache();
        let cached = cache.entry(key).or_insert_with(|| CachedVariant {
            manifest: manifest.clone(),
            history: VecDeque::new(),
            fetched_at_ms: now_ms,
            last_attempt_ms: now_ms,
        });
        cached.manifest = manifest.clone();
        cached.fetched_at_ms = now_ms;
        cached.last_attempt_ms = now_ms;
        cached.history.retain(|(known, _)| known != &id);
        cached.history.push_back((id, grid));
        while cached.history.len() > RETAINED_GENERATIONS {
            cached.history.pop_front();
        }
        manifest
    }

    fn store_failure(&self, key: VariantKey, now_ms: i64) -> WindManifest {
        let mut cache = self.lock_cache();
        let cached = cache.entry(key).or_insert_with(|| CachedVariant {
            manifest: Self::unavailable_manifest(key.0, key.1),
            history: VecDeque::new(),
            fetched_at_ms: 0,
            last_attempt_ms: now_ms,
        });
        cached.last_attempt_ms = now_ms;
        cached.manifest.stale = true;
        cached.manifest.reason = Some(UPSTREAM_UNAVAILABLE.into());
        cached.manifest.clone()
    }

    fn unavailable_manifest(model: WindModel, overlay: WeatherOverlay) -> WindManifest {
        WindManifest {
            schema_version: 1,
            model,
            cycle: None,
            overlay: if overlay == WeatherOverlay::None { None } else { Some(overlay) },
            scalar: None,
            scalar_error: None,
            fetched_at: None,
            level: None,
            units: None,
            grid: None,
            stale: true,
            unavailable: true,
            reason: Some(UPSTREAM_UNAVAILABLE.into()),
            grid_url: None,
            grid_id: None,
            attribution: Some(attribution(model).to_owned()),
        }
    }

    fn plan(&self, model: WindModel, now_ms: i64) -> Result<UpstreamPlan, ProviderError> {
        let source = source_id(model);
        let (date, hour) = match model {
            WindModel::Gfs => select_latest_gfs_cycle(now_ms, GFS_PUBLICATION_LAG_MS),
            WindModel::Ifs => select_latest_ifs_cycle(now_ms, IFS_PUBLICATION_LAG_MS),
        };
        let run = inventory::cycle_timestamp(&date, hour, source)?;
        let hours = inventory::elapsed_hours(now_ms, run.as_millisecond());
        let (forecast_hour, (object_url, inventory_url), base) = match model {
            WindModel::Gfs => {
                let step = nearest_gfs_step(hours);
                let base = &self.config.gfs_base_url;
                (step, gfs_urls(base, &date, hour, step), base)
            }
            WindModel::Ifs => {
                let step = nearest_ifs_step(hours, hour);
                let base = &self.config.ifs_base_url;
                (step, ifs_urls(base, &date, hour, step), base)
            }
        };
        let host = base.host_str().ok_or_else(|| {
            ProviderError::Unconfigured(format!("{source}: base URL has no host"))
        })?;
        let policy = match model {
            // NOAA's bucket may answer through S3's shared endpoints.
            WindModel::Gfs => {
                HostPolicy::new(&[host, "s3.amazonaws.com", "noaa-gfs-bdp-pds.s3.amazonaws.com"])
            }
            WindModel::Ifs => HostPolicy::new(&[host]),
        };
        Ok(UpstreamPlan {
            model,
            date,
            hour,
            forecast_hour,
            run,
            object_url,
            inventory_url,
            policy,
        })
    }

    async fn fetch_ranges(
        &self,
        plan: &UpstreamPlan,
        overlay: WeatherOverlay,
    ) -> Result<WindMessageRanges, ProviderError> {
        let options = FetchOptions {
            max_body_bytes: Some(INVENTORY_MAX_BYTES),
            accept: "text/plain",
            ..FetchOptions::default()
        };
        let response = self.fetcher.get(&plan.inventory_url, &plan.policy, &options).await?;
        let text = String::from_utf8_lossy(&response.body);
        match plan.model {
            WindModel::Gfs => gfs_wind_ranges(&parse_gfs_idx(&text)?, overlay),
            WindModel::Ifs => ifs_wind_ranges(&parse_ifs_index(&text)?, overlay),
        }
    }

    async fn fetch_range(
        &self,
        url: &Url,
        policy: &HostPolicy,
        range: &ByteRange,
    ) -> Result<Vec<u8>, ProviderError> {
        let expected_len = range
            .end
            .checked_sub(range.start)
            .and_then(|span| usize::try_from(span).ok())
            .and_then(|span| span.checked_add(1))
            .filter(|len| *len <= MAX_MESSAGE_BYTES)
            .ok_or_else(|| ProviderError::Parse {
                source_name: "wind",
                reason: format!(
                    "message range {}-{} exceeds the {} MiB limit",
                    range.start,
                    range.end,
                    MAX_MESSAGE_BYTES / (1024 * 1024)
                ),
            })?;
        let opts = FetchOptions {
            range: Some((range.start, range.end)),
            max_body_bytes: Some(MAX_MESSAGE_BYTES),
            accept: "application/octet-stream",
            ..FetchOptions::default()
        };
        let resp = self.fetcher.get(url, policy, &opts).await?;
        if resp.body.len() != expected_len {
            return Err(ProviderError::Parse {
                source_name: "wind",
                reason: format!(
                    "invalid range length: expected {expected_len}, got {}",
                    resp.body.len()
                ),
            });
        }
        Ok(resp.body.to_vec())
    }

    /// The optional scalar never fails the wind: errors and its own timeout yield `None`.
    async fn fetch_scalar(
        &self,
        url: &Url,
        policy: &HostPolicy,
        range: Option<&ByteRange>,
    ) -> Option<Vec<u8>> {
        let range = range?;
        let budget = Duration::from_secs(self.config.scalar_timeout_s);
        match tokio::time::timeout(budget, self.fetch_range(url, policy, range)).await {
            Ok(Ok(bytes)) => Some(bytes),
            Ok(Err(error)) => {
                tracing::warn!(error = %error.public_message(), "optional scalar fetch failed");
                None
            }
            Err(_) => {
                tracing::warn!("optional scalar fetch timed out");
                None
            }
        }
    }

    async fn fetch_variant(
        &self,
        model: WindModel,
        overlay: WeatherOverlay,
        now_ms: i64,
    ) -> Result<FetchedVariant, ProviderError> {
        let plan = self.plan(model, now_ms)?;
        let ranges = self.fetch_ranges(&plan, overlay).await?;
        let (u, v, scalar) = tokio::join!(
            self.fetch_range(&plan.object_url, &plan.policy, &ranges.u),
            self.fetch_range(&plan.object_url, &plan.policy, &ranges.v),
            self.fetch_scalar(&plan.object_url, &plan.policy, ranges.scalar.as_ref()),
        );
        let (u, v) = (u?, v?);
        let dx = self.config.target_dx;
        // Decoding a 0.25° field is CPU work; keep it off the async executor.
        let resampled = tokio::task::spawn_blocking(move || {
            decode_and_resample(&u, &v, scalar.as_deref(), overlay, dx)
        })
        .await
        .map_err(|_| ProviderError::Unavailable("Wind decoding was interrupted.".into()))??;
        Ok(build_variant(&plan, overlay, &resampled, now_ms))
    }
}

/// Decodes the wind (required) and the scalar (optional: a failure is logged and the grid
/// is served without it), each checked to be the field its inventory entry promised, then
/// resamples them.
fn decode_and_resample(
    u: &[u8],
    v: &[u8],
    scalar: Option<&[u8]>,
    overlay: WeatherOverlay,
    dx: f64,
) -> Result<ResampledGrid, ProviderError> {
    let u = decode_forecast_field(u, ForecastField::EastwardWind10m)?;
    let v = decode_forecast_field(v, ForecastField::NorthwardWind10m)?;
    let scalar = scalar.zip(scalar_field(overlay)).and_then(|(bytes, field)| {
        decode_forecast_field(bytes, field)
            .inspect_err(|error| {
                tracing::warn!(error = %error.public_message(), "optional scalar decode failed");
            })
            .ok()
    });
    resample_wind_grid(&u, &v, scalar.as_ref(), overlay, dx, dx)
}

/// The companion field an overlay shows.
const fn scalar_field(overlay: WeatherOverlay) -> Option<ForecastField> {
    match overlay {
        WeatherOverlay::Temperature => Some(ForecastField::Temperature2m),
        WeatherOverlay::Pressure => Some(ForecastField::PressureMsl),
        WeatherOverlay::None => None,
    }
}

/// The attribution every manifest of a model carries (the source's mandated text).
fn attribution(model: WindModel) -> &'static str {
    match model {
        WindModel::Gfs => sources::NOAA_GFS_ATTRIBUTION,
        WindModel::Ifs => sources::ECMWF_IFS_ATTRIBUTION,
    }
}

fn build_variant(
    plan: &UpstreamPlan,
    overlay: WeatherOverlay,
    resampled: &ResampledGrid,
    now_ms: i64,
) -> FetchedVariant {
    let missing_scalar = overlay != WeatherOverlay::None && resampled.scalar.is_none();
    let suffix = match (overlay, missing_scalar) {
        (WeatherOverlay::None, _) => String::new(),
        (_, false) => format!("-{}", overlay.as_str()),
        (_, true) => format!("-{}-wind-only", overlay.as_str()),
    };
    let model = plan.model.as_str();
    let id = format!(
        "{model}-{}-{}-f{}-{}{suffix}",
        plan.date,
        plan.hour,
        plan.forecast_hour,
        grid_resolution_label(resampled.dx)
    );
    let query = match overlay {
        WeatherOverlay::None => format!("model={model}"),
        _ => format!("model={model}&overlay={}", overlay.as_str()),
    };
    let utc = |ts: jiff::Timestamp| ts.to_zoned(jiff::tz::TimeZone::UTC).to_string();
    let valid = jiff::Timestamp::from_millisecond(
        plan.run.as_millisecond() + i64::from(plan.forecast_hour) * 3_600_000,
    )
    .unwrap_or(plan.run);
    let has_overlay = overlay != WeatherOverlay::None;
    let manifest = WindManifest {
        schema_version: 1,
        model: plan.model,
        cycle: Some(WindCycle {
            date: plan.date.clone(),
            hour: plan.hour,
            forecast_hour: plan.forecast_hour,
            run_iso: Some(utc(plan.run)),
            valid_iso: Some(utc(valid)),
        }),
        overlay: has_overlay.then_some(overlay),
        scalar: (has_overlay && !missing_scalar).then(|| weather_scalar_metadata(overlay)),
        scalar_error: missing_scalar.then(|| weather_scalar_error(overlay)),
        fetched_at: u64::try_from(now_ms).ok(),
        level: Some("10 m above ground".into()),
        units: Some("m/s".into()),
        grid: Some(WindGridDef {
            nx: resampled.nx,
            ny: resampled.ny,
            lo1: resampled.lo1,
            la1: resampled.la1,
            dx: resampled.dx,
            dy: resampled.dy,
        }),
        stale: false,
        unavailable: false,
        reason: None,
        grid_url: Some(format!("/api/v1/wind/grid/{id}.bin?{query}")),
        grid_id: Some(id.clone()),
        attribution: Some(attribution(plan.model).to_owned()),
    };
    FetchedVariant { manifest, grid: pack_grid_binary(resampled), id }
}
