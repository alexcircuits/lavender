//! Independent satellite.js/Vallado reference comparisons plus archived ISS passes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use jiff::{SignedDuration, Timestamp};
use lavender_domain::{GeoPoint, SatelliteElements};
use lavender_geo::{OrbitError, Propagator};

const ISS_TLE: (&str, &str) = (
    "1 25544U 98067A   08264.51782528 -.00002182  00000-0 -11606-4 0  2927",
    "2 25544  51.6416 247.4627 0006703 130.5360 325.0288 15.72125391563537",
);

fn elements(tle: (&str, &str)) -> SatelliteElements {
    let tle = sgp4::Elements::from_tle(None, tle.0.as_bytes(), tle.1.as_bytes()).unwrap();
    SatelliteElements {
        norad_id: u32::try_from(tle.norad_id).unwrap(),
        name: "Archived ISS".into(),
        intl_designator: tle.international_designator,
        group: "stations".into(),
        epoch: format!("{}Z", tle.datetime.format("%Y-%m-%dT%H:%M:%S%.f")).parse().unwrap(),
        mean_motion: tle.mean_motion,
        eccentricity: tle.eccentricity,
        inclination_deg: tle.inclination,
        raan_deg: tle.right_ascension,
        arg_of_pericenter_deg: tle.argument_of_perigee,
        mean_anomaly_deg: tle.mean_anomaly,
        bstar: tle.drag_term,
        mean_motion_dot: tle.mean_motion_dot,
        mean_motion_ddot: tle.mean_motion_ddot,
        rev_at_epoch: None,
        classification: None,
    }
}

fn rows(fixture: &str) -> impl Iterator<Item = Vec<&str>> {
    fixture.lines().filter(|l| !l.starts_with('#') && !l.is_empty()).map(|l| l.split(',').collect())
}

#[test]
fn independent_satellite_js_iss_positions_and_look_angles_match() {
    let propagator = Propagator::new(&elements(ISS_TLE)).unwrap();
    let observer = GeoPoint::new(30.2672, -97.7431).unwrap().with_altitude(Some(185.0)).unwrap();
    for row in rows(include_str!("fixtures/iss-satellite-js-vectors.csv")) {
        let at: Timestamp = row[0].parse().unwrap();
        let reference: Vec<f64> = row[1..].iter().map(|v| v.parse().unwrap()).collect();
        let got = propagator.teme_km(at).unwrap();
        let error_km = (0..3).map(|i| (got[i] - reference[i]).powi(2)).sum::<f64>().sqrt();
        // satellite.js propagates with WGS72, Lavender with WGS84. This envelope covers
        // that documented model difference over a day, without claiming identical models.
        assert!(error_km < 0.1, "{at}: TEME error {error_km}km, {got:?}");
        let look = propagator.look_angles(observer, at).unwrap();
        assert!((look.elevation_deg - reference[3]).abs() < 0.01, "{at}: {look:?}");
        let azimuth_error = (look.azimuth_deg - reference[4] + 180.0).rem_euclid(360.0) - 180.0;
        assert!(azimuth_error.abs() < 0.01, "{at}: {look:?}");
        assert!((look.range_km - reference[5]).abs() < 0.1, "{at}: {look:?}");
    }
}

#[test]
fn vallado_published_vanguard_vectors_match_with_wgs84_model_tolerance() {
    let elements = elements((
        "1 00005U 58002B   00179.78495062  .00000023  00000-0  28098-4 0  4753",
        "2 00005  34.2682 348.7242 1859667 331.7664  19.3264 10.82419157413667",
    ));
    let propagator = Propagator::new(&elements).unwrap();
    // AIAA 2006-6753 verification satellite 00005, tcppver.out; WGS72 reference.
    let vectors = [
        (0, [7_022.465_292_66, -1_400.082_967_55, 0.039_951_55]),
        (360, [-7_154.031_202_02, -3_783.176_825_04, -3_536.194_122_94]),
        (1_440, [-938.559_239_43, -6_268.187_488_31, -4_294.029_247_51]),
    ];
    for (minutes, reference) in vectors {
        let at = elements.epoch.checked_add(SignedDuration::from_mins(minutes)).unwrap();
        let position = propagator.teme_km(at).unwrap();
        let error_km = (0..3).map(|i| (position[i] - reference[i]).powi(2)).sum::<f64>().sqrt();
        assert!(error_km < 0.1, "t={minutes}min: error={error_km}km");
    }
}

#[test]
fn archived_iss_passes_match_independent_satellite_js_fixture() {
    let propagator = Propagator::new(&elements(ISS_TLE)).unwrap();
    let observer = GeoPoint::new(30.2672, -97.7431).unwrap();
    let from: Timestamp = "2008-09-20T12:30:00Z".parse().unwrap();
    let passes = propagator.passes(observer, from, 24, 10.0).unwrap();
    let expected: Vec<_> = rows(include_str!("fixtures/iss-austin-passes.csv")).collect();
    assert_eq!(passes.len(), expected.len());
    for (pass, row) in passes.iter().zip(expected) {
        for (got, value) in [(pass.rise, row[0]), (pass.peak, row[1]), (pass.set, row[2])] {
            let reference: Timestamp = value.parse().unwrap();
            assert!(
                got.duration_since(reference).abs().as_secs_f64() < 1.0,
                "{got} vs {reference}"
            );
        }
        let max_elevation: f64 = row[3].parse().unwrap();
        assert!((pass.max_elevation_deg - max_elevation).abs() < 0.1, "{pass:?}");
        assert!(!pass.already_above && !pass.continues_after_window);
        assert!(pass.rise <= pass.peak && pass.peak <= pass.set);
        for crossing in [pass.rise, pass.set] {
            assert!(
                (propagator.look_angles(observer, crossing).unwrap().elevation_deg - 10.0).abs()
                    < 0.03
            );
        }
    }
}

#[test]
fn iss_prediction_handles_poles_antimeridian_empty_and_clipped_windows() {
    let propagator = Propagator::new(&elements(ISS_TLE)).unwrap();
    let from: Timestamp = "2008-09-20T12:30:00Z".parse().unwrap();
    for lat in [-90.0, 90.0] {
        assert_eq!(
            propagator.passes(GeoPoint::new(lat, 180.0).unwrap(), from, 24, 0.0).unwrap(),
            Vec::new()
        );
    }
    let east = propagator.passes(GeoPoint::new(20.0, 180.0).unwrap(), from, 24, 0.0).unwrap();
    let west = propagator.passes(GeoPoint::new(20.0, -180.0).unwrap(), from, 24, 0.0).unwrap();
    assert_eq!(east.len(), west.len());
    for (a, b) in east.iter().zip(west) {
        assert!(a.peak.duration_since(b.peak).abs().as_secs_f64() < 0.05);
    }
    let observer = GeoPoint::new(30.2672, -97.7431).unwrap();
    assert_eq!(propagator.passes(observer, from, 1, 89.0).unwrap(), Vec::new());
    let passes = propagator.passes(observer, from, 24, 10.0).unwrap();
    let start = passes[0].peak;
    let clipped = propagator.passes(observer, start, 1, 10.0).unwrap();
    assert!(clipped[0].already_above);
    assert_eq!(clipped[0].rise, start);
    let start = passes[0].peak.checked_sub(SignedDuration::from_hours(1)).unwrap();
    let clipped = propagator.passes(observer, start, 1, 10.0).unwrap();
    assert!(clipped[0].continues_after_window);
    assert_eq!(clipped[0].set, passes[0].peak);
}

#[test]
fn public_geometry_and_pass_api_validate_parameters() {
    let propagator = Propagator::new(&elements(ISS_TLE)).unwrap();
    let mut observer = GeoPoint::new(0.0, 0.0).unwrap();
    observer.alt_m = Some(20_000.0);
    assert!(matches!(
        propagator.look_angles(observer, elements(ISS_TLE).epoch),
        Err(OrbitError::InvalidRequest(_))
    ));
    assert!(matches!(
        propagator.passes(observer, elements(ISS_TLE).epoch, 24, 0.0),
        Err(OrbitError::InvalidRequest(_))
    ));
}
