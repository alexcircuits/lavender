//! Every contract type survives a JSON round trip unchanged, and key shapes are pinned so
//! a serde change that would break the TypeScript side fails here first.

#![allow(clippy::unwrap_used)]

use jiff::Timestamp;
use lavender_domain::*;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;

fn roundtrip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(
    value: &T,
) -> serde_json::Value {
    let json = serde_json::to_value(value).unwrap();
    let back: T = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(&back, value);
    json
}

fn ts() -> Timestamp {
    "2026-10-11T12:00:00Z".parse().unwrap()
}

fn point() -> GeoPoint {
    GeoPoint::new(60.17, 24.94).unwrap()
}

fn quake() -> Earthquake {
    Earthquake {
        id: "us1".into(),
        magnitude: 4.5,
        magnitude_type: Some("mb".into()),
        depth_km: 10.0,
        position: point(),
        place: Some("Gulf of Finland".into()),
        time: ts(),
        updated: None,
        url: Some("https://earthquake.usgs.gov/x".into()),
        tsunami: false,
        alert: None,
        felt: Some(3),
        significance: Some(312),
        status: Some("reviewed".into()),
    }
}

fn aircraft() -> Aircraft {
    Aircraft {
        hex: "4ca1b2".into(),
        callsign: Some("RYR12AB".into()),
        registration: None,
        type_code: Some("B738".into()),
        category: None,
        position: point().with_altitude(Some(10_668.0)).unwrap(),
        altitude_baro_ft: Some(35_000.0),
        altitude_geom_ft: None,
        on_ground: false,
        ground_speed_kt: Some(450.0),
        track_deg: Some(91.5),
        vertical_rate_fpm: None,
        squawk: None,
        emergency: None,
        is_military: false,
        seen_pos_s: 1.5,
        source: "adsb-lol".into(),
    }
}

fn satellite() -> SatelliteElements {
    SatelliteElements {
        norad_id: 25544,
        name: "ISS".into(),
        intl_designator: None,
        group: "stations".into(),
        epoch: ts(),
        mean_motion: 15.5,
        eccentricity: 0.0007,
        inclination_deg: 51.6,
        raan_deg: 247.4,
        arg_of_pericenter_deg: 130.5,
        mean_anomaly_deg: 325.0,
        bstar: 0.0003,
        mean_motion_dot: 0.0001,
        mean_motion_ddot: 0.0,
        rev_at_epoch: Some(1),
        classification: Some("U".into()),
    }
}

fn snapshot() -> FeedSnapshot {
    FeedSnapshot {
        feed: FeedId::Earthquakes,
        sources: vec!["usgs-earthquakes".into()],
        fetched_at: Some(ts()),
        generated_at: ts(),
        freshness: Freshness::Degraded { reason: "timeout".into(), age_s: 120 },
        coverage: Coverage::Partial { reason: "zoom in".into() },
        source_count: 1,
        pending_regions: 0,
        items: FeedItems::Earthquakes(vec![quake()]),
        notices: vec![],
    }
}

fn source() -> SourceInfo {
    SourceInfo {
        id: "usgs-earthquakes".into(),
        name: "USGS".into(),
        homepage: "https://usgs.gov".into(),
        attribution: "Data: U.S. Geological Survey".into(),
        licence: Licence { name: "Public domain".into(), url: "https://usgs.gov".into() },
        commercial: CommercialUse::Allowed,
        requires_key: false,
        privacy: Privacy::Normal,
        notes: None,
    }
}

#[test]
fn entities_and_feeds_round_trip() {
    roundtrip(&quake());
    roundtrip(&aircraft());
    roundtrip(&satellite());
    let json = roundtrip(&snapshot());
    assert_eq!(json["freshness"], json!({"state": "degraded", "reason": "timeout", "ageS": 120}));
    assert_eq!(json["coverage"], json!({"kind": "partial", "reason": "zoom in"}));
    assert_eq!(json["items"]["kind"], "earthquakes");
    assert_eq!(json["fetchedAt"], "2026-10-11T12:00:00Z");
    roundtrip(&FeedItems::Aircraft(vec![aircraft()]));
    roundtrip(&FeedItems::Satellites(vec![satellite()]));
    let mut q =
        FeedQuery::new(FeedId::Aircraft).with_bbox(BBox::new(170.0, -10.0, -170.0, 10.0).unwrap());
    q.filters = FeedFilters {
        min_magnitude: None,
        military_only: Some(true),
        groups: Some(vec!["x".into()]),
    };
    let json = roundtrip(&q);
    assert_eq!(json["filters"], json!({"militaryOnly": true, "groups": ["x"]}));
}

#[test]
fn catalog_status_search_and_tracks_round_trip() {
    let catalog = Catalog {
        layers: vec![LayerDescriptor {
            feed: FeedId::Aircraft,
            name: "Aircraft".into(),
            description: "Live".into(),
            family: LayerFamily::Air,
            source_ids: vec!["adsb-lol".into()],
            availability: Availability::Unconfigured { missing: vec!["key".into()] },
            cadence_s: 15,
            scope: ProviderScope::Regional { cell_deg: 5.0, max_cells: 6 },
            default_on: false,
        }],
        sources: vec![source()],
        imagery: vec![ImageryDescriptor {
            id: "gibs".into(),
            name: "GIBS".into(),
            kind: ImageryKind::WmtsGeographic,
            url: "https://gibs/{TileMatrix}".into(),
            layer: Some("L".into()),
            tile_matrix_set: Some("500m".into()),
            format: Some("image/png".into()),
            maximum_level: 7,
            source_id: "nasa-gibs".into(),
            tile_hosts: vec!["gibs.earthdata.nasa.gov".into()],
            is_dark: true,
            is_default: true,
        }],
        licence_mode: LicenceMode::Commercial,
    };
    let json = roundtrip(&catalog);
    assert_eq!(
        json["layers"][0]["availability"],
        json!({"state": "unconfigured", "missing": ["key"]})
    );
    assert_eq!(json["licenceMode"], "commercial");
    assert_eq!(json["imagery"][0]["kind"], "wmtsGeographic");

    roundtrip(&SystemStatus {
        version: "0.1.0".into(),
        licence_mode: LicenceMode::Evaluation,
        started_at: ts(),
        sources: vec![ProviderHealth {
            source_id: "s".into(),
            state: HealthState::Degraded,
            last_success: Some(ts()),
            last_attempt: Some(ts()),
            last_error: Some("x".into()),
            consecutive_failures: 1,
            latency_ms_p50: Some(120),
        }],
        feeds: vec![FeedStatus {
            feed: FeedId::Satellites,
            availability: Availability::DisabledByLicence,
            active_regions: 1,
            subscribers: 2,
        }],
    });
    roundtrip(&SearchQuery { q: "x".into(), near: Some(point()), limit: Some(5) });
    roundtrip(&SearchResults {
        coordinates: vec![CoordinateHit {
            position: point(),
            format: "dms".into(),
            label: "L".into(),
        }],
        places: vec![PlaceHit {
            name: "Helsinki".into(),
            context: None,
            position: point(),
            bbox: None,
            kind: None,
            source_id: "p".into(),
        }],
        entities: vec![EntityHit {
            feed: FeedId::Aircraft,
            id: "4ca1b2".into(),
            label: "RYR".into(),
            detail: None,
            position: Some(point()),
        }],
        notices: vec![],
    });
    roundtrip(&SatelliteTrackRequest {
        norad_id: 1,
        from: Some(ts()),
        minutes: Some(10),
        step_s: None,
    });
    roundtrip(&SatelliteTrack {
        norad_id: 1,
        name: "S".into(),
        points: vec![TrackPoint { time: ts(), position: point() }],
        period_min: 92.9,
    });
    roundtrip(&ApiError::new(ErrorCode::RateLimited, "slow down"));
}

#[test]
fn stream_frames_round_trip() {
    let json = roundtrip(&ServerFrame::Snapshot { id: "s1".into(), snapshot: snapshot() });
    assert_eq!(json["type"], "snapshot");
    let json = roundtrip(&ServerFrame::Error {
        id: None,
        error: ApiError::new(ErrorCode::NotFound, "no"),
    });
    assert_eq!(
        json,
        json!({"type": "error", "error": {"code": "notFound", "message": "no", "retryable": false}})
    );
    roundtrip(&ClientFrame::Update { id: "s1".into(), query: FeedQuery::new(FeedId::Satellites) });
    roundtrip(&ClientFrame::Unsubscribe { id: "s1".into() });
}
