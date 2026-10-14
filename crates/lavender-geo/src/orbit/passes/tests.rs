use super::*;
use std::cell::Cell;

fn from() -> Timestamp {
    "2024-01-01T00:00:00Z".parse().unwrap()
}

#[test]
fn observer_frame_handles_axes_poles_and_altitude() {
    let frame = ObserverFrame::new(GeoPoint::new(0.0, 0.0).unwrap()).unwrap();
    let overhead = frame.look([WGS84_A_KM + 400.0, 0.0, 0.0]).unwrap();
    assert!((overhead.elevation_deg - 90.0).abs() < 1e-9);
    assert!((overhead.range_km - 400.0).abs() < 1e-9);
    let east = frame.look([WGS84_A_KM, 400.0, 0.0]).unwrap();
    assert!(east.elevation_deg.abs() < 1e-9);
    assert!((east.azimuth_deg - 90.0).abs() < 1e-9);
    let high =
        ObserverFrame::new(GeoPoint::new(0.0, 0.0).unwrap().with_altitude(Some(1_000.0)).unwrap())
            .unwrap();
    assert!((high.look([WGS84_A_KM + 400.0, 0.0, 0.0]).unwrap().range_km - 399.0).abs() < 1e-9);
    for lat in [-90.0, 90.0] {
        let pole = ObserverFrame::new(GeoPoint::new(lat, 180.0).unwrap()).unwrap();
        let look = pole.look([0.0, 0.0, (6_356.752_314_245 + 400.0) * lat.signum()]).unwrap();
        assert!((look.elevation_deg - 90.0).abs() < 1e-6);
        assert!(look.azimuth_deg.is_finite());
    }
}

#[test]
fn observer_validation_rejects_nonfinite_coordinates_and_bad_altitudes() {
    let mut observer = GeoPoint::new(0.0, 0.0).unwrap();
    for altitude in [-501.0, 10_001.0, f64::NAN, f64::INFINITY] {
        observer.alt_m = Some(altitude);
        assert!(matches!(ObserverFrame::new(observer), Err(OrbitError::InvalidRequest(_))));
    }
    for altitude in [-500.0, 10_000.0] {
        observer.alt_m = Some(altitude);
        assert!(ObserverFrame::new(observer).is_ok());
    }
    observer.lat = f64::NAN;
    assert!(ObserverFrame::new(observer).is_err());
}

#[test]
fn always_above_has_both_clipping_flags_and_empty_is_empty() {
    let from = from();
    let passes = predict(from, 1, 0.0, |_| Ok(30.0)).unwrap();
    assert_eq!(passes.len(), 1);
    assert_eq!(passes[0].rise, from);
    assert_eq!(passes[0].set, from.checked_add(SignedDuration::from_hours(1)).unwrap());
    assert!(passes[0].already_above && passes[0].continues_after_window);
    assert_eq!(passes[0].max_elevation_deg, 30.0);
    assert_eq!(predict(from, 24, 0.0, |_| Ok(-30.0)).unwrap(), Vec::new());
}

#[test]
fn clipped_peak_is_the_maximum_within_the_window() {
    let from = from();
    let passes =
        predict(from, 1, 0.0, |at| Ok(30.0 - at.duration_since(from).as_secs_f64() / 10.0))
            .unwrap();
    assert_eq!(passes.len(), 1);
    assert!(passes[0].already_above);
    assert!(!passes[0].continues_after_window);
    assert_eq!(passes[0].peak, from);
    assert!(
        (passes[0].set.duration_since(from).as_secs_f64() - 300.0).abs() <= BOUNDARY_TOLERANCE_S
    );

    let passes =
        predict(from, 1, 0.0, |at| Ok((at.duration_since(from).as_secs_f64() - 3_300.0) / 10.0))
            .unwrap();
    assert!(!passes[0].already_above);
    assert!(passes[0].continues_after_window);
    assert_eq!(passes[0].peak, passes[0].set);
}

#[test]
fn high_threshold_narrow_pass_is_found_between_coarse_samples() {
    let from = from();
    let passes = predict(from, 1, 89.0, |at| {
        let seconds = at.duration_since(from).as_secs_f64();
        Ok((90.0 - (seconds - 117.3).powi(2)).max(-90.0))
    })
    .unwrap();
    assert_eq!(passes.len(), 1);
    let pass = &passes[0];
    assert!((pass.rise.duration_since(from).as_secs_f64() - 116.3).abs() < 0.1);
    assert!((pass.peak.duration_since(from).as_secs_f64() - 117.3).abs() < 0.05);
    assert!((pass.set.duration_since(from).as_secs_f64() - 118.3).abs() < 0.1);
    assert!(pass.max_elevation_deg > 89.99);
}

#[test]
fn narrow_passes_inside_the_first_and_last_coarse_intervals_are_not_lost() {
    let from = from();
    for peak_s in [3.0, 3_597.0] {
        let passes = predict(from, 1, 89.0, |at| {
            let seconds = at.duration_since(from).as_secs_f64();
            Ok((90.0 - (seconds - peak_s).powi(2)).max(-90.0))
        })
        .unwrap();
        assert_eq!(passes.len(), 1);
        assert!((passes[0].peak.duration_since(from).as_secs_f64() - peak_s).abs() < 0.05);
        assert!((passes[0].rise.duration_since(from).as_secs_f64() - (peak_s - 1.0)).abs() < 0.1);
        assert!((passes[0].set.duration_since(from).as_secs_f64() - (peak_s + 1.0)).abs() < 0.1);
        assert!(!passes[0].already_above && !passes[0].continues_after_window);
    }
}

#[test]
fn twenty_four_hour_scan_is_bounded_and_errors_are_not_below_horizon() {
    let calls = Cell::new(0);
    assert_eq!(
        predict(from(), 24, 0.0, |_| {
            calls.set(calls.get() + 1);
            Ok(-30.0)
        })
        .unwrap(),
        Vec::new()
    );
    assert!(
        (MAX_PASS_COARSE_SAMPLES..=MAX_PASS_COARSE_SAMPLES + MAX_PASS_REFINEMENT_SAMPLES)
            .contains(&calls.get())
    );
    let calls = Cell::new(0);
    let result = predict(from(), 1, 0.0, |_| {
        calls.set(calls.get() + 1);
        if calls.get() == 5 {
            Err(OrbitError::Propagation("decayed orbit".into()))
        } else {
            Ok(30.0)
        }
    });
    assert!(matches!(result, Err(OrbitError::Propagation(_))));
    assert!(matches!(predict(from(), 1, 0.0, |_| Ok(f64::NAN)), Err(OrbitError::Propagation(_))));
}

#[test]
fn pathological_pass_count_or_refinement_has_an_explicit_limit() {
    let from = from();
    let calls = Cell::new(0);
    let result = predict(from, 24, 0.0, |at| {
        calls.set(calls.get() + 1);
        Ok((at.duration_since(from).as_secs_f64() * std::f64::consts::TAU / 40.0).sin() * 45.0)
    });
    assert!(matches!(result, Err(OrbitError::Limit(_))));
    assert!(calls.get() <= MAX_PASS_COARSE_SAMPLES + MAX_PASS_REFINEMENT_SAMPLES);
}

#[test]
fn invalid_windows_and_thresholds_are_rejected_before_sampling() {
    for hours in [0, 25, u32::MAX] {
        assert!(matches!(
            predict(from(), hours, 0.0, |_| panic!("must not sample")),
            Err(OrbitError::InvalidRequest(_))
        ));
    }
    for minimum in [-1.0, 89.1, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            predict(from(), 1, minimum, |_| panic!("must not sample")),
            Err(OrbitError::InvalidRequest(_))
        ));
    }
    assert!(matches!(
        predict(Timestamp::MAX, 1, 0.0, |_| panic!("must not sample")),
        Err(OrbitError::InvalidRequest(_))
    ));
}
