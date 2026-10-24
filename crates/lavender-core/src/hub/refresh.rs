//! Fetches one region on its provider cadence and retries failures without losing cached data.

use super::{HubDeps, MAX_BACKOFF, MAX_PROVIDER_NOTICES, SlotState};
use crate::aircraft_history::AircraftHistory;
use crate::analytics::{KinematicEventC, TrackObservation};
use crate::providers::{FeedProvider, FetchContext, Fetched, MAX_ERROR_CHARS, Region};
use futures::FutureExt;
use jiff::Timestamp;
use lavender_domain::text::clean;
use lavender_domain::{Aircraft, FeedItems, Vessel};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

/// Runs a slot's refresh loop and restarts it, with backoff, if the provider panics. The
/// panic is reported as a failure of the slot, so subscribers see an error state instead
/// of a slot that silently stopped.
pub(super) async fn supervise_refresh(
    provider: Arc<dyn FeedProvider>,
    region: Region,
    tx: watch::Sender<Arc<SlotState>>,
    cancel: CancellationToken,
    deps: HubDeps,
    aircraft_history: Arc<AircraftHistory>,
) {
    let source = provider.source_id();
    let mut crashes: u32 = 0;
    loop {
        let run = AssertUnwindSafe(refresh_region(
            provider.as_ref(),
            region,
            &tx,
            &cancel,
            &deps,
            &aircraft_history,
        ))
        .catch_unwind()
        .await;
        if run.is_ok() || cancel.is_cancelled() {
            return;
        }
        crashes = crashes.saturating_add(1);
        let message = format!("The {source} adapter failed unexpectedly; retrying.");
        tracing::error!(source, ?region, crashes, "provider refresh panicked; restarting it");
        deps.health.record_failure(source, &message);
        let next = SlotState { last_error: Some(message), ..(**tx.borrow()).clone() };
        tx.send_replace(Arc::new(next));
        let delay = retry_delay(crashes, provider.retry_floor(), provider.cadence());
        tokio::select! {
            () = tokio::time::sleep(delay) => {}
            () = cancel.cancelled() => return,
        }
    }
}

/// One slot's refresh loop: fetch, publish, wait for the cadence (or back off), repeat.
/// Returns only when cancelled.
async fn refresh_region(
    provider: &dyn FeedProvider,
    region: Region,
    tx: &watch::Sender<Arc<SlotState>>,
    cancel: &CancellationToken,
    deps: &HubDeps,
    aircraft_history: &AircraftHistory,
) {
    let source = provider.source_id();
    let cadence = provider.cadence();
    let floor = provider.retry_floor();
    let mut failures: u32 = 0;
    loop {
        let ctx = FetchContext {
            fetcher: &deps.fetcher,
            secrets: deps.secrets.as_ref(),
            cancel: cancel.child_token(),
            cache_dir: deps.cache_dir.as_deref(),
        };
        let started = Instant::now();
        let result = tokio::select! {
            r = provider.fetch(&ctx, region) => r,
            () = cancel.cancelled() => return,
        };
        let next = match result {
            Ok(fetched) => {
                // Receipt time may describe transport freshness, but it cannot establish
                // when an aircraft position was observed upstream.
                let observation_base = fetched.as_of;
                let state = provider_answer_to_state(fetched);
                if let (Some(items), Some(as_of)) = (&state.items, observation_base) {
                    match items.as_ref() {
                        FeedItems::Aircraft(aircraft) => {
                            ingest_aircraft(aircraft, as_of, source, deps, aircraft_history);
                        }
                        FeedItems::Vessels(vessels) => {
                            ingest_vessels(vessels, as_of, source, deps);
                        }
                        _ => {}
                    }
                }
                if let Some(items) = &state.items
                    && let FeedItems::Satellites(sats) = items.as_ref()
                {
                    deps.satellite_catalog.update(sats);
                }
                if let Some(reason) = &state.last_error {
                    failures = failures.saturating_add(1);
                    tracing::warn!(source, ?region, error = %reason, "provider answered only partly");
                    deps.health.record_degraded(source, started.elapsed(), reason);
                } else {
                    failures = 0;
                    deps.health.record_success(source, started.elapsed());
                }
                state
            }
            Err(err) => {
                failures = failures.saturating_add(1);
                let message = err.public_message();
                tracing::warn!(source, ?region, failures, error = %message, "provider fetch failed");
                deps.health.record_failure(source, &message);
                SlotState { last_error: Some(message), ..(**tx.borrow()).clone() }
            }
        };
        tx.send_replace(Arc::new(next));
        let delay = if failures == 0 {
            healthy_refresh_delay(cadence, floor)
        } else {
            retry_delay(failures, floor, cadence)
        };
        tokio::select! {
            () = tokio::time::sleep(delay) => {}
            () = cancel.cancelled() => return,
        }
    }
}

fn ingest_aircraft(
    aircraft: &[Aircraft],
    as_of: Timestamp,
    source: &str,
    deps: &HubDeps,
    aircraft_history: &AircraftHistory,
) {
    aircraft_history.capture(aircraft, as_of);
    if let Some(sink) = &deps.track_sink {
        let observations = aircraft.iter().map(|plane| {
            #[allow(clippy::cast_possible_truncation)]
            let altitude_m =
                plane.altitude_geom_ft.or(plane.altitude_baro_ft).map(|ft| (ft * 0.3048) as f32);
            #[allow(clippy::cast_possible_truncation)]
            let speed_knots = plane.ground_speed_kt.map(|s| s as f32);
            #[allow(clippy::cast_possible_truncation)]
            let heading_deg = plane.track_deg.map(|t| t as f32);
            let source_id = if plane.source.is_empty() { source } else { &plane.source };
            TrackObservation::new(
                &plane.hex,
                as_of,
                plane.position.lon,
                plane.position.lat,
                altitude_m,
                speed_knots,
                heading_deg,
                source_id,
            )
        });
        if let Err(err) = sink.push_batch(observations) {
            tracing::warn!(error = %err, "failed to push aircraft track observations");
        }
    }
    if let Some(rule_engine) = &deps.rule_engine {
        for plane in aircraft {
            #[allow(clippy::cast_possible_truncation)]
            let altitude_m = plane
                .altitude_geom_ft
                .or(plane.altitude_baro_ft)
                .map_or(0.0, |ft| (ft * 0.3048) as f32);
            #[allow(clippy::cast_possible_truncation)]
            let speed_knots = plane.ground_speed_kt.map_or(0.0, |s| s as f32);
            #[allow(clippy::cast_possible_truncation)]
            let heading_deg = plane.track_deg.map_or(0.0, |t| t as f32);
            let is_transponder_on =
                plane.squawk.as_deref().is_none_or(|sq| sq != "0000" && sq != "OFF");
            let event = KinematicEventC::new(
                &plane.hex,
                plane.position.lat,
                plane.position.lon,
                altitude_m,
                speed_knots,
                heading_deg,
                as_of.as_second(),
                is_transponder_on,
            );
            if let Some(alert) = rule_engine.evaluate(&event) {
                tracing::warn!(
                    entity_id = %alert.entity_id,
                    rule_id = alert.rule_id,
                    severity = alert.severity,
                    confidence = alert.confidence,
                    message = %alert.message,
                    "Tactical alerting rule triggered for aircraft"
                );
            }
        }
    }
}

fn ingest_vessels(vessels: &[Vessel], as_of: Timestamp, source: &str, deps: &HubDeps) {
    if let Some(sink) = &deps.track_sink {
        let observations = vessels.iter().map(|vessel| {
            #[allow(clippy::cast_possible_truncation)]
            let speed_knots = vessel.speed_knots.map(|s| s as f32);
            #[allow(clippy::cast_possible_truncation)]
            let heading_deg = vessel.heading_deg.or(vessel.course_deg).map(|d| d as f32);
            TrackObservation::new(
                &vessel.mmsi,
                as_of,
                vessel.position.lon,
                vessel.position.lat,
                Some(0.0),
                speed_knots,
                heading_deg,
                source,
            )
        });
        if let Err(err) = sink.push_batch(observations) {
            tracing::warn!(error = %err, "failed to push vessel track observations");
        }
    }
    if let Some(rule_engine) = &deps.rule_engine {
        for vessel in vessels {
            #[allow(clippy::cast_possible_truncation)]
            let speed_knots = vessel.speed_knots.map_or(0.0, |s| s as f32);
            #[allow(clippy::cast_possible_truncation)]
            let heading_deg = vessel.heading_deg.or(vessel.course_deg).map_or(0.0, |d| d as f32);
            let event = KinematicEventC::new(
                &vessel.mmsi,
                vessel.position.lat,
                vessel.position.lon,
                0.0,
                speed_knots,
                heading_deg,
                as_of.as_second(),
                true,
            );
            if let Some(alert) = rule_engine.evaluate(&event) {
                tracing::warn!(
                    entity_id = %alert.entity_id,
                    rule_id = alert.rule_id,
                    severity = alert.severity,
                    confidence = alert.confidence,
                    message = %alert.message,
                    "Tactical alerting rule triggered for vessel"
                );
            }
        }
    }
}

/// Slot state for a provider's answer: its real age (never in the future), and its
/// degradation reason and notices cleaned and length-capped.
fn provider_answer_to_state(fetched: Fetched) -> SlotState {
    let now = Timestamp::now();
    SlotState {
        items: Some(Arc::new(fetched.items)),
        as_of: Some(fetched.as_of.map_or(now, |t| t.min(now))),
        last_error: fetched.degraded.map(|r| clean(&r, MAX_ERROR_CHARS)),
        notices: fetched
            .notices
            .iter()
            .map(|n| clean(n, MAX_ERROR_CHARS))
            .filter(|n| !n.is_empty())
            .take(MAX_PROVIDER_NOTICES)
            .collect(),
    }
}

/// The wait after `failures` consecutive failures: 5 s, 10 s, 20 s … capped at the cadence
/// and five minutes, and never below the provider's retry floor.
pub(super) fn retry_delay(failures: u32, floor: Duration, cadence: Duration) -> Duration {
    let exponent = failures.clamp(1, 7) - 1;
    Duration::from_secs(5u64 << exponent).min(cadence).min(MAX_BACKOFF).max(floor)
}

/// ±10 % jitter so refresh tasks for many regions do not align.
fn jittered(base: Duration) -> Duration {
    let ms = u64::try_from(base.as_millis()).unwrap_or(u64::MAX);
    let spread = ms / 10;
    Duration::from_millis(ms.saturating_sub(spread) + fastrand::u64(0..=spread.saturating_mul(2)))
}

fn healthy_refresh_delay(cadence: Duration, floor: Duration) -> Duration {
    // A source's minimum spacing also applies after success: an early jitter tick
    // must not consume a whole cycle merely by returning a still-cooling cache.
    jittered(cadence).max(floor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_refresh_respects_source_floor_and_preserves_zero_floor_jitter() {
        let cadence = Duration::from_secs(600);
        for _ in 0..100 {
            let cyclone = healthy_refresh_delay(cadence, cadence);
            assert!((Duration::from_secs(600)..=Duration::from_mins(11)).contains(&cyclone));
            let ordinary = healthy_refresh_delay(Duration::from_secs(500), Duration::ZERO);
            assert!((Duration::from_secs(450)..=Duration::from_secs(550)).contains(&ordinary));
        }
    }

    #[test]
    fn firms_never_wakes_before_its_ten_minute_floor() {
        use crate::config::FirmsConfig;
        use crate::providers::firms::FirmsProvider;

        let provider = FirmsProvider::new(FirmsConfig { cadence_s: 0, ..FirmsConfig::default() });
        let (cadence, floor) = (provider.cadence(), provider.retry_floor());
        for failures in 0..=40 {
            assert!(retry_delay(failures, floor, cadence) >= Duration::from_mins(10));
        }
        for _ in 0..100 {
            assert!(healthy_refresh_delay(cadence, floor) >= Duration::from_mins(10));
        }
    }

    #[test]
    fn configured_cyclone_cadence_does_not_skip_an_early_refresh_cycle() {
        use crate::config::CyclonesConfig;
        use crate::providers::cyclones::CyclonesProvider;

        let provider =
            CyclonesProvider::new(CyclonesConfig { cadence_s: 1_800, ..CyclonesConfig::default() });
        for _ in 0..100 {
            let delay = healthy_refresh_delay(provider.cadence(), provider.retry_floor());
            assert!(
                (Duration::from_mins(30)..=Duration::from_mins(33)).contains(&delay),
                "the configured provider would return a cooling cache at {delay:?}, skipping its next refresh"
            );
        }
    }

    #[test]
    fn aircraft_ingestion_buffers_track_observations_and_converts_units() {
        use crate::analytics::TrackBatchBuffer;
        use lavender_domain::GeoPoint;

        let sink = Arc::new(TrackBatchBuffer::new(100, 60));
        let deps = HubDeps {
            fetcher: Arc::new(
                crate::fetch::Fetcher::new(crate::config::FetchConfig::default()).unwrap(),
            ),
            secrets: Arc::new(crate::secrets::MemorySecretStore::default()),
            health: Arc::new(crate::health::HealthTracker::default()),
            cache_dir: None,
            satellite_catalog: Arc::new(crate::satellite_catalog::SatelliteCatalog::default()),
            track_sink: Some(sink.clone()),
            rule_engine: None,
        };
        let history = AircraftHistory::default();
        let as_of = Timestamp::from_second(1_700_000_000).unwrap();
        let planes = vec![Aircraft {
            hex: "4b1812".to_string(),
            callsign: Some("SWR123".to_string()),
            registration: None,
            type_code: None,
            category: None,
            position: GeoPoint::new(47.45, 8.55).unwrap(),
            altitude_baro_ft: Some(10_000.0),
            altitude_geom_ft: Some(10_200.0),
            on_ground: false,
            ground_speed_kt: Some(420.0),
            track_deg: Some(250.0),
            vertical_rate_fpm: None,
            squawk: Some("7000".to_string()),
            emergency: None,
            is_military: false,
            seen_pos_s: 0.0,
            source: "adsb-lol".to_string(),
        }];

        ingest_aircraft(&planes, as_of, "adsb-lol", &deps, &history);

        assert_eq!(sink.len(), 1);
        let obs = sink.snapshot().unwrap();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].entity_id, "4b1812");
        assert_eq!(obs[0].timestamp, as_of);
        assert!((obs[0].lon - 8.55).abs() < 1e-5);
        assert!((obs[0].lat - 47.45).abs() < 1e-5);
        // 10200 ft * 0.3048 = 3108.96 m
        assert!((obs[0].altitude_m.unwrap() - 3108.96).abs() < 0.1);
        assert!((obs[0].speed_knots.unwrap() - 420.0).abs() < 0.1);
        assert!((obs[0].heading_deg.unwrap() - 250.0).abs() < 0.1);
        assert_eq!(obs[0].source_id, "adsb-lol");
    }

    #[test]
    fn vessel_ingestion_buffers_track_observations() {
        use crate::analytics::TrackBatchBuffer;
        use lavender_domain::GeoPoint;

        let sink = Arc::new(TrackBatchBuffer::new(100, 60));
        let deps = HubDeps {
            fetcher: Arc::new(
                crate::fetch::Fetcher::new(crate::config::FetchConfig::default()).unwrap(),
            ),
            secrets: Arc::new(crate::secrets::MemorySecretStore::default()),
            health: Arc::new(crate::health::HealthTracker::default()),
            cache_dir: None,
            satellite_catalog: Arc::new(crate::satellite_catalog::SatelliteCatalog::default()),
            track_sink: Some(sink.clone()),
            rule_engine: None,
        };
        let as_of = Timestamp::from_second(1_700_000_100).unwrap();
        let vessels = vec![Vessel {
            mmsi: "211281610".to_string(),
            name: Some("EVER GIVEN".to_string()),
            imo: None,
            vessel_type: None,
            destination: None,
            position: GeoPoint::new(53.5, 9.9).unwrap(),
            speed_knots: Some(14.5),
            course_deg: Some(182.0),
            heading_deg: Some(180.0),
            last_position_utc: None,
            last_position_epoch: None,
        }];

        ingest_vessels(&vessels, as_of, "aisstream", &deps);

        assert_eq!(sink.len(), 1);
        let obs = sink.snapshot().unwrap();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].entity_id, "211281610");
        assert_eq!(obs[0].timestamp, as_of);
        assert!((obs[0].lon - 9.9).abs() < 1e-5);
        assert!((obs[0].lat - 53.5).abs() < 1e-5);
        assert_eq!(obs[0].altitude_m, Some(0.0));
        assert!((obs[0].speed_knots.unwrap() - 14.5).abs() < 0.1);
        assert!((obs[0].heading_deg.unwrap() - 180.0).abs() < 0.1);
        assert_eq!(obs[0].source_id, "aisstream");
    }

    #[test]
    fn aircraft_evaluates_tactical_alert_rules() {
        use crate::analytics::NimRuleEngine;
        use lavender_domain::GeoPoint;

        let engine = Arc::new(NimRuleEngine::fallback());
        let deps = HubDeps {
            fetcher: Arc::new(
                crate::fetch::Fetcher::new(crate::config::FetchConfig::default()).unwrap(),
            ),
            secrets: Arc::new(crate::secrets::MemorySecretStore::default()),
            health: Arc::new(crate::health::HealthTracker::default()),
            cache_dir: None,
            satellite_catalog: Arc::new(crate::satellite_catalog::SatelliteCatalog::default()),
            track_sink: None,
            rule_engine: Some(engine),
        };
        let history = AircraftHistory::default();
        let as_of = Timestamp::from_second(1_700_000_000).unwrap();
        // Supersonic (700 kt) at low altitude (1500 ft = 457m <= 2000m)
        let planes = vec![Aircraft {
            hex: "ff0011".to_string(),
            callsign: Some("FAST1".to_string()),
            registration: None,
            type_code: None,
            category: None,
            position: GeoPoint::new(45.0, 10.0).unwrap(),
            altitude_baro_ft: Some(1500.0),
            altitude_geom_ft: None,
            on_ground: false,
            ground_speed_kt: Some(700.0),
            track_deg: Some(90.0),
            vertical_rate_fpm: None,
            squawk: Some("7700".to_string()),
            emergency: None,
            is_military: true,
            seen_pos_s: 0.0,
            source: "adsb-lol".to_string(),
        }];

        ingest_aircraft(&planes, as_of, "adsb-lol", &deps, &history);
    }
}
