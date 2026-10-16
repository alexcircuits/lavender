//! Unit tests on recorded WFIGS and InciWeb documents (tests/fixtures/perimeters-*;
//! provenance in perimeters-provenance.md) and on synthetic malformed ones.

use std::collections::HashMap;

use jiff::Timestamp;
use lavender_domain::{BBox, FirePerimeter};
use lavender_geo::polygon;
use url::Url;

use super::*;

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/", $name))
    };
}

const WFIGS: &str = fixture!("perimeters-wfigs.geojson");
const MALFORMED: &str = fixture!("perimeters-wfigs-malformed.geojson");
const EMPTY: &str = fixture!("perimeters-wfigs-empty.geojson");
const ERROR: &str = fixture!("perimeters-wfigs-error.json");
const INDEX: &str = fixture!("perimeters-inciweb-index.json");
const SAND_CREEK: &str = fixture!("perimeters-inciweb-mtbdf-sand-creek.html");
const GOLD_MOUNTAIN: &str = fixture!("perimeters-inciweb-cogmf-gold-mountain-fire.html");
const SINLAHEKIN: &str = fixture!("perimeters-inciweb-wanes-sinlahekin-fire.html");

fn now() -> Timestamp {
    Timestamp::now()
}

fn recorded() -> Vec<FirePerimeter> {
    let page = parse_wfigs_page(WFIGS.as_bytes(), now(), &mut Snapshot::default()).unwrap();
    assert_eq!((page.perimeters.len(), page.dropped, page.features), (7, 0, 7));
    assert!(!page.more);
    page.perimeters
}

fn fire<'a>(fires: &'a [FirePerimeter], id: &str) -> &'a FirePerimeter {
    fires.iter().find(|f| f.id == id).unwrap()
}

fn inciweb_root() -> Url {
    Url::parse("https://inciweb.wildfire.gov/").unwrap()
}

fn recorded_pages() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("mtbdf-sand-creek", SAND_CREEK),
        ("cogmf-gold-mountain-fire", GOLD_MOUNTAIN),
        ("wanes-sinlahekin-fire", SINLAHEKIN),
    ])
}

fn ts(s: &str) -> Timestamp {
    s.parse().unwrap()
}

fn vertices(p: &FirePerimeter) -> usize {
    p.polygons.iter().flatten().map(Vec::len).sum()
}

#[test]
fn query_asks_for_card_fields_generalised_paged_geojson() {
    let layer = Url::parse("https://services3.arcgis.com/x/FeatureServer/0/").unwrap();
    let url = query_url(&layer, 2_000).unwrap();
    assert_eq!(url.path(), "/x/FeatureServer/0/query");
    let pairs: HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(pairs["f"], "geojson");
    assert_eq!(pairs["outSR"], "4326");
    assert_eq!(pairs["maxAllowableOffset"], "0.0002");
    assert_eq!(pairs["resultOffset"], "2000");
    assert_eq!(pairs["resultRecordCount"], "1000");
    assert_eq!(pairs["orderByFields"], "OBJECTID");
    for field in
        ["poly_IncidentName", "attr_CpxName", "attr_EstimatedCostToDate", "poly_DateCurrent"]
    {
        assert!(pairs["outFields"].contains(field), "{field}");
    }
}

#[test]
fn recorded_snapshot_keeps_every_card_fact() {
    let fires = recorded();
    let camel = fire(&fires, "2026-ORPRD-000446");
    assert_eq!(camel.name.as_deref(), Some("Camel Hump"));
    assert_eq!(camel.acres, Some(6789.0));
    assert_eq!(camel.contained_pct, Some(100.0));
    assert_eq!(camel.state.as_deref(), Some("US-OR"));
    assert_eq!(camel.category.as_deref(), Some("WF"));
    assert_eq!(camel.discovered_at, Some(ts("2026-07-16T00:53:00Z")));
    assert_eq!(camel.updated_at, Some(ts("2026-09-08T15:49:39Z")));
    assert_eq!(camel.cause.as_deref(), Some("Natural"));
    assert_eq!(camel.behavior.as_deref(), Some("Active"));
    assert_eq!(camel.personnel, Some(1));
    assert_eq!(camel.county.as_deref(), Some("Wheeler"));
    assert_eq!(camel.cost_to_date_usd, Some(100_000.0));
    assert_eq!(camel.complexity.as_deref(), Some("Type 3 Incident"));
    assert_eq!(camel.complex_name.as_deref(), Some("ROWE CREEK COMPLEX"));
    assert_eq!(camel.polygons.len(), 1);
    assert_eq!(camel.polygons[0].len(), 3, "exterior and two recorded holes");
    let gold = fire(&fires, "2026-COGMF-000099");
    assert_eq!(gold.cost_to_date_usd, Some(65_347_773.0));
    assert_eq!(gold.complex_name, None);
    let rx = fire(&fires, "2026-GACHP-000103");
    assert_eq!(rx.category.as_deref(), Some("RX"));
    assert_eq!((rx.contained_pct, rx.personnel, rx.behavior.as_deref()), (None, None, None));
    // Millisecond precision survives.
    assert_eq!(rx.updated_at, Some(ts("2026-10-06T14:49:04.774Z")));
}

#[test]
fn every_recorded_ring_is_closed_oriented_and_inside_its_extent() {
    for fire in recorded() {
        assert!(fire.extent.contains(&fire.anchor), "{}", fire.id);
        for rings in &fire.polygons {
            for (index, ring) in rings.iter().enumerate() {
                assert!(ring.len() >= 4 && ring.first() == ring.last(), "{}", fire.id);
                let area = polygon::signed_area(ring);
                assert_eq!(area > 0.0, index == 0, "{} ring {index} winding", fire.id);
                for p in ring {
                    assert!(
                        fire.extent.contains(&lavender_domain::GeoPoint::new(p[1], p[0]).unwrap())
                    );
                }
            }
        }
    }
}

#[test]
fn malformed_features_are_skipped_one_by_one() {
    let page = parse_wfigs_page(MALFORMED.as_bytes(), now(), &mut Snapshot::default()).unwrap();
    assert_eq!((page.perimeters.len(), page.dropped, page.features), (3, 10, 13));
    let ids: Vec<&str> = page.perimeters.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["2099-TEST-000001", "2099-TEST-000012", "77"]);
    let valid = &page.perimeters[0];
    assert_eq!(valid.name.as_deref(), Some("Valid Triangle"));
    // Wrong types, out-of-range values and future or placeholder dates read as unknown.
    assert_eq!((valid.acres, valid.contained_pct, valid.cost_to_date_usd), (None, None, None));
    assert_eq!((valid.discovered_at, valid.updated_at, valid.personnel), (None, None, None));
    assert_eq!(valid.cause.as_deref(), Some("Natural"), "control characters are removed");
    let cleaned = &page.perimeters[1];
    assert_eq!(cleaned.polygons.len(), 1, "the flat second polygon is dropped");
    assert_eq!(cleaned.polygons[0].len(), 2, "the flat hole is dropped, the real one kept");
    assert!(polygon::signed_area(&cleaned.polygons[0][1]) < 0.0, "holes wind clockwise");
}

#[test]
fn unusable_documents_are_rejected_whole() {
    let mut snapshot = Snapshot::default();
    for body in [
        "<html>unavailable</html>",
        r#"{"type":"FeatureCollection"}"#,
        r#"{"type":"FeatureCollection","features":{}}"#,
        r#"{"type":"Feature","features":[]}"#,
        r#"{"features":[]}"#,
        "[]",
    ] {
        assert!(parse_wfigs_page(body.as_bytes(), now(), &mut snapshot).is_err(), "{body}");
    }
    let too_many = format!(
        r#"{{"type":"FeatureCollection","features":[{}]}}"#,
        vec!["{}"; wfigs::PAGE_SIZE + 1].join(",")
    );
    assert!(parse_wfigs_page(too_many.as_bytes(), now(), &mut snapshot).is_err());
    let oversized = vec![b' '; wfigs::MAX_PAGE_BYTES + 1];
    assert!(parse_wfigs_page(&oversized, now(), &mut snapshot).is_err());
}

#[test]
fn arcgis_error_documents_are_unavailable_without_echoing_upstream_text() {
    let error = parse_wfigs_page(ERROR.as_bytes(), now(), &mut Snapshot::default()).unwrap_err();
    assert!(matches!(error, ProviderError::Unavailable(_)), "{error}");
    let message = error.public_message();
    assert!(message.contains("error 400"), "{message}");
    assert!(!message.contains("outFields"), "{message}");
    let quota = r#"{"error":{"code":429,"message":"Too many requests <script>","details":[]}}"#;
    let message = parse_wfigs_page(quota.as_bytes(), now(), &mut Snapshot::default())
        .unwrap_err()
        .public_message();
    assert!(message.contains("429") && !message.contains("script"), "{message}");
}

#[test]
fn an_empty_season_is_a_valid_empty_page() {
    let page = parse_wfigs_page(EMPTY.as_bytes(), now(), &mut Snapshot::default()).unwrap();
    assert_eq!((page.perimeters.len(), page.dropped, page.more), (0, 0, false));
}

#[test]
fn transfer_limit_is_read_in_both_arcgis_spellings() {
    for body in [
        r#"{"type":"FeatureCollection","features":[],"exceededTransferLimit":true}"#,
        r#"{"type":"FeatureCollection","features":[],"properties":{"exceededTransferLimit":true}}"#,
    ] {
        assert!(parse_wfigs_page(body.as_bytes(), now(), &mut Snapshot::default()).unwrap().more);
    }
}

#[test]
fn duplicate_ids_span_pages() {
    // A later page repeating ids adds nothing.
    let mut snapshot = Snapshot::default();
    parse_wfigs_page(WFIGS.as_bytes(), now(), &mut snapshot).unwrap();
    let again = parse_wfigs_page(WFIGS.as_bytes(), now(), &mut snapshot).unwrap();
    assert_eq!((again.perimeters.len(), again.dropped), (0, 7));
}

/// The recorded WFIGS page split in two: its first four features, then the other three.
fn recorded_in_two_pages() -> [String; 2] {
    let document: serde_json::Value = serde_json::from_str(WFIGS).unwrap();
    let features = document["features"].as_array().unwrap();
    [&features[..4], &features[4..]].map(|part| {
        let mut page = document.clone();
        page["features"] = serde_json::json!(part);
        page.to_string()
    })
}

#[test]
fn the_snapshot_outline_budget_keeps_the_largest_fires_across_pages_and_says_so() {
    // Room for the three largest recorded fires (Sinlahekin, Gold Mountain, Sand Creek)
    // but not the next (Mukluk). The largest, Sinlahekin, is on the second page.
    let fires = recorded();
    let size = |id: &str| vertices(fire(&fires, id));
    let three = size("2026-WANES-001791") + size("2026-COGMF-000099") + size("2026-MTBDF-266319");
    assert!(size("2026-AKTAS-613487") > 10, "Mukluk must not fit in the slack");
    let mut read = WfigsRead::new(three + 10);
    let mut ids = Snapshot::default();
    for page in recorded_in_two_pages() {
        read.add(parse_wfigs_page(page.as_bytes(), now(), &mut ids).unwrap());
    }
    let mut kept: Vec<&str> = read.perimeters.iter().map(|p| p.id.as_str()).collect();
    kept.sort_unstable();
    assert_eq!(kept, ["2026-COGMF-000099", "2026-MTBDF-266319", "2026-WANES-001791"]);
    assert_eq!((read.skipped, read.over_budget), (0, 4));
    let notices = read.notices();
    let budget = format!(
        "4 smaller fire(s) left out: this snapshot reached Lavender's limit of {} outline positions.",
        three + 10
    );
    assert!(notices.iter().any(|n| n.ends_with(&budget)), "{notices:?}");
    assert!(!notices.iter().any(|n| n.contains("skipped")), "nothing was malformed");

    // Within the budget, nothing is left out and nothing is said.
    let mut roomy = WfigsRead::new(wfigs::MAX_SNAPSHOT_VERTICES);
    roomy.add(parse_wfigs_page(WFIGS.as_bytes(), now(), &mut Snapshot::default()).unwrap());
    assert_eq!((roomy.perimeters.len(), roomy.over_budget), (7, 0));
    assert_eq!(roomy.notices(), [COVERAGE_NOTICE]);
}

#[test]
fn keeping_the_largest_fires_stops_at_the_first_that_does_not_fit() {
    let fires = recorded();
    let (all, none_left) = keep_largest(fires.clone(), usize::MAX);
    assert_eq!((all, none_left), (fires.clone(), 0), "within budget, unchanged");
    let largest = vertices(fire(&fires, "2026-WANES-001791"));
    let (kept, left_out) = keep_largest(fires.clone(), largest);
    assert_eq!((kept.len(), left_out), (1, 6));
    assert_eq!(kept[0].id, "2026-WANES-001791");
    let (kept, left_out) = keep_largest(fires, largest - 1);
    assert_eq!((kept.len(), left_out), (0, 7), "smaller fires do not jump the queue");
}

/// A closed circular ring of `corners` distinct positions around (`lon`, 44°N), as JSON.
fn circle(lon: f64, corners: u32) -> String {
    let positions: Vec<String> = (0..=corners)
        .map(|i| {
            let angle = f64::from(i % corners) / f64::from(corners) * std::f64::consts::TAU;
            format!("[{:.6},{:.6}]", lon + 0.1 * angle.cos(), 44.0 + 0.1 * angle.sin())
        })
        .collect();
    format!("[{}]", positions.join(","))
}

fn one_feature(id: &str, geometry: &str) -> String {
    format!(
        r#"{{"type":"FeatureCollection","features":[{{"type":"Feature","properties":{{"attr_UniqueFireIdentifier":"{id}"}},"geometry":{geometry}}}]}}"#
    )
}

#[test]
fn rings_and_features_over_their_vertex_caps_are_skipped() {
    let parse = |body: String| {
        let page = parse_wfigs_page(body.as_bytes(), now(), &mut Snapshot::default()).unwrap();
        (page.perimeters.len(), page.dropped)
    };
    let ring =
        |corners| format!(r#"{{"type":"Polygon","coordinates":[{}]}}"#, circle(-120.0, corners));
    assert_eq!(parse(one_feature("2099-RING-OK", &ring(49_999))), (1, 0));
    assert_eq!(parse(one_feature("2099-RING-BIG", &ring(50_000))), (0, 1));
    let parts: Vec<String> =
        (0..3).map(|i| format!("[{}]", circle(-120.0 + f64::from(i), 34_000))).collect();
    let multi = format!(r#"{{"type":"MultiPolygon","coordinates":[{}]}}"#, parts.join(","));
    assert_eq!(parse(one_feature("2099-FEATURE-BIG", &multi)), (0, 1));
}

#[test]
fn each_fire_gets_every_detail_level_with_shrinking_outlines() {
    for fire in recorded() {
        let levels = all_levels(fire.clone());
        assert_eq!(levels.len(), LEVELS.len());
        assert_eq!(levels[0].polygons, fire.polygons, "level 0 is the published outline");
        let mut previous = usize::MAX;
        for (index, level) in levels.iter().enumerate() {
            assert_eq!(usize::from(level.detail.level), index);
            assert!((level.detail.tolerance_m - LEVELS[index].tolerance_m).abs() < f64::EPSILON);
            assert_eq!(
                (&level.id, level.acres, level.anchor, level.extent),
                (&fire.id, fire.acres, fire.anchor, fire.extent)
            );
            assert!(!level.polygons.is_empty(), "{} vanished at level {index}", fire.id);
            assert!(vertices(level) <= previous, "{} grew at level {index}", fire.id);
            previous = vertices(level);
            for rings in &level.polygons {
                for (r, ring) in rings.iter().enumerate() {
                    assert!(ring.len() >= 4 && ring.first() == ring.last());
                    assert_eq!(polygon::signed_area(ring) > 0.0, r == 0);
                }
            }
        }
    }
}

#[test]
fn coarse_levels_drop_specks_but_keep_the_main_burn() {
    let fires = recorded();
    let sand_creek = fire(&fires, "2026-MTBDF-266319");
    assert!(sand_creek.polygons.len() > 300, "the recorded fire has hundreds of parts");
    let coarse = &all_levels(sand_creek.clone())[3];
    assert!(coarse.polygons.len() < 20, "{} parts at 2.5 km", coarse.polygons.len());
    assert!(vertices(coarse) * 20 < vertices(sand_creek));
    // A fire far smaller than the tolerance still shows as a triangle.
    let tiny = fire(&fires, "2026-CACDD-011826");
    let tiny_coarse = &all_levels(tiny.clone())[3];
    assert_eq!(tiny_coarse.polygons.len(), 1);
    assert!(tiny_coarse.polygons[0][0].len() >= 4);
}

fn all_recorded_levels() -> Vec<FirePerimeter> {
    recorded().into_iter().flat_map(all_levels).collect()
}

fn json_len(items: &[FirePerimeter]) -> usize {
    serde_json::to_vec(items).unwrap().len()
}

#[test]
fn views_get_one_level_by_span_and_only_fires_they_touch() {
    let all = all_recorded_levels();
    let (world, fires, truncated) = select_for_view(&all, None, 20_000);
    assert_eq!((world.len(), fires, truncated), (7, 7, false));
    assert!(world.iter().all(|p| p.detail.level == 3));
    // Largest first.
    assert_eq!(world[0].id, "2026-WANES-001791");

    let us = BBox::new(-125.0, 25.0, -66.0, 50.0).unwrap();
    let (country, _, _) = select_for_view(&all, Some(&us), 20_000);
    assert_eq!(country.len(), 6, "Alaska is outside the box");
    assert!(country.iter().all(|p| p.detail.level == 3));

    let montana = BBox::new(-113.7, 45.4, -112.9, 45.8).unwrap();
    let (close, _, _) = select_for_view(&all, Some(&montana), 20_000);
    assert_eq!(close.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["2026-MTBDF-266319"]);
    assert_eq!(close[0].detail.level, 1, "0.8° span");

    let street = BBox::new(-113.4, 45.5, -113.2, 45.6).unwrap();
    assert_eq!(select_for_view(&all, Some(&street), 20_000).0[0].detail.level, 0);

    let europe = BBox::new(0.0, 40.0, 20.0, 60.0).unwrap();
    let (none, fires, _) = select_for_view(&all, Some(&europe), 20_000);
    assert_eq!((none.len(), fires), (0, 7));
}

#[test]
fn every_view_stays_within_the_size_limits() {
    let all = all_recorded_levels();
    let views = [
        None,
        Some(BBox::new(-125.0, 25.0, -66.0, 50.0).unwrap()),
        Some(BBox::new(-121.0, 44.0, -119.0, 46.0).unwrap()),
        Some(BBox::new(-113.4, 45.5, -113.2, 45.6).unwrap()),
    ];
    for view in &views {
        let (items, _, _) = select_for_view(&all, view.as_ref(), 20_000);
        assert!(json_len(&items) <= MAX_RESPONSE_BYTES, "{view:?}");
        assert!(items.iter().map(vertices).sum::<usize>() <= MAX_RESPONSE_VERTICES, "{view:?}");
    }
}

/// The JSON a snapshot carries for these perimeters, envelope included.
fn served_len(items: &[FirePerimeter]) -> usize {
    serde_json::to_vec(&lavender_domain::FeedItems::Perimeters(items.to_vec())).unwrap().len()
}

/// `count` fires at every detail level, each with the same outline `ring` at every level,
/// fire `i` being `i` acres large (so the last is the largest).
fn crowd(count: u32, ring: &[polygon::Position]) -> Vec<FirePerimeter> {
    let base = fire(&recorded(), "2026-CACDD-011826").clone();
    let mut all = Vec::new();
    for i in 0..count {
        for (index, level) in LEVELS.iter().enumerate() {
            all.push(FirePerimeter {
                id: format!("CROWD-{i:04}"),
                acres: Some(f64::from(i)),
                detail: lavender_domain::PerimeterDetail {
                    level: u8::try_from(index).unwrap(),
                    tolerance_m: level.tolerance_m,
                },
                polygons: vec![vec![ring.to_vec()]],
                ..base.clone()
            });
        }
    }
    all
}

fn largest_ids(count: u32, kept: usize) -> Vec<String> {
    (0..count).rev().take(kept).map(|i| format!("CROWD-{i:04}")).collect()
}

#[test]
fn heavy_views_fall_back_to_the_first_coarser_level_that_fits() {
    // Two hundred copies of the recorded Sand Creek perimeter in one small view: full
    // detail would be ~28 MB and 100 m detail too many positions.
    let sand_creek = all_levels(fire(&recorded(), "2026-MTBDF-266319").clone());
    let crowd: Vec<FirePerimeter> = (0..200)
        .flat_map(|i| {
            sand_creek.iter().map(move |level| FirePerimeter {
                id: format!("COPY-{i:03}"),
                acres: Some(f64::from(i)),
                ..level.clone()
            })
        })
        .collect();
    let view = BBox::new(-113.4, 45.5, -113.2, 45.6).unwrap();
    let (items, fires, truncated) = select_for_view(&crowd, Some(&view), 20_000);
    assert_eq!((fires, items.len(), truncated), (200, 200, false), "every fire, coarser");
    assert!(items.iter().all(|p| p.detail.level == 2), "level {}", items[0].detail.level);
    assert!(served_len(&items) <= MAX_RESPONSE_BYTES);
    assert!(items.iter().map(vertices).sum::<usize>() <= MAX_RESPONSE_VERTICES);
    let finer: Vec<FirePerimeter> = crowd.iter().filter(|p| p.detail.level == 1).cloned().collect();
    assert!(
        served_len(&finer) > MAX_RESPONSE_BYTES
            || finer.iter().map(vertices).sum::<usize>() > MAX_RESPONSE_VERTICES,
        "level 1 would not have fitted"
    );
    let (capped, _, truncated) = select_for_view(&crowd, Some(&view), 2);
    assert_eq!((capped.len(), truncated), (2, true));
    assert_eq!(capped[0].detail.level, 0, "an item cap alone does not coarsen outlines");
}

#[test]
fn a_view_too_many_positions_even_coarse_keeps_the_largest_fires_that_fit() {
    // 100 fires of 2000 positions each, at every level: 200 000 positions. Short
    // coordinates keep the JSON (about 20 KB a fire) well under the byte limit, so the
    // position limit decides: exactly 75 fires fit.
    let ring = vec![[0.5, 0.5]; 2_000];
    let (items, fires, truncated) = select_for_view(&crowd(100, &ring), None, 20_000);
    assert_eq!((fires, truncated), (100, true));
    assert!(items.iter().all(|p| usize::from(p.detail.level) == LEVELS.len() - 1));
    assert_eq!(items.iter().map(|p| p.id.clone()).collect::<Vec<_>>(), largest_ids(100, 75));
    assert_eq!(items.iter().map(vertices).sum::<usize>(), MAX_RESPONSE_VERTICES);
    assert!(served_len(&items) <= MAX_RESPONSE_BYTES);
}

#[test]
fn the_byte_limit_counts_the_envelope_and_every_comma_exactly() {
    // Twenty coarse fires of about 100 KB each (long names, tiny outlines), the last and
    // smallest padded so the snapshot's items take exactly the limit.
    let base = fire(&recorded(), "2026-CACDD-011826").clone();
    let coarsest = all_levels(base)[LEVELS.len() - 1].clone();
    let padded = |i: u32, pad: usize| FirePerimeter {
        id: format!("PAD-{i:02}"),
        acres: Some(f64::from(100 - i)),
        name: Some("x".repeat(pad)),
        ..coarsest.clone()
    };
    let mut fires: Vec<FirePerimeter> = (0..20).map(|i| padded(i, 100_000)).collect();
    let short = MAX_RESPONSE_BYTES - served_len(&fires);
    fires[19] = padded(19, 100_000 + short);
    assert_eq!(served_len(&fires), MAX_RESPONSE_BYTES);
    let (items, _, truncated) = select_for_view(&fires, None, 20_000);
    assert_eq!((items.len(), truncated), (20, false), "exactly the limit fits");

    // One byte more and the smallest fire no longer fits.
    fires[19] = padded(19, 100_000 + short + 1);
    let (items, _, truncated) = select_for_view(&fires, None, 20_000);
    assert_eq!((items.len(), truncated), (19, true));
    assert_eq!(items.last().map(|p| p.id.as_str()), Some("PAD-18"));
}

#[test]
fn a_view_too_large_even_coarse_keeps_the_largest_fires_that_fit_in_its_bytes() {
    // 100 fires of 1000 long-printed positions (about 37 KB of JSON a fire, 3.7 MB in
    // all) but only 100 000 positions: the byte limit decides.
    let ring = vec![[-120.123_456_789_012_3, 44.123_456_789_012_3]; 1_000];
    let all = crowd(100, &ring);
    let (items, fires, truncated) = select_for_view(&all, None, 20_000);
    assert_eq!((fires, truncated), (100, true));
    assert!(!items.is_empty() && items.len() < 100, "{} fires", items.len());
    assert_eq!(
        items.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
        largest_ids(100, items.len()),
        "the largest fires, largest first"
    );
    // As many as fit: the served JSON, envelope included, is within the limit, and one
    // more fire would break it.
    assert!(served_len(&items) <= MAX_RESPONSE_BYTES, "{}", served_len(&items));
    let next = all
        .iter()
        .find(|p| usize::from(p.detail.level) == LEVELS.len() - 1 && !items.contains(p))
        .unwrap()
        .clone();
    let one_more: Vec<FirePerimeter> = items.iter().cloned().chain([next]).collect();
    assert!(served_len(&one_more) > MAX_RESPONSE_BYTES);
}

#[test]
fn recorded_index_lists_active_incidents_with_safe_slugs() {
    let (entries, dropped) = parse_index(INDEX.as_bytes()).unwrap();
    assert_eq!((entries.len(), dropped), (8, 0));
    let sand = entries.iter().find(|e| e.id == 329_633).unwrap();
    assert_eq!(
        (sand.title.as_str(), sand.unit.as_str(), sand.slug.as_str()),
        ("Sand Creek", "MTBDF", "mtbdf-sand-creek")
    );
}

#[test]
fn malformed_index_rows_are_skipped_and_bad_documents_rejected() {
    let rows = r#"[
        {"id":"1","title":"Good","field_unit_code":"ORPRD","urlPath":"/orprd-good","field_active":"1"},
        {"id":2,"title":"Numeric Id","field_unit_code":"ORPRD","urlPath":"/orprd-numeric"},
        {"id":"abc","title":"Bad Id","field_unit_code":"ORPRD","urlPath":"/x"},
        {"id":"3","title":"Escaping Path","field_unit_code":"ORPRD","urlPath":"/../admin"},
        {"id":"4","title":"Query Path","field_unit_code":"ORPRD","urlPath":"/x?y=1"},
        {"id":"5","title":"Inactive","field_unit_code":"ORPRD","urlPath":"/x","field_active":"0"},
        {"id":"6","field_unit_code":"ORPRD","urlPath":"/no-title"},
        {"id":"7","title":"Bad Unit","field_unit_code":"O","urlPath":"/x"},
        "not an object"
    ]"#;
    let (entries, dropped) = parse_index(rows.as_bytes()).unwrap();
    assert_eq!(entries.iter().map(|e| e.id).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(dropped, 7);
    assert!(parse_index(b"{}").is_err());
    assert!(parse_index(b"<html>403 Forbidden</html>").is_err());
    assert!(parse_index(&vec![b' '; inciweb::MAX_INDEX_BYTES + 1]).is_err());
}

fn named(name: &str, state: Option<&str>, complex: Option<&str>) -> FirePerimeter {
    FirePerimeter {
        name: Some(name.to_owned()),
        state: state.map(str::to_owned),
        complex_name: complex.map(str::to_owned),
        ..recorded().remove(0)
    }
}

#[test]
fn names_match_by_state_then_complex_and_ambiguity_yields_nothing() {
    let (index, _) = parse_index(INDEX.as_bytes()).unwrap();
    let slug = |f: &FirePerimeter| find_entry(&index, f).map(|e| e.slug.clone());
    assert_eq!(
        slug(&named("Sand Creek", Some("US-MT"), None)).as_deref(),
        Some("mtbdf-sand-creek")
    );
    assert_eq!(
        slug(&named("Gold Mountain", Some("US-CO"), None)).as_deref(),
        Some("cogmf-gold-mountain-fire")
    );
    assert_eq!(slug(&named("Sand Creek", Some("US-OR"), None)), None, "state must agree");
    assert_eq!(
        slug(&named("Cottonwood", Some("US-OR"), None)).as_deref(),
        Some("or95s-cottonwood-fire")
    );
    assert_eq!(
        slug(&named("Cottonwood", Some("US-UT"), None)).as_deref(),
        Some("utfif-cottonwood-fire")
    );
    assert_eq!(slug(&named("Cottonwood", None, None)), None, "two states, no evidence");
    assert_eq!(
        slug(&named("Camel Hump", Some("US-OR"), Some("ROWE CREEK COMPLEX"))).as_deref(),
        Some("orprd-rowe-creek-complex"),
        "a complex member links to its complex"
    );
    assert_eq!(slug(&named("Mukluk", Some("US-AK"), None)), None);
}

#[test]
fn name_normalisation_matches_the_legacy_rules() {
    let entry = |id, title: &str, unit: &str, slug: &str| {
        IndexEntry::new(id, title.into(), unit.into(), slug.into())
    };
    let index = vec![
        entry(329_273, "2026 Coleman Creek", "ORBUD", "a"),
        entry(328_922, "Coyote  Fire", "OR95S", "b"),
        entry(1, "Twin Fire", "IDBOF", "old"),
        entry(2, "Twin", "IDBOF", "new"),
        entry(3, "Fire", "IDBOF", "fire"),
    ];
    let slug =
        |n: &str, s: &str| find_entry(&index, &named(n, Some(s), None)).map(|e| e.slug.as_str());
    assert_eq!(slug("Coleman Creek", "US-OR"), Some("a"));
    assert_eq!(slug("COYOTE", "US-OR"), Some("b"));
    assert_eq!(slug("twin", "US-ID"), Some("new"), "same-state duplicates: newest wins");
    assert_eq!(slug("Fire", "US-ID"), Some("fire"));
    assert_eq!(slug("   ", "US-ID"), None);
}

#[test]
fn recorded_incident_pages_give_origin_and_change_dates() {
    let sand = parse_incident_page(SAND_CREEK.as_bytes()).unwrap();
    assert_eq!(sand.origin, Some(ts("2026-08-01T17:45:28Z")));
    assert_eq!(sand.changed, Some(ts("2026-09-28T00:00:00Z")), "from dcterms.date");
    let gold = parse_incident_page(GOLD_MOUNTAIN.as_bytes()).unwrap();
    assert_eq!(gold.origin, Some(ts("2026-06-27T21:33:18Z")));
    let sinlahekin = parse_incident_page(SINLAHEKIN.as_bytes()).unwrap();
    assert_eq!(sinlahekin.origin, Some(ts("2026-07-26T18:12:00Z")));
}

#[test]
fn the_page_scanner_follows_html_rather_than_text_patterns() {
    // The legacy test page shape: og:updated_time plus a Date of Origin row.
    let legacy = r#"<meta property="og:updated_time" content="2026-09-23T11:32:31-04:00" />
<th>Date of Origin</th>
<td class="text-tabular text-left"><time datetime="2026-07-15T20:20:00Z">Origin</time></td>"#;
    let facts = parse_incident_page(legacy.as_bytes()).unwrap();
    assert_eq!(facts.origin, Some(ts("2026-07-15T20:20:00Z")));
    assert_eq!(facts.changed, Some(ts("2026-09-23T15:32:31Z")));

    let odd = r#"<meta == "x" name content='2026-09-23T11:32:31Z' property="og:updated_time"/>
<th>Date of Origin</th><td><time datetime="2026-07-02T00:00:00Z" class="a"></time></td>"#;
    let facts = parse_incident_page(odd.as_bytes()).unwrap();
    assert_eq!(facts.origin, Some(ts("2026-07-02T00:00:00Z")));
    assert_eq!(facts.changed, Some(ts("2026-09-23T11:32:31Z")));

    let reordered = r"<META CONTENT='2026-09-23T11:32:31Z' PROPERTY=og:updated_time>
<!-- <th>Date of Origin</th><td><time datetime='1999-01-01T00:00:00Z'></time></td> -->
<script>var t = '<th>Date of Origin</th><time datetime=1999-01-01T00:00:00Z>';</script>
<p title=O'Brien>Crew's notes</p>
<tr><TH scope=row><span>Date of</span>
  Origin</TH><td><TIME datetime=2026-07-01T00:00:00Z>x</TIME></td></tr>
<tr><th>Other</th><td><time datetime='2026-01-01T00:00:00Z'></time></td></tr>";
    let facts = parse_incident_page(reordered.as_bytes()).unwrap();
    assert_eq!(facts.origin, Some(ts("2026-07-01T00:00:00Z")));
    assert_eq!(facts.changed, Some(ts("2026-09-23T11:32:31Z")));

    for page in [
        "<html>No markers</html>",
        r#"<th>Unrelated</th><td><time datetime="2026-07-15T20:20:00Z"></time></td>"#,
        r#"<th>Date of Origin</th><td><time datetime="yesterday"></time></td>"#,
        r#"<meta property="og:updated_time" content="1800-01-01T00:00:00Z">"#,
        "<th>Date of Origin</th><td><time datetime=\"2026-07-15T20:20:00Z",
        "",
    ] {
        assert!(parse_incident_page(page.as_bytes()).is_err(), "{page}");
    }
    assert!(parse_incident_page(&vec![b'a'; inciweb::MAX_PAGE_BYTES + 1]).is_err());
}

#[test]
fn a_page_must_describe_this_seasons_fire() {
    let discovered = Some(ts("2026-08-01T19:31:00Z"));
    let now = ts("2026-10-15T00:00:00Z");
    let page = |origin: Option<&str>, changed: Option<&str>| PageFacts {
        origin: origin.map(ts),
        changed: changed.map(ts),
    };
    assert!(is_current(&page(Some("2026-08-01T17:45:28Z"), None), discovered, now));
    assert!(
        is_current(&page(Some("2026-06-03T00:00:00Z"), None), discovered, now),
        "within 60 days"
    );
    assert!(
        !is_current(
            &page(Some("2025-07-20T00:00:00Z"), Some("2026-10-01T00:00:00Z")),
            discovered,
            now
        ),
        "last season's page"
    );
    assert!(is_current(&page(None, Some("2026-10-01T00:00:00Z")), discovered, now));
    assert!(is_current(&page(None, Some("2026-05-01T00:00:00Z")), None, now));
    assert!(!is_current(&page(None, Some("2026-03-01T00:00:00Z")), None, now), "stale page");
    assert!(!is_current(&page(None, None), discovered, now));
}

#[test]
fn recorded_documents_link_only_verified_pages() {
    let pages = recorded_pages();
    let (items, notices) = from_recorded(
        WFIGS.as_bytes(),
        Some(INDEX.as_bytes()),
        |slug| pages.get(slug).map(|p| p.as_bytes().to_vec()),
        &inciweb_root(),
        now(),
    )
    .unwrap();
    assert_eq!(items.len(), 7 * LEVELS.len());
    assert_eq!(notices, [COVERAGE_NOTICE], "nothing skipped or left out");
    let link = |id: &str| fire(&items, id).inciweb_url.clone();
    assert_eq!(
        link("2026-MTBDF-266319").as_deref(),
        Some("https://inciweb.wildfire.gov/incident-information/mtbdf-sand-creek")
    );
    assert_eq!(
        link("2026-COGMF-000099").as_deref(),
        Some("https://inciweb.wildfire.gov/incident-information/cogmf-gold-mountain-fire")
    );
    assert_eq!(
        link("2026-WANES-001791").as_deref(),
        Some("https://inciweb.wildfire.gov/incident-information/wanes-sinlahekin-fire")
    );
    assert_eq!(link("2026-ORPRD-000446"), None, "the complex page was not recorded");
    assert_eq!(link("2026-AKTAS-613487"), None);
    // Every level of a linked fire carries the link.
    assert!(items.iter().filter(|p| p.id == "2026-MTBDF-266319").all(|p| p.inciweb_url.is_some()));

    // Without the index or with unreadable pages, perimeters still arrive, unlinked.
    let (bare, _) =
        from_recorded(WFIGS.as_bytes(), None, |_| None, &inciweb_root(), now()).unwrap();
    assert_eq!(bare.len(), items.len());
    assert!(bare.iter().all(|p| p.inciweb_url.is_none()));
    let (broken, _) = from_recorded(
        WFIGS.as_bytes(),
        Some(INDEX.as_bytes()),
        |_| Some(b"<html>new markup</html>".to_vec()),
        &inciweb_root(),
        now(),
    )
    .unwrap();
    assert!(broken.iter().all(|p| p.inciweb_url.is_none()));
}

#[test]
fn every_field_the_legacy_card_used_is_on_the_wire() {
    // legacy/src/layers/perimeters/records.js row fields and the card's InciWeb link,
    // with their Lavender names.
    let legacy_to_lavender = [
        ("stableId", "id"),
        ("name", "name"),
        ("acres", "acres"),
        ("containedPct", "containedPct"),
        ("state", "state"),
        ("category", "category"),
        ("discoveredTime", "discoveredAt"),
        ("updatedTime", "updatedAt"),
        ("cause", "cause"),
        ("behavior", "behavior"),
        ("personnel", "personnel"),
        ("county", "county"),
        ("costToDate", "costToDateUsd"),
        ("complexity", "complexity"),
        ("complexName", "complexName"),
        ("polygons", "polygons"),
        ("anchor (cards.js perimeterAnchorDegrees)", "anchor"),
        ("link (InciWeb)", "inciwebUrl"),
    ];
    let pages = recorded_pages();
    let (items, _) = from_recorded(
        WFIGS.as_bytes(),
        Some(INDEX.as_bytes()),
        |slug| pages.get(slug).map(|p| p.as_bytes().to_vec()),
        &inciweb_root(),
        now(),
    )
    .unwrap();
    // Camel Hump has every WFIGS attribute; Sand Creek has a verified link.
    let camel = serde_json::to_value(fire(&items, "2026-ORPRD-000446")).unwrap();
    let sand = serde_json::to_value(fire(&items, "2026-MTBDF-266319")).unwrap();
    for (legacy, field) in legacy_to_lavender {
        let present = camel.get(field).is_some_and(|v| !v.is_null())
            || (field == "inciwebUrl" && sand.get(field).is_some_and(serde_json::Value::is_string));
        assert!(present, "{legacy} → {field} is missing");
    }
}

mod refresh {
    use std::time::Duration;

    use jiff::SignedDuration;
    use tokio_util::sync::CancellationToken;
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::config::{FetchConfig, PerimetersConfig};
    use crate::fetch::Fetcher;
    use crate::secrets::MemorySecretStore;

    fn provider(server: &MockServer) -> PerimetersProvider {
        PerimetersProvider::new(PerimetersConfig {
            wfigs_base_url: Url::parse(&format!("{}/layer/", server.uri())).unwrap(),
            inciweb_enabled: false,
            ..PerimetersConfig::default()
        })
    }

    fn fetcher() -> Fetcher {
        Fetcher::new(FetchConfig {
            allow_http_hosts: vec!["127.0.0.1".to_owned()],
            ..FetchConfig::default()
        })
        .unwrap()
    }

    #[tokio::test]
    async fn cached_perimeters_keep_their_age_when_wfigs_fails_and_then_expire() {
        let server = MockServer::start().await;
        Mock::given(path("/layer/query"))
            .respond_with(ResponseTemplate::new(200).set_body_string(WFIGS))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(path("/layer/query"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let provider = provider(&server);
        let (fetcher, secrets) = (fetcher(), MemorySecretStore::default());
        let ctx = FetchContext {
            fetcher: &fetcher,
            secrets: &secrets,
            cancel: CancellationToken::new(),
            cache_dir: None,
        };
        let fresh = provider.fetch(&ctx, Region::Global).await.unwrap();
        assert_eq!(fresh.degraded, None);
        assert_eq!(fresh.items.len(), 7 * LEVELS.len());
        assert!(fresh.notices.iter().any(|n| n == COVERAGE_NOTICE));

        // Within the cadence the provider answers from its cache without a request.
        assert_eq!(provider.fetch(&ctx, Region::Global).await.unwrap(), fresh);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);

        // Once the cadence has passed, a failing refresh serves the cache with its age.
        provider.state.lock().await.attempted_at = None;
        let cached = provider.fetch(&ctx, Region::Global).await.unwrap();
        assert_eq!((cached.as_of, &cached.items), (fresh.as_of, &fresh.items));
        let reason = cached.degraded.unwrap();
        assert!(reason.contains("Cached perimeters") && reason.contains("503"), "{reason}");

        // Twelve hours on, the cache is too old to show.
        {
            let mut state = provider.state.lock().await;
            state.attempted_at = None;
            let cache = state.cache.as_mut().unwrap();
            cache.fetched_at =
                Timestamp::now().checked_sub(SignedDuration::from_hours(13)).unwrap();
        }
        assert!(provider.fetch(&ctx, Region::Global).await.is_err());
    }

    #[tokio::test]
    async fn one_empty_answer_keeps_the_previous_perimeters_until_the_next_confirms_it() {
        let server = MockServer::start().await;
        Mock::given(path("/layer/query"))
            .respond_with(ResponseTemplate::new(200).set_body_string(WFIGS))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(path("/layer/query"))
            .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY))
            .mount(&server)
            .await;
        let provider = provider(&server);
        let (fetcher, secrets) = (fetcher(), MemorySecretStore::default());
        let ctx = FetchContext {
            fetcher: &fetcher,
            secrets: &secrets,
            cancel: CancellationToken::new(),
            cache_dir: None,
        };
        let fresh = provider.fetch(&ctx, Region::Global).await.unwrap();
        assert_eq!(fresh.items.len(), 7 * LEVELS.len());

        // A sudden empty answer (ArcGIS republishing the layer) does not wipe the map.
        provider.state.lock().await.attempted_at = None;
        let held = provider.fetch(&ctx, Region::Global).await.unwrap();
        assert_eq!((held.as_of, &held.items), (fresh.as_of, &fresh.items));
        let reason = held.degraded.unwrap();
        assert!(reason.contains("no perimeters"), "{reason}");

        // The next refresh confirms it: the season is empty now.
        provider.state.lock().await.attempted_at = None;
        let confirmed = provider.fetch(&ctx, Region::Global).await.unwrap();
        assert_eq!((confirmed.items.len(), confirmed.degraded), (0, None));
        assert_eq!(server.received_requests().await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn concurrent_callers_share_one_refresh() {
        let server = MockServer::start().await;
        Mock::given(path("/layer/query"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(WFIGS)
                    .set_delay(Duration::from_millis(50)),
            )
            .mount(&server)
            .await;
        let provider = provider(&server);
        let (fetcher, secrets) = (fetcher(), MemorySecretStore::default());
        let ctx = FetchContext {
            fetcher: &fetcher,
            secrets: &secrets,
            cancel: CancellationToken::new(),
            cache_dir: None,
        };
        let (a, b) = tokio::join!(
            provider.fetch(&ctx, Region::Global),
            provider.fetch(&ctx, Region::Global)
        );
        assert_eq!(a.unwrap(), b.unwrap());
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[test]
    fn inciweb_gets_what_is_left_of_the_refresh_budget() {
        let budget = |elapsed| inciweb_budget(elapsed, REFRESH_BUDGET);
        assert_eq!(budget(Duration::from_secs(1)), INCIWEB_BUDGET);
        assert_eq!(budget(Duration::from_secs(15)), Duration::from_secs(5));
        assert_eq!(budget(Duration::from_millis(18_500)), Duration::ZERO);
        assert_eq!(budget(Duration::from_secs(60)), Duration::ZERO);
    }

    #[tokio::test]
    async fn a_slow_wfigs_answer_skips_inciweb_for_that_refresh() {
        // With a 3 s refresh budget, a 1.5 s WFIGS answer leaves less than InciWeb's 2 s
        // minimum; a quick one leaves enough for the index and one page.
        for (wfigs_delay, inciweb_asked) in
            [(Duration::ZERO, true), (Duration::from_millis(1_500), false)]
        {
            let server = MockServer::start().await;
            mount_recorded(&server, Some(WFIGS_PATH), wfigs_delay).await;
            let provider = PerimetersProvider {
                refresh_budget: Duration::from_secs(3),
                ..linked_provider(&server, 1)
            };
            let (fetcher, secrets) = (fetcher(), MemorySecretStore::default());
            let ctx = FetchContext {
                fetcher: &fetcher,
                secrets: &secrets,
                cancel: CancellationToken::new(),
                cache_dir: None,
            };
            let answer = provider.fetch(&ctx, Region::Global).await.unwrap();
            assert_eq!(answer.items.len(), 7 * LEVELS.len(), "perimeters arrive either way");
            let requests = inciweb_requests(&server).await;
            assert_eq!(!requests.is_empty(), inciweb_asked, "{wfigs_delay:?}: {requests:?}");
            let unavailable = answer.notices.iter().any(|n| n == LINKS_UNAVAILABLE_NOTICE);
            assert_eq!(unavailable, !inciweb_asked);
        }
    }

    const WFIGS_PATH: &str = "/layer/query";
    const INDEX_PATH: &str = "/inciweb/api/map_data";

    fn page_path(slug: &str) -> String {
        format!("/inciweb/incident-information/{slug}")
    }

    fn linked_provider(server: &MockServer, pages_per_refresh: usize) -> PerimetersProvider {
        PerimetersProvider::new(PerimetersConfig {
            wfigs_base_url: Url::parse(&format!("{}/layer/", server.uri())).unwrap(),
            inciweb_base_url: Url::parse(&format!("{}/inciweb/", server.uri())).unwrap(),
            inciweb_pages_per_refresh: pages_per_refresh,
            ..PerimetersConfig::default()
        })
    }

    /// Serves the recorded WFIGS page, index and incident pages; `slow` answers after
    /// `delay`. Rowe Creek's page is not recorded, so it answers 404.
    async fn mount_recorded(server: &MockServer, slow: Option<&str>, delay: Duration) {
        let delay_for = |p: &str| if slow == Some(p) { delay } else { Duration::ZERO };
        Mock::given(path(WFIGS_PATH))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(WFIGS).set_delay(delay_for(WFIGS_PATH)),
            )
            .mount(server)
            .await;
        Mock::given(path(INDEX_PATH))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(INDEX).set_delay(delay_for(INDEX_PATH)),
            )
            .mount(server)
            .await;
        for (slug, html) in recorded_pages() {
            let page = page_path(slug);
            Mock::given(path(page.as_str()))
                .respond_with(
                    ResponseTemplate::new(200).set_body_string(html).set_delay(delay_for(&page)),
                )
                .mount(server)
                .await;
        }
    }

    async fn inciweb_requests(server: &MockServer) -> Vec<String> {
        let requests = server.received_requests().await.unwrap();
        requests
            .iter()
            .map(|r| r.url.path().to_owned())
            .filter(|p| p.starts_with("/inciweb/"))
            .collect()
    }

    async fn wait_for_request(server: &MockServer, wanted: &str) {
        for _ in 0..500 {
            if inciweb_requests(server).await.iter().any(|p| p == wanted) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("{wanted} was never requested");
    }

    /// Ids of the fires the answer links to InciWeb.
    fn linked(answer: &Fetched) -> Vec<String> {
        let FeedItems::Perimeters(items) = &answer.items else { panic!("perimeters") };
        let mut ids: Vec<String> = items
            .iter()
            .filter(|p| p.detail.level == 0 && p.inciweb_url.is_some())
            .map(|p| p.id.clone())
            .collect();
        ids.sort();
        ids
    }

    #[tokio::test]
    async fn inciweb_pages_are_read_a_few_per_refresh_largest_first_then_reused() {
        let server = MockServer::start().await;
        mount_recorded(&server, None, Duration::ZERO).await;
        let provider = linked_provider(&server, 2);
        let (fetcher, secrets) = (fetcher(), MemorySecretStore::default());
        let ctx = FetchContext {
            fetcher: &fetcher,
            secrets: &secrets,
            cancel: CancellationToken::new(),
            cache_dir: None,
        };
        // Matched fires by size: Sinlahekin, Gold Mountain, Sand Creek, then Camel Hump
        // (through the Rowe Creek Complex page).
        let started = std::time::Instant::now();
        let first = provider.fetch(&ctx, Region::Global).await.unwrap();
        assert_eq!(
            inciweb_requests(&server).await,
            [
                INDEX_PATH.to_owned(),
                page_path("wanes-sinlahekin-fire"),
                page_path("cogmf-gold-mountain-fire")
            ],
            "the index, then the two largest fires' pages"
        );
        assert!(started.elapsed() >= Duration::from_secs(2), "one InciWeb request a second");
        assert_eq!(linked(&first), ["2026-COGMF-000099", "2026-WANES-001791"]);

        // Within the cadence nothing is requested at all.
        provider.fetch(&ctx, Region::Global).await.unwrap();
        assert_eq!(inciweb_requests(&server).await.len(), 3);

        // The next refresh reuses the index and reads the next two pages.
        provider.state.lock().await.attempted_at = None;
        let second = provider.fetch(&ctx, Region::Global).await.unwrap();
        assert_eq!(
            inciweb_requests(&server).await[3..],
            [page_path("mtbdf-sand-creek"), page_path("orprd-rowe-creek-complex")]
        );
        assert_eq!(
            linked(&second),
            ["2026-COGMF-000099", "2026-MTBDF-266319", "2026-WANES-001791"]
        );

        // Then InciWeb is left alone: pages read today, the missing one retried in an
        // hour, the index reused for hours.
        provider.state.lock().await.attempted_at = None;
        let third = provider.fetch(&ctx, Region::Global).await.unwrap();
        assert_eq!(inciweb_requests(&server).await.len(), 5);
        assert_eq!(linked(&third), linked(&second));
        assert_eq!(server.received_requests().await.unwrap().len(), 8, "3 WFIGS + 5 InciWeb");
    }

    #[tokio::test]
    async fn a_slow_inciweb_delays_the_perimeters_by_a_few_seconds_at_most() {
        // InciWeb answers, but far too late: the perimeters must not wait for it.
        let server = MockServer::start().await;
        mount_recorded(&server, Some(INDEX_PATH), Duration::from_secs(30)).await;
        let provider = linked_provider(&server, 4);
        let (fetcher, secrets) = (fetcher(), MemorySecretStore::default());
        let ctx = FetchContext {
            fetcher: &fetcher,
            secrets: &secrets,
            cancel: CancellationToken::new(),
            cache_dir: None,
        };
        let started = std::time::Instant::now();
        let answer = provider.fetch(&ctx, Region::Global).await.unwrap();
        let waited = started.elapsed();
        assert!(waited < Duration::from_secs(7), "InciWeb held the perimeters for {waited:?}");
        assert_eq!(answer.items.len(), 7 * LEVELS.len());
        assert!(answer.notices.iter().any(|n| n == LINKS_UNAVAILABLE_NOTICE));
    }

    #[tokio::test]
    async fn pages_a_refresh_had_no_time_for_wait_unmarked_for_the_next_one() {
        // A 2.5 s refresh leaves InciWeb about 2.4 s: the index and one page, since a
        // request starts only with two seconds left (one for the spacing, one for the
        // answer). The other pages are not marked as failed: each later refresh reads
        // the next one, instead of waiting an hour.
        let server = MockServer::start().await;
        mount_recorded(&server, None, Duration::ZERO).await;
        let provider = PerimetersProvider {
            refresh_budget: Duration::from_millis(2_500),
            ..linked_provider(&server, 8)
        };
        let (fetcher, secrets) = (fetcher(), MemorySecretStore::default());
        let ctx = FetchContext {
            fetcher: &fetcher,
            secrets: &secrets,
            cancel: CancellationToken::new(),
            cache_dir: None,
        };
        let mut answers = Vec::new();
        for _ in 0..4 {
            provider.state.lock().await.attempted_at = None;
            answers.push(provider.fetch(&ctx, Region::Global).await.unwrap());
        }
        assert_eq!(
            inciweb_requests(&server).await,
            [
                INDEX_PATH.to_owned(),
                page_path("wanes-sinlahekin-fire"),
                page_path("cogmf-gold-mountain-fire"),
                page_path("mtbdf-sand-creek"),
                page_path("orprd-rowe-creek-complex"),
            ],
            "one page per refresh, largest fires first, none skipped"
        );
        assert_eq!(linked(&answers[0]), ["2026-WANES-001791"]);
        assert_eq!(
            linked(&answers[3]),
            ["2026-COGMF-000099", "2026-MTBDF-266319", "2026-WANES-001791"]
        );
    }

    #[tokio::test]
    async fn an_abandoned_refresh_asks_inciweb_for_its_index_again_at_once() {
        let server = MockServer::start().await;
        mount_recorded(&server, Some(INDEX_PATH), Duration::from_millis(1_500)).await;
        let provider = linked_provider(&server, 8);
        let (fetcher, secrets) = (fetcher(), MemorySecretStore::default());
        let token = CancellationToken::new();
        let ctx = FetchContext {
            fetcher: &fetcher,
            secrets: &secrets,
            cancel: token.clone(),
            cache_dir: None,
        };
        // The hub cancels the token and drops the refresh when its subscriber leaves.
        tokio::select! {
            _ = provider.fetch(&ctx, Region::Global) => panic!("the index answers too late"),
            () = async {
                wait_for_request(&server, INDEX_PATH).await;
                token.cancel();
            } => {}
        }
        let ctx = FetchContext { cancel: CancellationToken::new(), ..ctx };
        let answer = provider.fetch(&ctx, Region::Global).await.unwrap();
        assert!(
            !answer.notices.iter().any(|n| n == LINKS_UNAVAILABLE_NOTICE),
            "{:?}",
            answer.notices
        );
        assert_eq!(linked(&answer).len(), 3);
        let requests = inciweb_requests(&server).await;
        assert_eq!(requests.iter().filter(|p| *p == INDEX_PATH).count(), 2);
    }

    #[tokio::test]
    async fn a_cancelled_page_read_is_retried_by_the_next_refresh() {
        let server = MockServer::start().await;
        let slow = page_path("wanes-sinlahekin-fire");
        mount_recorded(&server, Some(&slow), Duration::from_millis(1_500)).await;
        let provider = linked_provider(&server, 8);
        let (fetcher, secrets) = (fetcher(), MemorySecretStore::default());
        let token = CancellationToken::new();
        let ctx = FetchContext {
            fetcher: &fetcher,
            secrets: &secrets,
            cancel: token.clone(),
            cache_dir: None,
        };
        let cancel = async {
            wait_for_request(&server, &slow).await;
            token.cancel();
        };
        let (answer, ()) = tokio::join!(provider.fetch(&ctx, Region::Global), cancel);
        assert!(answer.is_err());
        let ctx = FetchContext { cancel: CancellationToken::new(), ..ctx };
        let answer = provider.fetch(&ctx, Region::Global).await.unwrap();
        assert!(linked(&answer).contains(&"2026-WANES-001791".to_owned()), "{:?}", answer.notices);
        let requests = inciweb_requests(&server).await;
        assert_eq!(requests.iter().filter(|p| **p == slow).count(), 2);
        assert_eq!(requests.iter().filter(|p| *p == INDEX_PATH).count(), 1, "index kept");
    }

    #[test]
    fn inciweb_page_cache_is_bounded_and_retries_failures_later() {
        let mut state = InciwebState::default();
        for i in 0..MAX_CACHED_PAGES + 10 {
            state.remember(format!("slug-{i}"), None);
        }
        assert_eq!(state.pages.len(), MAX_CACHED_PAGES);
        assert!(!state.pages.contains_key("slug-0"), "the oldest entry made room");
        let recent = format!("slug-{}", MAX_CACHED_PAGES + 5);
        assert!(!state.page_due(&recent), "a failed page waits before it is retried");
        assert!(state.page_due("never-seen"));
        assert!(state.index_due(Duration::from_secs(21_600)));
        state.index_attempt = Some((std::time::Instant::now(), true));
        assert!(!state.index_due(Duration::from_secs(21_600)), "a failed index waits too");
    }
}
