//! Observer passes through the transport-independent core facade, without upstream I/O.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use async_trait::async_trait;
use jiff::{SignedDuration, Timestamp};
use lavender_core::config::CoreConfig;
use lavender_core::providers::{FeedProvider, FetchContext, Fetched, ProviderError, Region};
use lavender_core::{CoreError, LavenderCore, sources};
use lavender_domain::{
    FeedId, FeedItems, GeoPoint, LicenceMode, ProviderScope, SatelliteElements,
    SatellitePassRequest,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

fn elements() -> SatelliteElements {
    SatelliteElements {
        norad_id: 25_544,
        name: "Archived ISS".into(),
        intl_designator: Some("1998-067A".into()),
        group: "stations".into(),
        epoch: "2008-09-20T12:25:40.104192Z".parse().unwrap(),
        mean_motion: 15.721_253_91,
        eccentricity: 0.000_670_3,
        inclination_deg: 51.6416,
        raan_deg: 247.4627,
        arg_of_pericenter_deg: 130.5360,
        mean_anomaly_deg: 325.0288,
        bstar: -0.000_011_606,
        mean_motion_dot: -0.000_021_82,
        mean_motion_ddot: 0.0,
        rev_at_epoch: None,
        classification: None,
    }
}

#[derive(Debug)]
struct RecordedSatellite {
    elements: SatelliteElements,
    fetches: AtomicUsize,
}

#[async_trait]
impl FeedProvider for RecordedSatellite {
    fn feed(&self) -> FeedId {
        FeedId::Satellites
    }
    fn source_id(&self) -> &'static str {
        sources::CELESTRAK
    }
    fn cadence(&self) -> Duration {
        Duration::from_hours(2)
    }
    fn scope(&self) -> ProviderScope {
        ProviderScope::Global
    }
    fn enabled(&self) -> bool {
        true
    }
    async fn fetch(
        &self,
        _ctx: &FetchContext<'_>,
        _region: Region,
    ) -> Result<Fetched, ProviderError> {
        self.fetches.fetch_add(1, Ordering::SeqCst);
        Ok(FeedItems::Satellites(vec![self.elements.clone()]).into())
    }
}

fn core_with(
    config: CoreConfig,
    elements: SatelliteElements,
) -> (LavenderCore, Arc<RecordedSatellite>) {
    let provider = Arc::new(RecordedSatellite { elements, fetches: AtomicUsize::new(0) });
    let core = LavenderCore::builder(config).providers(vec![provider.clone()]).build().unwrap();
    (core, provider)
}

fn request() -> SatellitePassRequest {
    SatellitePassRequest {
        norad_id: 25_544,
        observer: GeoPoint::new(30.2672, -97.7431).unwrap(),
        from: Some("2008-09-20T12:30:00Z".parse().unwrap()),
        hours: Some(24),
        min_elevation_deg: Some(10.0),
    }
}

#[tokio::test]
async fn facade_returns_archived_iss_passes_and_reuses_loaded_elements() {
    let (core, provider) = core_with(CoreConfig::default(), elements());
    let result = core.satellite_passes(request()).await.unwrap();
    assert_eq!(result.passes.len(), 3);
    assert_eq!(result.norad_id, 25_544);
    assert_eq!(result.name, "Archived ISS");
    assert_eq!(result.observer, request().observer);
    assert_eq!(result.min_elevation_deg, 10.0);
    assert_eq!(result.until.duration_since(result.from), SignedDuration::from_hours(24));
    let expected: Timestamp = "2008-09-21T00:20:21.094Z".parse().unwrap();
    assert!(result.passes[0].rise.duration_since(expected).abs().as_secs_f64() < 1.0);
    assert_eq!(core.satellite_passes(request()).await.unwrap(), result);
    assert_eq!(provider.fetches.load(Ordering::SeqCst), 1);
    core.shutdown();
}

#[tokio::test]
async fn facade_validates_bad_requests_before_fetching_elements() {
    let (core, provider) = core_with(CoreConfig::default(), elements());
    for hours in [0, 25, u32::MAX] {
        let mut req = request();
        req.hours = Some(hours);
        assert!(matches!(core.satellite_passes(req).await, Err(CoreError::BadRequest(_))));
    }
    for threshold in [-1.0, 90.0, f64::NAN, f64::INFINITY] {
        let mut req = request();
        req.min_elevation_deg = Some(threshold);
        assert!(matches!(core.satellite_passes(req).await, Err(CoreError::BadRequest(_))));
    }
    for altitude in [-501.0, 10_001.0, f64::NAN, f64::INFINITY] {
        let mut req = request();
        req.observer.alt_m = Some(altitude);
        assert!(matches!(core.satellite_passes(req).await, Err(CoreError::BadRequest(_))));
    }
    let mut req = request();
    req.norad_id = 0;
    assert!(matches!(core.satellite_passes(req).await, Err(CoreError::BadRequest(_))));
    let mut req = request();
    req.observer.lat = f64::NAN;
    assert!(matches!(core.satellite_passes(req).await, Err(CoreError::Invalid(_))));
    assert_eq!(provider.fetches.load(Ordering::SeqCst), 0);
    core.shutdown();
}

#[tokio::test]
async fn facade_bounds_epoch_offsets_including_default_start() {
    let (core, _) = core_with(CoreConfig::default(), elements());
    let mut req = request();
    req.from = None;
    assert!(matches!(core.satellite_passes(req).await, Err(CoreError::BadRequest(_))));
    let mut req = request();
    req.from = Some(elements().epoch.checked_add(SignedDuration::from_hours(14 * 24)).unwrap());
    assert!(matches!(core.satellite_passes(req).await, Err(CoreError::BadRequest(_))));
    let mut req = request();
    req.from = Some(Timestamp::MAX);
    assert!(matches!(core.satellite_passes(req).await, Err(CoreError::BadRequest(_))));
    let mut req = request();
    req.norad_id = 99_999;
    assert!(matches!(core.satellite_passes(req).await, Err(CoreError::NotFound(_))));
    core.shutdown();
}

#[tokio::test]
async fn facade_defaults_are_bounded_for_current_elements_and_shutdown_is_honest() {
    let mut current = elements();
    current.epoch = Timestamp::now();
    let (core, _) = core_with(CoreConfig::default(), current);
    let req =
        SatellitePassRequest { from: None, hours: None, min_elevation_deg: None, ..request() };
    let before = Timestamp::now();
    let result = core.satellite_passes(req).await.unwrap();
    assert!(result.from >= before);
    assert_eq!(result.until.duration_since(result.from), SignedDuration::from_hours(24));
    assert_eq!(result.min_elevation_deg, 0.0);
    assert!(result.passes.len() <= 64);
    core.shutdown();
    assert!(matches!(core.satellite_passes(request()).await, Err(CoreError::ShuttingDown)));
}

#[tokio::test]
async fn licence_refusal_and_bad_elements_do_not_become_empty_passes() {
    let config = CoreConfig { licence_mode: LicenceMode::Commercial, ..CoreConfig::default() };
    let (core, provider) = core_with(config, elements());
    assert!(matches!(
        core.satellite_passes(request()).await,
        Err(CoreError::DisabledByLicence { .. })
    ));
    assert_eq!(provider.fetches.load(Ordering::SeqCst), 0);
    core.shutdown();
    let mut invalid = elements();
    invalid.eccentricity = 1.2;
    let (core, _) = core_with(CoreConfig::default(), invalid);
    assert!(matches!(core.satellite_passes(request()).await, Err(CoreError::Upstream(_))));
    core.shutdown();
}
