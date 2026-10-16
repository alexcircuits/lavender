//! FIRMS acceptance contract, generated synthetic detections only; no live NASA calls.
//!
//! - Every valid detection is counted exactly once in every view (aggregation contract).
//! - The worldwide snapshot, serialised exactly as the HTTP API sends it, stays ≤ 1 MB.
//! - Upstream load does not depend on viewers: one request per source per cadence.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fmt::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lavender_core::config::CoreConfig;
use lavender_core::providers::{FeedProvider, FetchContext, Fetched, ProviderError, Region};
use lavender_core::{LavenderCore, MemorySecretStore, SecretKey};
use lavender_domain::{
    BBox, FeedId, FeedItems, FeedQuery, FeedSnapshot, FireCell, Freshness, GeoPoint, ProviderScope,
    Timestamp,
};
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "synthetic-key";
/// The acceptance bound for one snapshot, in bytes of JSON.
const ONE_MEGABYTE: usize = 1_000_000;
const HEADER: &str =
    "latitude,longitude,bright_ti4,acq_date,acq_time,satellite,confidence,frp,daynight\n";

/// A core whose FIRMS provider reads `sources` from `server`. With `linger_s = 0` every
/// snapshot that ends releases the hub slot, so the next one starts a new refresh task:
/// the worst case for upstream load.
fn firms_core(server: &MockServer, sources: &[&str]) -> LavenderCore {
    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".into()];
    // The provider must override this with no retries of its own.
    config.fetch.retries = 2;
    config.hub.linger_s = 0;
    config.hub.first_result_timeout_s = 20;
    config.providers.firms.base_url = Url::parse(&format!("{}/firms/", server.uri())).unwrap();
    config.providers.firms.sources = sources.iter().map(|s| (*s).to_owned()).collect();
    LavenderCore::builder(config)
        .secrets(Arc::new(MemorySecretStore::with([(SecretKey::FirmsMapKey, KEY)])))
        .build()
        .unwrap()
}

fn source_path(source: &str) -> String {
    format!("/firms/{KEY}/{source}/world/2")
}

fn acquired_now() -> (String, String) {
    let now = Timestamp::now();
    (now.strftime("%Y-%m-%d").to_string(), now.strftime("%H%M").to_string())
}

fn fires(snapshot: &FeedSnapshot) -> &[FireCell] {
    let FeedItems::Fires(cells) = &snapshot.items else { panic!("unexpected feed") };
    cells
}

fn total_count(snapshot: &FeedSnapshot) -> u64 {
    fires(snapshot).iter().map(|c| u64::from(c.count)).sum()
}

fn json_bytes(snapshot: &FeedSnapshot) -> usize {
    // `GET /api/v1/feeds/{feed}` answers `Json(snapshot)`: these exact bytes.
    serde_json::to_vec(snapshot).unwrap().len()
}

/// A worldwide CSV built to be as hard as possible for the aggregator and the 1 MB
/// bound, with the valid detections it contains (as latitude, longitude, FRP).
struct DenseCsv {
    body: String,
    valid: Vec<(f64, f64, f64)>,
    malformed: usize,
}

fn dense_adversarial_csv() -> DenseCsv {
    let (date, time) = acquired_now();
    // The longest metadata the parser keeps (16/16/8 characters), in 4-byte characters,
    // so every cell carries the most JSON it can.
    let (satellite, confidence, day_night) = ("𐀀".repeat(16), "𐀀".repeat(16), "𐀀".repeat(8));
    let mut valid = Vec::new();
    // One detection in every 1° cell of the world grid.
    for lat in -90_i32..90 {
        for lon in -180_i32..180 {
            let frp = f64::from((lat + lon).rem_euclid(40)) + 0.5;
            valid.push((f64::from(lat) + 0.123_456_789, f64::from(lon) + 0.987_654_321, frp));
        }
    }
    // Four hot spots of 10,000 distinct detections, each inside ~0.07°.
    for (hot_lat, hot_lon) in [(-15.0, 130.0), (60.0, 100.0), (0.0, 20.0), (40.0, -120.0)] {
        for i in 0..10_000_u32 {
            let step = f64::from(i);
            let lat = hot_lat + (step % 100.0) * 0.000_7;
            let lon = hot_lon + (step / 100.0).floor() * 0.000_7;
            valid.push((lat, lon, f64::from(i % 50)));
        }
    }
    // Poles, corners and both sides of the antimeridian, at the largest finite FRP.
    for (lat, lon) in [
        (90.0, 180.0),
        (90.0, -180.0),
        (-90.0, 180.0),
        (-90.0, -180.0),
        (0.0, 180.0),
        (0.0, -180.0),
        (10.0, 179.999_99),
        (10.0, -179.999_99),
    ] {
        valid.push((lat, lon, f64::MAX));
    }
    let mut body = String::from(HEADER);
    for (lat, lon, frp) in &valid {
        writeln!(
            body,
            "{lat},{lon},330.5,{date},{time},{satellite},{confidence},{frp},{day_night}"
        )
        .unwrap();
    }
    // An unreadable optional field (brightness) does not discard the detection.
    writeln!(body, "33.3,44.4,NaN,{date},{time},N20,n,7,D").unwrap();
    valid.push((33.3, 44.4, 7.0));
    // Exact duplicate rows describe the same detections: counted once.
    for (lat, lon, frp) in valid.iter().take(1_000) {
        writeln!(
            body,
            "{lat},{lon},330.5,{date},{time},{satellite},{confidence},{frp},{day_night}"
        )
        .unwrap();
    }
    let malformed = [
        format!("NaN,10,330,{date},{time},N20,n,5,D"),
        format!("1e999,10,330,{date},{time},N20,n,5,D"),
        format!(",10,330,{date},{time},N20,n,5,D"),
        format!("90.00001,10,330,{date},{time},N20,n,5,D"),
        format!("-90.5,10,330,{date},{time},N20,n,5,D"),
        format!("10,inf,330,{date},{time},N20,n,5,D"),
        format!("10,-inf,330,{date},{time},N20,n,5,D"),
        format!("10,180.00001,330,{date},{time},N20,n,5,D"),
        format!("10,-181,330,{date},{time},N20,n,5,D"),
        format!("10,10,330,{date},{time},N20,n,NaN,D"),
        format!("10,10,330,{date},{time},N20,n,-1,D"),
        format!("10,10,330,{date},{time},N20,n,1e309,D"),
        format!("10,10,330,{date},{time},N20,n,inf,D"),
        format!("10,10,330,2026-13-45,{time},N20,n,5,D"),
        format!("10,10,330,{date},2460,N20,n,5,D"),
        "10,10,330".to_owned(),
    ];
    for row in &malformed {
        writeln!(body, "{row}").unwrap();
    }
    DenseCsv { body, valid, malformed: malformed.len() }
}

#[tokio::test]
async fn dense_adversarial_world_counts_every_detection_once_in_every_view_under_one_megabyte() {
    let server = MockServer::start().await;
    let csv = dense_adversarial_csv();
    assert!(csv.valid.len() >= 100_000);
    Mock::given(method("GET"))
        .and(path(source_path("VIIRS_NOAA20_NRT")))
        .respond_with(ResponseTemplate::new(200).set_body_string(csv.body.clone()))
        .expect(1)
        .mount(&server)
        .await;
    let core = firms_core(&server, &["VIIRS_NOAA20_NRT"]);
    // One view per grid starting size: world (1°), ≤ 20° (0.5°), ≤ 10° (0.25°) and
    // ≤ 5° (individual detections or 0.1°), plus dateline, polar and hot-spot views.
    let views = [
        None,
        Some(BBox::WORLD),
        Some(BBox::new(-30.0, -20.0, 30.0, 20.0).unwrap()),
        Some(BBox::new(-10.0, -10.0, 10.0, 10.0).unwrap()),
        Some(BBox::new(125.0, -20.0, 135.0, -10.0).unwrap()),
        Some(BBox::new(128.0, -16.0, 133.0, -11.0).unwrap()),
        Some(BBox::new(129.0, -16.0, 131.0, -14.0).unwrap()),
        Some(BBox::new(130.0, -15.0, 130.1, -14.9).unwrap()),
        Some(BBox::new(10.0, 10.0, 12.0, 12.0).unwrap()),
        Some(BBox::new(170.0, -10.0, -170.0, 10.0).unwrap()),
        Some(BBox::new(179.0, 9.0, -179.0, 11.0).unwrap()),
        Some(BBox::new(-180.0, 85.0, 180.0, 90.0).unwrap()),
        Some(BBox::new(-180.0, -90.0, -175.0, -85.0).unwrap()),
    ];
    let malformed_notice = format!("NASA FIRMS: {} malformed records skipped.", csv.malformed);
    let mut report = String::new();
    for view in views {
        for min_frp in [None, Some(10.0)] {
            let mut query = FeedQuery::new(FeedId::Fires);
            query.bbox = view;
            query.filters.min_frp = min_frp;
            let snapshot = core.snapshot(query).await.unwrap();
            let expected = csv
                .valid
                .iter()
                .filter(|(_, _, frp)| min_frp.is_none_or(|min| *frp >= min))
                .filter(|(lat, lon, _)| {
                    let point = GeoPoint::new(*lat, *lon).unwrap();
                    view.is_none_or(|b| b.contains(&point))
                })
                .count() as u64;
            let label = format!("{view:?} minFrp {min_frp:?}");
            assert_eq!(snapshot.freshness, Freshness::Live, "{label}");
            assert_eq!(snapshot.source_count as usize, csv.valid.len(), "{label}");
            assert_eq!(total_count(&snapshot), expected, "{label}");
            assert_eq!(snapshot.notices, vec![malformed_notice.clone()], "{label}");
            let bytes = json_bytes(&snapshot);
            assert!(bytes <= ONE_MEGABYTE, "{label}: {bytes} bytes");
            writeln!(
                report,
                "{label}: {expected} detections in {} cells, {bytes} bytes",
                fires(&snapshot).len()
            )
            .unwrap();
        }
    }
    println!("{report}");
}

#[tokio::test]
async fn nearby_observations_remain_distinct_but_exact_duplicates_coalesce() {
    let server = MockServer::start().await;
    let (date, time) = acquired_now();
    let row = format!("10.00001,20.00001,320,{date},{time},N20,n,5,N\n");
    let csv = format!("{HEADER}{row}{row}10.00002,20.00002,320,{date},{time},N20,n,5,N\n");
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(csv))
        .expect(1)
        .mount(&server)
        .await;
    let core = firms_core(&server, &["VIIRS_NOAA20_NRT"]);
    let snapshot = core.snapshot(FeedQuery::new(FeedId::Fires)).await.unwrap();
    assert_eq!(snapshot.source_count, 2);
    assert_eq!(total_count(&snapshot), 2);
}

/// A fire provider that gives every region the same answer, as overlapping regional
/// fetches of one upstream would, and records which regions it was asked for.
#[derive(Debug)]
struct WeightedFires {
    answer: Fetched,
    scope: ProviderScope,
    regions: Mutex<Vec<Region>>,
}

#[async_trait::async_trait]
impl FeedProvider for WeightedFires {
    fn feed(&self) -> FeedId {
        FeedId::Fires
    }
    fn source_id(&self) -> &'static str {
        lavender_core::sources::NASA_FIRMS
    }
    fn cadence(&self) -> Duration {
        Duration::from_mins(10)
    }
    fn scope(&self) -> ProviderScope {
        self.scope
    }
    fn enabled(&self) -> bool {
        true
    }
    async fn fetch(
        &self,
        _ctx: &FetchContext<'_>,
        region: Region,
    ) -> Result<Fetched, ProviderError> {
        self.regions.lock().unwrap().push(region);
        Ok(self.answer.clone())
    }
}

/// A core over a [`WeightedFires`] provider. The NASA FIRMS source requires its map key
/// even when a test injects the provider, exactly as production gates the layer.
fn weighted_core(answer: Fetched, scope: ProviderScope) -> (LavenderCore, Arc<WeightedFires>) {
    let provider = Arc::new(WeightedFires { answer, scope, regions: Mutex::default() });
    let core = LavenderCore::builder(CoreConfig::default())
        .secrets(Arc::new(MemorySecretStore::with([(SecretKey::FirmsMapKey, KEY)])))
        .providers(vec![provider.clone()])
        .build()
        .unwrap();
    (core, provider)
}

/// Two detections of 5 MW each, already grouped upstream.
fn weighted_cell(time: Timestamp) -> FireCell {
    FireCell {
        lat: 10.0,
        lon: 20.0,
        brightness: 320.0,
        frp: 10.0,
        confidence: "n".into(),
        satellite: "synthetic".into(),
        acquired_at: time,
        day_night: "N".into(),
        count: 2,
        max_frp: 5.0,
        latest_acquired_at: time,
    }
}

async fn world_snapshot(cells: Vec<FireCell>, scope: ProviderScope) -> FeedSnapshot {
    let (core, _) = weighted_core(FeedItems::Fires(cells).into(), scope);
    core.snapshot(FeedQuery::new(FeedId::Fires)).await.unwrap()
}

#[tokio::test]
async fn distinct_weighted_cells_survive_hub_dedup_and_regrouping() {
    let earlier = Timestamp::now() - jiff::SignedDuration::from_mins(30);
    let later = Timestamp::now();
    let first = weighted_cell(earlier);
    // Three detections (9 + 3 + 3 MW), newer than `first`.
    let second = FireCell {
        count: 3,
        frp: 15.0,
        max_frp: 9.0,
        latest_acquired_at: later,
        acquired_at: later,
        ..first.clone()
    };
    let snapshot = world_snapshot(vec![first.clone(), first, second], ProviderScope::Global).await;
    let cells = fires(&snapshot);
    // The exact duplicate counts once; distinct cells at one place merge: counts and
    // total FRP add up, the peak FRP and the newest time are kept.
    assert_eq!(snapshot.source_count, 2);
    assert_eq!(cells.len(), 1);
    assert_eq!(cells[0].count, 5);
    assert!((cells[0].frp - 25.0).abs() < 1e-9, "total FRP {}", cells[0].frp);
    assert!((cells[0].max_frp - 9.0).abs() < 1e-9, "peak FRP {}", cells[0].max_frp);
    assert_eq!(cells[0].latest_acquired_at, later);
}

#[tokio::test]
async fn overlapping_regional_fetches_count_each_cell_once() {
    let time = Timestamp::now();
    let here = weighted_cell(time);
    let elsewhere = FireCell { lat: -40.0, lon: -170.0, count: 7, ..weighted_cell(time) };
    // A 90° grid plans eight world regions; each one answers both cells.
    let scope = ProviderScope::Regional { cell_deg: 90.0, max_cells: 8 };
    let (core, provider) = weighted_core(FeedItems::Fires(vec![here, elsewhere]).into(), scope);
    let snapshot = core.snapshot(FeedQuery::new(FeedId::Fires)).await.unwrap();
    assert_eq!(provider.regions.lock().unwrap().len(), 8);
    assert_eq!(snapshot.source_count, 2);
    assert_eq!(total_count(&snapshot), 9);
}

#[tokio::test]
async fn records_differing_in_any_field_are_not_duplicates() {
    let base = weighted_cell(Timestamp::now());
    let variants = vec![
        base.clone(),
        FireCell { brightness: 321.0, ..base.clone() },
        FireCell { confidence: "h".into(), ..base.clone() },
        FireCell { day_night: "D".into(), ..base.clone() },
    ];
    let snapshot = world_snapshot(variants, ProviderScope::Global).await;
    assert_eq!(snapshot.source_count, 4);
    assert_eq!(total_count(&snapshot), 8);
}

#[tokio::test]
async fn worst_case_world_envelope_still_fits_one_megabyte() {
    // Every 1° cell holds two weighted cells with the longest parser-kept metadata in
    // 4-byte characters and the largest finite numbers, and the provider reports a
    // degradation reason and more notices than the hub keeps, all oversized.
    let time = Timestamp::now();
    let mut cells = Vec::new();
    for lat in -90..90 {
        for lon in -180..180 {
            let cell = FireCell {
                lat: f64::from(lat) + 0.123_456_789_012_345,
                lon: f64::from(lon) + 0.987_654_321_098_765,
                brightness: f64::MAX,
                frp: f64::MAX,
                confidence: "𐀀".repeat(16),
                satellite: "𐀀".repeat(16),
                acquired_at: time,
                day_night: "𐀀".repeat(8),
                count: u32::MAX / 200_000,
                max_frp: f64::MAX,
                latest_acquired_at: time,
            };
            cells.push(FireCell { lat: cell.lat - 0.1, ..cell.clone() });
            cells.push(cell);
        }
    }
    let expected: u64 = cells.iter().map(|c| u64::from(c.count)).sum();
    let (core, _) = weighted_core(oversized_answer(cells), ProviderScope::Global);
    let snapshot = core.snapshot(FeedQuery::new(FeedId::Fires)).await.unwrap();
    assert!(matches!(snapshot.freshness, Freshness::Degraded { .. }));
    assert_eq!(total_count(&snapshot), expected);
    let bytes = json_bytes(&snapshot);
    println!("worst-case world envelope: {} cells, {bytes} bytes", fires(&snapshot).len());
    assert!(bytes <= ONE_MEGABYTE, "{bytes} bytes");
}

/// The degradation reason and notices with the most JSON bytes a provider can make the
/// hub report: the hub keeps at most 8 distinct notices of at most 200 characters, and
/// a 4-byte character is the most JSON one character can become.
fn oversized_answer(cells: Vec<FireCell>) -> Fetched {
    let oversized = |i: usize| format!("{i}{}", "𐀀".repeat(5_000));
    Fetched {
        items: FeedItems::Fires(cells),
        as_of: None,
        degraded: Some(oversized(0)),
        notices: (1..=20).map(oversized).collect(),
    }
}

/// `n` distinct individual detections inside a 1° view, each carrying the most JSON a
/// parsed FIRMS record can.
fn largest_detections(n: u32, time: Timestamp) -> Vec<FireCell> {
    (0..n)
        .map(|i| FireCell {
            lat: 10.000_000_000_000_1 + f64::from(i % 1_000) * 0.000_976_562_5,
            lon: 20.000_000_000_000_1 + f64::from(i / 1_000) * 0.000_976_562_5,
            brightness: f64::MAX,
            frp: f64::MAX,
            confidence: "𐀀".repeat(16),
            satellite: "𐀀".repeat(16),
            acquired_at: time,
            day_night: "𐀀".repeat(8),
            count: 1,
            max_frp: f64::MAX,
            latest_acquired_at: time,
        })
        .collect()
}

#[tokio::test]
async fn the_largest_answer_sent_before_grouping_still_fits_one_megabyte() {
    // A small view receives individual detections while they fit the cell byte budget.
    // Find the largest such answer, with the worst envelope, and measure it: this ties
    // the budget to the 1 MB bound whatever value the budget constant has.
    let time = Timestamp::now();
    let view = BBox::new(20.0, 10.0, 21.0, 11.0).unwrap();
    let snapshot_of = |n: u32| async move {
        let (core, _) =
            weighted_core(oversized_answer(largest_detections(n, time)), ProviderScope::Global);
        let snapshot = core.snapshot(FeedQuery::new(FeedId::Fires).with_bbox(view)).await.unwrap();
        assert_eq!(total_count(&snapshot), u64::from(n));
        core.shutdown();
        snapshot
    };
    let ungrouped = |snapshot: &FeedSnapshot, n: u32| fires(snapshot).len() == n as usize;
    // Binary search between an answer that is sent individually and one that is grouped.
    let (mut sent, mut grouped) = (1_u32, 10_000_u32);
    assert!(ungrouped(&snapshot_of(sent).await, sent));
    assert!(!ungrouped(&snapshot_of(grouped).await, grouped));
    while grouped - sent > 1 {
        let middle = sent + (grouped - sent) / 2;
        if ungrouped(&snapshot_of(middle).await, middle) {
            sent = middle;
        } else {
            grouped = middle;
        }
    }
    let largest = snapshot_of(sent).await;
    assert!(matches!(largest.freshness, Freshness::Degraded { .. }));
    assert_eq!(largest.notices.len(), 8, "every notice the hub keeps is filled");
    let bytes = json_bytes(&largest);
    println!("largest ungrouped answer: {sent} detections, {bytes} bytes");
    assert!(bytes <= ONE_MEGABYTE, "{sent} detections: {bytes} bytes");
}

/// Waits until the hub has dropped its FIRMS slot, so the next viewer starts a new
/// refresh task (which calls the provider again).
async fn wait_until_released(core: &LavenderCore) {
    for _ in 0..1_000 {
        let status = core.status();
        let fires = status.feeds.iter().find(|f| f.feed == FeedId::Fires).unwrap();
        if fires.active_regions == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("the hub kept its FIRMS slot");
}

/// The view of the n-th simulated viewer: a mix of world, regional and local views
/// with and without an FRP filter.
fn viewer_query(n: u32) -> FeedQuery {
    let mut query = FeedQuery::new(FeedId::Fires);
    let offset = f64::from(n % 30) * 10.0 - 150.0;
    query.bbox = match n % 4 {
        0 => None,
        1 => Some(BBox::new(offset, -10.0, offset + 30.0, 20.0).unwrap()),
        2 => Some(BBox::new(offset, 0.0, offset + 2.0, 2.0).unwrap()),
        _ => Some(BBox::new(170.0, -5.0, -170.0, 5.0).unwrap()),
    };
    query.filters.min_frp = n.is_multiple_of(3).then_some(1.0);
    query
}

#[tokio::test]
async fn any_number_of_viewers_cause_one_request_per_source_per_cadence() {
    let server = MockServer::start().await;
    let (date, time) = acquired_now();
    // The configuration allows at most ten sources; use all ten.
    let sources: Vec<String> = (1..=10).map(|i| format!("SOURCE_{i}")).collect();
    for (i, source) in sources.iter().enumerate() {
        let csv = format!("{HEADER}{i}.5,{i}.5,320,{date},{time},{source},n,5,D\n");
        Mock::given(method("GET"))
            .and(path(source_path(source)))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(csv)
                    .set_delay(Duration::from_millis(100)),
            )
            .expect(1)
            .mount(&server)
            .await;
    }
    let names: Vec<&str> = sources.iter().map(String::as_str).collect();
    let core = firms_core(&server, &names);
    // Live subscriptions and concurrent one-shot snapshots over many different views.
    let subscriptions: Vec<_> = (0..8).map(|n| core.subscribe(viewer_query(n)).unwrap()).collect();
    let snapshots =
        futures::future::join_all((0..60).map(|n| core.snapshot(viewer_query(n)))).await;
    for snapshot in snapshots {
        assert_eq!(snapshot.unwrap().freshness, Freshness::Live);
    }
    drop(subscriptions);
    // With no viewer left the hub slot is gone; each new viewer starts a new refresh
    // task, which the provider answers from its cache until the cadence has passed.
    for n in 0..20 {
        wait_until_released(&core).await;
        let snapshot = core.snapshot(viewer_query(n)).await.unwrap();
        assert_eq!(snapshot.freshness, Freshness::Live);
    }
    let world = core.snapshot(FeedQuery::new(FeedId::Fires)).await.unwrap();
    assert_eq!(total_count(&world), 10);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), sources.len(), "one request per source");
    // At most 10 sources per ≥ 10-minute cadence: far below 5,000 per 10 minutes.
    core.shutdown();
}

#[tokio::test]
async fn failed_and_partial_refreshes_are_not_retried_before_the_cadence() {
    let server = MockServer::start().await;
    let (date, time) = acquired_now();
    let responses = [
        ("FAILING", ResponseTemplate::new(503).insert_header("Retry-After", "0")),
        ("REFUSED", ResponseTemplate::new(200).set_body_string("Invalid MAP_KEY.")),
        (
            "WORKING",
            ResponseTemplate::new(200)
                .set_body_string(format!("{HEADER}1,1,320,{date},{time},N20,n,5,D\n")),
        ),
    ];
    for (source, response) in responses {
        Mock::given(method("GET"))
            .and(path(source_path(source)))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
    }
    // Partial success: the working source is served, degraded, and nothing is retried.
    // Viewers first share the subscription's slot, then each starts a new one.
    let partial = firms_core(&server, &["FAILING", "REFUSED", "WORKING"]);
    let subscription = partial.subscribe(FeedQuery::new(FeedId::Fires)).unwrap();
    let shared =
        futures::future::join_all((0..10).map(|n| partial.snapshot(viewer_query(n)))).await;
    for snapshot in shared {
        let snapshot = snapshot.unwrap();
        assert!(matches!(snapshot.freshness, Freshness::Degraded { .. }), "{snapshot:?}");
    }
    drop(subscription);
    for n in 0..10 {
        wait_until_released(&partial).await;
        let snapshot = partial.snapshot(FeedQuery::new(FeedId::Fires)).await.unwrap();
        assert!(matches!(snapshot.freshness, Freshness::Degraded { .. }), "{n}: {snapshot:?}");
        assert_eq!(total_count(&snapshot), 1, "the working source is still served");
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 3);

    // Total failure: unavailable, then cooling down without another request.
    server.reset().await;
    for source in ["FAILING", "REFUSED"] {
        let response = if source == "FAILING" {
            ResponseTemplate::new(503)
        } else {
            ResponseTemplate::new(200).set_body_string("Invalid MAP_KEY.")
        };
        Mock::given(method("GET"))
            .and(path(source_path(source)))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
    }
    let failing = firms_core(&server, &["FAILING", "REFUSED"]);
    for n in 0..20 {
        wait_until_released(&failing).await;
        let snapshot = failing.snapshot(viewer_query(n)).await.unwrap();
        assert!(matches!(snapshot.freshness, Freshness::Unavailable { .. }), "{snapshot:?}");
        assert_eq!(total_count(&snapshot), 0);
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
    partial.shutdown();
    failing.shutdown();
}

/// Opt-in measurement on real recorded FIRMS files, which are not committed. Set
/// `LAVENDER_FIRMS_RECORDED_CSV` to one or more CSV paths separated by commas (for
/// example NASA's public 24-hour global VIIRS/MODIS files) and run
/// `cargo test -p lavender-core --test firms -- --ignored --nocapture recorded`.
/// Each file is served as one source, exactly as the API would return it.
#[tokio::test]
#[ignore = "needs recorded FIRMS CSV files named in LAVENDER_FIRMS_RECORDED_CSV"]
async fn recorded_world_files_keep_every_detection_within_one_megabyte() {
    use lavender_core::providers::firms::{distinct_cells, filter_trailing_24h, parse_csv};

    let paths = std::env::var("LAVENDER_FIRMS_RECORDED_CSV").expect("no recorded files named");
    let server = MockServer::start().await;
    let mut sources = Vec::new();
    let mut parsed = Vec::new();
    for (i, file) in paths.split(',').enumerate() {
        let body = std::fs::read(file).unwrap();
        let (detections, dropped) = parse_csv(&body).unwrap();
        println!("{file}: {} bytes, {} rows, {dropped} malformed", body.len(), detections.len());
        parsed.extend(detections);
        let source = format!("RECORDED_{i}");
        Mock::given(method("GET"))
            .and(path(source_path(&source)))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .expect(1)
            .mount(&server)
            .await;
        sources.push(source);
    }
    // The default linger keeps one hub slot, so every view filters the same download.
    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".into()];
    config.hub.first_result_timeout_s = 60;
    config.providers.firms.base_url = Url::parse(&format!("{}/firms/", server.uri())).unwrap();
    config.providers.firms.sources = sources;
    let core = LavenderCore::builder(config)
        .secrets(Arc::new(MemorySecretStore::with([(SecretKey::FirmsMapKey, KEY)])))
        .build()
        .unwrap();
    let world = core.snapshot(FeedQuery::new(FeedId::Fires)).await.unwrap();
    // The provider kept the trailing day as of its download time.
    let recent: Vec<FireCell> = filter_trailing_24h(&parsed, world.fetched_at.unwrap())
        .into_iter()
        .map(Into::into)
        .collect();
    let distinct = distinct_cells(&recent);
    assert_eq!(world.source_count as usize, distinct.len());
    let views = [
        None,
        Some(BBox::WORLD),
        Some(BBox::new(-20.0, -35.0, 55.0, 38.0).unwrap()),
        Some(BBox::new(10.0, -20.0, 30.0, 0.0).unwrap()),
        Some(BBox::new(20.0, -10.0, 30.0, 0.0).unwrap()),
        Some(BBox::new(25.0, -8.0, 30.0, -3.0).unwrap()),
        Some(BBox::new(26.0, -6.0, 28.0, -4.0).unwrap()),
        Some(BBox::new(170.0, -60.0, -170.0, 60.0).unwrap()),
    ];
    for view in views {
        let mut query = FeedQuery::new(FeedId::Fires);
        query.bbox = view;
        let snapshot = core.snapshot(query).await.unwrap();
        let expected = distinct
            .iter()
            .filter(|c| view.is_none_or(|b| b.contains(&GeoPoint::new(c.lat, c.lon).unwrap())))
            .count() as u64;
        let bytes = json_bytes(&snapshot);
        println!(
            "{view:?}: {expected} detections in {} cells, {bytes} bytes",
            fires(&snapshot).len()
        );
        assert_eq!(total_count(&snapshot), expected, "{view:?}");
        assert!(bytes <= ONE_MEGABYTE, "{view:?}: {bytes} bytes");
    }
    core.shutdown();
}
