//! Regression tests for the parent module.

use super::refresh::retry_delay;
use super::snapshot::same_freshness_state;
use super::*;
use lavender_domain::Freshness;

#[test]
fn backoff_grows_is_capped_and_respects_the_floor() {
    let hour = Duration::from_secs(3_600);
    assert_eq!(retry_delay(1, Duration::ZERO, hour), Duration::from_secs(5));
    assert_eq!(retry_delay(3, Duration::ZERO, hour), Duration::from_secs(20));
    assert_eq!(retry_delay(50, Duration::ZERO, hour), MAX_BACKOFF);
    assert_eq!(retry_delay(2, Duration::ZERO, Duration::from_secs(7)), Duration::from_secs(7));
    let floor = Duration::from_secs(600);
    assert_eq!(retry_delay(1, floor, Duration::from_secs(7_200)), floor, "floor beats backoff");
}

#[test]
fn budgets_count_distinct_regions() {
    let budget = RegionBudget::new(2);
    let key = |lon_idx| SlotKey {
        feed: FeedId::Aircraft,
        region: Region::Cell(lavender_geo::Cell { size_mdeg: 5000, lat_idx: 1, lon_idx }),
    };
    assert!(budget.take(key(1)));
    assert!(budget.take(key(1)), "a region already held counts once");
    assert!(budget.take(key(2)));
    assert!(!budget.take(key(3)));
    assert_eq!(budget.in_use(), 2);
    assert!(!budget.give_back(key(1)), "still held once");
    assert!(budget.give_back(key(1)));
    assert!(budget.take(key(3)));
}

#[test]
fn freshness_states_compare_without_ages() {
    assert!(same_freshness_state(&Freshness::Stale { age_s: 1 }, &Freshness::Stale { age_s: 9 }));
    assert!(!same_freshness_state(&Freshness::Live, &Freshness::Stale { age_s: 9 }));
    let d = |r: &str| Freshness::Degraded { reason: r.to_owned(), age_s: 0 };
    assert!(same_freshness_state(&d("a"), &d("a")) && !same_freshness_state(&d("a"), &d("b")));
}
