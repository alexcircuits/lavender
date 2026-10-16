//! Launch Library 2 space launches ingestion and satellite matching test suite (ENG-957).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreadable_literal,
    clippy::assert_is_empty
)]

use lavender_core::config::CoreConfig;
use lavender_core::fetch::Fetcher;
use lavender_core::providers::celestrak;
use lavender_core::providers::launches::LaunchesProvider;
use lavender_core::providers::launches::parsing::parse;
use lavender_core::providers::{FeedProvider, FetchContext, Region};
use lavender_core::satellite_catalog::SatelliteCatalog;
use lavender_core::{LavenderCore, MemorySecretStore};
use lavender_domain::{BBox, FeedId, FeedItems, FeedQuery, SatelliteElements, SearchQuery};
use tokio_util::sync::CancellationToken;
use url::Url;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FIXTURE_LL2: &[u8] = include_bytes!("fixtures/launches-ll2.json");
const FIXTURE_EMPTY: &[u8] = include_bytes!("fixtures/launches-empty.json");
const FIXTURE_MALFORMED: &[u8] = include_bytes!("fixtures/launches-malformed.json");
/// A recorded LL2 2.3.0 detailed response and the whole CelesTrak `last-30-days` OMM group,
/// downloaded together (see `fixtures/launches-provenance.md`).
const RECORDED_LL2: &[u8] = include_bytes!("fixtures/launches-ll2-recorded.json");
const RECORDED_CELESTRAK: &[u8] = include_bytes!("fixtures/celestrak-last-30-days-recorded.json");
const RECORDED_AT: &str = "2026-10-15T20:31:16Z";

#[test]
fn request_window_includes_recent_and_upcoming_launches() {
    let now: jiff::Timestamp = "2026-10-15T12:00:00Z".parse().unwrap();
    let provider = LaunchesProvider::new(CoreConfig::default().providers.launches);
    let url = provider.build_url(now).unwrap();
    let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
    assert_eq!(query["net__gte"], "2026-09-15T12:00:00Z");
    assert_eq!(query["net__lte"], "2026-11-14T12:00:00Z");
    // One page holds the whole window (LL2 allows at most 100 per page). Newest first, so a
    // window too large for one page loses its oldest launches, never the upcoming ones.
    assert_eq!(query["ordering"], "-net");
    assert_eq!(query["limit"], "100");
}

#[test]
fn recorded_response_contains_recent_and_upcoming_launches() {
    let page = parse(RECORDED_LL2).unwrap();
    let launches = page.launches;
    assert_eq!((launches.len(), page.dropped, page.not_returned), (8, 0, 0));
    let recorded_at: jiff::Timestamp = RECORDED_AT.parse().unwrap();
    let recent = launches.iter().filter(|l| l.net.unwrap() <= recorded_at).count();
    assert_eq!((recent, launches.len() - recent), (5, 3));

    let dragon = launches.iter().find(|l| l.name.ends_with("Dragon CRS-2 SpX-35")).unwrap();
    assert_eq!(dragon.status, "Launch Successful");
    assert_eq!(dragon.provider.as_deref(), Some("SpaceX"));
    assert_eq!(dragon.rocket.as_deref(), Some("Falcon 9 Block 5"));
    assert_eq!(dragon.mission.as_deref(), Some("Dragon CRS-2 SpX-35"));
    let pad = dragon.pad.as_ref().unwrap();
    assert_eq!(pad.name, "Space Launch Complex 40");
    assert_eq!(pad.location_name.as_deref(), Some("Cape Canaveral SFS, FL, USA"));
    assert!((pad.latitude - 28.561_941_22).abs() < 1e-9);
    assert!((pad.longitude + 80.577_357_36).abs() < 1e-9);

    // Fields of the legacy mission card.
    assert_eq!(dragon.launch_designator.as_deref(), Some("2026-233"));
    assert_eq!(dragon.orbit.as_deref(), Some("Low Earth Orbit"));
    let description = dragon.mission_description.as_deref().unwrap();
    assert!(description.starts_with("35th commercial resupply services mission"), "{description}");
    let satnet = launches.iter().find(|l| l.name.ends_with("SatNet LEO Group 27")).unwrap();
    let description = satnet.mission_description.as_deref().unwrap();
    assert!(!description.contains('\r') && !description.contains('\n'), "{description:?}");

    let mmx = launches.iter().find(|l| l.name.ends_with("(MMX)")).unwrap();
    assert_eq!(mmx.status, "Go for Launch");
    assert_eq!(mmx.net.unwrap().to_string(), "2026-10-19T19:41:03Z");
    assert!(mmx.payload_names.is_empty(), "LL2 lists no payload flights for this launch");
    assert_eq!(mmx.launch_designator, None, "a designator is assigned only after launch");
    assert_eq!(mmx.orbit.as_deref(), Some("Mars Orbit"));
}

#[test]
fn an_upcoming_launch_is_never_matched_to_satellites_already_in_orbit() {
    let catalog = SatelliteCatalog::new();
    catalog.update(&[dummy_satellite(60100, "SXM-11", Some("2026-050A"), "2026")]);
    let launches = parse(FIXTURE_LL2).unwrap().launches;
    let sxm = |launches: &[lavender_domain::Launch]| {
        launches.iter().find(|l| l.name.contains("SXM-11")).unwrap().matched_norad_ids.clone()
    };

    // Seen before its 2026-06-01 launch, the SXM-11 mission carried nothing into orbit yet.
    let mut before = launches.clone();
    catalog.match_launches(&mut before, "2026-05-31T00:00:00Z".parse().unwrap());
    assert!(sxm(&before).is_empty());
    let mut after = launches;
    catalog.match_launches(&mut after, "2026-06-02T00:00:00Z".parse().unwrap());
    assert_eq!(sxm(&after), vec![60100]);
}

#[test]
fn recorded_launches_match_recorded_celestrak_objects_by_cospar_designator() {
    let mut launches = parse(RECORDED_LL2).unwrap().launches;
    let (elements, dropped) = celestrak::parse(RECORDED_CELESTRAK, "last-30-days").unwrap();
    assert_eq!((elements.len(), dropped), (282, 0));
    let catalog = SatelliteCatalog::new();
    catalog.update(&elements);
    catalog.match_launches(&mut launches, RECORDED_AT.parse().unwrap());

    let matched = |suffix: &str| {
        launches.iter().find(|l| l.name.ends_with(suffix)).unwrap().matched_norad_ids.clone()
    };
    // Every payload of each launch, and nothing else: rocket bodies (CZ-12 R/B,
    // ZHUQUE-2E R/B) and the objects of the other 20 launches in the group are not matched.
    assert_eq!(
        matched("SatNet LEO Group 27"),
        vec![100973, 100974, 100977, 100978, 100979, 100980, 100981]
    );
    assert_eq!(matched("SDA Tranche 1 Transport Layer A"), (100983..=101003).collect::<Vec<_>>());
    assert_eq!(matched("Starlink Group 15-25"), (101004..=101030).collect::<Vec<_>>());
    assert_eq!(matched("Dragon CRS-2 SpX-35"), vec![101031]);
    assert_eq!(matched("10 satellites"), (101060..=101069).collect::<Vec<_>>());
    for upcoming in ["USSF-481", "Mid-Inclination Rideshare)", "(MMX)"] {
        assert!(matched(upcoming).is_empty(), "{upcoming} has not launched");
    }
}

#[tokio::test]
async fn a_window_larger_than_one_page_is_reported_not_hidden() {
    let mut body: serde_json::Value = serde_json::from_slice(FIXTURE_LL2).unwrap();
    body["count"] = 150.into();
    body["next"] = "https://ll.thespacedevs.com/2.3.0/launches/?limit=100&offset=100".into();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/2.3.0/launches/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;
    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".to_owned()];
    config.providers.launches.base_url = Url::parse(&format!("{}/2.3.0/", server.uri())).unwrap();
    let fetcher = Fetcher::new(config.fetch).unwrap();
    let secrets = MemorySecretStore::default();
    let ctx = FetchContext {
        fetcher: &fetcher,
        secrets: &secrets,
        cancel: CancellationToken::new(),
        cache_dir: None,
    };
    let answer =
        LaunchesProvider::new(config.providers.launches).fetch(&ctx, Region::Global).await.unwrap();
    assert_eq!(answer.items.len(), 3);
    assert!(answer.degraded.is_none(), "a full page is a complete answer, not a failure");
    assert!(
        answer.notices.iter().any(|n| n.contains("150 launches") && n.contains("latest 4")),
        "{:?}",
        answer.notices
    );
}

/// The launches in `items`, by name suffix, with the satellites matched to each.
fn matches_by_launch(items: &FeedItems) -> impl Fn(&str) -> Vec<u32> + '_ {
    let FeedItems::Launches(launches) = items else { panic!("not launches: {items:?}") };
    move |suffix| {
        launches.iter().find(|l| l.name.ends_with(suffix)).unwrap().matched_norad_ids.clone()
    }
}

#[tokio::test]
async fn the_default_configuration_loads_the_satellites_recent_launches_carried() {
    // Recently launched payloads are in CelesTrak's `last-30-days` group and in none of the
    // operational groups, so the default satellite groups must include it for matching to
    // find them. Every other group answers with no objects here.
    let server = MockServer::start().await;
    Mock::given(path("/celestrak/gp.php"))
        .and(query_param("GROUP", "last-30-days"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(RECORDED_CELESTRAK))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/celestrak/gp.php"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.3.0/launches/"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(RECORDED_LL2))
        .expect(1)
        .mount(&server)
        .await;
    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".to_owned()];
    config.hub.linger_s = 0;
    config.providers.celestrak.base_url =
        Url::parse(&format!("{}/celestrak/", server.uri())).unwrap();
    config.providers.launches.base_url = Url::parse(&format!("{}/2.3.0/", server.uri())).unwrap();
    let core = LavenderCore::builder(config).build().unwrap();

    let satellites = core.snapshot(FeedQuery::new(FeedId::Satellites)).await.unwrap();
    assert_eq!(satellites.items.len(), 282);
    let launches = core.snapshot(FeedQuery::new(FeedId::Launches)).await.unwrap();
    let matched = matches_by_launch(&launches.items);
    assert_eq!(matched("SatNet LEO Group 27").len(), 7);
    assert_eq!(matched("SDA Tranche 1 Transport Layer A").len(), 21);
    assert_eq!(matched("Starlink Group 15-25").len(), 27);
    assert_eq!(matched("Dragon CRS-2 SpX-35"), vec![101031]);
    assert_eq!(matched("10 satellites").len(), 10);
    core.shutdown();
    server.verify().await;
}

#[test]
fn only_a_well_formed_cospar_designator_is_kept() {
    // Matching takes every catalogue object whose designator starts with the launch's, so a
    // short or garbled value must never reach it: "2026-" would claim every 2026 object.
    let row =
        |designator: &str| serde_json::json!({ "id": designator, "launch_designator": designator });
    let rejected = [
        "2026",
        "2026-",
        "2026-2",
        "2026-23",
        "2026-2301",
        "2026-23A",
        "26-230",
        "2026-230A",
        "２０２６-230",
        "2026_230",
        "",
    ];
    let mut rows: Vec<_> = rejected.iter().map(|d| row(d)).collect();
    rows.push(row(" 2026-230 "));
    rows.push(serde_json::json!({ "id": "number", "launch_designator": 2026 }));
    let page =
        parse(&serde_json::to_vec(&serde_json::json!({ "results": rows })).unwrap()).unwrap();
    let designator = |id: &str| -> Option<String> {
        page.launches.iter().find(|l| l.id == id).unwrap().launch_designator.clone()
    };
    for bad in rejected.iter().filter(|d| !d.is_empty()) {
        assert_eq!(designator(bad), None, "{bad:?} is not a COSPAR launch designator");
    }
    assert_eq!(designator("number"), None);
    assert_eq!(designator("2026-230").as_deref(), Some("2026-230"), "surrounding space is trimmed");
}

#[test]
fn untrusted_text_is_cleaned_and_bounded() {
    let long = "X".repeat(5_000);
    let payloads: Vec<_> = (0..500)
        .map(|i| serde_json::json!({ "payload": { "name": format!("SAT-{i}") } }))
        .collect();
    let body = serde_json::json!({ "results": [{
        "id": "launch\u{0}-1",
        "name": format!("Falcon\u{7} 9\n|   {long}"),
        "status": { "name": "Go\tfor Launch" },
        "rocket": { "payloads": payloads },
        "mission": { "name": long, "description": long },
    }]});
    let page = parse(&serde_json::to_vec(&body).unwrap()).unwrap();
    assert_eq!(page.dropped, 0);
    let launch = &page.launches[0];
    assert_eq!(launch.id, "launch-1");
    assert!(launch.name.starts_with("Falcon 9 | XXX"), "{}", launch.name);
    assert!(launch.name.chars().count() <= 200);
    assert_eq!(launch.status, "Go for Launch");
    assert!(launch.mission.as_ref().unwrap().chars().count() <= 200);
    assert!(launch.payload_names.len() <= 64, "{}", launch.payload_names.len());
}

fn dummy_satellite(
    norad_id: u32,
    name: &str,
    intl_designator: Option<&str>,
    year_str: &str,
) -> SatelliteElements {
    SatelliteElements {
        norad_id,
        name: name.to_owned(),
        intl_designator: intl_designator.map(str::to_owned),
        group: "test".to_owned(),
        epoch: format!("{year_str}-01-01T00:00:00Z").parse().unwrap(),
        mean_motion: 15.0,
        eccentricity: 0.001,
        inclination_deg: 51.6,
        raan_deg: 100.0,
        arg_of_pericenter_deg: 50.0,
        mean_anomaly_deg: 200.0,
        bstar: 0.0001,
        mean_motion_dot: 0.0,
        mean_motion_ddot: 0.0,
        rev_at_epoch: None,
        classification: None,
    }
}

#[test]
fn test_launches_fixture_parsing() {
    let page = parse(FIXTURE_LL2).expect("valid fixture must parse");
    let (launches, dropped) = (page.launches, page.dropped);
    assert_eq!(launches.len(), 3, "expected 3 valid launches");
    assert_eq!(dropped, 1, "expected 1 malformed row to be dropped");

    // SXM-11
    let sxm = launches.iter().find(|l| l.name.contains("Sirius SXM-11")).unwrap();
    assert_eq!(sxm.id, "11111111-2222-3333-4444-555555555555");
    assert_eq!(sxm.status, "Success");
    assert_eq!(sxm.provider.as_deref(), Some("SpaceX"));
    assert_eq!(sxm.rocket.as_deref(), Some("Falcon 9 Block 5"));
    assert_eq!(sxm.mission.as_deref(), Some("Sirius SXM-11"));
    assert_eq!(sxm.payload_names, vec!["SXM-11"]);

    let pad = sxm.pad.as_ref().expect("pad must be present");
    assert_eq!(pad.name, "Space Launch Complex 40");
    assert!((pad.latitude - 28.561941).abs() < 1e-4);
    assert!((pad.longitude - (-80.5772)).abs() < 1e-4);
    assert_eq!(pad.location_name.as_deref(), Some("Cape Canaveral SFS, FL, USA"));

    // Starlink
    let starlink = launches.iter().find(|l| l.name.contains("Starlink Group 17-40")).unwrap();
    assert_eq!(starlink.status, "Go for Launch");
    assert_eq!(starlink.provider.as_deref(), Some("SpaceX"));
    let sl_pad = starlink.pad.as_ref().unwrap();
    assert!((sl_pad.latitude - 34.632).abs() < 1e-3);
    assert!((sl_pad.longitude - (-120.611)).abs() < 1e-3);

    // Cygnus
    let cygnus = launches.iter().find(|l| l.name.contains("Cygnus NG-21")).unwrap();
    assert_eq!(cygnus.status, "Success");
    assert_eq!(cygnus.payload_names, vec!["Cygnus NG-21 (S.S. Francis R. Dick Scobee)"]);
}

#[test]
fn test_launches_empty_and_malformed_fixtures() {
    let page = parse(FIXTURE_EMPTY).expect("empty fixture should be valid");
    let (launches, dropped) = (page.launches, page.dropped);
    assert!(launches.is_empty());
    assert_eq!(dropped, 0);

    let err = parse(FIXTURE_MALFORMED).expect_err("malformed fixture must return error");
    assert!(matches!(err, lavender_core::providers::ProviderError::Parse { .. }));
}

#[test]
fn test_satellite_matching_logic() {
    let mut launches = parse(FIXTURE_LL2).unwrap().launches;
    let catalog = SatelliteCatalog::new();

    // Populate catalog with satellites
    let sats = vec![
        // Direct payload match for SXM-11 with matching 2026 launch year
        dummy_satellite(60100, "SXM-11", Some("2026-050A"), "2026"),
        // Year mismatch for SXM-11 (namesake from 2010 should be rejected!)
        dummy_satellite(30000, "Sirius SXM-11", Some("2010-025A"), "2010"),
        // Starlink satellite from older 2019 launch - weak name match score should not match
        dummy_satellite(44713, "STARLINK-1008", Some("2019-074A"), "2019"),
        // Cygnus payload match
        dummy_satellite(60500, "CYGNUS NG-21", Some("2026-065A"), "2026"),
    ];
    catalog.update(&sats);

    // The synthetic launches have no COSPAR designator, so name matching applies.
    let after_the_launches = "2026-10-15T00:00:00Z".parse().unwrap();
    catalog.match_launches(&mut launches, after_the_launches);

    // SXM-11 matches 60100, ignores 30000
    let sxm = launches.iter().find(|l| l.name.contains("Sirius SXM-11")).unwrap();
    assert_eq!(sxm.matched_norad_ids, vec![60100]);

    // Starlink does NOT match an arbitrary starlink satellite from 2019
    let starlink = launches.iter().find(|l| l.name.contains("Starlink Group 17-40")).unwrap();
    assert!(starlink.matched_norad_ids.is_empty());

    // Cygnus matches 60500
    let cygnus = launches.iter().find(|l| l.name.contains("Cygnus NG-21")).unwrap();
    assert_eq!(cygnus.matched_norad_ids, vec![60500]);
}

#[tokio::test]
async fn test_core_snapshot_filtering_and_search() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/2.3.0/launches/"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(FIXTURE_LL2))
        .expect(1)
        .mount(&server)
        .await;

    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".to_owned()];
    config.hub.linger_s = 0;
    config.providers.launches.base_url = Url::parse(&format!("{}/2.3.0/", server.uri())).unwrap();
    config.providers.launches.cadence_s = 60;

    let core = LavenderCore::builder(config).build().unwrap();

    // 1. Full snapshot
    let snapshot = core.snapshot(FeedQuery::new(FeedId::Launches)).await.unwrap();
    assert_eq!(snapshot.feed, FeedId::Launches);
    assert_eq!(snapshot.items.len(), 3);

    // 2. Viewport query centered on Florida / Cape Canaveral (28.56, -80.57)
    // Box: west -82.0, south 27.0, east -79.0, north 30.0
    let florida_bbox = BBox::new(-82.0, 27.0, -79.0, 30.0).unwrap();
    let fl_snapshot =
        core.snapshot(FeedQuery::new(FeedId::Launches).with_bbox(florida_bbox)).await.unwrap();
    assert_eq!(
        fl_snapshot.items.len(),
        2,
        "Florida box must include SXM-11 and Cygnus, but exclude Vandenberg"
    );

    // 3. Search for SXM
    let search_results = core
        .search(&SearchQuery { q: "SXM".to_owned(), near: None, limit: Some(10), places: None })
        .await
        .unwrap();
    assert!(!search_results.entities.is_empty());
    let hit = search_results
        .entities
        .iter()
        .find(|e| e.id == "11111111-2222-3333-4444-555555555555")
        .unwrap();
    assert_eq!(hit.feed, FeedId::Launches);
    assert!(hit.label.contains("Sirius SXM-11"));
}
