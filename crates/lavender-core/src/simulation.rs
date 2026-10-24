//! Deterministic synthetic aircraft (LB-111): a seeded, offline generator that
//! flies great-circle routes between synthesised waypoint pairs and emits
//! records in the normal `Aircraft` contract, so the interface can run the
//! whole aircraft pipeline — feed hub, renderer, inspector — with zero network
//! and zero external attribution (every record says `simulation`).

use lavender_domain::Aircraft;
use lavender_geo::geodesy;

use crate::motion::{CourseSpeedState, advance};

/// Source id stamped on every synthetic contact.
pub const SIMULATION_SOURCE: &str = "simulation";

struct SimAircraft {
    state: CourseSpeedState,
    /// Great-circle distance remaining to the destination waypoint, metres.
    remaining_m: f64,
    hex: String,
    callsign: String,
    altitude_ft: f64,
    climb_fpm: f64,
}

/// A deterministic fleet. Same seed and count always produce the same fleet,
/// the same ids, and the same positions after the same ticks.
pub struct AircraftSimulator {
    fleet: Vec<SimAircraft>,
    elapsed_s: f64,
}

impl AircraftSimulator {
    /// Spawn `count` aircraft on random short-to-medium great-circle legs.
    pub fn new(seed: u64, count: usize) -> Self {
        let mut rng = fastrand::Rng::with_seed(seed);
        let mut fleet = Vec::with_capacity(count);
        for i in 0..count {
            // Random-ish legs between mid-latitude waypoints, over water or land alike.
            let lat = (rng.f64() * 120.0 - 60.0).round() / 1.0;
            let lon = (rng.f64() * 340.0 - 170.0).round() / 1.0;
            let dlat = (rng.f64() * 16.0 - 8.0).round() / 1.0;
            let dlon = (rng.f64() * 24.0 - 12.0).round() / 1.0;
            let start = lavender_domain::GeoPoint { lat, lon, alt_m: None };
            let end = lavender_domain::GeoPoint {
                lat: (lat + dlat).clamp(-85.0, 85.0),
                lon: (lon + dlon).clamp(-179.0, 179.0),
                alt_m: None,
            };
            let heading = geodesy::initial_bearing_deg(&start, &end);
            let remaining_m = geodesy::distance_m(&start, &end);
            let speed_mps = 150.0 + rng.f64() * 100.0; // 290–490 kt ground speed
            let cruising = rng.f64() < 0.7;
            let altitude_ft = if cruising {
                28_000.0 + rng.f64() * 14_000.0
            } else {
                12_000.0 + rng.f64() * 10_000.0
            };
            fleet.push(SimAircraft {
                state: CourseSpeedState { position: start, heading_deg: heading, speed_mps },
                remaining_m,
                // Deterministic, stable, plausible mode-S style addresses (ICAO
                // block starts at 0xA00000) so ids never collide with real feeds
                // in a way the inspector could confuse.
                hex: format!("{:06x}", 0xa0_0000 + i * 7 + (seed as usize & 0xff)),
                callsign: format!("SIM{}", 100 + i),
                altitude_ft,
                climb_fpm: if cruising { 0.0 } else { 600.0 },
            });
        }
        Self { fleet, elapsed_s: 0.0 }
    }

    /// Advance the fleet by `dt_s` and snapshot it as normalised aircraft records.
    pub fn tick(&mut self, dt_s: f64) -> Vec<Aircraft> {
        self.elapsed_s += dt_s;
        self.fleet
            .iter_mut()
            .map(|a| {
                let before = a.state.position;
                a.state = advance(&a.state, dt_s);
                a.remaining_m =
                    (a.remaining_m - geodesy::distance_m(&before, &a.state.position)).max(0.0);
                // Arrived: turn around and fly the leg back (a simulator's airway).
                if a.remaining_m <= 0.0 {
                    let back = geodesy::initial_bearing_deg(&a.state.position, &before);
                    a.state.heading_deg = back;
                    a.remaining_m = geodesy::distance_m(&a.state.position, &before);
                }
                Aircraft {
                    hex: a.hex.clone(),
                    callsign: Some(a.callsign.clone()),
                    registration: None,
                    type_code: Some("SIM".to_string()),
                    category: None,
                    position: lavender_domain::GeoPoint {
                        lat: a.state.position.lat,
                        lon: a.state.position.lon,
                        alt_m: None,
                    },
                    altitude_baro_ft: Some(a.altitude_ft),
                    altitude_geom_ft: None,
                    on_ground: false,
                    ground_speed_kt: Some(a.state.speed_mps * 1.943_844),
                    track_deg: Some(a.state.heading_deg),
                    vertical_rate_fpm: Some(a.climb_fpm),
                    squawk: None,
                    emergency: None,
                    is_military: false,
                    seen_pos_s: 0.0,
                    source: SIMULATION_SOURCE.to_string(),
                }
            })
            .collect()
    }

    /// Mission-elapsed time the fleet has flown, seconds (determinism checks).
    pub fn elapsed_s(&self) -> f64 {
        self.elapsed_s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn same_seed_same_fleet_same_flight() {
        let mut a = AircraftSimulator::new(42, 12);
        let mut b = AircraftSimulator::new(42, 12);
        let sa = a.tick(10.0);
        let sb = b.tick(10.0);
        assert_eq!(sa, sb);
        assert_eq!(a.elapsed_s(), b.elapsed_s());
    }

    #[test]
    fn different_seeds_produce_different_fleets() {
        let a = AircraftSimulator::new(1, 8).tick(5.0);
        let b = AircraftSimulator::new(2, 8).tick(5.0);
        assert_ne!(a, b);
    }

    #[test]
    fn ids_are_unique_stable_and_simulation_sourced() {
        let records = AircraftSimulator::new(7, 25).tick(1.0);
        assert_eq!(records.len(), 25);
        let hexes: HashSet<_> = records.iter().map(|r| r.hex.clone()).collect();
        assert_eq!(hexes.len(), 25, "hex ids must be unique");
        assert!(records.iter().all(|r| r.source == SIMULATION_SOURCE));
        assert!(records.iter().all(|r| !r.on_ground));
    }

    #[test]
    fn positions_stay_valid_and_move_about_the_right_speed() {
        let mut sim = AircraftSimulator::new(9, 10);
        let first = sim.tick(0.0);
        let second = sim.tick(60.0);
        for (a, b) in first.iter().zip(second.iter()) {
            assert_eq!(a.hex, b.hex);
            assert!(b.position.lat.abs() <= 90.0);
            assert!((b.position.lon - a.position.lon).abs() <= 180.0);
            let moved = geodesy::distance_m(
                &lavender_domain::GeoPoint {
                    lat: a.position.lat,
                    lon: a.position.lon,
                    alt_m: None,
                },
                &lavender_domain::GeoPoint {
                    lat: b.position.lat,
                    lon: b.position.lon,
                    alt_m: None,
                },
            );
            let speed = b.ground_speed_kt.unwrap_or(0.0) / 1.943_844;
            // 60 s of flight: within 20 % of speed × time (great-circle rounding aside).
            let expected = speed * 60.0;
            assert!(
                (moved - expected).abs() < expected * 0.2,
                "moved {moved} m, expected ~{expected} m"
            );
        }
    }
}
