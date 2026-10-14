//! Aircraft history through the same facade used by HTTP and Tauri. Fake regional
//! providers prove ingestion belongs to shared refreshes, independently of snapshots.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use async_trait::async_trait;
use jiff::{SignedDuration, Timestamp};
use lavender_core::config::{CoreConfig, HubConfig};
use lavender_core::providers::{FeedProvider, FetchContext, Fetched, ProviderError, Region};
use lavender_core::{CoreError, FeedSubscription, LavenderCore, SubscriptionEvent, sources};
use lavender_domain::{
    Aircraft, BBox, FeedId, FeedItems, FeedQuery, FeedSnapshot, Freshness, GeoPoint, ProviderScope,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Debug)]
struct ObservedAircraft {
    base: Timestamp,
    fetches: AtomicUsize,
    /// 0: current observations, 1: upstream unavailable, 2: cached degraded answer.
    mode: AtomicU8,
}

impl ObservedAircraft {
    fn new() -> Self {
        Self {
            base: Timestamp::now().checked_sub(SignedDuration::from_secs(120)).unwrap(),
            fetches: AtomicUsize::new(0),
            mode: AtomicU8::new(0),
        }
    }

    fn row(step: u32) -> Aircraft {
        Aircraft {
            hex: "abcdef".to_owned(),
            callsign: Some("FIN7KV".to_owned()),
            registration: None,
            type_code: None,
            category: None,
            position: GeoPoint::new(61.0, 21.0 + f64::from(step) / 100.0)
                .unwrap()
                .with_altitude(Some(9_144.0))
                .unwrap(),
            altitude_baro_ft: Some(30_000.0),
            altitude_geom_ft: None,
            on_ground: false,
            ground_speed_kt: None,
            track_deg: None,
            vertical_rate_fpm: None,
            squawk: None,
            emergency: None,
            is_military: false,
            seen_pos_s: 2.5,
            source: sources::ADSB_LOL.to_owned(),
        }
    }
}

#[async_trait]
impl FeedProvider for ObservedAircraft {
    fn feed(&self) -> FeedId {
        FeedId::Aircraft
    }
    fn source_id(&self) -> &'static str {
        sources::ADSB_LOL
    }
    fn cadence(&self) -> Duration {
        Duration::from_millis(250)
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
        _region: Region,
    ) -> Result<Fetched, ProviderError> {
        let fetch = self.fetches.fetch_add(1, Ordering::SeqCst);
        let mode = self.mode.load(Ordering::SeqCst);
        if mode == 1 {
            return Err(ProviderError::Unavailable("Aircraft source is unavailable.".into()));
        }
        let step = if mode == 2 { 0 } else { u32::try_from(fetch).unwrap() };
        let row = Self::row(step);
        Ok(Fetched {
            items: FeedItems::Aircraft(vec![row.clone(), row]),
            as_of: Some(self.base.checked_add(SignedDuration::from_secs(i64::from(step))).unwrap()),
            degraded: (mode == 2)
                .then(|| "Serving cached aircraft after an upstream failure.".into()),
            notices: Vec::new(),
        })
    }
}

fn core(provider: Arc<ObservedAircraft>) -> LavenderCore {
    let config = CoreConfig {
        hub: HubConfig { linger_s: 0, ..HubConfig::default() },
        ..CoreConfig::default()
    };
    LavenderCore::builder(config).providers(vec![provider]).build().unwrap()
}

fn query() -> FeedQuery {
    FeedQuery::new(FeedId::Aircraft).with_bbox(BBox::new(20.5, 60.5, 24.5, 64.5).unwrap())
}

async fn snapshot_where(
    sub: &mut FeedSubscription,
    predicate: impl Fn(&FeedSnapshot) -> bool,
) -> Arc<FeedSnapshot> {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match sub.next().await.unwrap() {
                SubscriptionEvent::Snapshot(snapshot) if predicate(&snapshot) => return snapshot,
                SubscriptionEvent::Snapshot(_) => {}
                SubscriptionEvent::Error(error) => panic!("{error:?}"),
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn shared_region_refreshes_collect_history_without_snapshot_calls() {
    let provider = Arc::new(ObservedAircraft::new());
    let core = core(provider.clone());
    let mut first = core.subscribe(query()).unwrap();
    let mut second = core.subscribe(query()).unwrap();
    snapshot_where(&mut first, |s| s.pending_regions == 0).await;
    snapshot_where(&mut second, |s| s.pending_regions == 0).await;
    assert_eq!(provider.fetches.load(Ordering::SeqCst), 1, "viewers share their region fetch");
    let initial = core.aircraft_track(" ABCDEF ").unwrap();
    assert_eq!(initial.points.len(), 1, "duplicates in a refresh are not appended");
    assert_eq!(initial.sources, vec![sources::ADSB_LOL]);
    assert_eq!(
        initial.points[0].time,
        provider.base.checked_sub(SignedDuration::from_millis(2_500)).unwrap()
    );
    snapshot_where(
        &mut first,
        |s| matches!(&s.items, FeedItems::Aircraft(rows) if rows[0].position.lon > 21.0),
    )
    .await;
    let retained = core.aircraft_track("abcdef").unwrap();
    assert_eq!(retained.points.len(), 2);
    assert!(retained.points[0].time < retained.points[1].time);

    drop((first, second));
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if core.status().feeds.iter().all(|f| f.active_regions == 0 && f.subscribers == 0) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let fetches = provider.fetches.load(Ordering::SeqCst);
    assert_eq!(core.aircraft_track("abcdef").unwrap(), retained, "history survives region removal");
    assert_eq!(provider.fetches.load(Ordering::SeqCst), fetches, "history reads never fetch");
    core.shutdown();
}

#[tokio::test]
async fn snapshot_uses_the_same_history_ingestion_and_unknown_tracks_do_not_fetch() {
    let provider = Arc::new(ObservedAircraft::new());
    let core = core(provider.clone());
    assert!(matches!(core.aircraft_track("123456"), Err(CoreError::NotFound(_))));
    assert!(matches!(core.aircraft_track("ab~def"), Err(CoreError::BadRequest(_))));
    assert_eq!(provider.fetches.load(Ordering::SeqCst), 0);
    core.snapshot(query()).await.unwrap();
    assert_eq!(core.aircraft_track("abcdef").unwrap().points.len(), 1);
    core.shutdown();
    assert!(matches!(core.aircraft_track("abcdef"), Err(CoreError::ShuttingDown)));
}

#[tokio::test]
async fn failed_and_cached_degraded_refreshes_never_fabricate_points() {
    let provider = Arc::new(ObservedAircraft::new());
    let core = core(provider.clone());
    let mut sub = core.subscribe(query()).unwrap();
    snapshot_where(&mut sub, |s| s.pending_regions == 0).await;
    let initial = core.aircraft_track("abcdef").unwrap();
    provider.mode.store(1, Ordering::SeqCst);
    snapshot_where(&mut sub, |s| matches!(s.freshness, Freshness::Degraded { .. })).await;
    assert_eq!(core.aircraft_track("abcdef").unwrap(), initial);
    provider.mode.store(2, Ordering::SeqCst);
    snapshot_where(
        &mut sub,
        |s| matches!(&s.freshness, Freshness::Degraded { reason, .. } if reason.contains("cached")),
    )
    .await;
    assert_eq!(
        core.aircraft_track("abcdef").unwrap(),
        initial,
        "same as_of cache is not a new fix"
    );
    core.shutdown();
}
