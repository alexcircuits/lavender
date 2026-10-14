//! Feed hub behaviour through the public `LavenderCore` facade, with fake providers so the
//! tests are deterministic and offline (ENG-943 / ENG-945 acceptance criteria).

// Test helpers outside #[test] fns are not covered by clippy.toml's test allowances.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use lavender_core::config::{CoreConfig, HubConfig};
use lavender_core::providers::{FeedProvider, FetchContext, Fetched, ProviderError, Region};
use lavender_core::{LavenderCore, RegionBudget, SubscriptionEvent, sources};
use lavender_domain::{
    Aircraft, ApiError, BBox, Coverage, Earthquake, ErrorCode, FeedFilters, FeedId, FeedItems,
    FeedQuery, FeedSnapshot, Freshness, GeoPoint, ProviderScope,
};

fn quake(id: &str, lat: f64, lon: f64, magnitude: f64) -> Earthquake {
    Earthquake {
        id: id.to_owned(),
        magnitude,
        magnitude_type: None,
        depth_km: 10.0,
        position: GeoPoint::new(lat, lon).unwrap(),
        place: Some(format!("near {id}")),
        time: Timestamp::now(),
        updated: None,
        url: None,
        tsunami: false,
        alert: None,
        felt: None,
        significance: None,
        status: None,
    }
}

fn aircraft(hex: &str, lat: f64, lon: f64, military: bool) -> Aircraft {
    Aircraft {
        hex: hex.to_owned(),
        callsign: Some(format!("T{hex}")),
        registration: None,
        type_code: None,
        category: None,
        position: GeoPoint::new(lat, lon).unwrap(),
        altitude_baro_ft: Some(30_000.0),
        altitude_geom_ft: None,
        on_ground: false,
        ground_speed_kt: None,
        track_deg: None,
        vertical_rate_fpm: None,
        squawk: None,
        emergency: None,
        is_military: military,
        seen_pos_s: 0.0,
        source: sources::ADSB_LOL.to_owned(),
    }
}

/// A global earthquake provider that counts fetches and can be told to fail.
#[derive(Debug, Default)]
struct FakeQuakes {
    fetches: AtomicUsize,
    fail: AtomicBool,
}

#[async_trait]
impl FeedProvider for FakeQuakes {
    fn feed(&self) -> FeedId {
        FeedId::Earthquakes
    }
    fn source_id(&self) -> &'static str {
        sources::USGS
    }
    fn cadence(&self) -> Duration {
        Duration::from_millis(200)
    }
    fn scope(&self) -> ProviderScope {
        ProviderScope::Global
    }
    fn enabled(&self) -> bool {
        true
    }
    async fn fetch(
        &self,
        _ctx: &FetchContext<'_>,
        _region: Region,
    ) -> Result<Fetched, ProviderError> {
        self.fetches.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            return Err(ProviderError::Unsupported("upstream is down".to_owned()));
        }
        Ok(FeedItems::Earthquakes(vec![
            quake("fiji", -17.0, 179.5, 5.1),
            quake("tonga", -20.0, -175.0, 4.2),
            quake("helsinki", 60.2, 24.9, 2.6),
        ])
        .into())
    }
}

/// A regional aircraft provider: each cell returns one aircraft at the cell centre plus a
/// duplicate seen by every cell (overlapping radius queries); global returns military only.
#[derive(Debug, Default)]
struct FakeAircraft {
    regions: std::sync::Mutex<Vec<Region>>,
}

#[async_trait]
impl FeedProvider for FakeAircraft {
    fn feed(&self) -> FeedId {
        FeedId::Aircraft
    }
    fn source_id(&self) -> &'static str {
        sources::ADSB_LOL
    }
    fn cadence(&self) -> Duration {
        Duration::from_secs(30)
    }
    fn scope(&self) -> ProviderScope {
        ProviderScope::Regional { cell_deg: 5.0, max_cells: 4 }
    }
    fn enabled(&self) -> bool {
        true
    }
    fn global_fallback_notice(&self) -> Option<&'static str> {
        Some("Showing military aircraft worldwide. Zoom in to see all traffic.")
    }
    async fn fetch(
        &self,
        _ctx: &FetchContext<'_>,
        region: Region,
    ) -> Result<Fetched, ProviderError> {
        self.regions.lock().unwrap().push(region);
        Ok(match region {
            Region::Global => FeedItems::Aircraft(vec![aircraft("ae0001", 10.0, 10.0, true)]),
            Region::Cell(c) => {
                let centre = c.center();
                let hex = format!("{:06x}", (c.lat_idx * 1000 + c.lon_idx).unsigned_abs());
                FeedItems::Aircraft(vec![
                    aircraft(&hex, centre.lat, centre.lon, false),
                    aircraft("abcdef", 61.0, 24.0, false),
                ])
            }
        }
        .into())
    }
}

fn core_with(providers: Vec<Arc<dyn FeedProvider>>, linger_s: u64) -> LavenderCore {
    core_with_hub(providers, HubConfig { linger_s, ..HubConfig::default() })
}

fn core_with_hub(providers: Vec<Arc<dyn FeedProvider>>, hub: HubConfig) -> LavenderCore {
    let config = CoreConfig { hub, ..CoreConfig::default() };
    LavenderCore::builder(config).providers(providers).build().unwrap()
}

fn aircraft_in(view: BBox) -> FeedQuery {
    FeedQuery::new(FeedId::Aircraft).with_bbox(view)
}

/// One 5° cell (lon 15–20, lat 60–65).
fn cell_a() -> BBox {
    BBox::new(15.5, 60.5, 19.5, 64.5).unwrap()
}

/// Another single cell (lon 25–30).
fn cell_b() -> BBox {
    BBox::new(25.5, 60.5, 29.5, 64.5).unwrap()
}

/// Two adjacent cells (lon 35–45).
fn two_cells() -> BBox {
    BBox::new(35.5, 60.5, 44.5, 64.5).unwrap()
}

/// Waits for a snapshot matching `pred`, skipping others.
async fn snapshot_where(
    sub: &mut lavender_core::FeedSubscription,
    pred: impl Fn(&FeedSnapshot) -> bool,
) -> Arc<FeedSnapshot> {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let s = next_snapshot(sub).await;
            if pred(&s) {
                return s;
            }
        }
    })
    .await
    .expect("matching snapshot in time")
}

async fn next_snapshot(sub: &mut lavender_core::FeedSubscription) -> Arc<FeedSnapshot> {
    match tokio::time::timeout(Duration::from_secs(5), sub.next()).await.expect("event in time") {
        Some(SubscriptionEvent::Snapshot(s)) => s,
        Some(SubscriptionEvent::Error(e)) => panic!("unexpected error {e:?}"),
        None => panic!("subscription ended"),
    }
}

fn quake_ids(s: &FeedSnapshot) -> Vec<String> {
    match &s.items {
        FeedItems::Earthquakes(v) => {
            let mut ids: Vec<String> = v.iter().map(|q| q.id.clone()).collect();
            ids.sort();
            ids
        }
        other => panic!("expected earthquakes, got {other:?}"),
    }
}

#[tokio::test]
async fn many_subscribers_share_one_refresh() {
    let fake = Arc::new(FakeQuakes::default());
    let core = core_with(vec![fake.clone()], 0);
    let mut subs: Vec<_> =
        (0..10).map(|_| core.subscribe(FeedQuery::new(FeedId::Earthquakes)).unwrap()).collect();
    for s in &mut subs {
        next_snapshot(s).await;
    }
    // One fetch served all ten subscribers.
    assert_eq!(fake.fetches.load(Ordering::SeqCst), 1);
    tokio::time::sleep(Duration::from_millis(700)).await;
    let after = fake.fetches.load(Ordering::SeqCst);
    assert!((3..=5).contains(&after), "about one fetch per 200 ms cadence, got {after}");
    assert_eq!(
        core.status().feeds.iter().find(|f| f.feed == FeedId::Earthquakes).unwrap().subscribers,
        10
    );
}

#[tokio::test]
async fn last_unsubscribe_cancels_the_refresh_task() {
    let fake = Arc::new(FakeQuakes::default());
    let core = core_with(vec![fake.clone()], 0);
    let mut sub = core.subscribe(FeedQuery::new(FeedId::Earthquakes)).unwrap();
    next_snapshot(&mut sub).await;
    drop(sub);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let at_drop = fake.fetches.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(
        fake.fetches.load(Ordering::SeqCst),
        at_drop,
        "no fetches after the last subscriber left"
    );
    assert_eq!(
        core.status().feeds.iter().find(|f| f.feed == FeedId::Earthquakes).unwrap().active_regions,
        0
    );
}

#[tokio::test]
async fn linger_keeps_the_slot_for_repeated_snapshots() {
    let fake = Arc::new(FakeQuakes::default());
    let core = core_with(vec![fake.clone()], 60);
    core.snapshot(FeedQuery::new(FeedId::Earthquakes)).await.unwrap();
    core.snapshot(FeedQuery::new(FeedId::Earthquakes)).await.unwrap();
    assert_eq!(fake.fetches.load(Ordering::SeqCst), 1, "second snapshot reused the lingering slot");
}

#[tokio::test]
async fn failure_keeps_last_good_items_and_reports_degraded() {
    let fake = Arc::new(FakeQuakes::default());
    let core = core_with(vec![fake.clone()], 0);
    let mut sub = core.subscribe(FeedQuery::new(FeedId::Earthquakes)).unwrap();
    let first = next_snapshot(&mut sub).await;
    assert_eq!(first.freshness, Freshness::Live);
    fake.fail.store(true, Ordering::SeqCst);
    let degraded = loop {
        let s = next_snapshot(&mut sub).await;
        if matches!(s.freshness, Freshness::Degraded { .. }) {
            break s;
        }
    };
    assert_eq!(quake_ids(&degraded), quake_ids(&first), "last good items are retained");
    let health = core.status().sources.into_iter().find(|h| h.source_id == sources::USGS).unwrap();
    assert!(health.last_error.unwrap().contains("upstream is down"));
}

#[tokio::test]
async fn never_succeeding_feed_reports_unavailable() {
    let fake = Arc::new(FakeQuakes::default());
    fake.fail.store(true, Ordering::SeqCst);
    let core = core_with(vec![fake], 0);
    let s = core.snapshot(FeedQuery::new(FeedId::Earthquakes)).await.unwrap();
    assert!(matches!(s.freshness, Freshness::Unavailable { .. }), "{:?}", s.freshness);
    assert_eq!(s.items.len(), 0);
}

#[tokio::test]
async fn filters_by_bbox_across_the_antimeridian_and_by_magnitude() {
    let core = core_with(vec![Arc::new(FakeQuakes::default())], 0);
    let pacific = BBox::new(170.0, -30.0, -170.0, -10.0).unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Earthquakes).with_bbox(pacific)).await.unwrap();
    assert_eq!(quake_ids(&s), vec!["fiji", "tonga"]);
    assert_eq!(s.source_count, 3, "count before filtering");
    let mut q = FeedQuery::new(FeedId::Earthquakes).with_bbox(pacific);
    q.filters = FeedFilters { min_magnitude: Some(5.0), ..FeedFilters::default() };
    assert_eq!(quake_ids(&core.snapshot(q).await.unwrap()), vec!["fiji"]);
}

#[tokio::test]
async fn updating_the_viewport_refilters_without_refetching_global_feeds() {
    let fake = Arc::new(FakeQuakes::default());
    let core = core_with(vec![fake.clone()], 0);
    let mut sub = core.subscribe(FeedQuery::new(FeedId::Earthquakes)).unwrap();
    assert_eq!(quake_ids(&*next_snapshot(&mut sub).await).len(), 3);
    let before = fake.fetches.load(Ordering::SeqCst);
    sub.update(
        FeedQuery::new(FeedId::Earthquakes).with_bbox(BBox::new(20.0, 55.0, 30.0, 65.0).unwrap()),
    )
    .unwrap();
    let s = next_snapshot(&mut sub).await;
    assert_eq!(quake_ids(&s), vec!["helsinki"]);
    assert!(
        fake.fetches.load(Ordering::SeqCst) <= before + 1,
        "re-filtering does not trigger extra fetches"
    );
    assert!(
        sub.update(FeedQuery::new(FeedId::Aircraft)).is_err(),
        "a subscription cannot switch feeds"
    );
}

#[tokio::test]
async fn regional_feeds_merge_cells_and_dedupe() {
    let fake = Arc::new(FakeAircraft::default());
    let core = core_with(vec![fake.clone()], 0);
    // 10° × 5° in the middle of cells: two 5° cells.
    let view = BBox::new(15.0, 60.0, 25.0, 65.0).unwrap();
    let mut sub = core.subscribe(FeedQuery::new(FeedId::Aircraft).with_bbox(view)).unwrap();
    let s = loop {
        let s = next_snapshot(&mut sub).await;
        if s.notices.is_empty() {
            break s;
        }
    };
    assert_eq!(s.coverage, Coverage::Complete);
    let FeedItems::Aircraft(list) = &s.items else { panic!() };
    let hexes: Vec<&str> = list.iter().map(|a| a.hex.as_str()).collect();
    assert_eq!(hexes.iter().filter(|h| **h == "abcdef").count(), 1, "duplicate removed: {hexes:?}");
    assert_eq!(fake.regions.lock().unwrap().len(), 2, "one fetch per cell");
}

#[tokio::test]
async fn large_viewports_fall_back_to_the_global_view() {
    let fake = Arc::new(FakeAircraft::default());
    let core = core_with(vec![fake.clone()], 0);
    let s = core.snapshot(FeedQuery::new(FeedId::Aircraft).with_bbox(BBox::WORLD)).await.unwrap();
    assert!(matches!(&s.coverage, Coverage::Partial { reason } if reason.contains("military")));
    let FeedItems::Aircraft(list) = &s.items else { panic!() };
    assert!(list.iter().all(|a| a.is_military));
    assert_eq!(*fake.regions.lock().unwrap(), vec![Region::Global]);
}

#[tokio::test]
async fn catalog_status_search_and_shutdown() {
    let core =
        core_with(vec![Arc::new(FakeQuakes::default()), Arc::new(FakeAircraft::default())], 0);
    let catalog = core.catalog();
    assert_eq!(catalog.layers.len(), 2, "only feeds with providers are listed");
    assert!(catalog.imagery.iter().any(|i| i.is_default));
    assert!(catalog.sources.iter().all(|s| !s.attribution.is_empty()));

    let mut sub = core.subscribe(FeedQuery::new(FeedId::Earthquakes)).unwrap();
    next_snapshot(&mut sub).await;
    let results = core
        .search(&lavender_domain::SearchQuery {
            q: "helsinki".into(),
            near: None,
            limit: None,
            places: None,
        })
        .await
        .unwrap();
    assert_eq!(results.entities.len(), 1);
    let coords = core
        .search(&lavender_domain::SearchQuery {
            q: "60.17, 24.94".into(),
            near: None,
            limit: None,
            places: None,
        })
        .await
        .unwrap();
    assert_eq!(coords.coordinates.len(), 1);
    assert!(
        core.search(&lavender_domain::SearchQuery {
            q: "  ".into(),
            near: None,
            limit: None,
            places: None
        })
        .await
        .is_err()
    );

    core.shutdown();
    assert!(
        tokio::time::timeout(Duration::from_secs(2), sub.next()).await.unwrap().is_none(),
        "subscriptions end on shutdown"
    );
    assert!(core.subscribe(FeedQuery::new(FeedId::Earthquakes)).is_err());
}

#[tokio::test]
async fn flipping_between_viewports_keeps_one_linger_timer_per_slot() {
    // Review finding 1: every refs→0 transition spawned a 60 s linger task, so a client
    // flipping between two viewports leaked one task per flip.
    let fake = Arc::new(FakeAircraft::default());
    let core = core_with(vec![fake], 60);
    let mut sub = core.subscribe(aircraft_in(cell_a())).unwrap();
    next_snapshot(&mut sub).await;
    let metrics = tokio::runtime::Handle::current().metrics();
    let baseline = metrics.num_alive_tasks();
    for i in 0..2_000 {
        let view = if i % 2 == 0 { cell_b() } else { cell_a() };
        sub.update(aircraft_in(view)).unwrap();
        next_snapshot(&mut sub).await;
    }
    let alive = metrics.num_alive_tasks();
    // At most: one more refresh task and one linger timer per slot.
    assert!(alive <= baseline + 3, "linger timers leaked: {baseline} tasks before, {alive} after");
}

#[tokio::test]
async fn dropping_the_core_without_shutdown_stops_refreshing() {
    // Review finding 8: without `Drop for HubInner`, lingering refresh tasks kept fetching.
    let fake = Arc::new(FakeQuakes::default());
    let core = core_with(vec![fake.clone()], 60);
    core.snapshot(FeedQuery::new(FeedId::Earthquakes)).await.unwrap();
    drop(core);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let at_drop = fake.fetches.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(fake.fetches.load(Ordering::SeqCst), at_drop, "no fetches after the core is gone");
}

#[tokio::test]
async fn regions_that_do_not_fit_are_partial_and_load_when_capacity_frees() {
    // Review finding 3: a failed acquisition was dropped silently and coverage said
    // Complete; the missing region was never retried.
    let core = core_with_hub(
        vec![Arc::new(FakeAircraft::default())],
        HubConfig { linger_s: 0, max_slots: 2, ..HubConfig::default() },
    );
    let mut hog = core.subscribe(aircraft_in(cell_a())).unwrap();
    next_snapshot(&mut hog).await;
    let mut sub = core.subscribe(aircraft_in(two_cells())).unwrap();
    let partial = snapshot_where(&mut sub, |s| s.pending_regions == 0).await;
    assert!(
        matches!(&partial.coverage, Coverage::Partial { reason } if reason.contains("capacity"))
    );
    assert!(partial.notices.iter().any(|n| n.contains("capacity")), "{:?}", partial.notices);
    assert_eq!(partial.items.len(), 1, "the cell that fitted is shown");
    drop(hog);
    let full =
        snapshot_where(&mut sub, |s| s.coverage == Coverage::Complete && s.pending_regions == 0)
            .await;
    assert_eq!(full.items.len(), 2, "both cells are shown");
    assert!(full.notices.is_empty(), "{:?}", full.notices);
}

#[tokio::test]
async fn capacity_errors_keep_their_rate_limited_code() {
    // Review finding 12: a hub limit (429) reached HTTP clients as an upstream error (502).
    let core = core_with_hub(
        vec![Arc::new(FakeAircraft::default())],
        HubConfig { linger_s: 0, max_slots: 1, ..HubConfig::default() },
    );
    let mut hog = core.subscribe(aircraft_in(cell_a())).unwrap();
    next_snapshot(&mut hog).await;
    let err = core.snapshot(aircraft_in(cell_b())).await.unwrap_err();
    let api = ApiError::from(&err);
    assert_eq!(api.code, ErrorCode::RateLimited, "{api:?}");
    assert!(api.retryable);
}

#[tokio::test]
async fn lingering_slots_are_evicted_when_the_table_is_full() {
    // Review finding 5: idle (lingering) slots counted against max_slots, so one-shot
    // snapshots of distinct areas could lock everyone out for the linger time.
    let fake = Arc::new(FakeAircraft::default());
    let core = core_with_hub(
        vec![fake.clone()],
        HubConfig { linger_s: 60, max_slots: 1, ..HubConfig::default() },
    );
    core.snapshot(aircraft_in(cell_a())).await.unwrap();
    let s = core.snapshot(aircraft_in(cell_b())).await.unwrap();
    assert_eq!(s.coverage, Coverage::Complete);
    assert_eq!(fake.regions.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn region_budgets_cap_one_client_without_affecting_others() {
    // Review finding 5a: one client could pin enough slots to starve everyone else.
    let core = core_with(vec![Arc::new(FakeAircraft::default())], 0);
    let budget = Arc::new(RegionBudget::new(2));
    let mut first = core.subscribe_with_budget(aircraft_in(two_cells()), budget.clone()).unwrap();
    snapshot_where(&mut first, |s| s.pending_regions == 0 && s.coverage == Coverage::Complete)
        .await;
    let mut same = core.subscribe_with_budget(aircraft_in(two_cells()), budget.clone()).unwrap();
    let s = snapshot_where(&mut same, |s| s.pending_regions == 0).await;
    assert_eq!(s.coverage, Coverage::Complete, "regions the client already holds count once");
    assert_eq!(budget.in_use(), 2);

    let err = core.snapshot_with_budget(aircraft_in(cell_a()), budget.clone()).await.unwrap_err();
    assert_eq!(ApiError::from(&err).code, ErrorCode::RateLimited);
    let other_client = core.snapshot(aircraft_in(cell_a())).await.unwrap();
    assert_eq!(other_client.coverage, Coverage::Complete, "other clients are unaffected");

    drop((first, same));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(budget.in_use(), 0, "regions return to the budget");
}

/// Cells with an even longitude index fail at once; odd ones answer after 300 ms.
#[derive(Debug, Default)]
struct MixedCells;

#[async_trait]
impl FeedProvider for MixedCells {
    fn feed(&self) -> FeedId {
        FeedId::Aircraft
    }
    fn source_id(&self) -> &'static str {
        sources::ADSB_LOL
    }
    fn cadence(&self) -> Duration {
        Duration::from_secs(30)
    }
    fn scope(&self) -> ProviderScope {
        ProviderScope::Regional { cell_deg: 5.0, max_cells: 4 }
    }
    fn enabled(&self) -> bool {
        true
    }
    async fn fetch(
        &self,
        _ctx: &FetchContext<'_>,
        region: Region,
    ) -> Result<Fetched, ProviderError> {
        let Region::Cell(cell) = region else { panic!("regional only") };
        if cell.lon_idx % 2 == 0 {
            return Err(ProviderError::Unsupported("this cell is down".to_owned()));
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        let c = cell.center();
        Ok(FeedItems::Aircraft(vec![aircraft("4ca1b2", c.lat, c.lon, false)]).into())
    }
}

#[tokio::test]
async fn one_failing_region_does_not_end_a_snapshot_early() {
    // Review finding 4: the all-error branch reported pending_regions = 0, so a snapshot
    // returned Unavailable while another region was about to answer.
    let core = core_with(vec![Arc::new(MixedCells)], 0);
    let s = core.snapshot(aircraft_in(two_cells())).await.unwrap();
    assert_eq!(s.items.len(), 1, "the slower region's data is included");
    assert_eq!(s.pending_regions, 0);
    assert!(
        matches!(&s.freshness, Freshness::Degraded { reason, .. } if reason.contains("down")),
        "{:?}",
        s.freshness
    );
}

/// Fails with an enormous message that quotes "upstream content".
#[derive(Debug)]
struct Verbose;

#[async_trait]
impl FeedProvider for Verbose {
    fn feed(&self) -> FeedId {
        FeedId::Earthquakes
    }
    fn source_id(&self) -> &'static str {
        sources::USGS
    }
    fn cadence(&self) -> Duration {
        Duration::from_secs(60)
    }
    fn scope(&self) -> ProviderScope {
        ProviderScope::Global
    }
    fn enabled(&self) -> bool {
        true
    }
    async fn fetch(
        &self,
        _ctx: &FetchContext<'_>,
        _region: Region,
    ) -> Result<Fetched, ProviderError> {
        let echo = format!("invalid type: string \"{}\"\n\u{1b}[31m", "A".repeat(1 << 20));
        Err(ProviderError::Parse { source_name: "USGS", reason: echo })
    }
}

#[tokio::test]
async fn provider_errors_are_capped_before_frames_health_and_logs() {
    // Review finding 10: a 2 MB upstream string became a 2 MB error in every frame.
    let core = core_with(vec![Arc::new(Verbose)], 0);
    let s = core.snapshot(FeedQuery::new(FeedId::Earthquakes)).await.unwrap();
    let Freshness::Unavailable { reason } = &s.freshness else { panic!("{:?}", s.freshness) };
    assert!(reason.chars().count() <= 200, "{} chars", reason.len());
    assert!(!reason.contains('\u{1b}') && !reason.contains('\n'));
    let health = core.status().sources.into_iter().find(|h| h.source_id == sources::USGS).unwrap();
    assert!(health.last_error.unwrap().chars().count() <= 200);
}

/// Panics on its first fetch, then works.
#[derive(Debug, Default)]
struct PanicsOnce {
    calls: AtomicUsize,
}

#[async_trait]
impl FeedProvider for PanicsOnce {
    fn feed(&self) -> FeedId {
        FeedId::Earthquakes
    }
    fn source_id(&self) -> &'static str {
        sources::USGS
    }
    fn cadence(&self) -> Duration {
        Duration::from_millis(200)
    }
    fn scope(&self) -> ProviderScope {
        ProviderScope::Global
    }
    fn enabled(&self) -> bool {
        true
    }
    async fn fetch(
        &self,
        _ctx: &FetchContext<'_>,
        _region: Region,
    ) -> Result<Fetched, ProviderError> {
        assert!(
            self.calls.fetch_add(1, Ordering::SeqCst) > 0,
            "provider bug (expected by this test)"
        );
        Ok(FeedItems::Earthquakes(vec![quake("fiji", -17.0, 179.5, 5.1)]).into())
    }
}

#[tokio::test]
async fn a_panicking_provider_is_reported_and_restarted() {
    // Review LOW: a provider panic killed its refresh task and left subscribers attached
    // to a dead slot forever.
    let core = core_with(vec![Arc::new(PanicsOnce::default())], 60);
    let mut sub = core.subscribe(FeedQuery::new(FeedId::Earthquakes)).unwrap();
    let failed = snapshot_where(&mut sub, |_| true).await;
    assert!(
        matches!(&failed.freshness, Freshness::Unavailable { reason } if reason.contains("unexpectedly")),
        "{:?}",
        failed.freshness
    );
    let recovered = snapshot_where(&mut sub, |s| s.freshness == Freshness::Live).await;
    assert_eq!(recovered.items.len(), 1);
}

/// Answers once, then never again (a wedged upstream connection).
#[derive(Debug, Default)]
struct Wedged {
    calls: AtomicUsize,
}

#[async_trait]
impl FeedProvider for Wedged {
    fn feed(&self) -> FeedId {
        FeedId::Earthquakes
    }
    fn source_id(&self) -> &'static str {
        sources::USGS
    }
    fn cadence(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn scope(&self) -> ProviderScope {
        ProviderScope::Global
    }
    fn enabled(&self) -> bool {
        true
    }
    async fn fetch(
        &self,
        _ctx: &FetchContext<'_>,
        _region: Region,
    ) -> Result<Fetched, ProviderError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) > 0 {
            std::future::pending::<()>().await;
        }
        Ok(FeedItems::Earthquakes(vec![quake("fiji", -17.0, 179.5, 5.1)]).into())
    }
}

#[tokio::test]
async fn a_wedged_refresh_turns_stale_without_new_data() {
    // Review LOW: freshness was judged only when a slot published, so a refresher that
    // hung never turned Stale.
    let core = core_with(vec![Arc::new(Wedged::default())], 0);
    let mut sub = core.subscribe(FeedQuery::new(FeedId::Earthquakes)).unwrap();
    assert_eq!(next_snapshot(&mut sub).await.freshness, Freshness::Live);
    let stale = snapshot_where(&mut sub, |_| true).await;
    assert!(
        matches!(stale.freshness, Freshness::Stale { age_s } if age_s > 2),
        "{:?}",
        stale.freshness
    );
}
