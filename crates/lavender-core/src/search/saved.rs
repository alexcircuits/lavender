//! Persistent saved search engine evaluating live feed snapshots against registered queries.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, PoisonError, RwLock};

use jiff::Timestamp;
use lavender_domain::{
    AlertEvent, AlertKind, Coverage, FeedId, FeedItems, FeedSnapshot, Freshness, GeoPoint,
    SavedSearch, SavedSearchMatch,
};

use crate::alerting::{BoundedLru, GeofenceAlertEngine};
use crate::error::CoreError;

pub const DEFAULT_MAX_SEARCHES: usize = 1_000;
pub const DEFAULT_MAX_MATCHES_PER_SEARCH: usize = 500;
pub const MAX_QUERY_LEN: usize = 128;
pub const MAX_ID_LEN: usize = 64;
pub const MAX_NAME_LEN: usize = 128;

#[derive(Debug)]
struct MatchedMeta {
    entity_id: String,
    label: String,
    position: Option<GeoPoint>,
}

#[derive(Debug)]
struct SearchEntry {
    search: SavedSearch,
    seen_entities: BoundedLru<String, ()>,
    recent_matches: VecDeque<SavedSearchMatch>,
    matching_snapshots: HashMap<FeedId, FeedSnapshot>,
}

impl SearchEntry {
    fn new(search: SavedSearch, max_matches: usize) -> Self {
        Self {
            search,
            seen_entities: BoundedLru::new(max_matches),
            recent_matches: VecDeque::with_capacity(max_matches.min(128)),
            matching_snapshots: HashMap::new(),
        }
    }
}

/// Persistent saved search engine tracking queries across active feeds.
#[derive(Debug)]
pub struct SavedSearchEngine {
    searches: RwLock<HashMap<String, SearchEntry>>,
    alert_engine: Option<Arc<GeofenceAlertEngine>>,
    max_searches: usize,
    max_matches: usize,
}

impl Default for SavedSearchEngine {
    fn default() -> Self {
        Self::new(None, DEFAULT_MAX_SEARCHES, DEFAULT_MAX_MATCHES_PER_SEARCH)
    }
}

impl SavedSearchEngine {
    /// Constructs a new saved search engine with bounded capacity.
    #[must_use]
    pub fn new(
        alert_engine: Option<Arc<GeofenceAlertEngine>>,
        max_searches: usize,
        max_matches_per_search: usize,
    ) -> Self {
        Self {
            searches: RwLock::new(HashMap::with_capacity(max_searches.min(128))),
            alert_engine,
            max_searches: max_searches.max(1),
            max_matches: max_matches_per_search.max(1),
        }
    }

    /// Creates or replaces a persistent saved search.
    ///
    /// # Errors
    /// Returns [`CoreError::BadRequest`] if the ID, name, or query is empty or oversized,
    /// or [`CoreError::Limit`] if the maximum number of searches is exceeded.
    pub fn create_or_update(&self, search: SavedSearch) -> Result<SavedSearch, CoreError> {
        let trimmed_id = search.id.trim();
        if trimmed_id.is_empty() || trimmed_id.chars().count() > MAX_ID_LEN {
            return Err(CoreError::BadRequest(format!(
                "Saved search ID must be between 1 and {MAX_ID_LEN} characters."
            )));
        }
        let trimmed_name = search.name.trim();
        if trimmed_name.is_empty() || trimmed_name.chars().count() > MAX_NAME_LEN {
            return Err(CoreError::BadRequest(format!(
                "Saved search name must be between 1 and {MAX_NAME_LEN} characters."
            )));
        }
        let trimmed_query = search.query.trim();
        if trimmed_query.is_empty() || trimmed_query.chars().count() > MAX_QUERY_LEN {
            return Err(CoreError::BadRequest(format!(
                "Saved search query must be between 1 and {MAX_QUERY_LEN} characters."
            )));
        }

        let mut lock = self.searches.write().unwrap_or_else(PoisonError::into_inner);
        if !lock.contains_key(&search.id) && lock.len() >= self.max_searches {
            return Err(CoreError::Limit(format!(
                "Maximum number of saved searches ({}) exceeded.",
                self.max_searches
            )));
        }

        if let Some(existing) = lock.get_mut(&search.id) {
            if existing.search.query != search.query {
                existing.seen_entities.clear();
                existing.matching_snapshots.clear();
            }
            existing.search = search.clone();
        } else {
            lock.insert(search.id.clone(), SearchEntry::new(search.clone(), self.max_matches));
        }

        Ok(search)
    }

    /// Retrieves a saved search by its unique identifier.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<SavedSearch> {
        let lock = self.searches.read().unwrap_or_else(PoisonError::into_inner);
        lock.get(id).map(|e| e.search.clone())
    }

    /// Lists all registered saved searches ordered by creation timestamp descending.
    #[must_use]
    pub fn list(&self) -> Vec<SavedSearch> {
        let lock = self.searches.read().unwrap_or_else(PoisonError::into_inner);
        let mut list: Vec<_> = lock.values().map(|e| e.search.clone()).collect();
        list.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.id.cmp(&b.id)));
        list
    }

    /// Deletes a saved search by its identifier. Returns `true` if it existed.
    pub fn delete(&self, id: &str) -> bool {
        let mut lock = self.searches.write().unwrap_or_else(PoisonError::into_inner);
        lock.remove(id).is_some()
    }

    /// Returns the total number of registered saved searches.
    #[must_use]
    pub fn search_count(&self) -> usize {
        let lock = self.searches.read().unwrap_or_else(PoisonError::into_inner);
        lock.len()
    }

    /// Retrieves recent match events for a saved search up to `limit`.
    ///
    /// # Errors
    /// Returns [`CoreError::NotFound`] if the saved search does not exist.
    pub fn get_matches(
        &self,
        id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<SavedSearchMatch>, CoreError> {
        let lock = self.searches.read().unwrap_or_else(PoisonError::into_inner);
        let entry = lock
            .get(id)
            .ok_or_else(|| CoreError::NotFound(format!("Saved search '{id}' not found.")))?;
        let max_fetch = limit.unwrap_or(self.max_matches).min(self.max_matches);
        Ok(entry.recent_matches.iter().take(max_fetch).cloned().collect())
    }

    /// Generates or retrieves the dynamic virtual [`FeedSnapshot`] of matching items.
    ///
    /// # Errors
    /// Returns [`CoreError::NotFound`] if the saved search does not exist.
    pub fn get_feed_snapshot(&self, id: &str) -> Result<FeedSnapshot, CoreError> {
        let lock = self.searches.read().unwrap_or_else(PoisonError::into_inner);
        let entry = lock
            .get(id)
            .ok_or_else(|| CoreError::NotFound(format!("Saved search '{id}' not found.")))?;

        // 1. If explicit feed_filter provided:
        if let Some(ref feeds) = entry.search.feed_filter
            && !feeds.is_empty()
        {
            // Find first configured feed with non-empty matching items
            for feed in feeds {
                if let Some(snap) = entry.matching_snapshots.get(feed)
                    && !snap.items.is_empty()
                {
                    return Ok(snap.clone());
                }
            }
            // Otherwise find first configured feed with any snapshot
            for feed in feeds {
                if let Some(snap) = entry.matching_snapshots.get(feed) {
                    return Ok(snap.clone());
                }
            }
            // Fallback to empty snapshot for the first configured feed
            let primary = feeds[0];
            return Ok(empty_virtual_snapshot(&entry.search, primary));
        }

        // 2. No feed_filter: find any snapshot with non-empty items
        for snap in entry.matching_snapshots.values() {
            if !snap.items.is_empty() {
                return Ok(snap.clone());
            }
        }

        // 3. Any snapshot available
        if let Some(snap) = entry.matching_snapshots.values().next() {
            return Ok(snap.clone());
        }

        // 4. Default to empty aircraft snapshot
        Ok(empty_virtual_snapshot(&entry.search, FeedId::Aircraft))
    }

    /// Ingests a feed snapshot, evaluates string matches against all active saved searches,
    /// updates virtual feed snapshots, and optionally records alert events for newly seen matches.
    pub fn ingest_snapshot(&self, snapshot: &FeedSnapshot) -> Vec<SavedSearchMatch> {
        let mut lock = self.searches.write().unwrap_or_else(PoisonError::into_inner);
        let mut all_new_matches = Vec::new();

        for entry in lock.values_mut() {
            if let Some(ref feeds) = entry.search.feed_filter
                && !feeds.contains(&snapshot.feed)
            {
                continue;
            }

            let needle = entry.search.query.trim().to_lowercase();
            if needle.is_empty() {
                continue;
            }

            let (filtered_items, matches_meta) =
                filter_feed_items(snapshot.feed, &snapshot.items, &needle);

            let virtual_snap = FeedSnapshot {
                feed: snapshot.feed,
                sources: vec![format!("saved-search:{}", entry.search.id)],
                fetched_at: snapshot.fetched_at,
                generated_at: Timestamp::now(),
                freshness: snapshot.freshness.clone(),
                coverage: Coverage::Complete,
                source_count: u32::try_from(filtered_items.len()).unwrap_or(u32::MAX),
                pending_regions: 0,
                items: filtered_items,
                notices: vec![format!(
                    "Saved search '{}' ({})",
                    entry.search.name, entry.search.query
                )],
            };
            entry.matching_snapshots.insert(snapshot.feed, virtual_snap);

            for meta in matches_meta {
                if entry.seen_entities.peek(&meta.entity_id).is_none() {
                    entry.seen_entities.insert(meta.entity_id.clone(), ());
                    let match_record = SavedSearchMatch {
                        search_id: entry.search.id.clone(),
                        search_name: entry.search.name.clone(),
                        feed: snapshot.feed,
                        entity_id: meta.entity_id.clone(),
                        label: meta.label.clone(),
                        matched_at: Timestamp::now(),
                    };
                    entry.recent_matches.push_front(match_record.clone());
                    while entry.recent_matches.len() > self.max_matches {
                        entry.recent_matches.pop_back();
                    }

                    if entry.search.notify_on_match
                        && let Some(alert_engine) = &self.alert_engine
                    {
                        let alert = AlertEvent {
                            id: format!(
                                "search-{}-{}-{}",
                                entry.search.id,
                                meta.entity_id,
                                match_record.matched_at.as_millisecond()
                            ),
                            rule_id: entry.search.id.clone(),
                            geofence_id: None,
                            geofence_name: Some(entry.search.name.clone()),
                            feed: snapshot.feed,
                            entity_id: meta.entity_id,
                            kind: AlertKind::SearchMatch,
                            at: match_record.matched_at,
                            confidence: 1.0,
                            position: meta.position,
                            message: Some(format!(
                                "Saved search '{}' matched {}: {}",
                                entry.search.name, snapshot.feed, meta.label
                            )),
                        };
                        alert_engine.record_alert(alert);
                    }

                    all_new_matches.push(match_record);
                }
            }
        }

        all_new_matches
    }
}

fn empty_virtual_snapshot(search: &SavedSearch, feed: FeedId) -> FeedSnapshot {
    FeedSnapshot {
        feed,
        sources: vec![format!("saved-search:{}", search.id)],
        fetched_at: None,
        generated_at: Timestamp::now(),
        freshness: Freshness::Live,
        coverage: Coverage::Complete,
        source_count: 0,
        pending_regions: 0,
        items: FeedItems::empty(feed),
        notices: vec![format!("Saved search '{}' ({})", search.name, search.query)],
    }
}

#[allow(clippy::too_many_lines)]
fn filter_feed_items(
    _feed: FeedId,
    items: &FeedItems,
    needle: &str,
) -> (FeedItems, Vec<MatchedMeta>) {
    let mut meta = Vec::new();

    match items {
        FeedItems::Aircraft(list) => {
            let mut matched = Vec::new();
            for a in list {
                let hex_match = a.hex.to_lowercase().contains(needle);
                let callsign_match =
                    a.callsign.as_deref().is_some_and(|c| c.to_lowercase().contains(needle));
                let reg_match =
                    a.registration.as_deref().is_some_and(|r| r.to_lowercase().contains(needle));
                let type_match =
                    a.type_code.as_deref().is_some_and(|t| t.to_lowercase().contains(needle));

                if hex_match || callsign_match || reg_match || type_match {
                    matched.push(a.clone());
                    meta.push(MatchedMeta {
                        entity_id: a.hex.clone(),
                        label: a.callsign.clone().unwrap_or_else(|| a.hex.to_uppercase()),
                        position: Some(a.position),
                    });
                }
            }
            (FeedItems::Aircraft(matched), meta)
        }
        FeedItems::Vessels(list) => {
            let mut matched = Vec::new();
            for v in list {
                let mmsi_match = v.mmsi.to_lowercase().contains(needle);
                let name_match =
                    v.name.as_deref().is_some_and(|n| n.to_lowercase().contains(needle));
                let type_match =
                    v.vessel_type.as_deref().is_some_and(|t| t.to_lowercase().contains(needle));
                let imo_match =
                    v.imo.as_deref().is_some_and(|imo| imo.to_lowercase().contains(needle));

                if mmsi_match || name_match || type_match || imo_match {
                    matched.push(v.clone());
                    meta.push(MatchedMeta {
                        entity_id: v.mmsi.clone(),
                        label: v.name.clone().unwrap_or_else(|| format!("MMSI {}", v.mmsi)),
                        position: Some(v.position),
                    });
                }
            }
            (FeedItems::Vessels(matched), meta)
        }
        FeedItems::Satellites(list) => {
            let mut matched = Vec::new();
            for s in list {
                let norad_match = s.norad_id.to_string().contains(needle);
                let name_match = s.name.to_lowercase().contains(needle);
                let group_match = s.group.to_lowercase().contains(needle);
                let desig_match =
                    s.intl_designator.as_deref().is_some_and(|d| d.to_lowercase().contains(needle));

                if norad_match || name_match || group_match || desig_match {
                    matched.push(s.clone());
                    meta.push(MatchedMeta {
                        entity_id: s.norad_id.to_string(),
                        label: s.name.clone(),
                        position: None,
                    });
                }
            }
            (FeedItems::Satellites(matched), meta)
        }
        FeedItems::Earthquakes(list) => {
            let mut matched = Vec::new();
            for e in list {
                let id_match = e.id.to_lowercase().contains(needle);
                let place_match =
                    e.place.as_deref().is_some_and(|p| p.to_lowercase().contains(needle));
                let mag_type_match =
                    e.magnitude_type.as_deref().is_some_and(|m| m.to_lowercase().contains(needle));

                if id_match || place_match || mag_type_match {
                    matched.push(e.clone());
                    meta.push(MatchedMeta {
                        entity_id: e.id.clone(),
                        label: format!(
                            "M {:.1} — {}",
                            e.magnitude,
                            e.place.as_deref().unwrap_or("Unnamed location")
                        ),
                        position: Some(e.position),
                    });
                }
            }
            (FeedItems::Earthquakes(matched), meta)
        }
        FeedItems::Cyclones(list) => {
            let mut matched = Vec::new();
            for c in list {
                let id_match = c.id.to_lowercase().contains(needle);
                let name_match = c.name.to_lowercase().contains(needle);
                let basin_match = c.basin.to_lowercase().contains(needle);
                let class_match = c.classification.to_lowercase().contains(needle);

                if id_match || name_match || basin_match || class_match {
                    matched.push(c.clone());
                    meta.push(MatchedMeta {
                        entity_id: c.id.clone(),
                        label: c.name.clone(),
                        position: Some(c.position),
                    });
                }
            }
            (FeedItems::Cyclones(matched), meta)
        }
        FeedItems::Fires(list) => {
            let mut matched = Vec::new();
            for f in list {
                let sat_match = f.satellite.to_lowercase().contains(needle);
                let conf_match = f.confidence.to_lowercase().contains(needle);

                if sat_match || conf_match {
                    matched.push(f.clone());
                    meta.push(MatchedMeta {
                        entity_id: format!(
                            "{}_{}_{:.4}_{:.4}",
                            f.satellite,
                            f.acquired_at.as_millisecond(),
                            f.lat,
                            f.lon
                        ),
                        label: format!("Fire ({})", f.satellite),
                        position: GeoPoint::new(f.lat, f.lon).ok(),
                    });
                }
            }
            (FeedItems::Fires(matched), meta)
        }
        FeedItems::Launches(list) => {
            let mut matched = Vec::new();
            for l in list {
                let id_match = l.id.to_lowercase().contains(needle);
                let name_match = l.name.to_lowercase().contains(needle);
                let mission_match =
                    l.mission.as_deref().is_some_and(|m| m.to_lowercase().contains(needle));
                let provider_match =
                    l.provider.as_deref().is_some_and(|p| p.to_lowercase().contains(needle));
                let status_match = l.status.to_lowercase().contains(needle);

                if id_match || name_match || mission_match || provider_match || status_match {
                    matched.push(l.clone());
                    meta.push(MatchedMeta {
                        entity_id: l.id.clone(),
                        label: l.name.clone(),
                        position: l
                            .pad
                            .as_ref()
                            .and_then(|p| GeoPoint::new(p.latitude, p.longitude).ok()),
                    });
                }
            }
            (FeedItems::Launches(matched), meta)
        }
        FeedItems::Perimeters(list) => {
            let mut matched = Vec::new();
            for p in list {
                let id_match = p.id.to_lowercase().contains(needle);
                let name_match =
                    p.name.as_deref().is_some_and(|n| n.to_lowercase().contains(needle));
                let state_match =
                    p.state.as_deref().is_some_and(|s| s.to_lowercase().contains(needle));

                if id_match || name_match || state_match {
                    matched.push(p.clone());
                    meta.push(MatchedMeta {
                        entity_id: p.id.clone(),
                        label: p.name.clone().unwrap_or_else(|| p.id.clone()),
                        position: Some(p.anchor),
                    });
                }
            }
            (FeedItems::Perimeters(matched), meta)
        }
        FeedItems::Bikeshare(list) => {
            let mut matched = Vec::new();
            for b in list {
                let id_match = b.id.to_lowercase().contains(needle);
                let station_match = b.station_id.to_lowercase().contains(needle);
                let name_match = b.name.to_lowercase().contains(needle);
                let sys_match = b.system_id.to_lowercase().contains(needle);

                if id_match || station_match || name_match || sys_match {
                    matched.push(b.clone());
                    meta.push(MatchedMeta {
                        entity_id: b.id.clone(),
                        label: b.name.clone(),
                        position: Some(b.position),
                    });
                }
            }
            (FeedItems::Bikeshare(matched), meta)
        }
        FeedItems::Transit(list) => {
            let mut matched = Vec::new();
            for t in list {
                let id_match = t.id.to_lowercase().contains(needle);
                let veh_match = t.vehicle_id.to_lowercase().contains(needle);
                let label_match =
                    t.label.as_deref().is_some_and(|l| l.to_lowercase().contains(needle));
                let route_match =
                    t.route_id.as_deref().is_some_and(|r| r.to_lowercase().contains(needle));
                let feed_match = t.feed_id.to_lowercase().contains(needle);

                if id_match || veh_match || label_match || route_match || feed_match {
                    matched.push(t.clone());
                    meta.push(MatchedMeta {
                        entity_id: t.id.clone(),
                        label: t
                            .label
                            .clone()
                            .unwrap_or_else(|| format!("{}:{}", t.feed_id, t.vehicle_id)),
                        position: Some(t.position),
                    });
                }
            }
            (FeedItems::Transit(matched), meta)
        }
    }
}

#[path = "saved_tests.rs"]
#[cfg(test)]
mod saved_tests;
