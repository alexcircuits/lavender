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
        system: None,
        feed: None,
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

#[test]
fn route_contracts_round_trip() {
    let step = RouteStep {
        index: 0,
        r#type: "depart".into(),
        modifier: None,
        exit: None,
        name: "Main St".into(),
        r#ref: Some("A1".into()),
        distance_m: 500,
        duration_s: 45,
        lon: 13.4,
        lat: 52.5,
        instruction: "Head out on Main St (A1)".into(),
    };
    let step_json = roundtrip(&step);
    assert_eq!(step_json["type"], "depart");
    assert_eq!(step_json["name"], "Main St");
    assert_eq!(step_json["ref"], "A1");
    assert_eq!(step_json["distanceM"], 500);

    let route = Route {
        ok: true,
        profile: "car".into(),
        distance_m: 12000,
        duration_s: 850,
        geometry: vec![[13.4, 52.5], [13.41, 52.51]],
        steps: Some(vec![step]),
        steps_truncated: Some(false),
    };
    let route_json = roundtrip(&route);
    assert_eq!(route_json["ok"], true);
    assert_eq!(route_json["profile"], "car");
    assert_eq!(route_json["distanceM"], 12000);
    assert_eq!(route_json["stepsTruncated"], false);
}

#[test]
fn bikeshare_contracts_round_trip() {
    let system = BikeshareSystem {
        id: "nyc-citibike".into(),
        name: "Citi Bike".into(),
        city: "New York, NY".into(),
        center: GeoPoint::new(40.7484, -73.9967).unwrap(),
        load_radius_km: 140.0,
        provider: "Citi Bike".into(),
        gbfs_url: "https://gbfs.lyft.com/gbfs/2.3/bkn/en/gbfs.json".into(),
    };
    let system_json = roundtrip(&system);
    assert_eq!(system_json["id"], "nyc-citibike");
    assert_eq!(system_json["name"], "Citi Bike");
    assert_eq!(system_json["city"], "New York, NY");
    assert_eq!(system_json["loadRadiusKm"], 140.0);

    let station = BikeshareStation {
        id: "nyc-citibike:3790".into(),
        station_id: "3790".into(),
        system_id: "nyc-citibike".into(),
        name: "Broadway & 24th St".into(),
        position: GeoPoint::new(40.742, -73.989).unwrap(),
        capacity: Some(30),
        bikes_available: Some(12),
        docks_available: Some(18),
        is_installed: true,
        is_renting: true,
        is_returning: true,
        last_reported: Some(1_729_000_000),
    };
    let station_json = roundtrip(&station);
    assert_eq!(station_json["id"], "nyc-citibike:3790");
    assert_eq!(station_json["stationId"], "3790");
    assert_eq!(station_json["name"], "Broadway & 24th St");
    assert_eq!(station_json["capacity"], 30);
    assert_eq!(station_json["bikesAvailable"], 12);
    assert_eq!(station_json["docksAvailable"], 18);
    assert_eq!(station_json["isInstalled"], true);
    assert_eq!(station_json["isRenting"], true);
    assert_eq!(station_json["isReturning"], true);
    assert_eq!(station_json["lastReported"], 1_729_000_000);
}

#[test]
fn transit_contracts_round_trip() {
    let feed = TransitFeed {
        id: "mbta".into(),
        name: "MBTA".into(),
        operator: "Massachusetts Bay Transportation Authority".into(),
        region: "Boston, MA".into(),
        center: GeoPoint::new(42.3601, -71.0589).unwrap(),
        load_radius_km: 70.0,
        url: "https://cdn.mbta.com/realtime/VehiclePositions.pb".into(),
        license: "MassDOT Developers License Agreement".into(),
        attribution: "MBTA / MassDOT".into(),
        default_enabled: true,
        default_mode: "bus".into(),
    };
    let feed_json = roundtrip(&feed);
    assert_eq!(feed_json["id"], "mbta");
    assert_eq!(feed_json["name"], "MBTA");
    assert_eq!(feed_json["loadRadiusKm"], 70.0);
    assert_eq!(feed_json["defaultMode"], "bus");

    let vehicle = TransitVehicle {
        id: "mbta:1234".into(),
        vehicle_id: "1234".into(),
        feed_id: "mbta".into(),
        position: GeoPoint::new(42.355, -71.065).unwrap(),
        bearing: Some(180.0),
        speed_mps: Some(12.5),
        timestamp: Some(1_729_000_000),
        timestamp_source: "vehicle".into(),
        route_id: Some("1".into()),
        trip_id: Some("trip-abc".into()),
        direction_id: Some(0),
        label: Some("1234".into()),
        stop_id: Some("stop-xyz".into()),
        status: Some("IN_TRANSIT_TO".into()),
        occupancy: Some("FEW_SEATS_AVAILABLE".into()),
        mode: "bus".into(),
    };
    let vehicle_json = roundtrip(&vehicle);
    assert_eq!(vehicle_json["id"], "mbta:1234");
    assert_eq!(vehicle_json["vehicleId"], "1234");
    assert_eq!(vehicle_json["feedId"], "mbta");
    assert_eq!(vehicle_json["bearing"], 180.0);
    assert_eq!(vehicle_json["speedMps"], 12.5);
    assert_eq!(vehicle_json["timestampSource"], "vehicle");
    assert_eq!(vehicle_json["status"], "IN_TRANSIT_TO");
    assert_eq!(vehicle_json["mode"], "bus");

    let trail = TransitTrail {
        version: 1,
        feed_id: "mbta".into(),
        vehicle_id: "1234".into(),
        fixes: vec![[1_729_000_000_000.0, 42.355, -71.065, 0.0, 1.0]],
        epochs: vec![TransitEpoch {
            id: 1,
            trip: "trip-abc".into(),
            route: "1".into(),
            mode: "bus".into(),
        }],
        truncated: false,
    };
    let trail_json = roundtrip(&trail);
    assert_eq!(trail_json["version"], 1);
    assert_eq!(trail_json["feedId"], "mbta");
    assert_eq!(trail_json["vehicleId"], "1234");
    assert_eq!(trail_json["fixes"].as_array().unwrap().len(), 1);
    assert_eq!(trail_json["epochs"].as_array().unwrap().len(), 1);
    assert_eq!(trail_json["truncated"], false);
}

#[test]
fn terrain_contracts_round_trip() {
    use lavender_domain::{TerrainHeightsRequest, TerrainHeightsResponse, TerrainPointResult};

    let req = TerrainHeightsRequest { points: vec![[12.34567, 45.67891], [139.75, 35.68]] };
    let req_json = roundtrip(&req);
    assert_eq!(req_json["points"][0], json!([12.34567, 45.67891]));
    assert_eq!(req_json["points"][1], json!([139.75, 35.68]));

    let resp = TerrainHeightsResponse {
        results: vec![
            TerrainPointResult {
                id: Some("pt-1".into()),
                ellipsoid: 125.5,
                elevation: Some(105.0),
                geoid: Some(20.5),
            },
            TerrainPointResult { id: None, ellipsoid: 60.0, elevation: None, geoid: None },
        ],
    };
    let resp_json = roundtrip(&resp);
    assert_eq!(resp_json["results"][0]["id"], "pt-1");
    assert_eq!(resp_json["results"][0]["ellipsoid"], 125.5);
    assert_eq!(resp_json["results"][0]["elevation"], 105.0);
    assert_eq!(resp_json["results"][0]["geoid"], 20.5);
    assert!(resp_json["results"][1].get("id").is_none());
    assert_eq!(resp_json["results"][1]["ellipsoid"], 60.0);
}

#[test]
fn traffic_contracts_round_trip() {
    use lavender_domain::{TrafficRoadSegment, TrafficStatus, TrafficTileResponse};

    let status =
        TrafficStatus { has_key: true, daily_count: 142, budget: 6000, date: "2026-10-22".into() };
    let status_json = roundtrip(&status);
    assert_eq!(status_json["hasKey"], true);
    assert_eq!(status_json["dailyCount"], 142);
    assert_eq!(status_json["budget"], 6000);
    assert_eq!(status_json["date"], "2026-10-22");

    let tile_resp = TrafficTileResponse {
        z: 12,
        x: 935,
        y: 1686,
        roads: vec![TrafficRoadSegment {
            coords: vec![[-97.7431, 30.2672], [-97.7445, 30.2685]],
            traffic_level: 0.85,
            closure: false,
            road_type: Some("primary".into()),
        }],
    };
    let tile_json = roundtrip(&tile_resp);
    assert_eq!(tile_json["z"], 12);
    assert_eq!(tile_json["x"], 935);
    assert_eq!(tile_json["y"], 1686);
    assert_eq!(tile_json["roads"][0]["coords"][0], json!([-97.7431, 30.2672]));
    assert_eq!(tile_json["roads"][0]["trafficLevel"], 0.85);
    assert_eq!(tile_json["roads"][0]["closure"], false);
    assert_eq!(tile_json["roads"][0]["roadType"], "primary");
}

#[test]
fn aircraft_enrichment_contracts_round_trip() {
    let route = AircraftRouteEnrichment {
        callsign: "BAW123".into(),
        found: true,
        airline: Some("British Airways".into()),
        origin: Some(AirportInfo {
            code: "LHR".into(),
            name: "London Heathrow".into(),
            lat: Some(51.47),
            lon: Some(-0.454),
        }),
        destination: Some(AirportInfo {
            code: "JFK".into(),
            name: "John F Kennedy Intl".into(),
            lat: Some(40.64),
            lon: Some(-73.78),
        }),
    };
    let route_json = roundtrip(&route);
    assert_eq!(route_json["callsign"], "BAW123");
    assert_eq!(route_json["found"], true);
    assert_eq!(route_json["airline"], "British Airways");
    assert_eq!(route_json["origin"]["code"], "LHR");
    assert_eq!(route_json["destination"]["code"], "JFK");

    let not_found_route = AircraftRouteEnrichment {
        callsign: "UNKNOWN".into(),
        found: false,
        airline: None,
        origin: None,
        destination: None,
    };
    let not_found_json = roundtrip(&not_found_route);
    assert_eq!(not_found_json["callsign"], "UNKNOWN");
    assert_eq!(not_found_json["found"], false);
    assert!(not_found_json.get("airline").is_none());

    let aircraft_type = AircraftTypeEnrichment {
        hex: "4ca1b2".into(),
        found: true,
        type_code: Some("B738".into()),
        type_name: Some("Boeing 737-800".into()),
        registration: Some("EI-ABC".into()),
    };
    let type_json = roundtrip(&aircraft_type);
    assert_eq!(type_json["hex"], "4ca1b2");
    assert_eq!(type_json["found"], true);
    assert_eq!(type_json["typeCode"], "B738");
    assert_eq!(type_json["typeName"], "Boeing 737-800");
    assert_eq!(type_json["registration"], "EI-ABC");

    let boundary = BoundaryFeature {
        id: "disp-1".into(),
        name: Some("Line of Control".into()),
        class: BoundaryClass::DisputedLine,
        status: Some("Line of control".into()),
        sovereign: None,
        claimants: vec!["India".into(), "Pakistan".into()],
        note: Some("Admin. by India, Claimed by Pakistan".into()),
        lines: vec![vec![GeoPoint::new(34.0, 74.0).unwrap(), GeoPoint::new(34.5, 74.5).unwrap()]],
        bbox: BBox::new(74.0, 34.0, 74.5, 34.5).unwrap(),
    };
    let b_json = roundtrip(&boundary);
    assert_eq!(b_json["id"], "disp-1");
    assert_eq!(b_json["name"], "Line of Control");
    assert_eq!(b_json["class"], "disputedLine");
    assert_eq!(b_json["status"], "Line of control");
    assert_eq!(b_json["claimants"][0], "India");
    assert_eq!(b_json["claimants"][1], "Pakistan");

    let resp = BoundaryResponse {
        features: vec![boundary],
        total: 1,
        attribution: "Made with Natural Earth".into(),
    };
    let resp_json = roundtrip(&resp);
    assert_eq!(resp_json["total"], 1);
    assert_eq!(resp_json["attribution"], "Made with Natural Earth");
}

#[test]
fn air_quality_contracts_round_trip() {
    let reading = AirQualityReading {
        id: "eccc:60104".into(),
        provider: "eccc".into(),
        scale: "AQHI".into(),
        value: 4,
        band: AirQualityBand::Moderate,
        station_id: "60104".into(),
        name: "Toronto Downtown".into(),
        zone: Some("ont".into()),
        latitude: 43.65,
        longitude: -79.38,
        observed_at: ts(),
        color: "#ffd43b".into(),
        label: "4".into(),
        risk: "Moderate health risk".into(),
        note: None,
    };
    let r_json = roundtrip(&reading);
    assert_eq!(r_json["id"], "eccc:60104");
    assert_eq!(r_json["provider"], "eccc");
    assert_eq!(r_json["scale"], "AQHI");
    assert_eq!(r_json["value"], 4);
    assert_eq!(r_json["band"], "moderate");
    assert_eq!(r_json["stationId"], "60104");
    assert_eq!(r_json["name"], "Toronto Downtown");
    assert_eq!(r_json["zone"], "ont");
    assert_eq!(r_json["latitude"], 43.65);
    assert_eq!(r_json["longitude"], -79.38);
    assert_eq!(r_json["color"], "#ffd43b");
    assert_eq!(r_json["label"], "4");
    assert_eq!(r_json["risk"], "Moderate health risk");
    assert!(r_json.get("note").is_none());

    let high_reading = AirQualityReading {
        id: "eccc:99999".into(),
        provider: "eccc".into(),
        scale: "AQHI".into(),
        value: 11,
        band: AirQualityBand::VeryHigh,
        station_id: "99999".into(),
        name: "Wildfire Station".into(),
        zone: Some("pyr".into()),
        latitude: 50.0,
        longitude: -120.0,
        observed_at: ts(),
        color: "#c1121f".into(),
        label: "10+".into(),
        risk: "Very High health risk".into(),
        note: Some("Smoke advisory in effect".into()),
    };
    let high_json = roundtrip(&high_reading);
    assert_eq!(high_json["band"], "very-high");
    assert_eq!(high_json["label"], "10+");
    assert_eq!(high_json["note"], "Smoke advisory in effect");

    let snapshot = AirQualitySnapshot {
        readings: vec![reading, high_reading],
        freshness: Freshness::Live,
        updated_at: ts(),
    };
    let snap_json = roundtrip(&snapshot);
    assert_eq!(snap_json["readings"].as_array().unwrap().len(), 2);
    assert_eq!(snap_json["freshness"]["state"], "live");
}

#[test]
fn entity_resolution_contracts_round_trip() {
    use std::collections::HashMap;

    let id1 = EntityIdentifier::new("icao24", "4ca1b2");
    let id2 = EntityIdentifier::new("callsign", "RYR12AB");
    let id_json = roundtrip(&id1);
    assert_eq!(id_json["scheme"], "icao24");
    assert_eq!(id_json["value"], "4ca1b2");

    let mut attrs = HashMap::new();
    attrs.insert("typeCode".to_string(), "B738".to_string());
    attrs.insert("isMilitary".to_string(), "false".to_string());

    let canonical = CanonicalEntity {
        id: "canonical:aircraft:4ca1b2".to_string(),
        kind: EntityKind::Aircraft,
        primary_label: "RYR12AB".to_string(),
        identifiers: vec![id1, id2],
        attributes: attrs,
        first_seen: 1_729_000_000,
        last_seen: 1_729_000_120,
        confidence: 0.98,
    };
    let c_json = roundtrip(&canonical);
    assert_eq!(c_json["id"], "canonical:aircraft:4ca1b2");
    assert_eq!(c_json["kind"], "aircraft");
    assert_eq!(c_json["primaryLabel"], "RYR12AB");
    assert_eq!(c_json["identifiers"].as_array().unwrap().len(), 2);
    assert_eq!(c_json["identifiers"][0]["scheme"], "icao24");
    assert_eq!(c_json["identifiers"][0]["value"], "4ca1b2");
    assert_eq!(c_json["attributes"]["typeCode"], "B738");
    assert_eq!(c_json["firstSeen"], 1_729_000_000);
    assert_eq!(c_json["lastSeen"], 1_729_000_120);
    assert!((c_json["confidence"].as_f64().unwrap() - 0.98).abs() < 1e-4);

    let link = EntityLink {
        source_id: "canonical:aircraft:4ca1b2".to_string(),
        target_id: "callsign:RYR12AB".to_string(),
        relation: "alias_of".to_string(),
        confidence: 0.99,
        created_at: 1_729_000_000,
    };
    let link_json = roundtrip(&link);
    assert_eq!(link_json["sourceId"], "canonical:aircraft:4ca1b2");
    assert_eq!(link_json["targetId"], "callsign:RYR12AB");
    assert_eq!(link_json["relation"], "alias_of");
    assert_eq!(link_json["createdAt"], 1_729_000_000);
    assert!((link_json["confidence"].as_f64().unwrap() - 0.99).abs() < 1e-4);

    // Test other EntityKind variants
    assert_eq!(serde_json::to_value(EntityKind::Vessel).unwrap(), "vessel");
    assert_eq!(serde_json::to_value(EntityKind::Satellite).unwrap(), "satellite");
    assert_eq!(serde_json::to_value(EntityKind::Facility).unwrap(), "facility");
    assert_eq!(serde_json::to_value(EntityKind::Event).unwrap(), "event");
}

#[test]
fn saved_search_contracts_round_trip() {
    let search = SavedSearch {
        id: "search-1".to_string(),
        name: "Baltic Patrol".to_string(),
        query: "B738".to_string(),
        feed_filter: Some(vec![FeedId::Aircraft]),
        created_at: ts(),
        notify_on_match: true,
    };
    let json = roundtrip(&search);
    assert_eq!(json["id"], "search-1");
    assert_eq!(json["name"], "Baltic Patrol");
    assert_eq!(json["query"], "B738");
    assert_eq!(json["feedFilter"].as_array().unwrap().len(), 1);
    assert_eq!(json["feedFilter"][0], "aircraft");
    assert_eq!(json["createdAt"], "2026-10-11T12:00:00Z");
    assert_eq!(json["notifyOnMatch"], true);

    let search_no_filter = SavedSearch {
        id: "search-2".to_string(),
        name: "Ever Given".to_string(),
        query: "ever given".to_string(),
        feed_filter: None,
        created_at: ts(),
        notify_on_match: false,
    };
    let json_no_filter = roundtrip(&search_no_filter);
    assert_eq!(json_no_filter["id"], "search-2");
    assert!(json_no_filter.get("feedFilter").is_none());
    assert_eq!(json_no_filter["notifyOnMatch"], false);

    let search_match = SavedSearchMatch {
        search_id: "search-1".to_string(),
        search_name: "Baltic Patrol".to_string(),
        feed: FeedId::Aircraft,
        entity_id: "4ca1b2".to_string(),
        label: "RYR12AB".to_string(),
        matched_at: ts(),
    };
    let match_json = roundtrip(&search_match);
    assert_eq!(match_json["searchId"], "search-1");
    assert_eq!(match_json["searchName"], "Baltic Patrol");
    assert_eq!(match_json["feed"], "aircraft");
    assert_eq!(match_json["entityId"], "4ca1b2");
    assert_eq!(match_json["label"], "RYR12AB");
    assert_eq!(match_json["matchedAt"], "2026-10-11T12:00:00Z");
}
