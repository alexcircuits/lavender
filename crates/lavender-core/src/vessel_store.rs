//! A bounded, in-memory store of live vessel positions, independent of any transport.
//!
//! AIS is a continuous stream of small reports, so the store is shaped for that:
//! - an R-tree over current positions answers "which ships are in this area" without
//!   looking at the rest of the world;
//! - ships are grouped in time buckets by when they were last heard, so expiry and
//!   capacity eviction touch only the ships that leave (never a scan of every ship);
//! - each ship keeps a short, thinned track, with a cap per ship and one for all ships.
//!
//! Time comes from the caller (`now`), never from the system clock, so replays and tests
//! are deterministic. The store's clock only moves forward. Observation times later than
//! the clock are clamped to it; reports older than the retention window are refused.

mod buckets;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use lavender_domain::{
    BBox, GeoPoint, Vessel, VesselDimensions, VesselEta, VesselTrackSample, VesselVoyage,
};
use lavender_geo::distance_m;
use rstar::primitives::GeomWithData;
use rstar::{AABB, RTree};

use buckets::TimeBuckets;

/// A new track sample needs at least this much time since the previous one…
const TRACK_MIN_GAP_S: i64 = 30;
/// …and at least this much movement. A ship that stays put adds a point only once its
/// newest one is half the retention old, so its track never ages out while it is heard.
const TRACK_MIN_MOVE_M: f64 = 25.0;
/// Retention is enforced in this many buckets: expiry is late by at most 1/30 of it.
const BUCKETS_PER_TTL: u64 = 30;

/// A Maritime Mobile Service Identity: nine decimal digits, stored as a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Mmsi(u32);

impl Mmsi {
    /// Accepts 1–999 999 999 (AIS carries the MMSI as a 30-bit number).
    #[must_use]
    pub fn from_number(value: u64) -> Option<Self> {
        u32::try_from(value).ok().filter(|v| (1..=999_999_999).contains(v)).map(Self)
    }

    /// Accepts exactly nine ASCII digits, not all zero (leading zeros are meaningful).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() != 9 || !text.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        text.parse::<u64>().ok().and_then(Self::from_number)
    }
}

impl fmt::Display for Mmsi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:09}", self.0)
    }
}

/// How much the store may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VesselLimits {
    pub max_vessels: usize,
    pub max_track_points: usize,
    pub max_total_track_points: usize,
    /// Ships not heard from for this long are removed; track points older than this are
    /// dropped; reports older than this are refused.
    pub retention: Duration,
}

/// A position report, already validated by the caller's parser.
#[derive(Debug, Clone, PartialEq)]
pub struct VesselPosition {
    pub mmsi: Mmsi,
    pub position: GeoPoint,
    pub speed_knots: Option<f64>,
    pub course_deg: Option<f64>,
    pub heading_deg: Option<f64>,
    /// When the report was received upstream; `None` means "now".
    pub observed_at: Option<Timestamp>,
    /// A name the upstream attached to the report, used until static data arrives.
    pub name_hint: Option<String>,
    /// Navigational status in words (Class A reports only); static, so the hot path of
    /// position reports allocates nothing for it.
    pub navigational_status: Option<&'static str>,
}

/// Static and voyage data (name, IMO number, call sign, type, destination, ETA, draught,
/// dimensions). Absent fields keep the values already known.
#[derive(Debug, Clone, PartialEq)]
pub struct VesselDetails {
    pub mmsi: Mmsi,
    pub name: Option<String>,
    pub imo: Option<String>,
    pub call_sign: Option<String>,
    pub vessel_type: Option<String>,
    pub destination: Option<String>,
    pub eta: Option<VesselEta>,
    pub draught_m: Option<f64>,
    pub dimensions: Option<VesselDimensions>,
    pub observed_at: Option<Timestamp>,
}

impl VesselDetails {
    /// Details for `mmsi` with nothing known yet.
    #[must_use]
    pub fn new(mmsi: Mmsi) -> Self {
        Self {
            mmsi,
            name: None,
            imo: None,
            call_sign: None,
            vessel_type: None,
            destination: None,
            eta: None,
            draught_m: None,
            dimensions: None,
            observed_at: None,
        }
    }
}

/// What the store did with a report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recorded {
    Stored,
    /// Older than the retention window: refused, never shown.
    TooOld,
    /// Not newer than what the store already holds for that ship.
    Superseded,
}

/// Counts for health checks and tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StoreStats {
    /// Ships known, with or without a position yet.
    pub vessels: usize,
    /// Ships with a position (the ones a map shows).
    pub positioned: usize,
    pub track_points: usize,
}

#[derive(Debug, Clone, Copy)]
struct Fix {
    position: GeoPoint,
    speed_knots: Option<f64>,
    course_deg: Option<f64>,
    heading_deg: Option<f64>,
    navigational_status: Option<&'static str>,
    observed_at: Timestamp,
}

#[derive(Debug, Default)]
struct Entry {
    name: Option<String>,
    name_hint: Option<String>,
    imo: Option<String>,
    call_sign: Option<String>,
    vessel_type: Option<String>,
    destination: Option<String>,
    eta: Option<VesselEta>,
    draught_m: Option<f64>,
    dimensions: Option<VesselDimensions>,
    details_at: Option<Timestamp>,
    fix: Option<Fix>,
    /// The ship as the feed shows it, rebuilt when it changes so queries only clone.
    view: Option<Vessel>,
    track: VecDeque<VesselTrackSample>,
    heard_at: Timestamp,
    heard_bucket: i64,
    track_bucket: Option<i64>,
}

impl Entry {
    fn rebuild_view(&mut self, mmsi: Mmsi) {
        self.view = self.fix.map(|fix| {
            Vessel::new(
                mmsi.to_string(),
                self.name.clone().or_else(|| self.name_hint.clone()),
                self.imo.clone(),
                self.vessel_type.clone(),
                self.destination.clone(),
                fix.position,
                fix.speed_knots,
                fix.course_deg,
                fix.heading_deg,
                Some(fix.observed_at.to_string()),
                Some(fix.observed_at.as_second()),
            )
        });
    }

    /// The voyage and status details, for the selected ship only (not in feed views).
    fn voyage(&self) -> VesselVoyage {
        VesselVoyage {
            call_sign: self.call_sign.clone(),
            navigational_status: self.fix.and_then(|f| f.navigational_status).map(str::to_owned),
            eta: self.eta,
            draught_m: self.draught_m,
            dimensions: self.dimensions,
        }
    }
}

type IndexedPoint = GeomWithData<[f64; 2], Mmsi>;

fn indexed(mmsi: Mmsi, p: &GeoPoint) -> IndexedPoint {
    GeomWithData::new([p.lon, p.lat], mmsi)
}

#[derive(Debug)]
struct State {
    vessels: HashMap<Mmsi, Entry>,
    index: RTree<IndexedPoint>,
    /// Every ship, by when it was last heard.
    heard: TimeBuckets,
    /// Ships with track points, by when their track last grew.
    tracked: TimeBuckets,
    track_points: usize,
    clock: Timestamp,
}

/// The store; cheap to share behind an `Arc`. All methods take `&self`.
#[derive(Debug)]
pub struct VesselStore {
    limits: VesselLimits,
    retention: SignedDuration,
    state: Mutex<State>,
}

impl VesselStore {
    #[must_use]
    pub fn new(limits: VesselLimits) -> Self {
        let limits = VesselLimits {
            max_vessels: limits.max_vessels.max(1),
            max_track_points: limits.max_track_points.max(1),
            max_total_track_points: limits.max_total_track_points.max(1),
            retention: limits.retention.max(Duration::from_secs(1)),
        };
        let retention_s = limits.retention.as_secs();
        let width = i64::try_from((retention_s / BUCKETS_PER_TTL).max(1)).unwrap_or(i64::MAX);
        let retention = SignedDuration::from_secs(i64::try_from(retention_s).unwrap_or(i64::MAX));
        Self {
            limits,
            retention,
            state: Mutex::new(State {
                vessels: HashMap::new(),
                index: RTree::new(),
                heard: TimeBuckets::new(width),
                tracked: TimeBuckets::new(width),
                track_points: 0,
                clock: Timestamp::UNIX_EPOCH,
            }),
        }
    }

    #[must_use]
    pub fn limits(&self) -> VesselLimits {
        self.limits
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Moves the clock to `now` (never backwards) and removes ships not heard from within
    /// the retention window.
    pub fn advance(&self, now: Timestamp) {
        let mut state = self.lock();
        self.advance_locked(&mut state, now);
    }

    fn advance_locked(&self, state: &mut State, now: Timestamp) {
        state.clock = state.clock.max(now);
        let cutoff = state.clock.saturating_sub(self.retention).unwrap_or(Timestamp::MIN);
        for mmsi in state.heard.take_older_than(cutoff) {
            state.forget(mmsi, true);
        }
    }

    /// Records a position report received at `now`.
    pub fn record_position(&self, report: VesselPosition, now: Timestamp) -> Recorded {
        let mut state = self.lock();
        self.advance_locked(&mut state, now);
        let Some(observed) = self.observation_time(&state, report.observed_at) else {
            return Recorded::TooOld;
        };
        let mmsi = report.mmsi;
        if state.vessels.get(&mmsi).and_then(|e| e.fix).is_some_and(|f| observed <= f.observed_at) {
            return Recorded::Superseded;
        }
        self.make_room_for(&mut state, mmsi);
        let State { vessels, index, heard, .. } = &mut *state;
        let entry = entry_for(vessels, heard, mmsi, observed);
        let previous = entry.fix.map(|f| f.position);
        if previous.is_none_or(|p| p != report.position) {
            if let Some(old) = previous {
                index.remove(&indexed(mmsi, &old));
            }
            index.insert(indexed(mmsi, &report.position));
        }
        entry.fix = Some(Fix {
            position: report.position,
            speed_knots: report.speed_knots,
            course_deg: report.course_deg,
            heading_deg: report.heading_deg,
            navigational_status: report.navigational_status,
            observed_at: observed,
        });
        if report.name_hint.is_some() {
            entry.name_hint = report.name_hint;
        }
        entry.rebuild_view(mmsi);
        mark_heard(heard, entry, mmsi, observed);
        self.extend_track(&mut state, mmsi, &report.position, observed);
        Recorded::Stored
    }

    /// Records static and voyage data received at `now`. A ship may be known by its
    /// details before its first position; it is shown once a position arrives.
    pub fn record_details(&self, report: VesselDetails, now: Timestamp) -> Recorded {
        let mut state = self.lock();
        self.advance_locked(&mut state, now);
        let mmsi = report.mmsi;
        let Some(observed) = self.observation_time(&state, report.observed_at) else {
            return Recorded::TooOld;
        };
        if state.vessels.get(&mmsi).and_then(|e| e.details_at).is_some_and(|known| observed < known)
        {
            return Recorded::Superseded;
        }
        self.make_room_for(&mut state, mmsi);
        let State { vessels, heard, .. } = &mut *state;
        let entry = entry_for(vessels, heard, mmsi, observed);
        keep(report.name, &mut entry.name);
        keep(report.imo, &mut entry.imo);
        keep(report.call_sign, &mut entry.call_sign);
        keep(report.vessel_type, &mut entry.vessel_type);
        keep(report.destination, &mut entry.destination);
        keep(report.eta, &mut entry.eta);
        keep(report.draught_m, &mut entry.draught_m);
        keep(report.dimensions, &mut entry.dimensions);
        entry.details_at = Some(observed);
        entry.rebuild_view(mmsi);
        mark_heard(heard, entry, mmsi, observed);
        Recorded::Stored
    }

    /// Ships with a position inside `bbox` (antimeridian-crossing boxes included), heard
    /// within the retention window, ordered by MMSI, at most `limit`.
    #[must_use]
    pub fn in_area(&self, bbox: &BBox, limit: usize) -> Vec<Vessel> {
        let state = self.lock();
        let cutoff = state.clock.saturating_sub(self.retention).unwrap_or(Timestamp::MIN);
        let mut hits: Vec<Mmsi> = bbox
            .lon_ranges()
            .into_iter()
            .flat_map(|(west, east)| {
                let envelope = AABB::from_corners([west, bbox.south], [east, bbox.north]);
                state.index.locate_in_envelope(envelope).map(|p| p.data).collect::<Vec<_>>()
            })
            .collect();
        hits.sort_unstable();
        hits.dedup();
        hits.into_iter()
            .filter_map(|mmsi| state.vessels.get(&mmsi))
            .filter(|e| e.heard_at >= cutoff)
            .filter_map(|e| e.view.clone())
            .take(limit)
            .collect()
    }

    /// The most recently heard ships with a position, newest first (to bucket
    /// precision), at most `limit`. Used when a view is too large for area queries.
    #[must_use]
    pub fn most_recent(&self, limit: usize) -> Vec<Vessel> {
        let state = self.lock();
        let cutoff = state.clock.saturating_sub(self.retention).unwrap_or(Timestamp::MIN);
        state
            .heard
            .newest_first()
            .filter_map(|mmsi| state.vessels.get(&mmsi))
            .filter(|e| e.heard_at >= cutoff)
            .filter_map(|e| e.view.clone())
            .take(limit)
            .collect()
    }

    /// The ship's retained track, oldest first; `None` when nothing is retained.
    #[must_use]
    pub fn track(&self, mmsi: Mmsi) -> Option<Vec<VesselTrackSample>> {
        let state = self.lock();
        let cutoff = state.clock.saturating_sub(self.retention).unwrap_or(Timestamp::MIN);
        let entry = state.vessels.get(&mmsi).filter(|e| e.heard_at >= cutoff)?;
        let samples: Vec<VesselTrackSample> =
            entry.track.iter().filter(|s| s.epoch_sec >= cutoff.as_second()).cloned().collect();
        (!samples.is_empty()).then_some(samples)
    }

    /// What is known of the ship's voyage and status, heard within the retention window;
    /// `None` when the ship is unknown or nothing of it is known.
    #[must_use]
    pub fn voyage(&self, mmsi: Mmsi) -> Option<VesselVoyage> {
        let state = self.lock();
        let cutoff = state.clock.saturating_sub(self.retention).unwrap_or(Timestamp::MIN);
        let voyage = state.vessels.get(&mmsi).filter(|e| e.heard_at >= cutoff)?.voyage();
        (!voyage.is_empty()).then_some(voyage)
    }

    #[must_use]
    pub fn stats(&self) -> StoreStats {
        let state = self.lock();
        StoreStats {
            vessels: state.vessels.len(),
            positioned: state.index.size(),
            track_points: state.track_points,
        }
    }

    /// The observation time to store: the report's own time, clamped to the clock, or
    /// the clock itself; `None` when it is older than the retention window.
    fn observation_time(&self, state: &State, observed_at: Option<Timestamp>) -> Option<Timestamp> {
        let observed = observed_at.map_or(state.clock, |t| t.min(state.clock));
        let cutoff = state.clock.saturating_sub(self.retention).unwrap_or(Timestamp::MIN);
        (observed >= cutoff).then_some(observed)
    }

    /// Evicts the least recently heard ships until a new one fits.
    fn make_room_for(&self, state: &mut State, mmsi: Mmsi) {
        if state.vessels.contains_key(&mmsi) {
            return;
        }
        while state.vessels.len() >= self.limits.max_vessels {
            let Some(oldest) = state.heard.pop_oldest() else { break };
            state.forget(oldest, true);
        }
    }

    /// Appends a thinned track sample and enforces the track caps and window.
    fn extend_track(
        &self,
        state: &mut State,
        mmsi: Mmsi,
        position: &GeoPoint,
        observed: Timestamp,
    ) {
        let cutoff_s =
            state.clock.saturating_sub(self.retention).unwrap_or(Timestamp::MIN).as_second();
        let State { vessels, tracked, track_points, .. } = state;
        let Some(entry) = vessels.get_mut(&mmsi) else { return };
        let sample = VesselTrackSample {
            lat: position.lat,
            lon: position.lon,
            epoch_sec: observed.as_second(),
        };
        let refresh_s = self.retention.as_secs() / 2;
        let due = entry.track.back().is_none_or(|last| {
            let gap_s = sample.epoch_sec - last.epoch_sec;
            let moved =
                distance_m(&GeoPoint { lat: last.lat, lon: last.lon, alt_m: None }, position);
            gap_s >= TRACK_MIN_GAP_S && (moved >= TRACK_MIN_MOVE_M || gap_s >= refresh_s)
        });
        if !due {
            return;
        }
        entry.track.push_back(sample);
        *track_points += 1;
        while entry.track.len() > self.limits.max_track_points
            || entry.track.front().is_some_and(|s| s.epoch_sec < cutoff_s)
        {
            entry.track.pop_front();
            *track_points -= 1;
        }
        let bucket = tracked.key(observed);
        tracked.relocate(mmsi, entry.track_bucket, bucket);
        entry.track_bucket = Some(bucket);
        // The global cap drops whole least-recently-extended tracks, keeping the ships
        // on the map; recent tracks stay coherent instead of losing arbitrary segments.
        while *track_points > self.limits.max_total_track_points {
            let Some(oldest) = tracked.pop_oldest() else { break };
            if let Some(entry) = vessels.get_mut(&oldest) {
                *track_points -= entry.track.len();
                entry.track.clear();
                entry.track_bucket = None;
            }
        }
    }
}

impl State {
    /// Removes a ship entirely. `popped_from_heard` says its heard bucket entry is
    /// already gone (taken by an expiry or eviction).
    fn forget(&mut self, mmsi: Mmsi, popped_from_heard: bool) {
        let Some(entry) = self.vessels.remove(&mmsi) else { return };
        if let Some(fix) = entry.fix {
            self.index.remove(&indexed(mmsi, &fix.position));
        }
        if !popped_from_heard {
            self.heard.remove(mmsi, entry.heard_bucket);
        }
        if let Some(bucket) = entry.track_bucket {
            self.tracked.remove(mmsi, bucket);
        }
        self.track_points -= entry.track.len();
    }
}

/// The entry for `mmsi`, created (and filed as heard at `observed`) if it is new.
fn entry_for<'a>(
    vessels: &'a mut HashMap<Mmsi, Entry>,
    heard: &mut TimeBuckets,
    mmsi: Mmsi,
    observed: Timestamp,
) -> &'a mut Entry {
    vessels.entry(mmsi).or_insert_with(|| {
        let heard_bucket = heard.key(observed);
        heard.insert(mmsi, heard_bucket);
        Entry { heard_at: observed, heard_bucket, ..Entry::default() }
    })
}

/// Replaces a known value only with a reported one: absent fields keep what is known.
fn keep<T>(new: Option<T>, old: &mut Option<T>) {
    if new.is_some() {
        *old = new;
    }
}

fn mark_heard(heard: &mut TimeBuckets, entry: &mut Entry, mmsi: Mmsi, observed: Timestamp) {
    if observed > entry.heard_at {
        let bucket = heard.key(observed);
        heard.relocate(mmsi, Some(entry.heard_bucket), bucket);
        entry.heard_at = observed;
        entry.heard_bucket = bucket;
    }
}
