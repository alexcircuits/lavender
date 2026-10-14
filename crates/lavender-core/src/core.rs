//! The `LavenderCore` facade: the only API the HTTP server and the Tauri app call.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use lavender_domain::{
    AircraftTrack, Availability, Catalog, FeedId, FeedItems, FeedQuery, FeedSnapshot, FeedStatus,
    SatellitePassRequest, SatellitePasses, SatelliteTrack, SatelliteTrackRequest, SearchQuery,
    SearchResults, SystemStatus,
};
use lavender_geo::{OrbitError, Propagator};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::audit::{AuditEvent, AuditSink, NoAudit};
use crate::config::CoreConfig;
use crate::error::CoreError;
use crate::fetch::Fetcher;
use crate::health::HealthTracker;
use crate::hub::{FeedHub, FeedSubscription, HubDeps, RegionBudget, SubscriptionEvent};
use crate::providers::FeedProvider;
use crate::providers::adsb_lol::AdsbLolProvider;
use crate::providers::celestrak::CelestrakProvider;
use crate::providers::cyclones::CyclonesProvider;
use crate::providers::usgs::UsgsProvider;
use crate::registry::Registry;
use crate::search;
use crate::search::geocoder::{ConfiguredGeocoder, Geocoder};
use crate::secrets::{MemorySecretStore, SecretStore};
use crate::settings::{MemorySettingsStore, SettingsStore};

/// How far from an element set's epoch a requested track may start: SGP4 accuracy
/// degrades quickly beyond a couple of weeks.
const MAX_TRACK_EPOCH_OFFSET: SignedDuration = SignedDuration::from_hours(14 * 24);

#[derive(Debug)]
struct CoreInner {
    config: CoreConfig,
    registry: Registry,
    secrets: Arc<dyn SecretStore>,
    settings: Arc<dyn SettingsStore>,
    health: Arc<HealthTracker>,
    hub: FeedHub,
    audit: Arc<dyn AuditSink>,
    started_at: Timestamp,
    geocoder: Option<Arc<dyn Geocoder>>,
    cancel: CancellationToken,
    pass_capacity: Arc<Semaphore>,
}

/// Cheap to clone; all clones share one core.
///
/// Build one core per application and share its handle with the transport adapter.
/// Providers start fetching only when a snapshot or subscription needs them.
///
/// ```no_run
/// use lavender_core::{CoreConfig, LavenderCore};
/// use lavender_domain::{FeedId, FeedQuery};
///
/// # async fn example() -> Result<(), lavender_core::CoreError> {
/// let core = LavenderCore::builder(CoreConfig::default()).build()?;
/// let earthquakes = core.snapshot(FeedQuery::new(FeedId::Earthquakes)).await?;
/// println!("{:?}", earthquakes.freshness);
/// core.shutdown();
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct LavenderCore {
    inner: Arc<CoreInner>,
}

#[derive(Debug)]
pub struct LavenderCoreBuilder {
    config: CoreConfig,
    secrets: Arc<dyn SecretStore>,
    settings: Arc<dyn SettingsStore>,
    cache_dir: Option<PathBuf>,
    providers: Option<Vec<Arc<dyn FeedProvider>>>,
    audit: Arc<dyn AuditSink>,
}

impl LavenderCoreBuilder {
    /// Where audit events go (default: discarded).
    #[must_use]
    pub fn audit(mut self, audit: Arc<dyn AuditSink>) -> Self {
        self.audit = audit;
        self
    }

    #[must_use]
    pub fn secrets(mut self, secrets: Arc<dyn SecretStore>) -> Self {
        self.secrets = secrets;
        self
    }

    #[must_use]
    pub fn settings(mut self, settings: Arc<dyn SettingsStore>) -> Self {
        self.settings = settings;
        self
    }

    /// Where providers may cache upstream data (for example CelesTrak groups).
    #[must_use]
    pub fn cache_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(dir.into());
        self
    }

    /// Replaces the built-in providers (tests and specialised deployments).
    #[must_use]
    pub fn providers(mut self, providers: Vec<Arc<dyn FeedProvider>>) -> Self {
        self.providers = Some(providers);
        self
    }

    /// Validates the configuration and builds the core. Background work starts lazily,
    /// when the first subscription needs it.
    ///
    /// # Errors
    /// Returns every configuration problem at once, or a client initialisation failure.
    pub fn build(self) -> Result<LavenderCore, CoreError> {
        self.config.validate().map_err(|errors| {
            CoreError::BadRequest(format!("invalid configuration: {}", errors.join("; ")))
        })?;
        let fetcher = Arc::new(Fetcher::new(self.config.fetch.clone())?);
        let geocoder: Option<Arc<dyn Geocoder>> = if self.config.geocoder.enabled {
            Some(Arc::new(ConfiguredGeocoder::new(self.config.geocoder.clone(), fetcher.clone())?))
        } else {
            None
        };
        let providers = self.providers.unwrap_or_else(|| {
            let p = &self.config.providers;
            vec![
                Arc::new(AdsbLolProvider::new(p.adsb_lol.clone())) as Arc<dyn FeedProvider>,
                Arc::new(UsgsProvider::new(p.usgs.clone())),
                Arc::new(CelestrakProvider::new(p.celestrak.clone())),
                Arc::new(CyclonesProvider::new(p.cyclones.clone())),
            ]
        });
        let health = Arc::new(HealthTracker::default());
        let deps = HubDeps {
            fetcher,
            secrets: self.secrets.clone(),
            health: health.clone(),
            cache_dir: self.cache_dir,
        };
        let hub = FeedHub::new(providers, deps, self.config.hub.clone());
        let registry = Registry::new(self.config.licence_mode, &self.config.licence_acknowledged);
        Ok(LavenderCore {
            inner: Arc::new(CoreInner {
                config: self.config,
                registry,
                secrets: self.secrets,
                settings: self.settings,
                health,
                hub,
                audit: self.audit,
                started_at: Timestamp::now(),
                geocoder,
                cancel: CancellationToken::new(),
                pass_capacity: Arc::new(Semaphore::new(2)),
            }),
        })
    }
}

impl LavenderCore {
    #[must_use]
    pub fn builder(config: CoreConfig) -> LavenderCoreBuilder {
        LavenderCoreBuilder {
            config,
            secrets: Arc::new(MemorySecretStore::default()),
            settings: Arc::new(MemorySettingsStore::default()),
            cache_dir: None,
            providers: None,
            audit: Arc::new(NoAudit),
        }
    }

    fn feed_meta(&self, feed: FeedId) -> Option<(u32, lavender_domain::ProviderScope, bool)> {
        let p = self.inner.hub.provider(feed)?;
        Some((u32::try_from(p.cadence().as_secs()).unwrap_or(u32::MAX), p.scope(), p.enabled()))
    }

    fn availability(&self, feed: FeedId) -> Availability {
        let enabled = self.inner.hub.provider(feed).is_some_and(|p| p.enabled());
        self.inner.registry.availability(feed, self.inner.secrets.as_ref(), enabled)
    }

    /// What Lavender can show: layers with availability, sources with licences, imagery.
    #[must_use]
    pub fn catalog(&self) -> Catalog {
        let mut catalog =
            self.inner.registry.catalog(self.inner.secrets.as_ref(), |f| self.feed_meta(f));
        if let Some(geocoder) = &self.inner.geocoder {
            catalog.sources.push(geocoder.source_info());
        }
        catalog
    }

    /// Provider health and feed load.
    #[must_use]
    pub fn status(&self) -> SystemStatus {
        let registry = &self.inner.registry;
        let sources = registry
            .sources()
            .iter()
            .map(|s| self.inner.health.health(&s.id, registry.licence_permits(&s.id)))
            .collect();
        let feeds = FeedId::ALL
            .into_iter()
            .filter(|f| self.inner.hub.provider(*f).is_some())
            .map(|feed| {
                let (active_regions, subscribers) = self.inner.hub.load(feed);
                FeedStatus {
                    feed,
                    availability: self.availability(feed),
                    active_regions,
                    subscribers,
                }
            })
            .collect();
        SystemStatus {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            licence_mode: registry.mode(),
            started_at: self.inner.started_at,
            sources,
            feeds,
        }
    }

    fn check_available(&self, feed: FeedId) -> Result<(), CoreError> {
        if self.inner.hub.provider(feed).is_none() {
            return Err(CoreError::NotFound(format!("no provider serves the {feed} feed")));
        }
        match self.availability(feed) {
            Availability::Available => Ok(()),
            Availability::DisabledByLicence => Err(CoreError::DisabledByLicence { feed }),
            Availability::Unconfigured { missing } => {
                Err(CoreError::Unconfigured { feed, missing: missing.join(", ") })
            }
        }
    }

    /// Starts a live subscription (WebSocket on the web, a Tauri channel on desktop).
    ///
    /// # Errors
    /// Returns an error when the feed is unknown, unconfigured or disabled by licence.
    pub fn subscribe(&self, query: FeedQuery) -> Result<FeedSubscription, CoreError> {
        self.subscribe_inner(query, None)
    }

    /// Like [`Self::subscribe`], with the regions counted against a client's budget (the
    /// server shares one budget per client address).
    ///
    /// # Errors
    /// Returns an error when the feed is unknown, unconfigured or disabled by licence.
    pub fn subscribe_with_budget(
        &self,
        query: FeedQuery,
        budget: Arc<RegionBudget>,
    ) -> Result<FeedSubscription, CoreError> {
        self.subscribe_inner(query, Some(budget))
    }

    fn subscribe_inner(
        &self,
        query: FeedQuery,
        budget: Option<Arc<RegionBudget>>,
    ) -> Result<FeedSubscription, CoreError> {
        self.check_available(query.feed)?;
        self.inner.audit.record(AuditEvent::Subscribed { feed: query.feed });
        self.inner.hub.subscribe_with_budget(query, budget)
    }

    /// One consistent answer for a query, waiting for the first upstream result if needed.
    ///
    /// # Errors
    /// Returns an error when the feed is not available, the upstream fails before any data
    /// exists, the hub has no capacity left, or nothing arrives in time.
    pub async fn snapshot(&self, query: FeedQuery) -> Result<FeedSnapshot, CoreError> {
        self.snapshot_inner(query, None).await
    }

    /// Like [`Self::snapshot`], with the regions counted against a client's budget while
    /// the snapshot is being assembled.
    ///
    /// # Errors
    /// As [`Self::snapshot`]; also when the client's budget is used up.
    pub async fn snapshot_with_budget(
        &self,
        query: FeedQuery,
        budget: Arc<RegionBudget>,
    ) -> Result<FeedSnapshot, CoreError> {
        self.snapshot_inner(query, Some(budget)).await
    }

    async fn snapshot_inner(
        &self,
        query: FeedQuery,
        budget: Option<Arc<RegionBudget>>,
    ) -> Result<FeedSnapshot, CoreError> {
        let feed = query.feed;
        self.check_available(feed)?;
        self.inner.audit.record(AuditEvent::Snapshot { feed });
        let mut sub = self.inner.hub.subscribe_with_budget(query, budget)?;
        let deadline = tokio::time::Instant::now()
            + Duration::from_secs(self.inner.config.hub.first_result_timeout_s);
        // Wait until every region has reported, or return the best answer at the deadline.
        let mut event = sub.latest();
        while !matches!(&event, Some(SubscriptionEvent::Snapshot(s)) if s.pending_regions == 0)
            && !matches!(&event, Some(SubscriptionEvent::Error(_)))
        {
            match tokio::time::timeout_at(deadline, sub.next()).await {
                Ok(Some(next)) => event = Some(next),
                Ok(None) => break,
                Err(_) if event.is_some() => break,
                Err(_) => return Err(CoreError::Timeout(format!("the {feed} feed"))),
            }
        }
        match event {
            Some(SubscriptionEvent::Snapshot(s)) => Ok(Arc::unwrap_or_clone(s)),
            // Keep the category: a capacity limit is a 429, not an upstream failure.
            Some(SubscriptionEvent::Error(e)) => Err(CoreError::Api(e)),
            None => Err(CoreError::ShuttingDown),
        }
    }

    /// Coordinates and loaded entities, with configured place lookup on explicit submission.
    ///
    /// # Errors
    /// Returns an error for invalid queries.
    pub async fn search(&self, query: &SearchQuery) -> Result<SearchResults, CoreError> {
        if self.is_shut_down() {
            return Err(CoreError::ShuttingDown);
        }
        self.inner.audit.record(AuditEvent::Searched { chars: query.q.chars().count() });
        if let Some(geocoder) = &self.inner.geocoder
            && self.inner.config.licence_mode == lavender_domain::LicenceMode::Commercial
            && !self.inner.config.licence_acknowledged.contains(&geocoder.source_info().id)
        {
            let mut results = search::search(query, &self.inner.hub)?;
            if query.places == Some(true) && results.coordinates.is_empty() {
                results.notices.clear();
                results.notices.push("Place search is disabled until this service's commercial terms are acknowledged.".to_owned());
            }
            return Ok(results);
        }
        search::search_with_geocoder(
            query,
            &self.inner.hub,
            self.inner.geocoder.as_deref(),
            self.inner.cancel.child_token(),
        )
        .await
    }

    /// Recent positions observed by the shared feed hub. Reading history causes no network call.
    ///
    /// # Errors
    /// Invalid ICAO address, unavailable aircraft feed, or no retained observations.
    pub fn aircraft_track(&self, hex: &str) -> Result<AircraftTrack, CoreError> {
        if self.is_shut_down() {
            return Err(CoreError::ShuttingDown);
        }
        self.check_available(FeedId::Aircraft)?;
        self.inner.audit.record(AuditEvent::AircraftTrack);
        self.inner.hub.aircraft_track(hex)
    }

    /// A propagated ground track for a satellite in the loaded catalogue.
    ///
    /// # Errors
    /// Returns an error for invalid parameters, unknown satellites or invalid elements.
    pub async fn satellite_track(
        &self,
        req: SatelliteTrackRequest,
    ) -> Result<SatelliteTrack, CoreError> {
        self.inner.audit.record(AuditEvent::SatelliteTrack { norad_id: req.norad_id });
        if req.norad_id == 0 {
            return Err(CoreError::BadRequest("NORAD id must be a positive integer".to_owned()));
        }
        let step_s = req.step_s.unwrap_or(60);
        if !(10..=600).contains(&step_s) {
            return Err(CoreError::BadRequest("stepS must be between 10 and 600".to_owned()));
        }
        if let Some(minutes) = req.minutes
            && (!(1..=1_440).contains(&minutes) || minutes * 60 / step_s + 1 > 1_440)
        {
            return Err(CoreError::BadRequest(
                "minutes must be 1–1440 with at most 1440 points".to_owned(),
            ));
        }
        let mut elements = self.find_satellite(req.norad_id);
        if elements.is_none() {
            self.snapshot(FeedQuery::new(FeedId::Satellites)).await?;
            elements = self.find_satellite(req.norad_id);
        }
        let elements = elements.ok_or_else(|| {
            CoreError::NotFound(format!(
                "satellite {} is not in the loaded catalogue",
                req.norad_id
            ))
        })?;
        let propagator =
            Propagator::new(&elements).map_err(|e| CoreError::Upstream(e.to_string()))?;
        let default_minutes = propagator.period_min().clamp(1.0, 120.0).round();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let minutes = req.minutes.unwrap_or(default_minutes as u32);
        if !(1..=1_440).contains(&minutes) || minutes * 60 / step_s + 1 > 1_440 {
            return Err(CoreError::BadRequest(
                "minutes must be 1–1440 with at most 1440 points".to_owned(),
            ));
        }
        if let Some(from) = req.from {
            let start = from.duration_since(elements.epoch);
            let end = start.saturating_add(SignedDuration::from_mins(i64::from(minutes)));
            if start.abs() > MAX_TRACK_EPOCH_OFFSET || end.abs() > MAX_TRACK_EPOCH_OFFSET {
                return Err(CoreError::BadRequest(format!(
                    "from must be within 14 days of the element set's epoch ({})",
                    elements.epoch
                )));
            }
        }
        let from = req.from.unwrap_or_else(Timestamp::now);
        Ok(SatelliteTrack {
            norad_id: elements.norad_id,
            name: elements.name.clone(),
            points: propagator.ground_track(from, minutes, step_s),
            period_min: propagator.period_min(),
        })
    }

    /// Geometric observer passes in a bounded window; computation runs off the async executor.
    ///
    /// # Errors
    /// Invalid observer/window, unavailable elements, excessive work, or propagation failure.
    pub async fn satellite_passes(
        &self,
        req: SatellitePassRequest,
    ) -> Result<SatellitePasses, CoreError> {
        if self.is_shut_down() {
            return Err(CoreError::ShuttingDown);
        }
        let hours = req.hours.unwrap_or(24);
        let threshold = req.min_elevation_deg.unwrap_or(0.0);
        if req.norad_id == 0
            || !(1..=24).contains(&hours)
            || !threshold.is_finite()
            || !(0.0..=89.0).contains(&threshold)
        {
            return Err(CoreError::BadRequest(
                "NORAD id must be positive; hours must be 1–24 and minElevationDeg 0–89".to_owned(),
            ));
        }
        lavender_domain::GeoPoint::new(req.observer.lat, req.observer.lon)?;
        if !req.observer.alt_m.unwrap_or(0.0).is_finite()
            || !(-500.0..=10_000.0).contains(&req.observer.alt_m.unwrap_or(0.0))
        {
            return Err(CoreError::BadRequest(
                "observer altitude must be -500–10000 metres".to_owned(),
            ));
        }
        self.check_available(FeedId::Satellites)?;
        let permit = self.inner.pass_capacity.clone().try_acquire_owned().map_err(|_| {
            CoreError::Limit("Satellite pass prediction is busy; try again shortly.".to_owned())
        })?;
        let mut elements = self.find_satellite(req.norad_id);
        if elements.is_none() {
            self.snapshot(FeedQuery::new(FeedId::Satellites)).await?;
            elements = self.find_satellite(req.norad_id);
        }
        let elements = elements.ok_or_else(|| {
            CoreError::NotFound(format!(
                "satellite {} is not in the loaded catalogue",
                req.norad_id
            ))
        })?;
        let from = req.from.unwrap_or_else(Timestamp::now);
        let until =
            from.checked_add(SignedDuration::from_hours(i64::from(hours))).map_err(|_| {
                CoreError::BadRequest(
                    "pass window is outside the supported timestamp range".to_owned(),
                )
            })?;
        if from.duration_since(elements.epoch).abs() > MAX_TRACK_EPOCH_OFFSET
            || until.duration_since(elements.epoch).abs() > MAX_TRACK_EPOCH_OFFSET
        {
            return Err(CoreError::BadRequest(
                "pass window must be within 14 days of the orbital element epoch".to_owned(),
            ));
        }
        self.inner.audit.record(AuditEvent::SatellitePasses { norad_id: req.norad_id });
        let compute = tokio::task::spawn_blocking(move || {
            // Keep the permit until CPU work ends even if the calling request is cancelled.
            let _permit = permit;
            let propagator = Propagator::new(&elements).map_err(orbit_error)?;
            let passes =
                propagator.passes(req.observer, from, hours, threshold).map_err(orbit_error)?;
            Ok(SatellitePasses {
                norad_id: req.norad_id,
                name: elements.name,
                observer: req.observer,
                from,
                until,
                min_elevation_deg: threshold,
                passes,
            })
        });
        tokio::select! {
            biased;
            () = self.inner.cancel.cancelled() => Err(CoreError::ShuttingDown),
            result = compute => result.map_err(|_| CoreError::Upstream("Satellite pass computation failed.".to_owned()))?,
        }
    }

    fn find_satellite(&self, norad_id: u32) -> Option<lavender_domain::SatelliteElements> {
        self.inner.hub.current_items(FeedId::Satellites).iter().find_map(|items| {
            match items.as_ref() {
                FeedItems::Satellites(list) => {
                    list.iter().find(|s| s.norad_id == norad_id).cloned()
                }
                _ => None,
            }
        })
    }

    /// Settings persisted through the adapter's store.
    #[must_use]
    pub fn settings(&self) -> crate::settings::CoreSettings {
        self.inner.settings.load()
    }

    /// Stops every background task. Subscriptions end; later calls fail.
    pub fn shutdown(&self) {
        self.inner.cancel.cancel();
        self.inner.hub.shutdown();
    }

    #[must_use]
    pub fn is_shut_down(&self) -> bool {
        self.inner.hub.is_shut_down()
    }
}

fn orbit_error(error: OrbitError) -> CoreError {
    match error {
        OrbitError::InvalidRequest(message) => CoreError::BadRequest(message),
        OrbitError::Limit(message) => CoreError::Limit(message),
        other => CoreError::Upstream(other.to_string()),
    }
}
