//! Dead-reckoning motion for live entities (LB-111), ported from upstream's
//! `legacy/src/cockpitMath.js` and `legacy/src/layers/flights/motion.js`
//! behaviour: headings normalise to [0, 360), slew along the shortest arc, a
//! display anchor converges on delayed feed fixes without ever surging past
//! the entity's own forward speed, and positions advance along great circles.

use lavender_domain::GeoPoint;
use lavender_geo::geodesy;

/// Normalise a heading into [0, 360); NaN reads as 0 like upstream.
#[must_use]
pub fn normalize_heading_deg(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    let v = value % 360.0;
    if v < 0.0 { v + 360.0 } else { v }
}

/// Advance a displayed heading toward `target` along the shortest arc without
/// exceeding `max_step_deg`; a non-positive step freezes the display heading.
#[must_use]
pub fn slew_heading_deg(current: f64, target: f64, max_step_deg: f64) -> f64 {
    let from = normalize_heading_deg(current);
    let to = normalize_heading_deg(target);
    if !max_step_deg.is_finite() || max_step_deg <= 0.0 {
        return from;
    }
    let delta = ((to - from + 540.0) % 360.0) - 180.0;
    let step = delta.clamp(-max_step_deg, max_step_deg);
    normalize_heading_deg(from + step)
}

/// A bounded per-step correction (metres) that converges a dead-reckoned
/// display position onto a delayed feed fix. Capping below the entity's
/// forward speed stops a late fix from becoming a first-person surge or
/// reversal while still removing drift (upstream `cockpitAnchorCorrectionStep`).
#[must_use]
pub fn anchor_correction_step_m(distance_m: f64, speed_mps: f64, dt_s: f64) -> f64 {
    if !distance_m.is_finite() || distance_m <= 0.0 || !dt_s.is_finite() || dt_s <= 0.0 {
        return 0.0;
    }
    let dt = dt_s.min(0.1);
    let speed = if speed_mps.is_finite() { speed_mps.max(0.0) } else { 0.0 };
    let eased = distance_m * (1.0 - (-1.25 * dt).exp());
    let correction_rate_mps = (speed * 0.22).max(0.75);
    distance_m.min(eased).min(correction_rate_mps * dt)
}

/// A dead-reckoning state: where the entity is and how it moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CourseSpeedState {
    pub position: GeoPoint,
    /// True track over ground, degrees clockwise from north.
    pub heading_deg: f64,
    /// Ground speed, metres per second.
    pub speed_mps: f64,
}

/// Advance a state by `dt_s` seconds along the great circle its track implies.
/// A non-finite or non-positive speed or time leaves it untouched.
#[must_use]
pub fn advance(state: &CourseSpeedState, dt_s: f64) -> CourseSpeedState {
    if !dt_s.is_finite() || dt_s <= 0.0 || !state.speed_mps.is_finite() || state.speed_mps <= 0.0 {
        return *state;
    }
    let distance = state.speed_mps * dt_s;
    CourseSpeedState {
        position: geodesy::destination(&state.position, state.heading_deg, distance),
        heading_deg: state.heading_deg,
        speed_mps: state.speed_mps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_like_upstream() {
        assert_eq!(normalize_heading_deg(-10.0), 350.0);
        assert_eq!(normalize_heading_deg(370.0), 10.0);
        assert_eq!(normalize_heading_deg(720.0), 0.0);
        assert_eq!(normalize_heading_deg(f64::NAN), 0.0);
    }

    #[test]
    fn slew_takes_the_shortest_arc_and_respects_the_cap() {
        // Across north: 350 → 10 is +20°, within the cap.
        assert_eq!(slew_heading_deg(350.0, 10.0, 30.0), 10.0);
        assert_eq!(slew_heading_deg(10.0, 350.0, 5.0), 5.0);
        assert_eq!(slew_heading_deg(10.0, 30.0, 0.0), 10.0);
        assert_eq!(slew_heading_deg(10.0, 30.0, f64::NAN), 10.0);
    }

    #[test]
    fn anchor_correction_is_bounded_and_tolerates_bad_input() {
        // Upstream's own edge cases: zero distance and NaN read as "no correction".
        assert_eq!(anchor_correction_step_m(0.0, 250.0, 0.1), 0.0);
        assert_eq!(anchor_correction_step_m(f64::NAN, 250.0, 0.1), 0.0);
        // A correction step never exceeds a fraction of one step's forward travel.
        assert!(anchor_correction_step_m(5_000.0, 250.0, 2.0) <= 5.5);
        // And never exceeds the remaining distance.
        assert!(anchor_correction_step_m(0.2, 250.0, 0.1) <= 0.2 + 1e-9);
    }

    #[test]
    fn advance_moves_the_expected_downrange_distance() {
        let state = CourseSpeedState {
            position: GeoPoint { lat: 0.0, lon: 0.0, alt_m: None },
            heading_deg: 90.0,
            speed_mps: 100.0,
        };
        let next = advance(&state, 60.0);
        let moved = geodesy::distance_m(&state.position, &next.position);
        assert!((moved - 6_000.0).abs() < 5.0, "moved {moved} m");
        assert_eq!(next.heading_deg, 90.0);
    }

    #[test]
    fn advance_ignores_nonsense_input() {
        let state = CourseSpeedState {
            position: GeoPoint { lat: 10.0, lon: 20.0, alt_m: None },
            heading_deg: 45.0,
            speed_mps: 0.0,
        };
        assert_eq!(advance(&state, 10.0), state);
        let negative = advance(&state, -1.0);
        assert_eq!(negative, state);
    }
}
