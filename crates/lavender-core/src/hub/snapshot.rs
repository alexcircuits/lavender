//! Merges provider results, removes duplicate records, applies filters, and reports freshness.

use super::subscription::FeedMeta;
use super::{Lease, MAX_PROVIDER_NOTICES, Plan, Shortfall, SlotKey, SlotState};
use jiff::{SignedDuration, Timestamp};
use lavender_domain::{Aircraft, Coverage, FeedId, FeedItems, FeedQuery, FeedSnapshot, Freshness};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

fn whole_seconds(d: SignedDuration) -> u32 {
    u32::try_from(d.as_secs().max(0)).unwrap_or(u32::MAX)
}

/// Freshness of the slots that hold items, and the time of their oldest data; `None`
/// while no slot holds items.
fn judge_freshness(
    states: &[Arc<SlotState>],
    now: Timestamp,
    cadence: Duration,
) -> Option<(Freshness, Option<Timestamp>)> {
    if states.iter().all(|s| s.items.is_none()) {
        return None;
    }
    let fetched_at = states.iter().filter(|s| s.items.is_some()).filter_map(|s| s.as_of).min();
    let age_s = fetched_at.map_or(0, |t| whole_seconds(now.duration_since(t)));
    let cadence_s = u32::try_from(cadence.as_secs()).unwrap_or(u32::MAX);
    let freshness = if let Some(reason) = states.iter().find_map(|s| s.last_error.as_deref()) {
        Freshness::Degraded { reason: reason.to_owned(), age_s }
    } else if age_s > cadence_s.saturating_mul(2) {
        Freshness::Stale { age_s }
    } else {
        Freshness::Live
    };
    Some((freshness, fetched_at))
}

/// Whether two freshness values are the same state (ages may differ).
pub(super) fn same_freshness_state(a: &Freshness, b: &Freshness) -> bool {
    match (a, b) {
        (Freshness::Live, Freshness::Live) | (Freshness::Stale { .. }, Freshness::Stale { .. }) => {
            true
        }
        (Freshness::Degraded { reason: x, .. }, Freshness::Degraded { reason: y, .. })
        | (Freshness::Unavailable { reason: x }, Freshness::Unavailable { reason: y }) => x == y,
        _ => false,
    }
}

/// Whether the freshness judged now differs from what was last sent (without consuming
/// slot change notifications).
pub(super) fn freshness_changed(
    leases: &HashMap<SlotKey, Lease>,
    meta: &FeedMeta,
    sent: Option<&Freshness>,
) -> bool {
    let states: Vec<Arc<SlotState>> = leases.values().map(|l| l.rx.borrow().clone()).collect();
    match (judge_freshness(&states, Timestamp::now(), meta.cadence), sent) {
        (Some((now, _)), Some(before)) => !same_freshness_state(&now, before),
        _ => false,
    }
}

/// Merges the subscription's slots, filters them to its query and judges freshness.
/// Returns `None` until at least one slot has produced a result or an error.
pub(super) fn compose_snapshot(
    query: &FeedQuery,
    plan: &Plan,
    leases: &mut HashMap<SlotKey, Lease>,
    meta: &FeedMeta,
    shortfall: Option<Shortfall>,
) -> Option<FeedSnapshot> {
    let states: Vec<Arc<SlotState>> =
        leases.values_mut().map(|l| l.rx.borrow_and_update().clone()).collect();
    let now = Timestamp::now();
    let sources = vec![meta.source_id.to_owned()];
    // Regions that have neither data nor an error have not reported yet.
    let pending = states.iter().filter(|s| s.items.is_none() && s.last_error.is_none()).count();
    let pending_regions = u32::try_from(pending).unwrap_or(u32::MAX);

    let mut notices: Vec<String> = Vec::new();
    for notice in states.iter().flat_map(|s| s.notices.iter()) {
        if notices.len() < MAX_PROVIDER_NOTICES && !notices.contains(notice) {
            notices.push(notice.clone());
        }
    }
    let mut coverage = plan.coverage.clone();
    if query.feed == FeedId::Cyclones {
        coverage =
            Coverage::Partial { reason: crate::providers::cyclones::COVERAGE_NOTICE.to_owned() };
    }
    if query.feed == FeedId::Perimeters {
        coverage =
            Coverage::Partial { reason: crate::providers::perimeters::COVERAGE_NOTICE.to_owned() };
    }
    if let Some(short) = shortfall {
        notices.push(short.notice().to_owned());
        if coverage == Coverage::Complete {
            coverage = Coverage::Partial { reason: short.notice().to_owned() };
        }
    }

    let Some((freshness, fetched_at)) = judge_freshness(&states, now, meta.cadence) else {
        let reason = states.iter().find_map(|s| s.last_error.as_deref())?;
        return Some(FeedSnapshot {
            feed: query.feed,
            sources,
            fetched_at: None,
            generated_at: now,
            freshness: Freshness::Unavailable { reason: reason.to_owned() },
            coverage,
            source_count: 0,
            pending_regions,
            items: FeedItems::empty(query.feed),
            notices,
        });
    };

    let lists: Vec<&FeedItems> = states.iter().filter_map(|s| s.items.as_deref()).collect();
    let (items, source_count, truncated) = filter_items(query, &lists, meta.max_items);
    if truncated && query.feed == FeedId::Perimeters {
        // Perimeters keep the largest fires when a view's size limits cut it short.
        notices.push(crate::providers::perimeters::TRUNCATED_NOTICE.to_owned());
    } else if truncated {
        notices.push(format!(
            "Showing the first {} items. Zoom in or add filters to see the rest.",
            meta.max_items
        ));
    }
    Some(FeedSnapshot {
        feed: query.feed,
        sources,
        fetched_at,
        generated_at: now,
        freshness,
        coverage,
        source_count,
        pending_regions,
        items,
        notices,
    })
}

/// Concatenates lists, drops duplicate ids (overlapping regional queries), applies the
/// query's spatial and attribute filters and the item cap. Returns (items, count before
/// filtering, whether the cap truncated).
#[allow(clippy::too_many_lines)]
fn filter_items(
    query: &FeedQuery,
    lists: &[&FeedItems],
    max_items: usize,
) -> (FeedItems, u32, bool) {
    fn dedup_by<T: Clone>(lists: &[&Vec<T>], key: impl Fn(&T) -> String) -> Vec<T> {
        let mut seen = HashSet::new();
        lists.iter().flat_map(|l| l.iter()).filter(|x| seen.insert(key(x))).cloned().collect()
    }
    fn cap<T>(mut v: Vec<T>, max: usize) -> (Vec<T>, bool) {
        let truncated = v.len() > max;
        v.truncate(max);
        (v, truncated)
    }
    let in_view = |p: &lavender_domain::GeoPoint| query.bbox.is_none_or(|b| b.contains(p));
    let f = &query.filters;
    match query.feed {
        FeedId::Aircraft => {
            let all: Vec<&Vec<_>> = lists
                .iter()
                .filter_map(|l| if let FeedItems::Aircraft(v) = l { Some(v) } else { None })
                .collect();
            let mut by_hex = HashMap::new();
            let mut order = Vec::new();
            for a in all.iter().flat_map(|l| l.iter()) {
                match by_hex.entry(a.hex.clone()) {
                    std::collections::hash_map::Entry::Vacant(e) => {
                        order.push(a.hex.clone());
                        e.insert(a.clone());
                    }
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        let existing = e.get_mut();
                        if a.is_military {
                            existing.is_military = true;
                        }
                        if a.seen_pos_s < existing.seen_pos_s {
                            let is_mil = existing.is_military || a.is_military;
                            *existing = a.clone();
                            existing.is_military = is_mil;
                        }
                    }
                }
            }
            let merged: Vec<Aircraft> =
                order.into_iter().filter_map(|hex| by_hex.remove(&hex)).collect();
            let count = merged.len();
            let filtered = merged
                .into_iter()
                .filter(|a| {
                    in_view(&a.position) && (f.military_only != Some(true) || a.is_military)
                })
                .collect();
            let (v, t) = cap(filtered, max_items);
            (FeedItems::Aircraft(v), u32::try_from(count).unwrap_or(u32::MAX), t)
        }
        FeedId::Earthquakes => {
            let all: Vec<&Vec<_>> = lists
                .iter()
                .filter_map(|l| if let FeedItems::Earthquakes(v) = l { Some(v) } else { None })
                .collect();
            let merged = dedup_by(&all, |q| q.id.clone());
            let count = merged.len();
            let min_mag = f.min_magnitude.unwrap_or(f64::NEG_INFINITY);
            let filtered = merged
                .into_iter()
                .filter(|q| in_view(&q.position) && q.magnitude >= min_mag)
                .collect();
            let (v, t) = cap(filtered, max_items);
            (FeedItems::Earthquakes(v), u32::try_from(count).unwrap_or(u32::MAX), t)
        }
        FeedId::Cyclones => {
            let all: Vec<&Vec<_>> = lists
                .iter()
                .filter_map(|l| if let FeedItems::Cyclones(v) = l { Some(v) } else { None })
                .collect();
            let merged = dedup_by(&all, |c| c.id.clone());
            let count = merged.len();
            let filtered = merged.into_iter().filter(|c| in_view(&c.position)).collect();
            let (v, t) = cap(filtered, max_items);
            (FeedItems::Cyclones(v), u32::try_from(count).unwrap_or(u32::MAX), t)
        }
        FeedId::Satellites => {
            let all: Vec<&Vec<_>> = lists
                .iter()
                .filter_map(|l| if let FeedItems::Satellites(v) = l { Some(v) } else { None })
                .collect();
            let merged = dedup_by(&all, |s| s.norad_id.to_string());
            let count = merged.len();
            let filtered = merged
                .into_iter()
                .filter(|s| f.groups.as_ref().is_none_or(|g| g.contains(&s.group)))
                .collect();
            let (v, t) = cap(filtered, max_items);
            (FeedItems::Satellites(v), u32::try_from(count).unwrap_or(u32::MAX), t)
        }
        FeedId::Fires => {
            let all = lists
                .iter()
                .filter_map(|l| if let FeedItems::Fires(v) = l { Some(v) } else { None })
                .flatten();
            // Exact copies (overlapping regions, repeated rows) count once; distinct
            // detections and pre-weighted cells at one place are merged by the grid.
            let merged = crate::providers::firms::distinct_cells(all);
            let count = merged.len();
            let (cells, truncated) = crate::providers::firms::aggregate_fires(
                merged,
                query.bbox.as_ref(),
                f.min_frp,
                max_items,
            );
            (FeedItems::Fires(cells), u32::try_from(count).unwrap_or(u32::MAX), truncated)
        }
        FeedId::Launches => {
            let all: Vec<&Vec<_>> = lists
                .iter()
                .filter_map(|l| if let FeedItems::Launches(v) = l { Some(v) } else { None })
                .collect();
            let merged = dedup_by(&all, |l| l.id.clone());
            let count = merged.len();
            let filtered = merged
                .into_iter()
                .filter(|l| {
                    l.pad.as_ref().is_none_or(|p| {
                        lavender_domain::GeoPoint::new(p.latitude, p.longitude)
                            .is_ok_and(|pos| in_view(&pos))
                    })
                })
                .collect();
            let (v, t) = cap(filtered, max_items);
            (FeedItems::Launches(v), u32::try_from(count).unwrap_or(u32::MAX), t)
        }
        FeedId::Vessels => {
            let all: Vec<&Vec<_>> = lists
                .iter()
                .filter_map(|l| if let FeedItems::Vessels(v) = l { Some(v) } else { None })
                .collect();
            let merged = dedup_by(&all, |v| v.mmsi.clone());
            let count = merged.len();
            let filtered = merged.into_iter().filter(|v| in_view(&v.position)).collect();
            let (v, t) = cap(filtered, max_items);
            (FeedItems::Vessels(v), u32::try_from(count).unwrap_or(u32::MAX), t)
        }
        FeedId::Perimeters => {
            let all: Vec<&Vec<_>> = lists
                .iter()
                .filter_map(|l| if let FeedItems::Perimeters(v) = l { Some(v) } else { None })
                .collect();
            // Providers publish every detail level; the view picks one of them.
            let merged = dedup_by(&all, |p| format!("{}:{}", p.id, p.detail.level));
            let (v, count, t) = crate::providers::perimeters::select_for_view(
                &merged,
                query.bbox.as_ref(),
                max_items,
            );
            (FeedItems::Perimeters(v), count, t)
        }
        FeedId::Bikeshare => {
            let all: Vec<&Vec<_>> = lists
                .iter()
                .filter_map(|l| if let FeedItems::Bikeshare(v) = l { Some(v) } else { None })
                .collect();
            let merged = dedup_by(&all, |s| s.id.clone());
            let count = merged.len();
            let filtered = merged
                .into_iter()
                .filter(|s| {
                    in_view(&s.position) && f.system.as_ref().is_none_or(|sys| sys == &s.system_id)
                })
                .collect();
            let (v, t) = cap(filtered, max_items);
            (FeedItems::Bikeshare(v), u32::try_from(count).unwrap_or(u32::MAX), t)
        }
        FeedId::Transit => {
            let all: Vec<&Vec<_>> = lists
                .iter()
                .filter_map(|l| if let FeedItems::Transit(v) = l { Some(v) } else { None })
                .collect();
            let merged = dedup_by(&all, |v| v.id.clone());
            let count = merged.len();
            let filtered = merged
                .into_iter()
                .filter(|v| {
                    in_view(&v.position) && f.feed.as_ref().is_none_or(|fd| fd == &v.feed_id)
                })
                .collect();
            let (v, t) = cap(filtered, max_items);
            (FeedItems::Transit(v), u32::try_from(count).unwrap_or(u32::MAX), t)
        }
    }
}
