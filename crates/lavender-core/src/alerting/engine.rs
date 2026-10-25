//! Geofence alerting rule engine evaluating live feed snapshots against spatial boundaries.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, PoisonError, RwLock};

use lavender_domain::{AlertEvent, AlertKind, FeedId, FeedItems, FeedSnapshot, GeofenceRule};
use lavender_geo::Propagator;

use super::geometry::{antimeridian_point_in_polygon, polygon_centroid};
use super::state::BoundedLru;
use crate::error::CoreError;

type EntityKey = (String, String, String);
type PassKey = (String, String, u32, i64);

/// Core alerting engine managing geofence rules, tracking entity spatial state,
/// and buffering alert events in a bounded ring buffer.
#[derive(Debug)]
pub struct GeofenceAlertEngine {
    rules: RwLock<HashMap<String, GeofenceRule>>,
    tracking_state: Mutex<BoundedLru<EntityKey, bool>>,
    passes_seen: Mutex<BoundedLru<PassKey, bool>>,
    inbox: Mutex<VecDeque<AlertEvent>>,
    max_rules: usize,
    inbox_capacity: usize,
}

impl GeofenceAlertEngine {
    /// Constructs a new alerting engine with the configured capacities.
    #[must_use]
    pub fn new(max_rules: usize, inbox_capacity: usize, state_capacity: usize) -> Self {
        Self {
            rules: RwLock::new(HashMap::with_capacity(max_rules.min(100))),
            tracking_state: Mutex::new(BoundedLru::new(state_capacity)),
            passes_seen: Mutex::new(BoundedLru::new(state_capacity.min(5_000))),
            inbox: Mutex::new(VecDeque::with_capacity(inbox_capacity.min(500))),
            max_rules: max_rules.max(1),
            inbox_capacity: inbox_capacity.max(1),
        }
    }

    /// Creates or replaces a geofence alerting rule.
    ///
    /// # Errors
    /// Returns [`CoreError::BadRequest`] if the rule or its geofences are malformed,
    /// or [`CoreError::Limit`] if the maximum number of rules has been reached.
    pub fn set_rule(&self, rule: GeofenceRule) -> Result<(), CoreError> {
        let trimmed_id = rule.id.trim();
        if trimmed_id.is_empty() || trimmed_id.len() > 64 {
            return Err(CoreError::BadRequest(
                "Rule id must be between 1 and 64 characters.".to_owned(),
            ));
        }
        let trimmed_name = rule.name.trim();
        if trimmed_name.is_empty() || trimmed_name.len() > 128 {
            return Err(CoreError::BadRequest(
                "Rule name must be between 1 and 128 characters.".to_owned(),
            ));
        }
        if rule.geofences.is_empty() {
            return Err(CoreError::BadRequest(
                "Rule must define at least one geofence polygon.".to_owned(),
            ));
        }

        for geofence in &rule.geofences {
            if geofence.id.trim().is_empty() || geofence.name.trim().is_empty() {
                return Err(CoreError::BadRequest(
                    "Each geofence must have a non-empty id and name.".to_owned(),
                ));
            }
            if geofence.polygon.len() < 3 {
                return Err(CoreError::BadRequest(format!(
                    "Geofence '{}' must have at least 3 vertices (found {}).",
                    geofence.name,
                    geofence.polygon.len()
                )));
            }
            if geofence.polygon.len() > 10_000 {
                return Err(CoreError::BadRequest(format!(
                    "Geofence '{}' exceeds vertex limit of 10,000 points.",
                    geofence.name
                )));
            }
        }

        let mut lock = self.rules.write().unwrap_or_else(PoisonError::into_inner);
        if !lock.contains_key(&rule.id) && lock.len() >= self.max_rules {
            return Err(CoreError::Limit(format!(
                "Maximum number of alerting rules ({}) exceeded.",
                self.max_rules
            )));
        }
        lock.insert(rule.id.clone(), rule);
        Ok(())
    }

    /// Deletes an alerting rule by its identifier. Returns `true` if the rule existed.
    pub fn delete_rule(&self, id: &str) -> bool {
        let mut lock = self.rules.write().unwrap_or_else(PoisonError::into_inner);
        lock.remove(id).is_some()
    }

    /// Fetches a rule by its identifier if registered.
    #[must_use]
    pub fn get_rule(&self, id: &str) -> Option<GeofenceRule> {
        let lock = self.rules.read().unwrap_or_else(PoisonError::into_inner);
        lock.get(id).cloned()
    }

    /// Lists all registered alerting rules.
    #[must_use]
    pub fn list_rules(&self) -> Vec<GeofenceRule> {
        let lock = self.rules.read().unwrap_or_else(PoisonError::into_inner);
        lock.values().cloned().collect()
    }

    /// Retrieves recent alert events from the bounded ring buffer up to `limit`.
    #[must_use]
    pub fn inbox(&self, limit: Option<usize>) -> Vec<AlertEvent> {
        let max_fetch = limit.unwrap_or(100).min(self.inbox_capacity);
        let lock = self.inbox.lock().unwrap_or_else(PoisonError::into_inner);
        lock.iter().take(max_fetch).cloned().collect()
    }

    /// Clears the recent alert events inbox.
    pub fn clear_inbox(&self) {
        let mut lock = self.inbox.lock().unwrap_or_else(PoisonError::into_inner);
        lock.clear();
    }

    /// Evaluates a feed snapshot against all active geofence rules.
    pub fn evaluate_snapshot(&self, snapshot: &FeedSnapshot) -> Vec<AlertEvent> {
        let as_of = snapshot.fetched_at.unwrap_or(snapshot.generated_at);
        self.evaluate_feed_items(snapshot.feed, as_of, &snapshot.items)
    }

    /// Evaluates items for a specific feed against all active geofence rules,
    /// recording state transitions and buffering alert events.
    #[allow(clippy::too_many_lines)]
    pub fn evaluate_feed_items(
        &self,
        feed: FeedId,
        as_of: jiff::Timestamp,
        items: &FeedItems,
    ) -> Vec<AlertEvent> {
        let active_rules = {
            let lock = self.rules.read().unwrap_or_else(PoisonError::into_inner);
            lock.values().filter(|r| r.enabled).cloned().collect::<Vec<_>>()
        };

        if active_rules.is_empty() {
            return Vec::new();
        }

        let mut generated = Vec::new();

        match items {
            FeedItems::Aircraft(aircraft) => {
                for plane in aircraft {
                    let desc = plane.callsign.as_deref().unwrap_or(&plane.hex);
                    self.evaluate_point_entity(
                        &active_rules,
                        feed,
                        &plane.hex,
                        plane.position,
                        as_of,
                        &format!("Aircraft {desc}"),
                        &mut generated,
                    );
                }
            }
            FeedItems::Vessels(vessels) => {
                for vessel in vessels {
                    let desc = vessel.name.as_deref().unwrap_or(&vessel.mmsi);
                    self.evaluate_point_entity(
                        &active_rules,
                        feed,
                        &vessel.mmsi,
                        vessel.position,
                        as_of,
                        &format!("Vessel {desc}"),
                        &mut generated,
                    );
                }
            }
            FeedItems::Earthquakes(quakes) => {
                for quake in quakes {
                    let desc = quake.place.as_deref().unwrap_or("Seismic event");
                    self.evaluate_point_entity(
                        &active_rules,
                        feed,
                        &quake.id,
                        quake.position,
                        as_of,
                        &format!("Earthquake M{:.1}: {desc}", quake.magnitude),
                        &mut generated,
                    );
                }
            }
            FeedItems::Fires(fires) => {
                for fire in fires {
                    if let Ok(pos) = lavender_domain::GeoPoint::new(fire.lat, fire.lon) {
                        let entity_id = format!("fire-{:.4}-{:.4}", fire.lat, fire.lon);
                        self.evaluate_point_entity(
                            &active_rules,
                            feed,
                            &entity_id,
                            pos,
                            as_of,
                            &format!("Fire thermal hotspot FRP {:.1} MW", fire.frp),
                            &mut generated,
                        );
                    }
                }
            }
            FeedItems::Transit(transit) => {
                for vehicle in transit {
                    let desc = vehicle.label.as_deref().unwrap_or(&vehicle.id);
                    self.evaluate_point_entity(
                        &active_rules,
                        feed,
                        &vehicle.id,
                        vehicle.position,
                        as_of,
                        &format!("Transit vehicle {desc}"),
                        &mut generated,
                    );
                }
            }
            FeedItems::Bikeshare(stations) => {
                for station in stations {
                    self.evaluate_point_entity(
                        &active_rules,
                        feed,
                        &station.id,
                        station.position,
                        as_of,
                        &format!("Bikeshare dock '{}'", station.name),
                        &mut generated,
                    );
                }
            }
            FeedItems::Launches(launches) => {
                for launch in launches {
                    if let Some(pad) = &launch.pad
                        && let Ok(pos) = lavender_domain::GeoPoint::new(pad.latitude, pad.longitude)
                    {
                        self.evaluate_point_entity(
                            &active_rules,
                            feed,
                            &launch.id,
                            pos,
                            as_of,
                            &format!("Launch pad activity: {}", launch.name),
                            &mut generated,
                        );
                    }
                }
            }
            FeedItems::Satellites(satellites) => {
                self.evaluate_satellites(&active_rules, satellites, as_of, &mut generated);
            }
            _ => {}
        }

        if !generated.is_empty() {
            let mut inbox = self.inbox.lock().unwrap_or_else(PoisonError::into_inner);
            for alert in generated.iter().rev() {
                inbox.push_front(alert.clone());
            }
            while inbox.len() > self.inbox_capacity {
                inbox.pop_back();
            }
        }

        generated
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_point_entity(
        &self,
        rules: &[GeofenceRule],
        feed: FeedId,
        entity_id: &str,
        position: lavender_domain::GeoPoint,
        as_of: jiff::Timestamp,
        summary: &str,
        emitted: &mut Vec<AlertEvent>,
    ) {
        let mut state_lock = self.tracking_state.lock().unwrap_or_else(PoisonError::into_inner);

        for rule in rules {
            for geofence in &rule.geofences {
                let is_inside =
                    antimeridian_point_in_polygon(position.lon, position.lat, &geofence.polygon);
                let key: EntityKey = (rule.id.clone(), geofence.id.clone(), entity_id.to_owned());
                let was_inside = state_lock.peek(&key).copied();

                match was_inside {
                    None => {
                        state_lock.insert(key, is_inside);
                        if is_inside {
                            emitted.push(AlertEvent {
                                id: format!(
                                    "alert-{}-{}-{}",
                                    rule.id,
                                    entity_id,
                                    fastrand::u32(..)
                                ),
                                rule_id: rule.id.clone(),
                                geofence_id: Some(geofence.id.clone()),
                                geofence_name: Some(geofence.name.clone()),
                                feed,
                                entity_id: entity_id.to_owned(),
                                kind: AlertKind::Enter,
                                at: as_of,
                                confidence: 1.0,
                                position: Some(position),
                                message: Some(format!("{summary} entered '{}'", geofence.name)),
                            });
                        }
                    }
                    Some(false) if is_inside => {
                        state_lock.insert(key, true);
                        emitted.push(AlertEvent {
                            id: format!("alert-{}-{}-{}", rule.id, entity_id, fastrand::u32(..)),
                            rule_id: rule.id.clone(),
                            geofence_id: Some(geofence.id.clone()),
                            geofence_name: Some(geofence.name.clone()),
                            feed,
                            entity_id: entity_id.to_owned(),
                            kind: AlertKind::Enter,
                            at: as_of,
                            confidence: 1.0,
                            position: Some(position),
                            message: Some(format!("{summary} entered '{}'", geofence.name)),
                        });
                    }
                    Some(true) if !is_inside => {
                        state_lock.insert(key, false);
                        emitted.push(AlertEvent {
                            id: format!("alert-{}-{}-{}", rule.id, entity_id, fastrand::u32(..)),
                            rule_id: rule.id.clone(),
                            geofence_id: Some(geofence.id.clone()),
                            geofence_name: Some(geofence.name.clone()),
                            feed,
                            entity_id: entity_id.to_owned(),
                            kind: AlertKind::Exit,
                            at: as_of,
                            confidence: 1.0,
                            position: Some(position),
                            message: Some(format!("{summary} exited '{}'", geofence.name)),
                        });
                    }
                    _ => {
                        // Position unchanged relative to geofence: touch LRU order
                        let _ = state_lock.get_mut(&key);
                    }
                }
            }
        }
    }

    fn evaluate_satellites(
        &self,
        rules: &[GeofenceRule],
        satellites: &[lavender_domain::SatelliteElements],
        as_of: jiff::Timestamp,
        emitted: &mut Vec<AlertEvent>,
    ) {
        let mut passes_lock = self.passes_seen.lock().unwrap_or_else(PoisonError::into_inner);

        for rule in rules {
            for geofence in &rule.geofences {
                let Some(centroid) = polygon_centroid(&geofence.polygon) else {
                    continue;
                };

                for sat in satellites {
                    let Ok(propagator) = Propagator::new(sat) else {
                        continue;
                    };

                    // Predict passes over the centroid for the next 2 hours above 25° elevation
                    let Ok(passes) = propagator.passes(centroid, as_of, 2, 25.0) else {
                        continue;
                    };

                    for pass in passes {
                        let pass_key: PassKey = (
                            rule.id.clone(),
                            geofence.id.clone(),
                            sat.norad_id,
                            pass.peak.as_second(),
                        );

                        if passes_lock.peek(&pass_key).is_none() {
                            passes_lock.insert(pass_key, true);
                            #[allow(clippy::cast_possible_truncation)]
                            let conf = (pass.max_elevation_deg / 90.0).clamp(0.5, 1.0);
                            emitted.push(AlertEvent {
                                id: format!(
                                    "alert-overflight-{}-{}-{}",
                                    rule.id,
                                    sat.norad_id,
                                    fastrand::u32(..)
                                ),
                                rule_id: rule.id.clone(),
                                geofence_id: Some(geofence.id.clone()),
                                geofence_name: Some(geofence.name.clone()),
                                feed: FeedId::Satellites,
                                entity_id: sat.norad_id.to_string(),
                                kind: AlertKind::Overflight,
                                at: pass.rise,
                                confidence: conf,
                                position: Some(centroid),
                                message: Some(format!(
                                    "Satellite '{}' (#{}) pass over '{}' peaking at {} UTC ({:.1}°)",
                                    sat.name, sat.norad_id, geofence.name, pass.peak, pass.max_elevation_deg
                                )),
                            });
                        }
                    }
                }
            }
        }
    }
}
