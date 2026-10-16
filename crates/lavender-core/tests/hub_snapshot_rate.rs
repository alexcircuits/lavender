//! A provider's `snapshot_interval` gives each subscriber one snapshot per interval,
//! however many regional cells it covers (the vessels feed is push-based and re-reads
//! its cells constantly).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use lavender_core::config::{CoreConfig, HubConfig};
use lavender_core::providers::{FeedProvider, FetchContext, Fetched, ProviderError, Region};
use lavender_core::{LavenderCore, MemorySecretStore, SecretKey, SubscriptionEvent, sources};
use lavender_domain::{BBox, FeedId, FeedItems, FeedQuery, GeoPoint, ProviderScope, Vessel};

/// One vessel per cell whose speed changes on every fetch, so every refresh is news.
#[derive(Debug)]
struct ChangingVessels {
    interval: Option<Duration>,
    fetches: AtomicU64,
}

#[async_trait]
impl FeedProvider for ChangingVessels {
    fn feed(&self) -> FeedId {
        FeedId::Vessels
    }
    fn source_id(&self) -> &'static str {
        sources::AISSTREAM
    }
    fn cadence(&self) -> Duration {
        Duration::from_millis(40)
    }
    fn scope(&self) -> ProviderScope {
        ProviderScope::Regional { cell_deg: 5.0, max_cells: 4 }
    }
    fn enabled(&self) -> bool {
        true
    }
    fn snapshot_interval(&self) -> Option<Duration> {
        self.interval
    }
    async fn fetch(&self, _: &FetchContext<'_>, region: Region) -> Result<Fetched, ProviderError> {
        let Region::Cell(cell) = region else { panic!("regional") };
        let n = self.fetches.fetch_add(1, Ordering::SeqCst);
        let c = cell.center();
        let speed = f64::from(u32::try_from(n % 50).unwrap());
        let mmsi = format!("2309{:05}", (cell.lat_idx * 100 + cell.lon_idx).rem_euclid(100_000));
        let vessel = Vessel::new(
            mmsi,
            None,
            None,
            None,
            None,
            GeoPoint::new(c.lat, c.lon).unwrap(),
            Some(speed),
            None,
            None,
            None,
            None,
        );
        Ok(Fetched::fresh(FeedItems::Vessels(vec![vessel])))
    }
}

/// What one subscription saw in `window` after its first complete snapshot.
struct Seen {
    snapshots: usize,
    /// Provider fetches (each one a slot update) during the window.
    updates: u64,
}

async fn seen_in(interval: Option<Duration>, window: Duration) -> Seen {
    let provider = Arc::new(ChangingVessels { interval, fetches: AtomicU64::new(0) });
    let counter = provider.clone();
    let core = LavenderCore::builder(CoreConfig {
        hub: HubConfig { linger_s: 0, ..HubConfig::default() },
        ..CoreConfig::default()
    })
    .providers(vec![provider as Arc<dyn FeedProvider>])
    .secrets(Arc::new(MemorySecretStore::with([(SecretKey::AisStreamApiKey, "k")])))
    .build()
    .unwrap();
    // Four 5° cells.
    let query =
        FeedQuery::new(FeedId::Vessels).with_bbox(BBox::new(21.0, 56.0, 29.0, 64.0).unwrap());
    let mut sub = core.subscribe(query).unwrap();
    loop {
        match sub.next().await.unwrap() {
            SubscriptionEvent::Snapshot(s) if s.pending_regions == 0 => {
                assert_eq!(s.items.len(), 4);
                break;
            }
            SubscriptionEvent::Snapshot(_) => {}
            SubscriptionEvent::Error(e) => panic!("{e:?}"),
        }
    }
    let mut snapshots = 0;
    let fetches_before = counter.fetches.load(Ordering::SeqCst);
    let deadline = tokio::time::Instant::now() + window;
    while let Ok(Some(event)) = tokio::time::timeout_at(deadline, sub.next()).await {
        assert!(matches!(event, SubscriptionEvent::Snapshot(_)));
        snapshots += 1;
    }
    let updates = counter.fetches.load(Ordering::SeqCst) - fetches_before;
    core.shutdown();
    Seen { snapshots, updates }
}

#[tokio::test]
async fn throttled_subscriptions_get_one_snapshot_per_interval() {
    let window = Duration::from_millis(1_250);
    let throttled = seen_in(Some(Duration::from_millis(250)), window).await;
    // Snapshots are at least 250 ms apart, so 1.25 s holds at most 6 (5 intervals plus
    // the edge). A loaded machine may deliver fewer, never more; updates keep coming.
    let Seen { snapshots, updates } = throttled;
    assert!((2..=6).contains(&snapshots), "{snapshots} snapshots");
    assert!(updates >= 3 * snapshots as u64, "{updates} slot updates, {snapshots} snapshots");
    // Without the throttle the same updates are sent as they happen.
    let unthrottled = seen_in(None, window).await;
    assert!(unthrottled.snapshots > 6, "{} unthrottled snapshots", unthrottled.snapshots);
}
