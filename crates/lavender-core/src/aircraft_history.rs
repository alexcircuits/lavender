//! Bounded, process-local aircraft observations collected by shared provider refreshes.
//!
//! Reads never contact a provider. Idle records expire on the next ingestion or read;
//! monotonic time controls retention, while provider time controls observation ordering.

use crate::error::CoreError;
use jiff::{SignedDuration, Timestamp};
use lavender_domain::text::clean;
use lavender_domain::{Aircraft, AircraftTrack, GeoPoint, TrackPoint};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

const MAX_TRACKS: usize = 4_096;
const MAX_POINTS_PER_TRACK: usize = 256;
const MAX_TOTAL_POINTS: usize = 65_536;
const IDLE_TTL: Duration = Duration::from_mins(30);
const MAX_SOURCE_CHARS: usize = 64;

#[derive(Debug, Clone, Copy)]
struct Limits {
    tracks: usize,
    points_per_track: usize,
    total_points: usize,
    idle_ttl: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            tracks: MAX_TRACKS,
            points_per_track: MAX_POINTS_PER_TRACK,
            total_points: MAX_TOTAL_POINTS,
            idle_ttl: IDLE_TTL,
        }
    }
}

#[derive(Debug)]
struct Observation {
    point: TrackPoint,
    source: String,
}

#[derive(Debug)]
struct RetainedTrack {
    points: VecDeque<Observation>,
    /// A newer stationary fix still proves this aircraft is being observed, even though
    /// repeating its coordinates would add no useful geometry to the trail.
    latest_observation: Timestamp,
    last_seen: Instant,
}

#[derive(Debug, Default)]
struct State {
    tracks: HashMap<String, RetainedTrack>,
    /// Eviction order is bounded by the number of tracks; replacing a record removes its
    /// old key instead of accumulating one timer/queue entry for every refresh.
    oldest: BTreeSet<(Instant, String)>,
    total_points: usize,
}

#[derive(Debug, Default)]
pub(crate) struct AircraftHistory {
    state: Mutex<State>,
    limits: Limits,
}

impl AircraftHistory {
    pub(crate) fn capture(&self, aircraft: &[Aircraft], as_of: Timestamp) {
        self.capture_at(aircraft, as_of, Timestamp::now(), Instant::now());
    }

    pub(crate) fn track(&self, hex: &str) -> Result<AircraftTrack, CoreError> {
        self.track_at(hex, Instant::now())
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn capture_at(&self, aircraft: &[Aircraft], as_of: Timestamp, now: Timestamp, at: Instant) {
        let as_of = as_of.min(now);
        let mut state = self.lock();
        state.expire(at, self.limits.idle_ttl);
        for aircraft in aircraft {
            let Some(hex) = normalize_hex(&aircraft.hex) else { continue };
            let Some(time) = observation_time(as_of, aircraft.seen_pos_s) else { continue };
            // A cached/aged response cannot recreate a track that has already expired.
            if now.duration_since(time).unsigned_abs() >= self.limits.idle_ttl {
                continue;
            }
            let Ok(position) = GeoPoint::new(aircraft.position.lat, aircraft.position.lon)
                .and_then(|p| p.with_altitude(aircraft.position.alt_m))
            else {
                continue;
            };
            let source = clean(&aircraft.source, MAX_SOURCE_CHARS);
            if source.is_empty() {
                continue;
            }
            state.record(hex, TrackPoint { time, position }, source, at, self.limits);
        }
    }

    fn track_at(&self, hex: &str, at: Instant) -> Result<AircraftTrack, CoreError> {
        let hex = normalize_hex(hex).ok_or_else(|| {
            CoreError::BadRequest(
                "Aircraft hex must contain six hexadecimal digits, optionally prefixed with '~'."
                    .to_owned(),
            )
        })?;
        let mut state = self.lock();
        state.expire(at, self.limits.idle_ttl);
        let track = state.tracks.get(&hex).ok_or_else(|| {
            CoreError::NotFound(format!("No retained track is available for aircraft {hex}."))
        })?;
        let sources = track.points.iter().map(|p| p.source.clone()).collect::<BTreeSet<_>>();
        Ok(AircraftTrack {
            hex,
            points: track.points.iter().map(|p| p.point.clone()).collect(),
            sources: sources.into_iter().collect(),
        })
    }
}

impl State {
    fn record(
        &mut self,
        hex: String,
        point: TrackPoint,
        source: String,
        at: Instant,
        limits: Limits,
    ) {
        if let Some(track) = self.tracks.get_mut(&hex) {
            if point.time <= track.latest_observation {
                return;
            }
            self.oldest.remove(&(track.last_seen, hex.clone()));
            track.latest_observation = point.time;
            track.last_seen = at;
            self.oldest.insert((at, hex));
            if track.points.back().is_some_and(|p| p.point.position == point.position) {
                return;
            }
            if track.points.len() >= limits.points_per_track {
                track.points.pop_front();
                self.total_points -= 1;
            }
            track.points.push_back(Observation { point, source });
        } else {
            while self.tracks.len() >= limits.tracks {
                self.evict_oldest();
            }
            let latest_observation = point.time;
            self.oldest.insert((at, hex.clone()));
            self.tracks.insert(
                hex,
                RetainedTrack {
                    points: VecDeque::from([Observation { point, source }]),
                    latest_observation,
                    last_seen: at,
                },
            );
        }
        self.total_points += 1;
        // The global cap evicts the least recently observed aircraft as a whole. Recent
        // tracks remain coherent instead of losing an arbitrary segment of their trail.
        while self.total_points > limits.total_points {
            self.evict_oldest();
        }
    }

    fn evict_oldest(&mut self) {
        if let Some((_, hex)) = self.oldest.pop_first()
            && let Some(track) = self.tracks.remove(&hex)
        {
            self.total_points -= track.points.len();
        }
    }

    fn expire(&mut self, at: Instant, idle_ttl: Duration) {
        while self.oldest.first().is_some_and(|(seen, _)| at.duration_since(*seen) >= idle_ttl) {
            self.evict_oldest();
        }
    }
}

fn normalize_hex(hex: &str) -> Option<String> {
    let hex = hex.trim();
    let body = hex.strip_prefix('~').unwrap_or(hex);
    (body.len() == 6 && body.bytes().all(|c| c.is_ascii_hexdigit()))
        .then(|| hex.to_ascii_lowercase())
}

fn observation_time(as_of: Timestamp, seen_pos_s: f64) -> Option<Timestamp> {
    if !seen_pos_s.is_finite() || seen_pos_s < 0.0 {
        return None;
    }
    let age = SignedDuration::try_from_secs_f64(seen_pos_s).ok()?;
    as_of.checked_sub(age).ok()
}

#[cfg(test)]
mod tests;
