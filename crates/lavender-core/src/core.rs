//! The `LavenderCore` facade: the only API the HTTP server and the Tauri app call.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use lavender_domain::{
    AirQualitySnapshot, AircraftTrack, Availability, BBox, BoundaryClass, BoundaryFeature, Catalog,
    FeedId, FeedItems, FeedQuery, FeedSnapshot, FeedStatus, GnssProvenance, GnssSnapshot, Route,
    SatellitePassRequest, SatellitePasses, SatelliteTrack, SatelliteTrackRequest, SearchQuery,
    SearchResults, SpatialFilter, SpatialSearchRequest, SpatialSearchResponse, SystemStatus,
    VesselTrackResponse, WeatherOverlay, WindManifest, WindModel, WindStatus,
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
use crate::providers::aisstream::AisStreamProvider;
use crate::providers::boundaries::BoundaryStore;
use crate::providers::cables::{CableStore, LandingPoint, SubmarineCable};
use crate::providers::celestrak::CelestrakProvider;
use crate::providers::cyclones::CyclonesProvider;
use crate::providers::divisions::{DivisionIndex, DivisionRecord};
use crate::providers::firms::FirmsProvider;
use crate::providers::infrastructure::{InfrastructureIndex, InfrastructureItem};
use crate::providers::launches::LaunchesProvider;
use crate::providers::perimeters::PerimetersProvider;
use crate::providers::regions::{RegionIndex, RegionRecord};
use crate::providers::usgs::UsgsProvider;
use crate::providers::wind::{self, WindProvider};
use crate::registry::Registry;
use crate::satellite_catalog::SatelliteCatalog;
use crate::search;
use crate::search::geocoder::{ConfiguredGeocoder, Geocoder};
use crate::secrets::{MemorySecretStore, SecretStore};
use crate::settings::{MemorySettingsStore, SettingsStore};
use crate::vessel_store::Mmsi;

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
    #[allow(dead_code)]
    satellite_catalog: Arc<SatelliteCatalog>,
    cancel: CancellationToken,
    pass_capacity: Arc<Semaphore>,
    fetcher: Arc<Fetcher>,
    wind: WindProvider,
    track_sink: Option<Arc<crate::analytics::TrackBatchBuffer>>,
    rule_engine: Option<Arc<crate::analytics::NimRuleEngine>>,
    alert_engine: Option<Arc<crate::alerting::GeofenceAlertEngine>>,
    infrastructure: Arc<InfrastructureIndex>,
    regions: Arc<RegionIndex>,
    divisions: Arc<DivisionIndex>,
    cables: Arc<CableStore>,
    boundaries: Arc<BoundaryStore>,
    overpass: Arc<crate::providers::overpass::OverpassClient>,
    installations: Arc<tokio::sync::RwLock<crate::providers::installations::InstallationStore>>,
    weather: crate::providers::weather::WeatherProvider,
    imagery_search: Arc<crate::providers::imagery_search::ImagerySearchProvider>,
    mapillary: Arc<crate::providers::mapillary::MapillaryProvider>,
    cctv: Arc<crate::providers::cctv::CctvCatalog>,
    routing: Arc<crate::providers::routing::RouteEngine>,
    bikeshare: Arc<crate::providers::bikeshare::BikeshareEngine>,
    transit: Arc<crate::providers::transit::TransitEngine>,
    terrain: Arc<crate::providers::terrain::TerrainEngine>,
    traffic: Arc<crate::providers::traffic::TrafficEngine>,
    aircraft_enrichment: Arc<crate::providers::aircraft_enrichment::AircraftEnrichmentService>,
    opensky: Option<Arc<crate::providers::opensky::OpenSkyProvider>>,
    gnss: Arc<crate::gnss::GnssEngine>,
    air_quality: Arc<crate::providers::air_quality::EcccAirQualityProvider>,
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
    track_sink: Option<Arc<crate::analytics::TrackBatchBuffer>>,
    rule_engine: Option<Arc<crate::analytics::NimRuleEngine>>,
    alert_engine: Option<Arc<crate::alerting::GeofenceAlertEngine>>,
    infrastructure: Option<Arc<InfrastructureIndex>>,
    regions: Option<Arc<RegionIndex>>,
    divisions: Option<Arc<DivisionIndex>>,
    cables: Option<Arc<CableStore>>,
    boundaries: Option<Arc<BoundaryStore>>,
    overpass: Option<Arc<crate::providers::overpass::OverpassClient>>,
    installations:
        Option<Arc<tokio::sync::RwLock<crate::providers::installations::InstallationStore>>>,
    imagery_search: Option<Arc<crate::providers::imagery_search::ImagerySearchProvider>>,
    mapillary: Option<Arc<crate::providers::mapillary::MapillaryProvider>>,
    cctv: Option<Arc<crate::providers::cctv::CctvCatalog>>,
    air_quality: Option<Arc<crate::providers::air_quality::EcccAirQualityProvider>>,
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

    /// Configures an analytics Parquet track batch buffer.
    #[must_use]
    pub fn track_sink(mut self, sink: Arc<crate::analytics::TrackBatchBuffer>) -> Self {
        self.track_sink = Some(sink);
        self
    }

    /// Configures a Nim CEP tactical alerting rule engine.
    #[must_use]
    pub fn rule_engine(mut self, engine: Arc<crate::analytics::NimRuleEngine>) -> Self {
        self.rule_engine = Some(engine);
        self
    }

    /// Configures a geofence alerting rule engine.
    #[must_use]
    pub fn alert_engine(mut self, engine: Arc<crate::alerting::GeofenceAlertEngine>) -> Self {
        self.alert_engine = Some(engine);
        self
    }

    /// Configures the static infrastructure provider index.
    #[must_use]
    pub fn infrastructure(mut self, infrastructure: Arc<InfrastructureIndex>) -> Self {
        self.infrastructure = Some(infrastructure);
        self
    }

    /// Configures the geographic regions provider index.
    #[must_use]
    pub fn regions(mut self, regions: Arc<RegionIndex>) -> Self {
        self.regions = Some(regions);
        self
    }

    /// Configures the recent satellite imagery search provider.
    #[must_use]
    pub fn imagery_search(
        mut self,
        imagery_search: Arc<crate::providers::imagery_search::ImagerySearchProvider>,
    ) -> Self {
        self.imagery_search = Some(imagery_search);
        self
    }

    #[must_use]
    pub fn mapillary(
        mut self,
        mapillary: Arc<crate::providers::mapillary::MapillaryProvider>,
    ) -> Self {
        self.mapillary = Some(mapillary);
        self
    }

    #[must_use]
    pub fn cctv(mut self, cctv: Arc<crate::providers::cctv::CctvCatalog>) -> Self {
        self.cctv = Some(cctv);
        self
    }

    #[must_use]
    pub fn air_quality(
        mut self,
        air_quality: Arc<crate::providers::air_quality::EcccAirQualityProvider>,
    ) -> Self {
        self.air_quality = Some(air_quality);
        self
    }

    /// Validates the configuration and builds the core. Background work starts lazily,
    /// when the first subscription needs it.
    ///
    /// # Errors
    /// Returns every configuration problem at once, or a client initialisation failure.
    #[allow(clippy::too_many_lines)]
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
        let satellite_catalog = Arc::new(crate::satellite_catalog::SatelliteCatalog::default());
        let opensky_provider = if self.config.providers.opensky.enabled {
            Some(Arc::new(crate::providers::opensky::OpenSkyProvider::new(
                self.config.providers.opensky.clone(),
            )))
        } else {
            None
        };
        let providers = self.providers.unwrap_or_else(|| {
            let p = &self.config.providers;
            // Simulation replaces the live aircraft source for that feed when
            // enabled: one source per feed, swapped at registration.
            let aircraft: Arc<dyn FeedProvider> = if p.simulation.enabled {
                Arc::new(crate::providers::simulation::SimulationProvider::new(p.simulation))
            } else {
                Arc::new(AdsbLolProvider::new(p.adsb_lol.clone()))
            };
            let mut list: Vec<Arc<dyn FeedProvider>> = vec![
                aircraft,
                Arc::new(UsgsProvider::new(p.usgs.clone())),
                Arc::new(CelestrakProvider::new(p.celestrak.clone())),
                Arc::new(CyclonesProvider::new(p.cyclones.clone())),
                Arc::new(FirmsProvider::new(p.firms.clone())),
                Arc::new(LaunchesProvider::with_satellite_catalog(
                    p.launches.clone(),
                    satellite_catalog.clone(),
                )),
                Arc::new(AisStreamProvider::new(p.aisstream.clone(), fetcher.clone())),
                Arc::new(PerimetersProvider::new(p.perimeters.clone())),
            ];
            if let Some(op) = &opensky_provider {
                list.push(op.clone());
            }
            list
        });
        let health = Arc::new(HealthTracker::default());
        let tile_cache_root = self
            .cache_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from(".gev-cache"))
            .join("mapillary/tiles");
        let alert_engine = if self.config.alerting.enabled || self.alert_engine.is_some() {
            Some(self.alert_engine.unwrap_or_else(|| {
                Arc::new(crate::alerting::GeofenceAlertEngine::new(
                    self.config.alerting.max_rules,
                    self.config.alerting.inbox_capacity,
                    self.config.alerting.state_capacity,
                ))
            }))
        } else {
            None
        };
        let deps = HubDeps {
            fetcher: fetcher.clone(),
            secrets: self.secrets.clone(),
            health: health.clone(),
            cache_dir: self.cache_dir,
            satellite_catalog: satellite_catalog.clone(),
            track_sink: self.track_sink.clone(),
            rule_engine: self.rule_engine.clone(),
            alert_engine: alert_engine.clone(),
        };
        let hub = FeedHub::new(providers, deps, self.config.hub.clone());
        let registry = Registry::new(self.config.licence_mode, &self.config.licence_acknowledged);
        let wind =
            WindProvider::new(fetcher.clone(), self.config.providers.wind.clone(), health.clone());
        let infrastructure =
            self.infrastructure.unwrap_or_else(|| Arc::new(InfrastructureIndex::load_default()));
        let regions = self.regions.unwrap_or_else(|| Arc::new(RegionIndex::load_default()));
        let divisions = self.divisions.unwrap_or_else(|| Arc::new(DivisionIndex::load_default()));
        let cables = self.cables.unwrap_or_else(|| Arc::new(CableStore::load_bundled()));
        let boundaries = self.boundaries.unwrap_or_else(|| Arc::new(BoundaryStore::load_bundled()));
        let overpass = self.overpass.unwrap_or_else(|| {
            let raw = std::env::var("OVERPASS_UPSTREAMS").unwrap_or_default();
            Arc::new(crate::providers::overpass::OverpassClient::new(&raw))
        });
        let installations = self.installations.unwrap_or_else(|| {
            Arc::new(tokio::sync::RwLock::new(
                crate::providers::installations::InstallationStore::default(),
            ))
        });
        let weather = crate::providers::weather::WeatherProvider::new(fetcher.clone());
        let imagery_search = self.imagery_search.unwrap_or_else(|| {
            Arc::new(crate::providers::imagery_search::ImagerySearchProvider::new(fetcher.clone()))
        });
        let mapillary = self.mapillary.unwrap_or_else(|| {
            Arc::new(crate::providers::mapillary::MapillaryProvider::new(
                fetcher.clone(),
                self.secrets.clone(),
                tile_cache_root,
            ))
        });
        let cctv = self
            .cctv
            .unwrap_or_else(|| Arc::new(crate::providers::cctv::CctvCatalog::new_with_bundled()));
        let pacer = Arc::new(crate::providers::routing::RoutePacer::new(
            Duration::from_millis(self.config.routing.min_request_interval_ms),
            self.config.routing.max_queue_depth,
        ));
        let routing = Arc::new(crate::providers::routing::RouteEngine::new(
            fetcher.client().clone(),
            pacer,
            self.config.routing.car_endpoint.clone(),
            self.config.routing.bike_endpoint.clone(),
            self.config.routing.foot_endpoint.clone(),
        ));
        let bikeshare = Arc::new(crate::providers::bikeshare::BikeshareEngine::new(
            fetcher.clone(),
            self.config.bikeshare.systems.clone(),
        ));
        let transit_history = Arc::new(crate::providers::transit::TransitHistoryStore::new(
            self.config.transit.feed_bytes_limit,
            self.config.transit.process_bytes_limit,
            self.config.transit.max_keys,
        ));
        let transit = Arc::new(crate::providers::transit::TransitEngine::with_feeds(
            fetcher.clone(),
            transit_history,
            self.config.transit.feeds.clone(),
        ));
        let terrain = Arc::new(crate::providers::terrain::TerrainEngine::new(
            self.config.terrain.clone(),
            fetcher.clone(),
        ));
        let traffic = Arc::new(crate::providers::traffic::TrafficEngine::new(
            fetcher.clone(),
            self.secrets.clone(),
            self.config.traffic.clone(),
        ));
        let aircraft_enrichment =
            Arc::new(crate::providers::aircraft_enrichment::AircraftEnrichmentService::new(
                self.config.providers.adsbdb.clone(),
                fetcher.clone(),
                self.config.licence_mode,
                self.config.licence_acknowledged.clone(),
            ));
        let air_quality = self.air_quality.unwrap_or_else(|| {
            Arc::new(crate::providers::air_quality::EcccAirQualityProvider::new(fetcher.clone()))
        });
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
                satellite_catalog,
                cancel: CancellationToken::new(),
                pass_capacity: Arc::new(Semaphore::new(2)),
                fetcher: fetcher.clone(),
                wind,
                track_sink: self.track_sink,
                rule_engine: self.rule_engine,
                alert_engine,
                infrastructure,
                regions,
                divisions,
                cables,
                boundaries,
                overpass,
                installations,
                weather,
                imagery_search,
                mapillary,
                cctv,
                routing,
                bikeshare,
                transit,
                terrain,
                traffic,
                aircraft_enrichment,
                opensky: opensky_provider,
                gnss: Arc::new(crate::gnss::GnssEngine::new()),
                air_quality,
            }),
        })
    }
}

impl LavenderCore {
    /// Returns reference to the active core configuration.
    #[must_use]
    pub fn config(&self) -> &CoreConfig {
        &self.inner.config
    }

    #[must_use]
    pub fn builder(config: CoreConfig) -> LavenderCoreBuilder {
        LavenderCoreBuilder {
            config,
            secrets: Arc::new(MemorySecretStore::default()),
            settings: Arc::new(MemorySettingsStore::default()),
            cache_dir: None,
            providers: None,
            audit: Arc::new(NoAudit),
            track_sink: None,
            rule_engine: None,
            alert_engine: None,
            infrastructure: None,
            regions: None,
            divisions: None,
            cables: None,
            boundaries: None,
            overpass: None,
            installations: None,
            imagery_search: None,
            mapillary: None,
            cctv: None,
            air_quality: None,
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
            .map(|s| self.inner.health.health(&s.id, self.source_enabled(&s.id)))
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

    /// Returns which upstream credentials are configured, without exposing secret values.
    #[must_use]
    pub fn secret_status(&self) -> std::collections::HashMap<String, bool> {
        let mut map = std::collections::HashMap::new();
        for key in crate::secrets::SecretKey::ALL {
            map.insert(key.as_id().to_owned(), self.inner.secrets.is_set(key));
        }
        map
    }

    /// Sets a credential in the core's secret store (desktop mode).
    ///
    /// # Errors
    /// Returns `CoreError::BadRequest` if the key identifier is unknown or if the
    /// secret store is read-only or unavailable.
    pub fn set_secret(&self, key_id: &str, value: String) -> Result<(), CoreError> {
        let key = crate::secrets::SecretKey::from_identifier(key_id)
            .ok_or_else(|| CoreError::BadRequest(format!("unknown secret key: {key_id}")))?;
        self.inner.secrets.set(key, value).map_err(|e| CoreError::BadRequest(e.to_string()))
    }

    /// Clears a credential from the core's secret store (desktop mode).
    ///
    /// # Errors
    /// Returns `CoreError::BadRequest` if the key identifier is unknown or if the
    /// secret store is read-only or unavailable.
    pub fn clear_secret(&self, key_id: &str) -> Result<(), CoreError> {
        let key = crate::secrets::SecretKey::from_identifier(key_id)
            .ok_or_else(|| CoreError::BadRequest(format!("unknown secret key: {key_id}")))?;
        self.inner.secrets.clear(key).map_err(|e| CoreError::BadRequest(e.to_string()))
    }

    /// Whether a source may run: permitted by the licence policy and, for the wind models,
    /// not switched off in the provider settings.
    fn source_enabled(&self, id: &str) -> bool {
        self.inner.registry.licence_permits(id)
            && (!wind::is_wind_source(id) || self.inner.wind.enabled())
    }

    #[must_use]
    pub fn satellite_catalog(&self) -> &Arc<SatelliteCatalog> {
        &self.inner.satellite_catalog
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

    /// Executes a spatial query against currently held live feed items.
    ///
    /// # Errors
    /// Returns an error for invalid query parameters or when shut down.
    pub fn spatial_search(
        &self,
        request: &SpatialSearchRequest,
    ) -> Result<SpatialSearchResponse, CoreError> {
        if self.is_shut_down() {
            return Err(CoreError::ShuttingDown);
        }
        self.inner.audit.record(AuditEvent::Searched {
            chars: match &request.filter {
                SpatialFilter::BBox { .. } => 4,
                SpatialFilter::Circle { .. } => 6,
                SpatialFilter::Polygon { points } => points.len(),
                SpatialFilter::Corridor { waypoints, .. } => waypoints.len(),
            },
        });
        search::spatial_search(request, &self.inner.hub)
    }

    /// Evaluates current GNSS navigation integrity cells, optionally filtered by a bounding box.
    ///
    /// # Errors
    /// Returns an error when shut down.
    pub fn gnss_snapshot(&self, bbox: Option<&BBox>) -> Result<GnssSnapshot, CoreError> {
        if self.is_shut_down() {
            return Err(CoreError::ShuttingDown);
        }
        Ok(self.inner.gnss.snapshot(bbox))
    }

    /// Returns machine-readable provenance metadata for the GNSS navigation integrity classifier.
    #[must_use]
    pub fn gnss_provenance(&self) -> GnssProvenance {
        self.inner.gnss.provenance()
    }

    /// Ingests a raw readsb/adsb.lol JSON payload into the GNSS engine.
    ///
    /// # Errors
    /// Returns an error if the payload is malformed or when shut down.
    pub fn ingest_gnss_readsb(&self, bytes: &[u8]) -> Result<usize, CoreError> {
        if self.is_shut_down() {
            return Err(CoreError::ShuttingDown);
        }
        self.inner.gnss.ingest_readsb_payload(bytes, Timestamp::now())
    }

    /// Access to the underlying GNSS integrity engine.
    #[must_use]
    pub fn gnss(&self) -> &crate::gnss::GnssEngine {
        &self.inner.gnss
    }

    /// Obtains the latest air quality reading snapshot across Canadian AQHI stations.
    ///
    /// # Errors
    /// Returns [`CoreError`] on network failure, parse error, or when no stations are available.
    pub async fn air_quality_snapshot(&self) -> Result<AirQualitySnapshot, CoreError> {
        if self.is_shut_down() {
            return Err(CoreError::ShuttingDown);
        }
        self.inner.air_quality.get_snapshot().await.map_err(CoreError::from)
    }

    /// Access to the underlying air quality provider.
    #[must_use]
    pub fn air_quality(&self) -> &Arc<crate::providers::air_quality::EcccAirQualityProvider> {
        &self.inner.air_quality
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

    /// Returns the OpenSky provider if enabled in configuration.
    #[must_use]
    pub fn opensky_provider(&self) -> Option<Arc<crate::providers::opensky::OpenSkyProvider>> {
        self.inner.opensky.clone()
    }

    /// Looks up route metadata by callsign through the cached adsbdb service.
    ///
    /// # Errors
    /// Returns `CoreError::BadRequest` if callsign is invalid,
    /// or `CoreError::DisabledByLicence` if licence policy restricts it.
    pub async fn aircraft_enrichment_route(
        &self,
        callsign: &str,
    ) -> Result<lavender_domain::AircraftRouteEnrichment, CoreError> {
        if self.is_shut_down() {
            return Err(CoreError::ShuttingDown);
        }
        self.inner.aircraft_enrichment.lookup_route(callsign).await
    }

    /// Looks up aircraft type and registration metadata by hex through the cached adsbdb service.
    ///
    /// # Errors
    /// Returns `CoreError::BadRequest` if hex is invalid,
    /// or `CoreError::DisabledByLicence` if licence policy restricts it.
    pub async fn aircraft_enrichment_type(
        &self,
        hex: &str,
    ) -> Result<lavender_domain::AircraftTypeEnrichment, CoreError> {
        if self.is_shut_down() {
            return Err(CoreError::ShuttingDown);
        }
        self.inner.aircraft_enrichment.lookup_type(hex).await
    }

    /// The recent track of a vessel, accumulated from the live stream since it was first
    /// heard (at most the configured retention). Reading it causes no network call.
    ///
    /// # Errors
    /// An MMSI that is not nine digits (bad request), an unavailable vessels feed, or a
    /// vessel with no retained positions (not found).
    pub fn vessel_track(&self, mmsi: &str) -> Result<VesselTrackResponse, CoreError> {
        if self.is_shut_down() {
            return Err(CoreError::ShuttingDown);
        }
        let mmsi = Mmsi::parse(mmsi).ok_or_else(|| {
            CoreError::BadRequest("An MMSI must be exactly nine digits.".to_owned())
        })?;
        self.check_available(FeedId::Vessels)?;
        self.inner.audit.record(AuditEvent::VesselTrack);
        self.inner.hub.provider(FeedId::Vessels).and_then(|p| p.vessel_track(mmsi)).ok_or_else(
            || CoreError::NotFound(format!("No retained track is available for vessel {mmsi}.")),
        )
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

    /// Refuses wind calls after shutdown, while wind is disabled in the settings, or when
    /// the licence policy does not permit the model's source. Makes no network request.
    fn check_wind(&self, model: WindModel) -> Result<(), CoreError> {
        if self.is_shut_down() {
            return Err(CoreError::ShuttingDown);
        }
        self.inner.wind.ensure_enabled()?;
        wind::licence_gate(&self.inner.registry, model)
    }

    /// Global wind forecast manifest for a model and overlay variant. Upstream failures
    /// are reported in the manifest (`stale`, `unavailable`) and in [`Self::status`].
    ///
    /// # Errors
    /// Returns an error when wind is disabled, refused by the licence policy, or the core
    /// is shutting down.
    pub async fn wind_manifest(
        &self,
        model: WindModel,
        overlay: WeatherOverlay,
    ) -> Result<WindManifest, CoreError> {
        self.check_wind(model)?;
        self.inner.wind.manifest(model, overlay).await
    }

    /// Global wind forecast status: the manifest without `gridUrl` and `gridId`.
    ///
    /// # Errors
    /// As [`Self::wind_manifest`].
    pub async fn wind_status(
        &self,
        model: WindModel,
        overlay: WeatherOverlay,
    ) -> Result<WindStatus, CoreError> {
        self.check_wind(model)?;
        let mut manifest = self.inner.wind.manifest(model, overlay).await?;
        manifest.grid_url = None;
        manifest.grid_id = None;
        Ok(manifest)
    }

    /// One retained binary grid (format: [`lavender_domain::WindGridDef`]).
    ///
    /// # Errors
    /// As [`Self::wind_manifest`]; [`CoreError::NotFound`] if the grid is not retained.
    pub async fn wind_grid(
        &self,
        model: WindModel,
        overlay: WeatherOverlay,
        id: &str,
    ) -> Result<Vec<u8>, CoreError> {
        self.check_wind(model)?;
        self.inner.wind.grid(model, overlay, id).await
    }

    /// Returns the analytics Parquet batch buffer if configured.
    #[must_use]
    pub fn track_batch_buffer(&self) -> Option<&Arc<crate::analytics::TrackBatchBuffer>> {
        self.inner.track_sink.as_ref()
    }

    /// Returns the Nim CEP tactical rule engine if configured.
    #[must_use]
    pub fn rule_engine(&self) -> Option<&Arc<crate::analytics::NimRuleEngine>> {
        self.inner.rule_engine.as_ref()
    }

    /// Returns the geofence alerting engine if configured.
    #[must_use]
    pub fn alert_engine(&self) -> Option<&Arc<crate::alerting::GeofenceAlertEngine>> {
        self.inner.alert_engine.as_ref()
    }

    /// Creates or replaces a geofence alerting rule.
    ///
    /// # Errors
    /// Returns [`CoreError::Api`] with [`ErrorCode::Unconfigured`](lavender_domain::ErrorCode::Unconfigured)
    /// if geofence alerting is not enabled, or [`CoreError::BadRequest`]/[`CoreError::Limit`] on invalid rules.
    pub fn create_alert_rule(
        &self,
        rule: lavender_domain::GeofenceRule,
    ) -> Result<lavender_domain::GeofenceRule, CoreError> {
        if self.is_shut_down() {
            return Err(CoreError::ShuttingDown);
        }
        let Some(engine) = &self.inner.alert_engine else {
            return Err(CoreError::Api(lavender_domain::ApiError::new(
                lavender_domain::ErrorCode::Unconfigured,
                "geofence alerting is not enabled in [core.alerting] configuration",
            )));
        };
        engine.set_rule(rule.clone())?;
        Ok(rule)
    }

    /// Deletes an alerting rule by its identifier.
    ///
    /// # Errors
    /// Returns [`CoreError::Api`] with [`ErrorCode::Unconfigured`](lavender_domain::ErrorCode::Unconfigured)
    /// if alerting is disabled, or [`CoreError::NotFound`] if the rule does not exist.
    pub fn delete_alert_rule(&self, id: &str) -> Result<(), CoreError> {
        if self.is_shut_down() {
            return Err(CoreError::ShuttingDown);
        }
        let Some(engine) = &self.inner.alert_engine else {
            return Err(CoreError::Api(lavender_domain::ApiError::new(
                lavender_domain::ErrorCode::Unconfigured,
                "geofence alerting is not enabled in [core.alerting] configuration",
            )));
        };
        if engine.delete_rule(id) {
            Ok(())
        } else {
            Err(CoreError::NotFound(format!("Alert rule '{id}' does not exist.")))
        }
    }

    /// Fetches an alerting rule by identifier.
    ///
    /// # Errors
    /// Returns [`CoreError::Api`] with [`ErrorCode::Unconfigured`](lavender_domain::ErrorCode::Unconfigured)
    /// if alerting is disabled, or [`CoreError::NotFound`] if the rule does not exist.
    pub fn get_alert_rule(&self, id: &str) -> Result<lavender_domain::GeofenceRule, CoreError> {
        if self.is_shut_down() {
            return Err(CoreError::ShuttingDown);
        }
        let Some(engine) = &self.inner.alert_engine else {
            return Err(CoreError::Api(lavender_domain::ApiError::new(
                lavender_domain::ErrorCode::Unconfigured,
                "geofence alerting is not enabled in [core.alerting] configuration",
            )));
        };
        engine
            .get_rule(id)
            .ok_or_else(|| CoreError::NotFound(format!("Alert rule '{id}' does not exist.")))
    }

    /// Lists all registered alerting rules.
    ///
    /// # Errors
    /// Returns [`CoreError::Api`] with [`ErrorCode::Unconfigured`](lavender_domain::ErrorCode::Unconfigured)
    /// if alerting is disabled.
    pub fn list_alert_rules(&self) -> Result<Vec<lavender_domain::GeofenceRule>, CoreError> {
        if self.is_shut_down() {
            return Err(CoreError::ShuttingDown);
        }
        let Some(engine) = &self.inner.alert_engine else {
            return Err(CoreError::Api(lavender_domain::ApiError::new(
                lavender_domain::ErrorCode::Unconfigured,
                "geofence alerting is not enabled in [core.alerting] configuration",
            )));
        };
        Ok(engine.list_rules())
    }

    /// Retrieves recent alerts from the bounded ring buffer up to `limit`.
    ///
    /// # Errors
    /// Returns [`CoreError::Api`] with [`ErrorCode::Unconfigured`](lavender_domain::ErrorCode::Unconfigured)
    /// if alerting is disabled.
    pub fn alert_inbox(
        &self,
        limit: Option<usize>,
    ) -> Result<Vec<lavender_domain::AlertEvent>, CoreError> {
        if self.is_shut_down() {
            return Err(CoreError::ShuttingDown);
        }
        let Some(engine) = &self.inner.alert_engine else {
            return Err(CoreError::Api(lavender_domain::ApiError::new(
                lavender_domain::ErrorCode::Unconfigured,
                "geofence alerting is not enabled in [core.alerting] configuration",
            )));
        };
        Ok(engine.inbox(limit))
    }

    /// Flushes buffered telemetry track observations to partitioned Parquet files under `base_dir`.
    ///
    /// # Errors
    /// Returns [`AnalyticsError`](crate::analytics::AnalyticsError) on serialization, I/O, or lock failure.
    pub fn flush_analytics_to_parquet(
        &self,
        base_dir: &std::path::Path,
    ) -> Result<usize, crate::analytics::AnalyticsError> {
        if let Some(sink) = &self.inner.track_sink {
            sink.flush_to_parquet(base_dir)
        } else {
            Ok(0)
        }
    }

    /// Stops every background task. Subscriptions end; later calls fail.
    /// Queries static infrastructure items (dams, datacenters) intersecting `bbox`.
    ///
    /// # Errors
    /// Returns an error if the query parameters are invalid.
    pub fn infrastructure_query(
        &self,
        bbox: &BBox,
        zoom: u8,
        limit: usize,
    ) -> Result<Vec<InfrastructureItem>, CoreError> {
        Ok(self.inner.infrastructure.query_bbox(bbox, zoom, limit))
    }

    /// Looks up the most specific geographic region containing `(lon, lat)`.
    ///
    /// # Errors
    /// Returns an error if coordinates are not finite or out of range.
    pub fn region_at(&self, lon: f64, lat: f64) -> Result<Option<RegionRecord>, CoreError> {
        if !lon.is_finite() || !(-180.0..=180.0).contains(&lon) {
            return Err(CoreError::BadRequest(
                "lon must be a finite number between -180 and 180".to_owned(),
            ));
        }
        if !lat.is_finite() || !(-90.0..=90.0).contains(&lat) {
            return Err(CoreError::BadRequest(
                "lat must be a finite number between -90 and 90".to_owned(),
            ));
        }
        Ok(self.inner.regions.region_at(lon, lat).cloned())
    }

    /// Searches geographic regions by name, ID or ISO code.
    ///
    /// # Errors
    /// Returns an error if the search cannot be executed.
    pub fn search_regions(&self, query: &str) -> Result<Vec<RegionRecord>, CoreError> {
        let limit = 50;
        Ok(self.inner.regions.search_regions(query, limit).into_iter().cloned().collect())
    }

    /// Admin-1 divisions containing `(lon, lat)`, most specific first.
    ///
    /// # Errors
    /// Returns an error if coordinates are not finite or out of range.
    pub fn divisions_at(&self, lon: f64, lat: f64) -> Result<Vec<DivisionRecord>, CoreError> {
        if !lon.is_finite() || !(-180.0..=180.0).contains(&lon) {
            return Err(CoreError::BadRequest(
                "lon must be a finite number between -180 and 180".to_owned(),
            ));
        }
        if !lat.is_finite() || !(-90.0..=90.0).contains(&lat) {
            return Err(CoreError::BadRequest(
                "lat must be a finite number between -90 and 90".to_owned(),
            ));
        }
        Ok(self.inner.divisions.divisions_at(lon, lat).into_iter().cloned().collect())
    }

    /// Case-insensitive admin-1 division search by name, country or ISO code.
    ///
    /// # Errors
    /// Returns an error if the search cannot be executed.
    pub fn divisions_search(&self, query: &str) -> Result<Vec<DivisionRecord>, CoreError> {
        Ok(self.inner.divisions.search(query, 20).into_iter().cloned().collect())
    }

    /// Queries submarine cables intersecting `bbox`.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn cables_query(
        &self,
        bbox: &BBox,
        zoom: u8,
        limit: usize,
    ) -> Result<Vec<SubmarineCable>, CoreError> {
        Ok(self.inner.cables.query_cables(bbox, zoom, limit))
    }

    /// Queries submarine cable landing stations inside `bbox`.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn landings_query(
        &self,
        bbox: &BBox,
        zoom: u8,
        limit: usize,
    ) -> Result<Vec<LandingPoint>, CoreError> {
        Ok(self.inner.cables.query_landings(bbox, zoom, limit))
    }

    /// Finds a single submarine cable by ID.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn cable_by_id(&self, id: &str) -> Result<Option<SubmarineCable>, CoreError> {
        Ok(self.inner.cables.find_cable(id).cloned())
    }

    /// Queries administrative and territorial boundary features intersecting `bbox`.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn boundaries_query(
        &self,
        bbox: &BBox,
        classes: Option<&[BoundaryClass]>,
        zoom: Option<u8>,
        limit: Option<usize>,
    ) -> Result<Vec<BoundaryFeature>, CoreError> {
        Ok(self.inner.boundaries.query_bbox(bbox, classes, zoom, limit))
    }

    /// Finds a single boundary feature by ID.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn boundary_by_id(&self, id: &str) -> Result<Option<BoundaryFeature>, CoreError> {
        Ok(self.inner.boundaries.find_by_id(id).cloned())
    }

    /// Reference to the underlying boundary store.
    #[must_use]
    pub fn boundary_store(&self) -> &BoundaryStore {
        &self.inner.boundaries
    }

    /// Whether an upstream Overpass server is configured.
    #[must_use]
    pub fn overpass_is_configured(&self) -> bool {
        self.inner.overpass.is_configured()
    }

    /// Queries the Overpass API through the security sanitiser and failover transport.
    ///
    /// # Errors
    /// Returns [`CoreError::BadRequest`] if the query contains unbounded selectors or is malformed.
    pub async fn overpass_query(
        &self,
        raw_body: &str,
    ) -> Result<crate::providers::overpass::OverpassPayload, CoreError> {
        self.inner.overpass.execute(raw_body).await.map_err(CoreError::from)
    }

    /// Queries military installations in `bbox`, querying upstream Overpass if configured.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub async fn military_installations_query(
        &self,
        bbox: &BBox,
        limit: usize,
    ) -> Result<Vec<crate::providers::installations::MilitaryInstallation>, CoreError> {
        if self.inner.overpass.is_configured() {
            let snapped = crate::providers::installations::quantize_military_installation_box(bbox);
            let ql = format!(
                "[out:json][timeout:20];(nwr[\"military\"~\"^(airfield|naval_base|range|barracks|base)$\"]({s},{w},{n},{e});nwr[\"landuse\"=\"military\"]({s},{w},{n},{e}););out center tags geom 2000;",
                s = snapped.south,
                w = snapped.west,
                n = snapped.north,
                e = snapped.east
            );
            let encoded: String = url::form_urlencoded::byte_serialize(ql.as_bytes()).collect();
            let raw_body = format!("data={encoded}");
            if let Ok(payload) = self.inner.overpass.execute(&raw_body).await
                && payload.status == 200
            {
                let store = crate::providers::installations::InstallationStore::from_overpass_json(
                    &payload.body,
                );
                if !store.is_empty() {
                    let mut lock = self.inner.installations.write().await;
                    *lock = store;
                }
            }
        }

        let lock = self.inner.installations.read().await;
        Ok(lock.query_in_bbox(bbox, limit).into_iter().cloned().collect())
    }

    /// Obtains observed weather manifest (times, bounds, tile URLs) for a given product.
    pub async fn weather_manifest(
        &self,
        product: &str,
    ) -> crate::providers::weather::WeatherManifest {
        let meta = self.inner.weather.get_metadata(product).await.ok();
        self.inner.weather.describe_manifest(product, meta.as_ref())
    }

    /// Fetches and caches a weather WMS tile.
    ///
    /// # Errors
    /// Returns [`CoreError`] on tile fetch, validation, or network failure.
    pub async fn weather_tile(
        &self,
        product: &str,
        time: &str,
        z: u32,
        x: u32,
        y: u32,
        size: u32,
    ) -> Result<bytes::Bytes, CoreError> {
        self.inner.weather.get_tile(product, time, z, x, y, size).await.map_err(CoreError::from)
    }

    /// Searches recent satellite imagery from NASA CMR/HLS and merges with VIIRS daily overviews.
    ///
    /// # Errors
    /// Returns [`CoreError::BadRequest`] if bounding box or parameters are invalid.
    pub async fn imagery_search(
        &self,
        bbox: &BBox,
        days: u32,
    ) -> Result<crate::providers::imagery_search::RecentImageryResponse, CoreError> {
        self.inner
            .imagery_search
            .search(bbox, days, None)
            .await
            .map_err(|e| CoreError::BadRequest(e.to_string()))
    }

    /// Probes Worldview Snapshots to verify whether imagery pixels are present for a given day and area.
    ///
    /// # Errors
    /// Returns [`CoreError::BadRequest`] on invalid parameters or network errors.
    pub async fn imagery_thumbnail_probe(
        &self,
        product: &str,
        day: &str,
        bbox: &BBox,
    ) -> Result<crate::providers::imagery_search::ThumbnailProbeResponse, CoreError> {
        self.inner
            .imagery_search
            .probe_thumbnail(product, day, bbox)
            .await
            .map_err(|e| CoreError::BadRequest(e.to_string()))
    }

    /// Relays a Worldview Snapshots thumbnail image.
    ///
    /// # Errors
    /// Returns [`CoreError`] on image validation or upstream failure.
    pub async fn imagery_thumbnail(
        &self,
        product: &str,
        day: &str,
        bbox: &BBox,
        size: u32,
    ) -> Result<(bytes::Bytes, Option<String>, bool), CoreError> {
        self.inner
            .imagery_search
            .fetch_thumbnail(product, day, bbox, size)
            .await
            .map_err(|e| CoreError::BadRequest(e.to_string()))
    }

    /// Whether Mapillary is configured with an API token.
    #[must_use]
    pub fn mapillary_is_configured(&self) -> bool {
        self.inner.mapillary.is_configured()
    }

    /// Fetches a Mapillary coverage vector tile.
    ///
    /// # Errors
    /// Returns [`CoreError`] on invalid tile coordinates, missing token, or upstream errors.
    pub async fn mapillary_tile(
        &self,
        layer: &str,
        z: u32,
        x: u32,
        y: u32,
    ) -> Result<(bytes::Bytes, crate::providers::mapillary::TileCacheSource), CoreError> {
        self.inner.mapillary.fetch_tile(layer, z, x, y).await.map_err(CoreError::from)
    }

    /// Searches Mapillary street-level photos inside `bbox`.
    ///
    /// # Errors
    /// Returns [`CoreError`] on invalid parameters, missing token, or upstream errors.
    pub async fn mapillary_search_images(
        &self,
        bbox: &BBox,
        limit: usize,
    ) -> Result<crate::providers::mapillary::MapillaryImageSearchResponse, CoreError> {
        self.inner.mapillary.search_images(bbox, limit).await.map_err(CoreError::from)
    }

    /// Returns direct reference to Mapillary provider (for test seams).
    #[must_use]
    pub fn mapillary_provider(&self) -> &Arc<crate::providers::mapillary::MapillaryProvider> {
        &self.inner.mapillary
    }

    /// Access the CCTV catalog directly.
    #[must_use]
    pub fn cctv_catalog(&self) -> &Arc<crate::providers::cctv::CctvCatalog> {
        &self.inner.cctv
    }

    /// Query CCTV cameras, optionally filtered by viewport bounding box and count ceiling.
    #[must_use]
    pub fn cctv_sources(
        &self,
        bbox: Option<&BBox>,
        limit: Option<usize>,
    ) -> Vec<lavender_domain::CameraSource> {
        self.inner.cctv.query_cameras(bbox, limit, self.inner.registry.mode())
    }

    /// Retrieve snapshot of camera health entries.
    #[must_use]
    pub fn cctv_health(&self) -> Vec<lavender_domain::CameraHealth> {
        self.inner.cctv.list_health()
    }

    /// Retrieve streaming and frame URLs for a camera.
    #[must_use]
    pub fn cctv_stream_info(&self, camera_id: &str) -> Option<lavender_domain::CctvStreamInfo> {
        self.inner.cctv.get_stream_info(camera_id)
    }

    /// Fetch a single frame from a camera with SSRF protection and deterministic synthetic fallback.
    /// Returns `(image_bytes, content_type, source_kind)` where `source_kind` is `"upstream"` or `"synthetic"`.
    pub async fn cctv_frame(&self, camera_id: &str) -> (bytes::Bytes, String, &'static str) {
        let Some(cam) = self.inner.cctv.get_camera(camera_id) else {
            let svg = crate::providers::cctv::build_synthetic_cctv_svg(
                camera_id,
                camera_id,
                "",
                "NOT_FOUND",
            );
            return (bytes::Bytes::from(svg), "image/svg+xml".to_owned(), "synthetic");
        };

        if self.inner.registry.mode() == lavender_domain::LicenceMode::Commercial
            && (cam.source_kind == "municipal-webcam" || cam.source_kind == "tallinn-ristmikud")
        {
            let svg = crate::providers::cctv::build_synthetic_cctv_svg(
                &cam.id,
                &cam.name,
                &cam.city,
                "LICENCE_EXCLUDED",
            );
            return (bytes::Bytes::from(svg), "image/svg+xml".to_owned(), "synthetic");
        }

        let client = self.inner.fetcher.client();
        let result = if cam.source_kind == "txdot-its" {
            crate::providers::cctv::fetch_txdot_snapshot(
                client,
                &cam.snapshot_url,
                crate::providers::cctv::CCTV_FRAME_MAX_BODY_BYTES,
            )
            .await
        } else {
            crate::providers::cctv::fetch_within_host(
                client,
                &cam.snapshot_url,
                crate::providers::cctv::CCTV_FRAME_MAX_BODY_BYTES,
                crate::providers::cctv::CCTV_FRAME_FETCH_TIMEOUT,
            )
            .await
        };

        match result {
            Ok((bytes, content_type)) => {
                self.inner.cctv.record_health(&cam.id, "online", "Frame rendered");
                (bytes, content_type, "upstream")
            }
            Err(e) => {
                self.inner.cctv.record_health(&cam.id, "offline", &e.to_string());
                let svg = crate::providers::cctv::build_synthetic_cctv_svg(
                    &cam.id,
                    &cam.name,
                    &cam.city,
                    "UNAVAILABLE",
                );
                (bytes::Bytes::from(svg), "image/svg+xml".to_owned(), "synthetic")
            }
        }
    }

    /// Query automated computer vision object detections for a CCTV camera.
    /// Uses the `lavender-vision` Mojo perception sidecar daemon over Unix Domain Socket IPC,
    /// with calibrated deterministic perception evaluation fallback.
    pub async fn cctv_detections(
        &self,
        camera_id: &str,
    ) -> lavender_domain::entities::CctvDetectionsResponse {
        let client = crate::providers::cctv::VisionPerceptionClient::default();
        client.query_detections(camera_id).await
    }

    /// Calculate a route between coordinates with pacing, caching, and turn-by-turn steps.
    ///
    /// # Errors
    /// Returns [`crate::providers::routing::RouteError`] on validation errors, rate limits, or upstream failures.
    pub async fn calculate_route(
        &self,
        profile: &str,
        coords: &str,
        with_steps: bool,
    ) -> Result<Arc<Route>, crate::providers::routing::RouteError> {
        self.inner.routing.calculate_route(profile, coords, with_steps).await
    }

    #[must_use]
    pub fn route_engine(&self) -> &Arc<crate::providers::routing::RouteEngine> {
        &self.inner.routing
    }

    #[must_use]
    pub fn bikeshare_engine(&self) -> &Arc<crate::providers::bikeshare::BikeshareEngine> {
        &self.inner.bikeshare
    }

    /// Returns the configured bike-share systems registry.
    #[must_use]
    pub fn bikeshare_systems(&self) -> Vec<lavender_domain::BikeshareSystem> {
        self.inner.bikeshare.systems()
    }

    /// Retrieve and filter bike-share stations for a registered system.
    ///
    /// # Errors
    /// Returns [`crate::providers::bikeshare::BikeshareError`] on unknown system ID, network failure, or upstream parsing error.
    pub async fn bikeshare_stations(
        &self,
        system_id: &str,
        bbox: Option<&lavender_domain::BBox>,
    ) -> Result<Vec<lavender_domain::BikeshareStation>, crate::providers::bikeshare::BikeshareError>
    {
        self.inner.bikeshare.get_stations(system_id, bbox).await
    }

    #[must_use]
    pub fn transit_engine(&self) -> &Arc<crate::providers::transit::TransitEngine> {
        &self.inner.transit
    }

    /// Returns the configured GTFS-Realtime transit feeds catalog.
    #[must_use]
    pub fn transit_feeds(&self) -> Vec<lavender_domain::TransitFeed> {
        self.inner.transit.feeds()
    }

    /// Retrieve and filter live transit vehicles for a registered feed.
    ///
    /// # Errors
    /// Returns [`crate::providers::transit::TransitError`] on unknown feed ID, network failure, or rate limits.
    pub async fn transit_vehicles(
        &self,
        feed_id: &str,
        bbox: Option<&lavender_domain::BBox>,
    ) -> Result<Vec<lavender_domain::TransitVehicle>, crate::providers::transit::TransitError> {
        self.inner.transit.get_vehicles(feed_id, bbox).await
    }

    /// Retrieve the historical position trail for a vehicle in a feed.
    ///
    /// # Errors
    /// Returns [`crate::providers::transit::TransitError`] on unknown feed or vehicle ID, or when no trail exists.
    pub fn transit_trail(
        &self,
        feed_id: &str,
        vehicle_id: &str,
    ) -> Result<lavender_domain::TransitTrail, crate::providers::transit::TransitError> {
        self.inner.transit.get_trail(feed_id, vehicle_id)
    }

    /// Returns reference to the terrain heights engine.
    #[must_use]
    pub fn terrain_engine(&self) -> &Arc<crate::providers::terrain::TerrainEngine> {
        &self.inner.terrain
    }

    /// Query ellipsoidal ground heights for a slice of `[longitude, latitude]` points.
    ///
    /// # Errors
    /// Returns [`crate::providers::terrain::TerrainError`] on invalid coordinates,
    /// exceeded point limits, or upstream fetch failure.
    pub async fn query_terrain_heights(
        &self,
        points: &[[f64; 2]],
    ) -> Result<lavender_domain::TerrainHeightsResponse, crate::providers::terrain::TerrainError>
    {
        self.inner.terrain.resolve_heights(points).await
    }

    /// Returns reference to the traffic flow engine.
    #[must_use]
    pub fn traffic_engine(&self) -> &Arc<crate::providers::traffic::TrafficEngine> {
        &self.inner.traffic
    }

    /// Read the current traffic provider status and daily request budget.
    #[must_use]
    pub fn traffic_status(&self) -> lavender_domain::TrafficStatus {
        self.inner.traffic.status()
    }

    /// Query matched roads with traffic flow for a tile coordinate.
    ///
    /// # Errors
    /// Returns [`crate::providers::traffic::TrafficError`] on invalid tile, missing key,
    /// or unrecoverable upstream failures.
    pub async fn query_traffic_tile(
        &self,
        z: u32,
        x: u32,
        y: u32,
    ) -> Result<lavender_domain::TrafficTileResponse, crate::providers::traffic::TrafficError> {
        self.inner.traffic.get_traffic_tile(z, x, y).await
    }

    /// Query raw `TomTom` traffic flow vector tile bytes and cache status.
    ///
    /// # Errors
    /// Returns [`crate::providers::traffic::TrafficError`] on invalid tile, missing key,
    /// budget exhaustion, or unrecoverable upstream failures.
    pub async fn query_flow_tile(
        &self,
        z: u32,
        x: u32,
        y: u32,
    ) -> Result<
        (Vec<u8>, crate::providers::traffic::CacheStatus),
        crate::providers::traffic::TrafficError,
    > {
        self.inner.traffic.get_flow_tile(z, x, y).await
    }

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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use lavender_domain::{ApiError, CommercialUse, ErrorCode, HealthState, LicenceMode};
    use url::Url;
    use wiremock::MockServer;

    use super::*;
    use crate::sources;

    /// A core whose catalogue lists ECMWF's commercial terms as unknown, as after a licence
    /// change. Both real wind sources are commercially allowed, so no configuration alone
    /// can make the commercial policy refuse wind.
    fn core_with_unverified_ecmwf_terms(
        config: CoreConfig,
        acknowledged: &[String],
    ) -> LavenderCore {
        let defs = sources::all()
            .into_iter()
            .map(|mut def| {
                if def.info.id == sources::ECMWF_IFS {
                    def.info.commercial = CommercialUse::Unknown;
                }
                def
            })
            .collect();
        let registry = Registry::with_sources(defs, config.licence_mode, acknowledged);
        let core = LavenderCore::builder(config).build().unwrap();
        let inner = Arc::try_unwrap(core.inner).expect("no other handle exists yet");
        LavenderCore { inner: Arc::new(CoreInner { registry, ..inner }) }
    }

    #[tokio::test]
    async fn wind_calls_pass_the_licence_gate_before_any_upstream_request() {
        let upstream = MockServer::start().await;
        let base = Url::parse(&format!("{}/", upstream.uri())).unwrap();
        let mut config =
            CoreConfig { licence_mode: LicenceMode::Commercial, ..CoreConfig::default() };
        config.fetch.allow_http_hosts = vec!["127.0.0.1".to_owned()];
        config.fetch.retries = 0;
        config.providers.wind.gfs_base_url = base.clone();
        config.providers.wind.ifs_base_url = base;

        let core = core_with_unverified_ecmwf_terms(config.clone(), &[]);
        let error_code = |error: CoreError| ApiError::from(&error).code;
        for overlay in [WeatherOverlay::None, WeatherOverlay::Temperature, WeatherOverlay::Pressure]
        {
            let refused = core.wind_manifest(WindModel::Ifs, overlay).await.unwrap_err();
            assert_eq!(error_code(refused), ErrorCode::DisabledByLicence);
            let refused = core.wind_status(WindModel::Ifs, overlay).await.unwrap_err();
            assert_eq!(error_code(refused), ErrorCode::DisabledByLicence);
            let refused = core.wind_grid(WindModel::Ifs, overlay, "ifs-1").await.unwrap_err();
            assert_eq!(error_code(refused), ErrorCode::DisabledByLicence);
        }
        assert!(upstream.received_requests().await.unwrap().is_empty());
        let ifs = core.status().sources.into_iter().find(|s| s.source_id == sources::ECMWF_IFS);
        assert_eq!(ifs.unwrap().state, HealthState::Disabled);

        // The gate is per model: GFS is still allowed and goes upstream (which has nothing).
        let gfs = core.wind_manifest(WindModel::Gfs, WeatherOverlay::None).await.unwrap();
        assert!(gfs.unavailable);
        assert!(!upstream.received_requests().await.unwrap().is_empty());
        core.shutdown();

        // Acknowledging ECMWF's terms lifts the refusal.
        let core = core_with_unverified_ecmwf_terms(config, &[sources::ECMWF_IFS.to_owned()]);
        let ifs = core.wind_manifest(WindModel::Ifs, WeatherOverlay::None).await.unwrap();
        assert!(ifs.unavailable, "permitted; the empty upstream has no forecast");
        core.shutdown();
    }
}
