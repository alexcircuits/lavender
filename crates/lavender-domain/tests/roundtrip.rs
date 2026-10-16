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

fn launch() -> Launch {
    Launch {
        id: "launch-1".into(),
        name: "Falcon 9 Block 5 | Sirius SXM-11".into(),
        status: "Launch Successful".into(),
        net: Some(ts()),
        window_start: Some(ts()),
        window_end: Some(ts()),
        launch_designator: Some("2026-050".into()),
        provider: Some("SpaceX".into()),
        rocket: Some("Falcon 9".into()),
        mission: Some("Sirius SXM-11".into()),
        mission_description: Some("Broadcast satellite.".into()),
        orbit: Some("Geostationary Transfer Orbit".into()),
        pad: Some(LaunchPad {
            name: "LC-39A".into(),
            location_name: Some("KSC".into()),
            latitude: 28.608,
            longitude: -80.604,
        }),
        payload_names: vec!["SXM-11".into()],
        matched_norad_ids: vec![69728],
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
    roundtrip(&FireDetection {
        lat: 38.99,
        lon: -121.67,
        brightness: 303.6,
        frp: 0.53,
        confidence: "n".into(),
        satellite: "N20".into(),
        acquired_at: ts(),
        day_night: "N".into(),
    });
    let cell = FireCell {
        lat: 38.99,
        lon: -121.67,
        brightness: 303.6,
        frp: 12.5,
        confidence: "n".into(),
        satellite: "N20".into(),
        acquired_at: ts(),
        day_night: "N".into(),
        count: 5,
        max_frp: 4.2,
        latest_acquired_at: ts(),
    };
    roundtrip(&cell);
    roundtrip(&FeedItems::Fires(vec![cell]));
    roundtrip(&launch());
    roundtrip(&FeedItems::Launches(vec![launch()]));
    let vessel = Vessel::new(
        "367123456".into(),
        Some("EVER GIVEN".into()),
        Some("9811000".into()),
        Some("Cargo".into()),
        Some("ROTTERDAM".into()),
        point(),
        Some(14.5),
        Some(270.0),
        Some(272.0),
        Some("2026-10-15T12:00:00Z".into()),
        Some(1_729_000_000),
    );
    roundtrip(&vessel);
    roundtrip(&FeedItems::Vessels(vec![vessel.clone()]));
    let track_resp = VesselTrackResponse {
        mmsi: "367123456".into(),
        samples: vec![VesselTrackSample { lat: 60.17, lon: 24.94, epoch_sec: 1_729_000_000 }],
        source: "AISStream (accumulated since server start)".into(),
        retained_sec: 1800,
        voyage: Some(VesselVoyage {
            call_sign: Some("XTEST".into()),
            navigational_status: Some("At anchor".into()),
            eta: Some(VesselEta { month: 10, day: 17, hour: Some(8), minute: None }),
            draught_m: Some(6.2),
            dimensions: Some(VesselDimensions { length_m: Some(70), beam_m: None }),
        }),
    };
    let json = roundtrip(&track_resp);
    assert_eq!(json["voyage"]["eta"], json!({"month": 10, "day": 17, "hour": 8}));
    assert_eq!(json["voyage"]["dimensions"], json!({"lengthM": 70}));
    assert_eq!(json["voyage"]["callSign"], "XTEST");
    let mut q =
        FeedQuery::new(FeedId::Aircraft).with_bbox(BBox::new(170.0, -10.0, -170.0, 10.0).unwrap());
    q.filters = FeedFilters {
        min_magnitude: None,
        military_only: Some(true),
        groups: Some(vec!["x".into()]),
        min_frp: Some(10.0),
    };
    let json = roundtrip(&q);
    assert_eq!(json["filters"], json!({"militaryOnly": true, "groups": ["x"], "minFrp": 10.0}));
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
    roundtrip(&SearchQuery {
        q: "x".into(),
        near: Some(point()),
        limit: Some(5),
        places: Some(true),
    });
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

#[test]
fn wind_contracts_round_trip() {
    let manifest = WindManifest {
        schema_version: 1,
        model: WindModel::Gfs,
        cycle: Some(WindCycle {
            date: "20261015".into(),
            hour: 12,
            forecast_hour: 3,
            run_iso: Some("2026-10-15T12:00:00.000Z".into()),
            valid_iso: Some("2026-10-15T15:00:00.000Z".into()),
        }),
        overlay: Some(WeatherOverlay::Temperature),
        scalar: Some(WeatherScalarMetadata {
            kind: WeatherOverlay::Temperature,
            units: "°C".into(),
            level: "2 m above ground".into(),
        }),
        scalar_error: None,
        fetched_at: Some(1_729_000_000_000),
        level: Some("10 m above ground".into()),
        units: Some("m/s".into()),
        grid: Some(WindGridDef { nx: 360, ny: 181, lo1: 0.0, la1: 90.0, dx: 1.0, dy: 1.0 }),
        stale: false,
        unavailable: false,
        reason: None,
        grid_url: Some(
            "/api/v1/wind/grid/gfs-20261015-12-f3-1-temperature.bin?model=gfs&overlay=temperature"
                .into(),
        ),
        grid_id: Some("gfs-20261015-12-f3-1-temperature".into()),
        attribution: Some("Data: ECMWF Open Data (CC BY 4.0)".into()),
    };
    let json = roundtrip(&manifest);
    assert_eq!(json["schemaVersion"], 1);
    assert_eq!(json["model"], "gfs");
    assert_eq!(json["cycle"]["date"], "20261015");
    assert_eq!(json["cycle"]["hour"], 12);
    assert_eq!(json["cycle"]["forecastHour"], 3);
    assert_eq!(json["overlay"], "temperature");
    assert_eq!(json["scalar"]["kind"], "temperature");
    assert_eq!(json["scalar"]["units"], "°C");
    assert_eq!(json["grid"]["nx"], 360);
    assert_eq!(json["gridId"], "gfs-20261015-12-f3-1-temperature");
    assert_eq!(json["stale"], false);
    assert_eq!(json["unavailable"], false);

    // Status endpoint shape (omits gridUrl and gridId)
    let mut status = manifest.clone();
    status.grid_url = None;
    status.grid_id = None;
    let status_json = roundtrip(&status);
    assert_eq!(status_json.get("gridUrl"), None);
    assert_eq!(status_json.get("gridId"), None);
}

#[test]
fn vessel_and_wind_integer_fields_remain_json_and_typescript_numbers() {
    use ts_rs::TS;

    let config = ts_rs::Config::default();
    let vessel: Vessel = serde_json::from_value(json!({
        "mmsi": "367123456",
        "position": {"lat": 60.17, "lon": 24.94},
        "lastPositionEpoch": 1_729_000_000
    }))
    .unwrap();
    assert_eq!(roundtrip(&vessel)["lastPositionEpoch"].as_i64(), Some(1_729_000_000));
    assert!(Vessel::decl(&config).contains("lastPositionEpoch?: number"));

    let track = VesselTrackResponse {
        mmsi: vessel.mmsi,
        samples: vec![VesselTrackSample { lat: 60.17, lon: 24.94, epoch_sec: 1_729_000_000 }],
        source: "fixture".into(),
        retained_sec: 1_800,
        voyage: None,
    };
    let track_json = roundtrip(&track);
    assert!(track_json.get("voyage").is_none(), "absent, not null");
    assert_eq!(track_json["samples"][0]["epochSec"].as_i64(), Some(1_729_000_000));
    assert_eq!(track_json["retainedSec"].as_u64(), Some(1_800));
    assert!(VesselTrackSample::decl(&config).contains("epochSec: number"));
    assert!(VesselTrackResponse::decl(&config).contains("retainedSec: number"));

    let manifest: WindManifest = serde_json::from_value(json!({
        "schemaVersion": 1, "model": "gfs", "fetchedAt": 1_729_000_000_000_u64,
        "stale": false, "unavailable": false, "reason": null
    }))
    .unwrap();
    assert_eq!(roundtrip(&manifest)["fetchedAt"].as_u64(), Some(1_729_000_000_000));
    assert!(WindManifest::decl(&config).contains("fetchedAt?: number"));
}
