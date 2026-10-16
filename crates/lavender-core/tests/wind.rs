//! Unit and integration tests for global wind forecast decoding, resampling,
//! wiremock-backed upstream fetching, caching, generation rollover, and error envelopes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fmt::Write as _;
use std::sync::Arc;

use grib::{def::grib2, encoder, encoder::WriteGrib2Message as _};
use lavender_core::config::{FetchConfig, WindConfig};
use lavender_core::fetch::Fetcher;
use lavender_core::health::HealthTracker;
use lavender_core::providers::wind::*;
use lavender_core::registry::Registry;
use lavender_core::sources;
use lavender_domain::{
    ApiError, CommercialUse, ErrorCode, HealthState, LicenceMode, WeatherOverlay, WindModel,
};
use url::Url;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// What a synthetic message says it holds: parameter category and number (discipline 0),
/// then the first fixed surface's type and value.
type TestField = ((u8, u8), (u8, u32));
const U_10M: TestField = ((2, 2), (103, 10));
const V_10M: TestField = ((2, 3), (103, 10));
const T_2M: TestField = ((0, 0), (103, 2));
const PRMSL: TestField = ((3, 1), (101, 0));

/// Helper to create a valid synthetic GRIB2 message using grib's `SingleGrib2Message` encoder.
fn create_test_grib2(field: TestField, lats: &[f64], lngs: &[f64], values: &[f32]) -> Vec<u8> {
    let ((category, number), (surface, level)) = field;
    let ni = lngs.len();
    let nj = lats.len();
    let values_f64: Vec<f64> = values.iter().map(|&v| f64::from(v)).collect();

    let grid = grib2::Section3Payload {
        grid_def_source: 0,
        num_points: u32::try_from(ni * nj).unwrap(),
        num_point_list_octets: 0,
        point_list_interpretation: 0,
        template_num: 0,
        template: grib2::GridDefinitionTemplate::_3_0(grib2::template::Template3_0 {
            earth: grib2::template::param_set::EarthShape {
                shape: 6,
                spherical_earth_radius: grib2::template::param_set::ScaledValue {
                    scale_factor: 0xff,
                    scaled_value: 0xffff_ffff,
                },
                major_axis: grib2::template::param_set::ScaledValue {
                    scale_factor: 0xff,
                    scaled_value: 0xffff_ffff,
                },
                minor_axis: grib2::template::param_set::ScaledValue {
                    scale_factor: 0xff,
                    scaled_value: 0xffff_ffff,
                },
            },
            lat_lon: grib2::template::param_set::LatLonGrid::from(&encoder::LatLonGrid {
                shape: (ni, nj),
                first_point: (lats[0], lngs[0]),
                last_point: (*lats.last().unwrap(), *lngs.last().unwrap()),
                i_consecutive: true,
            }),
        }),
    };

    let ident = grib2::Section1Payload {
        centre_id: 7,
        subcentre_id: 0,
        master_table_version: 2,
        local_table_version: 1,
        ref_time_significance: 1,
        ref_time: grib::def::grib2::template::param_set::DateTime {
            year: 2026,
            month: 10,
            day: 15,
            hour: 12,
            minute: 0,
            second: 0,
        },
        prod_status: 0,
        data_type: 1,
        optional: None,
    };

    // Section 4 from octet 6: the encoder writes the length and section number itself.
    let mut product_bytes = vec![
        0, 0, // 0 coordinates
        0, 0,        // template 4.0
        category, // parameter category
        number,   // parameter number
        2,        // forecast
        0, 0, // generating process
        0, 0, 0, // cutoff
        1, // hour
        0, 0, 0, 0,       // forecast time
        surface, // first fixed surface
        0,       // scale factor
    ];
    product_bytes.extend_from_slice(&level.to_be_bytes()); // first fixed surface value
    product_bytes.extend_from_slice(&[255, 0, 0, 0, 0, 0]); // no second surface

    let encoded = encoder::GpvEncoder::new(
        std::borrow::Cow::Borrowed(&values_f64),
        encoder::EncodingMethod::SimplePacking(encoder::SimplePackingStrategy::Decimal(2)),
    );

    let message = encoder::SingleGrib2Message::new(
        0,
        &ident,
        None::<&[u8]>,
        grid,
        product_bytes.as_slice(),
        encoded,
    );

    let mut buf = vec![0; message.num_octets()];
    message.write(&mut buf).unwrap();
    buf
}

#[test]
fn cycle_math_and_nearest_steps() {
    // GFS cycle: 5-hour lag. 17:00 UTC -> 12:00 UTC shifted -> 06:00 cycle
    let dt =
        jiff::civil::date(2026, 10, 15).at(17, 0, 0, 0).to_zoned(jiff::tz::TimeZone::UTC).unwrap();
    let now_ms = dt.timestamp().as_millisecond();
    let (gfs_date, gfs_hour) = select_latest_gfs_cycle(now_ms, 5 * 3_600_000);
    assert_eq!(gfs_date, "20261015");
    assert_eq!(gfs_hour, 12);

    // GFS steps: hourly <= 120, 3-hourly > 120
    assert_eq!(nearest_gfs_step(0.4), 0);
    assert_eq!(nearest_gfs_step(1.2), 1);
    assert_eq!(nearest_gfs_step(119.6), 120);
    assert_eq!(nearest_gfs_step(121.0), 120);
    assert_eq!(nearest_gfs_step(122.0), 123);
    assert_eq!(nearest_gfs_step(400.0), 384);

    // IFS cycle: 6-hour lag
    let (ifs_date, ifs_hour) = select_latest_ifs_cycle(now_ms, 6 * 3_600_000);
    assert_eq!(ifs_date, "20261015");
    assert_eq!(ifs_hour, 6);

    // IFS steps: 3-hourly <= 144, 6-hourly > 144
    assert_eq!(nearest_ifs_step(1.0, 12), 0);
    assert_eq!(nearest_ifs_step(4.0, 12), 3);
    assert_eq!(nearest_ifs_step(145.0, 12), 144);
    assert_eq!(nearest_ifs_step(148.0, 12), 150);
    assert_eq!(nearest_ifs_step(400.0, 6), 144); // 06z caps at 144
    assert_eq!(nearest_ifs_step(400.0, 12), 360); // 12z caps at 360
}

#[test]
fn index_parsing_and_range_extraction() {
    let gfs_idx = r"
1:0:d=2026101512:UGRD:10 m above ground:3 hour fcst:
2:1000:d=2026101512:VGRD:10 m above ground:3 hour fcst:
3:2000:d=2026101512:TMP:2 m above ground:3 hour fcst:
4:3000:d=2026101512:PRMSL:mean sea level:3 hour fcst:
5:4000:d=2026101512:HGT:surface:3 hour fcst:
";
    let gfs_entries = parse_gfs_idx(gfs_idx).unwrap();
    assert_eq!(gfs_entries.len(), 5);

    let ranges = gfs_wind_ranges(&gfs_entries, WeatherOverlay::Temperature).unwrap();
    assert_eq!(ranges.u, ByteRange { start: 0, end: 999 });
    assert_eq!(ranges.v, ByteRange { start: 1000, end: 1999 });
    assert_eq!(ranges.scalar, Some(ByteRange { start: 2000, end: 2999 }));

    let ranges_pr = gfs_wind_ranges(&gfs_entries, WeatherOverlay::Pressure).unwrap();
    assert_eq!(ranges_pr.scalar, Some(ByteRange { start: 3000, end: 3999 }));

    let ifs_index = r#"
{"param": "10u", "_offset": 0, "_length": 500}
{"param": "10v", "_offset": 500, "_length": 500}
{"param": "2t", "_offset": 1000, "_length": 600}
{"param": "msl", "_offset": 1600, "_length": 700}
"#;
    let ifs_entries = parse_ifs_index(ifs_index).unwrap();
    assert_eq!(ifs_entries.len(), 4);

    let ifs_r = ifs_wind_ranges(&ifs_entries, WeatherOverlay::Temperature).unwrap();
    assert_eq!(ifs_r.u, ByteRange { start: 0, end: 499 });
    assert_eq!(ifs_r.v, ByteRange { start: 500, end: 999 });
    assert_eq!(ifs_r.scalar, Some(ByteRange { start: 1000, end: 1599 }));
}

#[test]
fn ifs_index_ranges_that_overflow_are_rejected_not_wrapped() {
    let index = r#"
{"param": "10u", "_offset": 18446744073709551615, "_length": 10}
{"param": "10v", "_offset": 500, "_length": 500}
"#;
    let entries = parse_ifs_index(index).unwrap();
    assert!(ifs_wind_ranges(&entries, WeatherOverlay::None).is_err());
}

#[test]
fn decodes_grib2_and_resamples_geometry() {
    let lats = [90.0, 0.0, -90.0];
    let lngs = [0.0, 90.0, 180.0, 270.0];
    let values_u = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0];
    let values_v = [21.0f32, 22.0, 23.0, 24.0, 25.0, 26.0, 27.0, 28.0, 29.0, 30.0, 31.0, 32.0];
    let values_t = [293.15f32; 12]; // 20.0 °C

    let grib_u = create_test_grib2(U_10M, &lats, &lngs, &values_u);
    let grib_v = create_test_grib2(V_10M, &lats, &lngs, &values_v);
    let grib_t = create_test_grib2(T_2M, &lats, &lngs, &values_t);

    let dec_u = decode_grib_message(&grib_u).unwrap();
    let dec_v = decode_grib_message(&grib_v).unwrap();
    let dec_t = decode_grib_message(&grib_t).unwrap();

    assert_eq!(dec_u.ni, 4);
    assert_eq!(dec_u.nj, 3);
    assert_eq!(dec_u.lo1, 0.0);
    assert_eq!(dec_u.la1, 90.0);

    // Resample with dx=90, dy=90 -> 4x3 points
    let resampled =
        resample_wind_grid(&dec_u, &dec_v, Some(&dec_t), WeatherOverlay::Temperature, 90.0, 90.0)
            .unwrap();

    assert_eq!(resampled.nx, 4);
    assert_eq!(resampled.ny, 3);
    assert_eq!(resampled.u.len(), 12);
    assert_eq!(resampled.v.len(), 12);
    let scalar = resampled.scalar.as_ref().unwrap();
    assert_eq!(scalar.len(), 12);
    // 293.15 K - 273.15 = 20.0 °C
    assert!((scalar[0] - 20.0).abs() < 0.1);

    // Pack into little-endian bytes
    let bytes = pack_grid_binary(&resampled);
    assert_eq!(bytes.len(), 12 * 4 * 3); // 12 points * 4 bytes/float * 3 components
}

#[test]
fn resampling_rejects_invalid_dimensions_spacing_and_short_buffers() {
    let grid = DecodedMessage {
        ni: 2,
        nj: 2,
        lo1: 0.0,
        la1: 90.0,
        di: 180.0,
        dj: 180.0,
        values: vec![1.0; 4],
    };
    for spacing in [f64::NAN, f64::INFINITY, 0.0, 0.1, 181.0] {
        assert!(
            resample_wind_grid(&grid, &grid, None, WeatherOverlay::None, spacing, 90.0).is_err()
        );
    }
    for invalid in [
        DecodedMessage { ni: 0, ..grid.clone() },
        DecodedMessage { ni: usize::MAX, ..grid.clone() },
        DecodedMessage { values: vec![1.0], ..grid.clone() },
        DecodedMessage { di: f64::NAN, ..grid.clone() },
        // Two 45° columns cover 90° of longitude: wrapping east would invent data.
        DecodedMessage { di: 45.0, ..grid.clone() },
    ] {
        assert!(
            resample_wind_grid(&invalid, &invalid, None, WeatherOverlay::None, 90.0, 90.0).is_err()
        );
    }
    assert!(
        resample_wind_grid(
            &grid,
            &grid,
            Some(&DecodedMessage { values: vec![], ..grid.clone() }),
            WeatherOverlay::Temperature,
            90.0,
            90.0
        )
        .is_err()
    );
}

#[test]
fn resampling_survives_longitudes_that_round_onto_the_wrap_point() {
    // (0 − lo1) / di is about −1e-17, which rem_euclid rounds up to exactly ni.
    let grid = DecodedMessage {
        ni: 4,
        nj: 3,
        lo1: 9e-16,
        la1: 90.0,
        di: 90.0,
        dj: 90.0,
        values: (0..12u8).map(f32::from).collect(),
    };
    let resampled =
        resample_wind_grid(&grid, &grid, None, WeatherOverlay::None, 90.0, 90.0).unwrap();
    assert!((resampled.u[0] - 0.0).abs() < 1e-3, "{}", resampled.u[0]);
}

#[test]
fn packed_grid_bytes_are_little_endian_f32_u_then_v_then_scalar() {
    let grid = ResampledGrid {
        nx: 2,
        ny: 1,
        lo1: 0.0,
        la1: 90.0,
        dx: 180.0,
        dy: 180.0,
        u: vec![1.5, -2.0],
        v: vec![0.25, 3.0],
        scalar: Some(vec![1013.25, -0.5]),
    };
    assert_eq!(
        pack_grid_binary(&grid),
        [
            [0x00, 0x00, 0xc0, 0x3f], // U  1.5
            [0x00, 0x00, 0x00, 0xc0], // U -2.0
            [0x00, 0x00, 0x80, 0x3e], // V  0.25
            [0x00, 0x00, 0x40, 0x40], // V  3.0
            [0x00, 0x50, 0x7d, 0x44], // scalar 1013.25
            [0x00, 0x00, 0x00, 0xbf], // scalar -0.5
        ]
        .concat()
    );
    assert_eq!(pack_grid_binary(&ResampledGrid { scalar: None, ..grid }).len(), 16);
}

#[test]
fn the_licence_gate_applies_to_each_model_source() {
    // In commercial mode a source must be commercially allowed or acknowledged.
    let mut unverified = sources::all();
    for def in &mut unverified {
        if def.info.id == sources::ECMWF_IFS {
            def.info.commercial = CommercialUse::Unknown;
        }
    }
    let registry = Registry::with_sources(unverified.clone(), LicenceMode::Commercial, &[]);
    let refused = licence_gate(&registry, WindModel::Ifs).unwrap_err();
    assert_eq!(ApiError::from(&refused).code, ErrorCode::DisabledByLicence);
    assert!(licence_gate(&registry, WindModel::Gfs).is_ok());

    let acknowledged = [sources::ECMWF_IFS.to_owned()];
    let registry = Registry::with_sources(unverified, LicenceMode::Commercial, &acknowledged);
    assert!(licence_gate(&registry, WindModel::Ifs).is_ok());

    // The real registry: both are open licences, permitted in either mode.
    for mode in [LicenceMode::Evaluation, LicenceMode::Commercial] {
        let registry = Registry::new(mode, &[]);
        assert!(licence_gate(&registry, WindModel::Gfs).is_ok());
        assert!(licence_gate(&registry, WindModel::Ifs).is_ok());
    }
    assert_eq!(source_id(WindModel::Gfs), sources::NOAA_GFS);
    assert_eq!(source_id(WindModel::Ifs), sources::ECMWF_IFS);
}

fn test_fetch_config() -> FetchConfig {
    FetchConfig {
        allow_http_hosts: vec!["127.0.0.1".into()],
        retries: 0,
        connect_timeout_s: 2,
        request_timeout_s: 2,
        ..FetchConfig::default()
    }
}

#[tokio::test]
async fn disabled_wind_rejects_every_variant_without_contacting_upstream() {
    let server = MockServer::start().await;
    let root = Url::parse(&format!("{}/", server.uri())).unwrap();
    let config = WindConfig {
        enabled: false,
        gfs_base_url: root.clone(),
        ifs_base_url: root,
        ..WindConfig::default()
    };
    let provider = WindProvider::new(
        Arc::new(Fetcher::new(test_fetch_config()).unwrap()),
        config,
        Arc::new(HealthTracker::default()),
    );
    for model in [WindModel::Gfs, WindModel::Ifs] {
        for overlay in [WeatherOverlay::None, WeatherOverlay::Temperature, WeatherOverlay::Pressure]
        {
            let error = provider.manifest(model, overlay).await.unwrap_err();
            assert_eq!(
                lavender_domain::ApiError::from(&error).code,
                lavender_domain::ErrorCode::Unconfigured
            );
            let error = provider.grid(model, overlay, "any.bin").await.unwrap_err();
            assert_eq!(
                lavender_domain::ApiError::from(&error).code,
                lavender_domain::ErrorCode::Unconfigured
            );
        }
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn mock_upstream_manifest_status_and_grid_flow() {
    let server = MockServer::start().await;
    let lats = [90.0, 0.0, -90.0];
    let lngs = [0.0, 90.0, 180.0, 270.0];
    let values_u = [1.0f32; 12];
    let values_v = [2.0f32; 12];
    let grib_u = create_test_grib2(U_10M, &lats, &lngs, &values_u);
    let grib_v = create_test_grib2(V_10M, &lats, &lngs, &values_v);

    let idx = format!(
        "1:0:d=2026101512:UGRD:10 m above ground:anl:\n2:{}:d=2026101512:VGRD:10 m above ground:anl:\n3:{}:d=2026101512:HGT:surface:anl:\n",
        grib_u.len(),
        grib_u.len() + grib_v.len()
    );

    let mut combined_grib = Vec::new();
    combined_grib.extend_from_slice(&grib_u);
    combined_grib.extend_from_slice(&grib_v);

    Mock::given(method("GET"))
        .and(path("/gfs.20261015/12/atmos/gfs.t12z.pgrb2.0p25.f005.idx"))
        .respond_with(ResponseTemplate::new(200).set_body_string(idx))
        .mount(&server)
        .await;

    let eastward_range = format!("bytes=0-{}", grib_u.len() - 1);
    Mock::given(method("GET"))
        .and(path("/gfs.20261015/12/atmos/gfs.t12z.pgrb2.0p25.f005"))
        .and(header("range", eastward_range.as_str()))
        .respond_with(ResponseTemplate::new(206).set_body_bytes(grib_u.clone()))
        .mount(&server)
        .await;

    let northward_range = format!("bytes={}-{}", grib_u.len(), grib_u.len() + grib_v.len() - 1);
    Mock::given(method("GET"))
        .and(path("/gfs.20261015/12/atmos/gfs.t12z.pgrb2.0p25.f005"))
        .and(header("range", northward_range.as_str()))
        .respond_with(ResponseTemplate::new(206).set_body_bytes(grib_v.clone()))
        .mount(&server)
        .await;

    let server_url = Url::parse(&format!("{}/", server.uri())).unwrap();
    let fetcher = Arc::new(Fetcher::new(test_fetch_config()).unwrap());
    let wind_config = WindConfig {
        gfs_base_url: server_url.clone(),
        ifs_base_url: server_url.clone(),
        target_dx: 90.0,
        ..WindConfig::default()
    };

    // Fixed reference time: 2026-10-15T17:00:00Z -> GFS cycle 12z (5h lag), elapsed 5h -> step f005
    let fixed_now = jiff::civil::date(2026, 10, 15)
        .at(17, 0, 0, 0)
        .to_zoned(jiff::tz::TimeZone::UTC)
        .unwrap()
        .timestamp()
        .as_millisecond();

    let provider = Arc::new(
        WindProvider::new(fetcher, wind_config, Arc::new(HealthTracker::default()))
            .with_clock(Arc::new(move || fixed_now)),
    );

    let manifest = provider.manifest(WindModel::Gfs, WeatherOverlay::None).await.unwrap();
    assert_eq!(manifest.model, WindModel::Gfs);
    assert_eq!(manifest.schema_version, 1);
    assert!(!manifest.stale);
    assert!(!manifest.unavailable);
    assert!(manifest.grid_url.is_some());
    let grid_url = manifest.grid_url.unwrap();
    assert!(grid_url.contains("model=gfs"));

    let grid_id = "gfs-20261015-12-f5-90".to_string();
    let grid_bytes = provider.grid(WindModel::Gfs, WeatherOverlay::None, &grid_id).await.unwrap();
    assert_eq!(grid_bytes.len(), 12 * 4 * 2); // 4x3 * 4 bytes * 2 components

    // Unknown grid ID returns 404 / NotFound
    let err = provider.grid(WindModel::Gfs, WeatherOverlay::None, "unknown").await.unwrap_err();
    assert!(matches!(err, lavender_core::CoreError::NotFound(_)));
}

#[tokio::test]
async fn stale_cache_on_upstream_failure_and_generation_retention() {
    let server = MockServer::start().await;
    let lats = [90.0, 0.0, -90.0];
    let lngs = [0.0, 90.0, 180.0, 270.0];
    let values_u = [1.0f32; 12];
    let values_v = [2.0f32; 12];
    let grib_u = create_test_grib2(U_10M, &lats, &lngs, &values_u);
    let grib_v = create_test_grib2(V_10M, &lats, &lngs, &values_v);

    let idx = format!(
        "1:0:d=2026101512:UGRD:10 m above ground:anl:\n2:{}:d=2026101512:VGRD:10 m above ground:anl:\n3:{}:d=2026101512:HGT:surface:anl:\n",
        grib_u.len(),
        grib_u.len() + grib_v.len()
    );

    Mock::given(method("GET"))
        .and(path("/gfs.20261015/12/atmos/gfs.t12z.pgrb2.0p25.f005.idx"))
        .respond_with(ResponseTemplate::new(200).set_body_string(idx.clone()))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let eastward_range = format!("bytes=0-{}", grib_u.len() - 1);
    Mock::given(method("GET"))
        .and(path("/gfs.20261015/12/atmos/gfs.t12z.pgrb2.0p25.f005"))
        .and(header("range", eastward_range.as_str()))
        .respond_with(ResponseTemplate::new(206).set_body_bytes(grib_u.clone()))
        .mount(&server)
        .await;

    let northward_range = format!("bytes={}-{}", grib_u.len(), grib_u.len() + grib_v.len() - 1);
    Mock::given(method("GET"))
        .and(path("/gfs.20261015/12/atmos/gfs.t12z.pgrb2.0p25.f005"))
        .and(header("range", northward_range.as_str()))
        .respond_with(ResponseTemplate::new(206).set_body_bytes(grib_v.clone()))
        .mount(&server)
        .await;

    let server_url = Url::parse(&format!("{}/", server.uri())).unwrap();
    let fetcher = Arc::new(Fetcher::new(test_fetch_config()).unwrap());
    let wind_config = WindConfig {
        gfs_base_url: server_url.clone(),
        ifs_base_url: server_url.clone(),
        target_dx: 90.0,
        ttl_s: 60,
        failure_backoff_s: 60,
        ..WindConfig::default()
    };

    let clock_val = Arc::new(std::sync::atomic::AtomicI64::new(
        jiff::civil::date(2026, 10, 15)
            .at(17, 0, 0, 0)
            .to_zoned(jiff::tz::TimeZone::UTC)
            .unwrap()
            .timestamp()
            .as_millisecond(),
    ));

    let clock_clone = clock_val.clone();
    let health = Arc::new(HealthTracker::default());
    let provider = Arc::new(
        WindProvider::new(fetcher, wind_config, health.clone())
            .with_clock(Arc::new(move || clock_clone.load(std::sync::atomic::Ordering::SeqCst))),
    );

    let first = provider.manifest(WindModel::Gfs, WeatherOverlay::None).await.unwrap();
    assert!(!first.stale);
    assert_eq!(health.health("noaa-gfs", true).state, HealthState::Healthy);

    // A cache hit within the TTL is not an upstream attempt.
    provider.manifest(WindModel::Gfs, WeatherOverlay::None).await.unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 3);

    // Advance clock past TTL (60s)
    clock_val.fetch_add(120_000, std::sync::atomic::Ordering::SeqCst);

    // Second fetch upstream will fail (idx mock was up_to_n_times 1 -> 404)
    let second = provider.manifest(WindModel::Gfs, WeatherOverlay::None).await.unwrap();
    assert!(second.stale);
    assert_eq!(second.reason.as_deref(), Some("Wind upstream unavailable"));
    // Old data is still served, so the failure is reported as degraded, not healthy.
    let degraded = health.health("noaa-gfs", true);
    assert_eq!(degraded.state, HealthState::Degraded);
    assert_eq!(degraded.consecutive_failures, 1);
    assert!(degraded.last_error.is_some() && degraded.last_success.is_some());

    // Previous grid remains readable
    let grid_bytes =
        provider.grid(WindModel::Gfs, WeatherOverlay::None, "gfs-20261015-12-f5-90").await.unwrap();
    assert_eq!(grid_bytes.len(), 12 * 4 * 2);
}

/// GFS lists PRMSL as message 1, so its range starts at byte 0 (as on NOAA's bucket).
#[tokio::test]
async fn overlay_scalars_are_converted_served_and_checked_against_their_index_entry() {
    let server = MockServer::start().await;
    let lats = [90.0, 0.0, -90.0];
    let lngs = [0.0, 90.0, 180.0, 270.0];
    let pascals: Vec<f32> = (0..12u8).map(|i| 100_000.0 + 100.0 * f32::from(i)).collect();
    let kelvins = [293.15f32; 12];
    let pressure = create_test_grib2(PRMSL, &lats, &lngs, &pascals);
    // The index says TMP here, but the message is another field of the same size.
    let not_temperature = create_test_grib2(PRMSL, &lats, &lngs, &kelvins);
    assert_eq!(not_temperature.len(), create_test_grib2(T_2M, &lats, &lngs, &kelvins).len());
    let wind_u = create_test_grib2(U_10M, &lats, &lngs, &[1.0; 12]);
    let wind_v = create_test_grib2(V_10M, &lats, &lngs, &[2.0; 12]);

    let mut offset = 0;
    let mut idx = String::new();
    let object = "/gfs.20261015/12/atmos/gfs.t12z.pgrb2.0p25.f005";
    for (n, (name, level, bytes)) in [
        ("PRMSL", "mean sea level", &pressure),
        ("TMP", "2 m above ground", &not_temperature),
        ("UGRD", "10 m above ground", &wind_u),
        ("VGRD", "10 m above ground", &wind_v),
    ]
    .into_iter()
    .enumerate()
    {
        writeln!(idx, "{}:{offset}:d=2026101512:{name}:{level}:5 hour fcst:", n + 1).unwrap();
        let range = format!("bytes={offset}-{}", offset + bytes.len() - 1);
        Mock::given(method("GET"))
            .and(path(object))
            .and(header("range", range.as_str()))
            .respond_with(ResponseTemplate::new(206).set_body_bytes(bytes.clone()))
            .mount(&server)
            .await;
        offset += bytes.len();
    }
    writeln!(idx, "5:{offset}:d=2026101512:HGT:surface:5 hour fcst:").unwrap();
    Mock::given(method("GET"))
        .and(path(format!("{object}.idx")))
        .respond_with(ResponseTemplate::new(200).set_body_string(idx))
        .mount(&server)
        .await;

    let root = Url::parse(&format!("{}/", server.uri())).unwrap();
    let config = WindConfig {
        gfs_base_url: root.clone(),
        ifs_base_url: root,
        target_dx: 90.0,
        ..WindConfig::default()
    };
    let fixed_now = jiff::civil::date(2026, 10, 15)
        .at(17, 0, 0, 0)
        .to_zoned(jiff::tz::TimeZone::UTC)
        .unwrap()
        .timestamp()
        .as_millisecond();
    let provider = WindProvider::new(
        Arc::new(Fetcher::new(test_fetch_config()).unwrap()),
        config,
        Arc::new(HealthTracker::default()),
    )
    .with_clock(Arc::new(move || fixed_now));

    // Pressure: Pa from the byte-0 message, served as hPa in the third block.
    let manifest = provider.manifest(WindModel::Gfs, WeatherOverlay::Pressure).await.unwrap();
    assert_eq!(manifest.scalar.as_ref().map(|s| s.units.as_str()), Some("hPa"));
    assert!(manifest.scalar_error.is_none());
    let id = manifest.grid_id.unwrap();
    assert!(id.ends_with("-pressure"), "{id}");
    let bytes = provider.grid(WindModel::Gfs, WeatherOverlay::Pressure, &id).await.unwrap();
    assert_eq!(bytes.len(), 12 * 4 * 3);
    for (i, chunk) in bytes[12 * 4 * 2..].as_chunks::<4>().0.iter().enumerate() {
        let hpa = f32::from_le_bytes(*chunk);
        assert!((hpa - (1000.0 + f32::from(u8::try_from(i).unwrap()))).abs() < 1e-3, "{i}: {hpa}");
    }

    // Temperature: the message is not 2 m temperature, so the wind is served without it.
    let manifest = provider.manifest(WindModel::Gfs, WeatherOverlay::Temperature).await.unwrap();
    assert!(manifest.scalar.is_none() && !manifest.unavailable);
    assert_eq!(manifest.scalar_error.as_deref(), Some(TEMPERATURE_UNAVAILABLE));
    let id = manifest.grid_id.unwrap();
    assert!(id.ends_with("-temperature-wind-only"), "{id}");
    let bytes = provider.grid(WindModel::Gfs, WeatherOverlay::Temperature, &id).await.unwrap();
    assert_eq!(bytes.len(), 12 * 4 * 2);
}

/// A caller that gives up mid-refresh (a client disconnect, or the server's request timeout,
/// 30 s by default, which is shorter than the 40 s wind budget) must not discard the
/// refresh: it finishes, fills the cache and records the outcome in health.
#[tokio::test]
async fn a_refresh_completes_and_is_recorded_after_its_caller_gives_up() {
    let server = MockServer::start().await;
    let lats = [90.0, 0.0, -90.0];
    let lngs = [0.0, 90.0, 180.0, 270.0];
    let grib_u = create_test_grib2(U_10M, &lats, &lngs, &[1.0; 12]);
    let grib_v = create_test_grib2(V_10M, &lats, &lngs, &[2.0; 12]);
    let object = "/gfs.20261015/12/atmos/gfs.t12z.pgrb2.0p25.f005";
    let idx = format!(
        "1:0:d=2026101512:UGRD:10 m above ground:5 hour fcst:\n2:{}:d=2026101512:VGRD:10 m above ground:5 hour fcst:\n3:{}:d=2026101512:HGT:surface:5 hour fcst:\n",
        grib_u.len(),
        grib_u.len() + grib_v.len()
    );
    let slow = std::time::Duration::from_millis(300);
    Mock::given(method("GET"))
        .and(path(format!("{object}.idx")))
        .respond_with(ResponseTemplate::new(200).set_body_string(idx).set_delay(slow))
        .mount(&server)
        .await;
    for (range, bytes) in [
        (format!("bytes=0-{}", grib_u.len() - 1), grib_u.clone()),
        (format!("bytes={}-{}", grib_u.len(), grib_u.len() + grib_v.len() - 1), grib_v),
    ] {
        Mock::given(method("GET"))
            .and(path(object))
            .and(header("range", range.as_str()))
            .respond_with(ResponseTemplate::new(206).set_body_bytes(bytes))
            .mount(&server)
            .await;
    }
    // ECMWF answers slowly with nothing.
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404).set_delay(slow))
        .mount(&server)
        .await;

    let root = Url::parse(&format!("{}/", server.uri())).unwrap();
    let config = WindConfig {
        gfs_base_url: root.clone(),
        ifs_base_url: root,
        target_dx: 90.0,
        ..WindConfig::default()
    };
    let fixed_now = jiff::civil::date(2026, 10, 15)
        .at(17, 0, 0, 0)
        .to_zoned(jiff::tz::TimeZone::UTC)
        .unwrap()
        .timestamp()
        .as_millisecond();
    let health = Arc::new(HealthTracker::default());
    let provider = WindProvider::new(
        Arc::new(Fetcher::new(test_fetch_config()).unwrap()),
        config,
        health.clone(),
    )
    .with_clock(Arc::new(move || fixed_now));

    let impatient = std::time::Duration::from_millis(50);
    for model in [WindModel::Gfs, WindModel::Ifs] {
        let answer =
            tokio::time::timeout(impatient, provider.manifest(model, WeatherOverlay::None));
        assert!(answer.await.is_err(), "{model:?}: the caller gave up first");
    }
    let settled = |source: &str, state: HealthState| {
        let health = health.clone();
        let source = source.to_owned();
        async move {
            for _ in 0..250 {
                if health.health(&source, true).state == state {
                    return true;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            false
        }
    };
    assert!(settled(sources::NOAA_GFS, HealthState::Healthy).await, "GFS success recorded");
    assert!(settled(sources::ECMWF_IFS, HealthState::Failing).await, "IFS failure recorded");

    // The next callers are answered from what the abandoned refreshes stored.
    let requests = server.received_requests().await.unwrap().len();
    let gfs = provider.manifest(WindModel::Gfs, WeatherOverlay::None).await.unwrap();
    assert!(!gfs.stale && gfs.grid_id.is_some());
    let ifs = provider.manifest(WindModel::Ifs, WeatherOverlay::None).await.unwrap();
    assert!(ifs.unavailable, "failure backoff, not a new attempt");
    assert_eq!(server.received_requests().await.unwrap().len(), requests);
}
