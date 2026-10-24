//! End-to-end verification of live feed ingestion wired to the Parquet analytical track sink
//! and Nim CEP tactical alerting engine.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::too_many_lines)]

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use lavender_core::analytics::{NimRuleEngine, TrackBatchBuffer};
use lavender_core::config::{CoreConfig, HubConfig};
use lavender_core::providers::{FeedProvider, FetchContext, Fetched, ProviderError, Region};
use lavender_core::secrets::{MemorySecretStore, SecretKey};
use lavender_core::{LavenderCore, sources};
use lavender_domain::{Aircraft, FeedId, FeedItems, FeedQuery, GeoPoint, ProviderScope, Vessel};
use tempfile::tempdir;

#[derive(Debug)]
struct MockAircraftProvider {
    planes: Vec<Aircraft>,
}

#[async_trait]
impl FeedProvider for MockAircraftProvider {
    fn feed(&self) -> FeedId {
        FeedId::Aircraft
    }
    fn source_id(&self) -> &'static str {
        sources::ADSB_LOL
    }
    fn cadence(&self) -> Duration {
        Duration::from_secs(1)
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
        let mut fetched = Fetched::fresh(FeedItems::Aircraft(self.planes.clone()));
        fetched.as_of = Some(Timestamp::from_second(1_715_000_000).unwrap());
        Ok(fetched)
    }
}

#[derive(Debug)]
struct MockVesselProvider {
    vessels: Vec<Vessel>,
}

#[async_trait]
impl FeedProvider for MockVesselProvider {
    fn feed(&self) -> FeedId {
        FeedId::Vessels
    }
    fn source_id(&self) -> &'static str {
        sources::AISSTREAM
    }
    fn cadence(&self) -> Duration {
        Duration::from_secs(1)
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
        let mut fetched = Fetched::fresh(FeedItems::Vessels(self.vessels.clone()));
        fetched.as_of = Some(Timestamp::from_second(1_715_000_000).unwrap());
        Ok(fetched)
    }
}

#[tokio::test]
async fn live_aircraft_and_vessels_feed_ingestion_buffered_and_flushed_to_parquet() {
    let aircraft = vec![
        Aircraft {
            hex: "a12345".to_string(),
            callsign: Some("TOPGUN".to_string()),
            registration: None,
            type_code: Some("F18".to_string()),
            category: None,
            position: GeoPoint::new(32.7, -117.2).unwrap(),
            altitude_baro_ft: Some(1500.0),
            altitude_geom_ft: Some(1600.0), // 1600 * 0.3048 = 487.68 m
            on_ground: false,
            ground_speed_kt: Some(720.0), // Supersonic low-altitude triggers Rule 101
            track_deg: Some(270.0),
            vertical_rate_fpm: None,
            squawk: Some("7700".to_string()),
            emergency: None,
            is_military: true,
            seen_pos_s: 0.0,
            source: sources::ADSB_LOL.to_string(),
        },
        Aircraft {
            hex: "b67890".to_string(),
            callsign: Some("CIVIL1".to_string()),
            registration: None,
            type_code: Some("B738".to_string()),
            category: None,
            position: GeoPoint::new(40.7, -74.0).unwrap(),
            altitude_baro_ft: Some(33_000.0),
            altitude_geom_ft: None, // 33000 * 0.3048 = 10058.4 m
            on_ground: false,
            ground_speed_kt: Some(450.0),
            track_deg: Some(90.0),
            vertical_rate_fpm: None,
            squawk: Some("1200".to_string()),
            emergency: None,
            is_military: false,
            seen_pos_s: 0.0,
            source: sources::ADSB_LOL.to_string(),
        },
    ];

    let vessels = vec![Vessel {
        mmsi: "367123456".to_string(),
        name: Some("PACIFIC TRADER".to_string()),
        imo: Some("9123456".to_string()),
        vessel_type: Some("Cargo".to_string()),
        destination: Some("HONOLULU".to_string()),
        position: GeoPoint::new(21.3, -157.8).unwrap(),
        speed_knots: Some(18.2),
        course_deg: Some(265.0),
        heading_deg: Some(264.0),
        last_position_utc: None,
        last_position_epoch: None,
    }];

    let aircraft_provider = Arc::new(MockAircraftProvider { planes: aircraft });
    let vessel_provider = Arc::new(MockVesselProvider { vessels });

    let sink = Arc::new(TrackBatchBuffer::new(1000, 300));
    let rule_engine = Arc::new(NimRuleEngine::fallback());

    let config = CoreConfig {
        hub: HubConfig { first_result_timeout_s: 5, ..HubConfig::default() },
        ..CoreConfig::default()
    };

    let secrets =
        Arc::new(MemorySecretStore::with([(SecretKey::AisStreamApiKey, "test-aisstream-key")]));

    let core = LavenderCore::builder(config)
        .providers(vec![aircraft_provider, vessel_provider])
        .secrets(secrets)
        .track_sink(sink.clone())
        .rule_engine(rule_engine.clone())
        .build()
        .unwrap();

    // Verify facade accessors
    assert!(core.track_batch_buffer().is_some());
    assert!(core.rule_engine().is_some());

    // Trigger aircraft ingestion via snapshot
    let aircraft_snap = core.snapshot(FeedQuery::new(FeedId::Aircraft)).await.unwrap();
    assert_eq!(aircraft_snap.items.len(), 2);

    // Trigger vessel ingestion via snapshot
    let vessel_snap = core.snapshot(FeedQuery::new(FeedId::Vessels)).await.unwrap();
    assert_eq!(vessel_snap.items.len(), 1);

    // Verify buffer has accumulated observations (2 aircraft + 1 vessel = 3)
    let buffer = core.track_batch_buffer().unwrap();
    assert_eq!(buffer.len(), 3);

    let observations = buffer.snapshot().unwrap();
    let plane1 = observations.iter().find(|o| o.entity_id == "a12345").unwrap();
    assert!((plane1.altitude_m.unwrap() - 487.68).abs() < 0.1);
    assert!((plane1.speed_knots.unwrap() - 720.0).abs() < 0.1);
    assert!((plane1.heading_deg.unwrap() - 270.0).abs() < 0.1);

    let ship = observations.iter().find(|o| o.entity_id == "367123456").unwrap();
    assert_eq!(ship.altitude_m, Some(0.0));
    assert!((ship.speed_knots.unwrap() - 18.2).abs() < 0.1);
    assert!((ship.heading_deg.unwrap() - 264.0).abs() < 0.1);
    assert_eq!(ship.source_id, sources::AISSTREAM);

    // Flush to Parquet partition structure
    let temp_dir = tempdir().unwrap();
    let flushed_count = core.flush_analytics_to_parquet(temp_dir.path()).unwrap();
    assert_eq!(flushed_count, 3);
    assert_eq!(buffer.len(), 0);

    // Verify partition structure: 1715000000 corresponds to 2024-05-06
    let date =
        Timestamp::from_second(1_715_000_000).unwrap().to_zoned(jiff::tz::TimeZone::UTC).date();
    let partition_dir = temp_dir
        .path()
        .join(format!("year={:04}", date.year()))
        .join(format!("month={:02}", date.month()))
        .join(format!("day={:02}", date.day()));

    assert!(partition_dir.exists(), "Partition directory {partition_dir:?} should exist");
    let part_file = partition_dir.join("part-0001.parquet");
    assert!(part_file.exists(), "Parquet part file {part_file:?} should exist");

    let file_bytes = std::fs::read(&part_file).unwrap();
    assert!(file_bytes.len() > 12);
    assert_eq!(&file_bytes[..4], b"PAR1");
    assert_eq!(&file_bytes[file_bytes.len() - 4..], b"PAR1");

    // Second flush when buffer is empty returns 0
    let second_flush = core.flush_analytics_to_parquet(temp_dir.path()).unwrap();
    assert_eq!(second_flush, 0);

    core.shutdown();
}

#[tokio::test]
async fn core_without_analytics_sink_returns_zero_on_flush() {
    let temp_dir = tempdir().unwrap();
    let core = LavenderCore::builder(CoreConfig::default()).providers(vec![]).build().unwrap();

    assert!(core.track_batch_buffer().is_none());
    assert!(core.rule_engine().is_none());
    let flushed = core.flush_analytics_to_parquet(temp_dir.path()).unwrap();
    assert_eq!(flushed, 0);
    core.shutdown();
}
