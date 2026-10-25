#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreadable_literal,
    clippy::too_many_lines,
    clippy::assert_is_empty
)]

use lavender_core::search::spatial::{
    evaluate_spatial_filter, point_to_segment_distance_m, search_feed_items,
};
use lavender_domain::{
    Aircraft, BBox, FeedId, FeedItems, GeoPoint, SpatialFilter, SpatialSearchRequest, Vessel,
};

#[test]
fn test_point_to_segment_distance() {
    // Segment along equator from (0, 0) to (0, 10)
    let a = GeoPoint::new(0.0, 0.0).unwrap();
    let b = GeoPoint::new(0.0, 10.0).unwrap();

    // Point exactly at midpoint on segment
    let p_on = GeoPoint::new(0.0, 5.0).unwrap();
    let d_on = point_to_segment_distance_m(&p_on, &a, &b);
    assert!(d_on < 1.0, "Expected distance near zero, got {d_on}");

    // Point 1 degree north of midpoint (1 deg lat ~ 111,195m)
    let p_north = GeoPoint::new(1.0, 5.0).unwrap();
    let d_north = point_to_segment_distance_m(&p_north, &a, &b);
    assert!((d_north - 111_195.0).abs() < 500.0, "Expected distance near 111km, got {d_north}");

    // Point past endpoint b (0, 12) -> nearest point is b (0, 10)
    let p_past = GeoPoint::new(0.0, 12.0).unwrap();
    let d_past = point_to_segment_distance_m(&p_past, &a, &b);
    let d_to_b = lavender_geo::distance_m(&p_past, &b);
    assert!((d_past - d_to_b).abs() < 1.0);
}

#[test]
fn test_spatial_filter_evaluations() {
    let helsinki = GeoPoint::new(60.1699, 24.9384).unwrap();
    let tallinn = GeoPoint::new(59.4370, 24.7536).unwrap();
    let stockholm = GeoPoint::new(59.3293, 18.0686).unwrap();

    // 1. Circle Filter: 100km around Helsinki -> Tallinn (~82km) is inside, Stockholm (~400km) is outside
    let circle = SpatialFilter::Circle { center: helsinki, radius_m: 100_000.0 };
    let res_helsinki = evaluate_spatial_filter(&helsinki, &circle);
    assert!(res_helsinki.is_some());
    assert!(res_helsinki.unwrap().unwrap() < 1.0);

    let res_tallinn = evaluate_spatial_filter(&tallinn, &circle);
    assert!(res_tallinn.is_some());
    assert!((res_tallinn.unwrap().unwrap() - 82_000.0).abs() < 5_000.0);

    let res_stockholm = evaluate_spatial_filter(&stockholm, &circle);
    assert!(res_stockholm.is_none());

    // 2. BBox Filter: Gulf of Finland [24.0, 59.0, 26.0, 61.0]
    let bbox = SpatialFilter::BBox { bbox: BBox::new(24.0, 59.0, 26.0, 61.0).unwrap() };
    assert!(evaluate_spatial_filter(&helsinki, &bbox).is_some());
    assert!(evaluate_spatial_filter(&tallinn, &bbox).is_some());
    assert!(evaluate_spatial_filter(&stockholm, &bbox).is_none());

    // 3. Polygon Filter: Triangle enclosing Helsinki & Tallinn
    let poly = SpatialFilter::Polygon {
        points: vec![
            GeoPoint::new(61.0, 23.0).unwrap(),
            GeoPoint::new(61.0, 27.0).unwrap(),
            GeoPoint::new(59.0, 25.0).unwrap(),
        ],
    };
    assert!(evaluate_spatial_filter(&helsinki, &poly).is_some());
    assert!(evaluate_spatial_filter(&tallinn, &poly).is_some());
    assert!(evaluate_spatial_filter(&stockholm, &poly).is_none());

    // 4. Corridor Filter: Helsinki to Stockholm via Mariehamn route with 20km buffer
    let mariehamn = GeoPoint::new(60.1, 19.9).unwrap();
    let corridor = SpatialFilter::Corridor {
        waypoints: vec![helsinki, mariehamn, stockholm],
        buffer_m: 20_000.0,
    };
    // Mariehamn is a waypoint on the route
    assert!(evaluate_spatial_filter(&mariehamn, &corridor).is_some());
    // Point 5km north of Mariehamn is within 20km buffer
    let near_mariehamn = GeoPoint::new(60.14, 19.9).unwrap();
    assert!(evaluate_spatial_filter(&near_mariehamn, &corridor).is_some());
    // Tallinn is ~80km south of Helsinki -> outside 20km buffer
    assert!(evaluate_spatial_filter(&tallinn, &corridor).is_none());
}

#[test]
fn test_spatial_search_feed_items_execution() {
    let plane1 = Aircraft {
        hex: "4ca123".to_owned(),
        callsign: Some("FIN123".to_owned()),
        registration: Some("OH-LZA".to_owned()),
        type_code: Some("A321".to_owned()),
        category: Some("A3".to_owned()),
        position: GeoPoint::new(60.1699, 24.9384).unwrap(), // Helsinki
        altitude_baro_ft: Some(33000.0),
        altitude_geom_ft: Some(33500.0),
        on_ground: false,
        ground_speed_kt: Some(450.0),
        track_deg: Some(90.0),
        vertical_rate_fpm: Some(0.0),
        squawk: Some("1200".to_owned()),
        emergency: None,
        is_military: false,
        seen_pos_s: 1.0,
        source: "adsb_lol".to_owned(),
    };
    let plane2 = Aircraft {
        hex: "4ca456".to_owned(),
        callsign: Some("FIN456".to_owned()),
        registration: Some("OH-LZB".to_owned()),
        type_code: Some("A320".to_owned()),
        category: Some("A3".to_owned()),
        position: GeoPoint::new(59.3293, 18.0686).unwrap(), // Stockholm
        altitude_baro_ft: Some(28000.0),
        altitude_geom_ft: Some(28200.0),
        on_ground: false,
        ground_speed_kt: Some(420.0),
        track_deg: Some(270.0),
        vertical_rate_fpm: Some(0.0),
        squawk: Some("1200".to_owned()),
        emergency: None,
        is_military: false,
        seen_pos_s: 2.0,
        source: "adsb_lol".to_owned(),
    };

    let vessel1 = Vessel {
        mmsi: "230123456".to_owned(),
        name: Some("BALTIC CARRIER".to_owned()),
        imo: None,
        vessel_type: None,
        destination: None,
        position: GeoPoint::new(59.8, 24.8).unwrap(), // Mid Gulf of Finland (~42km south of Helsinki)
        speed_knots: Some(15.0),
        course_deg: Some(180.0),
        heading_deg: None,
        last_position_utc: None,
        last_position_epoch: None,
    };

    let aircraft_items = FeedItems::Aircraft(vec![plane1, plane2]);
    let vessel_items = FeedItems::Vessels(vec![vessel1]);

    let items = vec![(FeedId::Aircraft, &aircraft_items), (FeedId::Vessels, &vessel_items)];

    // 1. Search with Circle filter around Helsinki (radius 50km)
    let circle_req = SpatialSearchRequest {
        filter: SpatialFilter::Circle {
            center: GeoPoint::new(60.1699, 24.9384).unwrap(),
            radius_m: 50_000.0,
        },
        feeds: None,
        limit: Some(10),
    };
    let res = search_feed_items(&circle_req, &items).unwrap();
    assert_eq!(res.total_matched, 2); // plane1 and vessel1 (mid gulf is ~42km from Helsinki)
    assert_eq!(res.hits.len(), 2);
    assert_eq!(res.hits[0].id, "4ca123"); // plane1 at 0m distance comes first
    assert!(res.hits[0].distance_m.unwrap() < 10.0);

    // 2. Feed filter: Vessels only
    let vessel_only_req = SpatialSearchRequest {
        filter: SpatialFilter::Circle {
            center: GeoPoint::new(60.1699, 24.9384).unwrap(),
            radius_m: 50_000.0,
        },
        feeds: Some(vec![FeedId::Vessels]),
        limit: Some(10),
    };
    let res_vessel = search_feed_items(&vessel_only_req, &items).unwrap();
    assert_eq!(res_vessel.total_matched, 1);
    assert_eq!(res_vessel.hits[0].feed, FeedId::Vessels);
    assert_eq!(res_vessel.hits[0].id, "230123456");

    // 3. Truncation with limit 1
    let limited_req = SpatialSearchRequest {
        filter: SpatialFilter::Circle {
            center: GeoPoint::new(60.1699, 24.9384).unwrap(),
            radius_m: 50_000.0,
        },
        feeds: None,
        limit: Some(1),
    };
    let res_limited = search_feed_items(&limited_req, &items).unwrap();
    assert_eq!(res_limited.hits.len(), 1);
    assert_eq!(res_limited.total_matched, 2);
    assert!(res_limited.truncated);

    // 4. Invalid requests
    let bad_radius = SpatialSearchRequest {
        filter: SpatialFilter::Circle {
            center: GeoPoint::new(60.1699, 24.9384).unwrap(),
            radius_m: -100.0,
        },
        feeds: None,
        limit: None,
    };
    assert!(search_feed_items(&bad_radius, &items).is_err());

    let bad_poly = SpatialSearchRequest {
        filter: SpatialFilter::Polygon {
            points: vec![GeoPoint::new(0.0, 0.0).unwrap(), GeoPoint::new(1.0, 1.0).unwrap()],
        },
        feeds: None,
        limit: None,
    };
    assert!(search_feed_items(&bad_poly, &items).is_err());
}
