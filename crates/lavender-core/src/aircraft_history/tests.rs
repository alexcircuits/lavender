use super::*;

fn aircraft(hex: &str, lon: f64, seen_pos_s: f64) -> Aircraft {
    Aircraft {
        hex: hex.to_owned(),
        callsign: None,
        registration: None,
        type_code: None,
        category: None,
        position: GeoPoint::new(60.0, lon).unwrap().with_altitude(Some(9_144.0)).unwrap(),
        altitude_baro_ft: Some(30_000.0),
        altitude_geom_ft: None,
        on_ground: false,
        ground_speed_kt: None,
        track_deg: None,
        vertical_rate_fpm: None,
        squawk: None,
        emergency: None,
        is_military: false,
        seen_pos_s,
        source: "adsb-lol".to_owned(),
    }
}

fn history(tracks: usize, points_per_track: usize, total_points: usize) -> AircraftHistory {
    AircraftHistory {
        state: Mutex::new(State::default()),
        limits: Limits { tracks, points_per_track, total_points, ..Limits::default() },
    }
}

fn time(seconds: i64) -> Timestamp {
    Timestamp::from_second(1_700_000_000 + seconds).unwrap()
}

fn capture(history: &AircraftHistory, rows: &[Aircraft], seconds: u32, at: Instant) {
    let timestamp = time(i64::from(seconds));
    history.capture_at(rows, timestamp, timestamp, at + Duration::from_secs(u64::from(seconds)));
}

#[test]
fn per_aircraft_rings_drop_oldest_points() {
    let history = history(8, 3, 10);
    let at = Instant::now();
    for i in 0..10 {
        capture(&history, &[aircraft("abcdef", 20.0 + f64::from(i), 0.0)], i, at);
    }
    let track = history.track_at("ABCDEF", at + Duration::from_secs(10)).unwrap();
    assert_eq!(track.points.len(), 3);
    assert_eq!(track.points[0].time, time(7));
    assert_eq!(track.points[2].position.alt_m, Some(9_144.0));
    let state = history.lock();
    assert_eq!(state.total_points, 3);
    assert_eq!(state.oldest.len(), 1);
}

#[test]
fn aircraft_cap_evicts_least_recently_observed() {
    let history = history(2, 3, 6);
    let at = Instant::now();
    capture(&history, &[aircraft("000001", 1.0, 0.0)], 0, at);
    capture(&history, &[aircraft("000002", 2.0, 0.0)], 1, at);
    capture(&history, &[aircraft("000001", 3.0, 0.0)], 2, at);
    capture(&history, &[aircraft("000003", 4.0, 0.0)], 3, at);
    let state = history.lock();
    assert_eq!(state.tracks.len(), 2);
    assert!(!state.tracks.contains_key("000002"));
    assert_eq!(state.oldest.len(), 2);
    assert_eq!(state.total_points, 3);
}

#[test]
fn global_point_cap_evicts_whole_oldest_track() {
    let history = history(8, 3, 3);
    let at = Instant::now();
    capture(&history, &[aircraft("000001", 1.0, 0.0)], 0, at);
    capture(&history, &[aircraft("000001", 2.0, 0.0)], 1, at);
    capture(&history, &[aircraft("000002", 3.0, 0.0)], 2, at);
    capture(&history, &[aircraft("000002", 4.0, 0.0)], 3, at);
    let state = history.lock();
    assert_eq!(state.total_points, 2);
    assert!(!state.tracks.contains_key("000001"));
    assert_eq!(state.tracks["000002"].points.len(), 2);
}

#[test]
fn eviction_bookkeeping_stays_bounded_under_many_refreshes() {
    let history = history(8, 5, 20);
    let at = Instant::now();
    for i in 0..1_000 {
        let hex = format!("{:06x}", i % 11);
        capture(&history, &[aircraft(&hex, f64::from(i % 180), 0.0)], i, at);
        let state = history.lock();
        assert!(state.tracks.len() <= 8);
        assert!(state.total_points <= 20);
        assert!(state.tracks.values().all(|t| t.points.len() <= 5));
        assert_eq!(state.oldest.len(), state.tracks.len());
        assert_eq!(
            state.total_points,
            state.tracks.values().map(|t| t.points.len()).sum::<usize>()
        );
    }
}

#[test]
fn idle_ttl_expires_without_reads_extending_it() {
    let history = history(8, 3, 10);
    let at = Instant::now();
    capture(&history, &[aircraft("abcdef", 20.0, 0.0)], 0, at);
    let just_before = (at + IDLE_TTL).checked_sub(Duration::from_millis(1)).unwrap();
    assert!(history.track_at("abcdef", just_before).is_ok());
    assert!(matches!(history.track_at("abcdef", at + IDLE_TTL), Err(CoreError::NotFound(_))));
    assert_eq!(history.lock().total_points, 0);
    assert!(history.lock().oldest.is_empty());
}

#[test]
fn cached_observations_do_not_keep_expired_tracks_alive() {
    let history = history(8, 3, 10);
    let at = Instant::now();
    let row = aircraft("abcdef", 20.0, 0.0);
    capture(&history, std::slice::from_ref(&row), 0, at);
    history.capture_at(std::slice::from_ref(&row), time(0), time(1_799), at + IDLE_TTL / 2);
    assert!(matches!(history.track_at("abcdef", at + IDLE_TTL), Err(CoreError::NotFound(_))));
    history.capture_at(&[row], time(0), time(1_800), at + IDLE_TTL);
    assert!(history.lock().tracks.is_empty(), "an aged cache cannot recreate an expired trail");
}

#[test]
fn times_use_provider_age_and_skip_duplicates_or_older_observations() {
    let history = history(8, 10, 20);
    let at = Instant::now();
    capture(&history, &[aircraft("abcdef", 20.0, 2.5)], 10, at);
    capture(&history, &[aircraft("abcdef", 21.0, 3.5)], 11, at); // Same fix time.
    capture(&history, &[aircraft("abcdef", 22.0, 10.0)], 12, at); // Older fix.
    capture(&history, &[aircraft("abcdef", 20.0, 0.0)], 13, at); // Stationary repeat.
    capture(&history, &[aircraft("abcdef", 24.0, 2.0)], 14, at); // Older than repeat.
    capture(&history, &[aircraft("abcdef", 25.0, 1.25)], 15, at);
    let track = history.track_at("abcdef", at + Duration::from_secs(16)).unwrap();
    assert_eq!(track.points.len(), 2);
    assert_eq!(
        track.points[0].time,
        time(10).checked_sub(SignedDuration::from_millis(2_500)).unwrap()
    );
    assert_eq!(
        track.points[1].time,
        time(15).checked_sub(SignedDuration::from_millis(1_250)).unwrap()
    );
    assert!(track.points.windows(2).all(|p| p[0].time < p[1].time));
}

#[test]
fn future_provider_as_of_is_clamped_to_now() {
    let history = history(8, 3, 10);
    let at = Instant::now();
    history.capture_at(&[aircraft("abcdef", 20.0, 2.0)], time(20), time(10), at);
    let track = history.track_at("abcdef", at).unwrap();
    assert_eq!(track.points[0].time, time(8));
}

#[test]
fn invalid_age_hex_and_position_are_skipped_and_tis_b_is_distinct() {
    let history = history(8, 3, 20);
    let at = Instant::now();
    let mut invalid_position = aircraft("000001", 20.0, 0.0);
    invalid_position.position.lat = f64::NAN;
    capture(
        &history,
        &[
            aircraft("nothex", 20.0, 0.0),
            aircraft("ab~def", 20.0, 0.0),
            aircraft("~~abcdef", 20.0, 0.0),
            aircraft("000002", 20.0, -1.0),
            aircraft("000003", 20.0, f64::NAN),
            aircraft("000004", 20.0, f64::INFINITY),
            aircraft("000005", 20.0, f64::MAX),
            invalid_position,
            aircraft("abcdef", 21.0, 0.0),
            aircraft(" ~ABCDEF ", 22.0, 0.0),
        ],
        0,
        at,
    );
    assert_eq!(history.lock().tracks.len(), 2);
    assert_eq!(history.track_at("~ABCDEF", at).unwrap().hex, "~abcdef");
    assert_eq!(history.track_at("abcdef", at).unwrap().points[0].position.lon, 21.0);
    for hex in ["ab~def", "~~abcdef", "abcde", "abcdefg", "abcdef/", "ébcdef"] {
        assert!(matches!(history.track_at(hex, at), Err(CoreError::BadRequest(_))));
    }
    assert!(matches!(history.track_at("123456", at), Err(CoreError::NotFound(_))));
}

#[test]
fn provenance_only_lists_sources_of_retained_points() {
    let history = history(8, 2, 20);
    let at = Instant::now();
    let mut row = aircraft("abcdef", 20.0, 0.0);
    row.source = "old-source".into();
    capture(&history, &[row], 0, at);
    capture(&history, &[aircraft("abcdef", 21.0, 0.0)], 1, at);
    let mut row = aircraft("abcdef", 22.0, 0.0);
    row.source = "new-source".into();
    capture(&history, &[row], 2, at);
    let track = history.track_at("abcdef", at + Duration::from_secs(3)).unwrap();
    assert_eq!(track.sources, vec!["adsb-lol", "new-source"]);
}
