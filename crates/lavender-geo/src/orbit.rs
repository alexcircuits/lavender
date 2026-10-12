//! SGP4 propagation of mean orbital elements to geodetic positions.
//!
//! Construction mirrors `sgp4::Constants::from_elements` (WGS84 geopotential, IAU sidereal
//! time) but starts from Lavender's [`SatelliteElements`], so callers never handle the
//! crate's `chrono` types. TEME → ECEF uses Greenwich mean sidereal time (polar motion and
//! the equation of the equinoxes are ignored: sub-kilometre, appropriate for display and
//! pass prediction at map scale).

use jiff::{SignedDuration, Timestamp};
use lavender_domain::{GeoPoint, SatelliteElements, TrackPoint};
use sgp4::{Constants, MinutesSinceEpoch, Orbit, WGS84, iau_epoch_to_sidereal_time};

/// J2000.0 (2000-01-01T12:00:00Z) as Unix seconds.
const J2000_UNIX_S: f64 = 946_728_000.0;
const SECONDS_PER_JULIAN_YEAR: f64 = 365.25 * 86_400.0;
const WGS84_A_KM: f64 = 6_378.137;
const WGS84_F: f64 = 1.0 / 298.257_223_563;

#[derive(Debug, thiserror::Error)]
pub enum OrbitError {
    #[error("orbital elements are invalid: {0}")]
    Elements(String),
    #[error("propagation failed: {0}")]
    Propagation(String),
}

/// A ready-to-use propagator for one satellite.
#[derive(Debug)]
pub struct Propagator {
    constants: Constants,
    epoch: Timestamp,
    mean_motion_rev_per_day: f64,
}

fn years_since_j2000(t: Timestamp) -> f64 {
    // Timestamps in the supported range are well inside f64's exact integer range.
    #[allow(clippy::cast_precision_loss)]
    let unix = t.as_second() as f64 + f64::from(t.subsec_nanosecond()) * 1e-9;
    (unix - J2000_UNIX_S) / SECONDS_PER_JULIAN_YEAR
}

impl Propagator {
    /// Builds the SGP4 constants for an element set.
    ///
    /// # Errors
    /// Returns an error for non-physical elements (eccentricity outside 0..1, non-positive
    /// mean motion).
    pub fn new(elements: &SatelliteElements) -> Result<Self, OrbitError> {
        let deg = std::f64::consts::PI / 180.0;
        let orbit = Orbit::from_kozai_elements(
            &WGS84,
            elements.inclination_deg * deg,
            elements.raan_deg * deg,
            elements.eccentricity,
            elements.arg_of_pericenter_deg * deg,
            elements.mean_anomaly_deg * deg,
            elements.mean_motion * (std::f64::consts::PI / 720.0),
        )
        .map_err(|e| OrbitError::Elements(format!("{e:?}")))?;
        let constants = Constants::new(
            WGS84,
            iau_epoch_to_sidereal_time,
            years_since_j2000(elements.epoch),
            elements.bstar,
            orbit,
        )
        .map_err(|e| OrbitError::Elements(format!("{e:?}")))?;
        Ok(Self { constants, epoch: elements.epoch, mean_motion_rev_per_day: elements.mean_motion })
    }

    /// Orbital period in minutes.
    #[must_use]
    pub fn period_min(&self) -> f64 {
        1_440.0 / self.mean_motion_rev_per_day
    }

    /// Position in the TEME frame (km) at `t`.
    ///
    /// # Errors
    /// Returns an error when SGP4 diverges (decayed orbit, far from epoch).
    pub fn teme_km(&self, t: Timestamp) -> Result<[f64; 3], OrbitError> {
        let minutes = t.duration_since(self.epoch).as_secs_f64() / 60.0;
        self.constants
            .propagate(MinutesSinceEpoch(minutes))
            .map(|p| p.position)
            .map_err(|e| OrbitError::Propagation(e.to_string()))
    }

    /// Sub-satellite point with altitude in metres at `t`.
    ///
    /// # Errors
    /// Returns an error when propagation fails.
    pub fn position(&self, t: Timestamp) -> Result<GeoPoint, OrbitError> {
        let [tx, ty, tz] = self.teme_km(t)?;
        let gmst = iau_epoch_to_sidereal_time(years_since_j2000(t));
        let (s, c) = gmst.sin_cos();
        let ecef = [c * tx + s * ty, -s * tx + c * ty, tz];
        Ok(ecef_to_geodetic(ecef))
    }

    /// Positions from `from` for `minutes`, every `step_s` seconds. Points where SGP4
    /// fails are skipped.
    #[must_use]
    pub fn ground_track(&self, from: Timestamp, minutes: u32, step_s: u32) -> Vec<TrackPoint> {
        let step = i64::from(step_s.max(1));
        let total = i64::from(minutes) * 60;
        (0..=total / step)
            .filter_map(|i| {
                let t = from.checked_add(SignedDuration::from_secs(i * step)).ok()?;
                self.position(t).ok().map(|position| TrackPoint { time: t, position })
            })
            .collect()
    }
}

/// ECEF (km) → WGS84 geodetic, iterating Bowring's latitude to convergence.
#[must_use]
#[allow(clippy::many_single_char_names)] // x, y, z, p, n are the standard symbols.
pub fn ecef_to_geodetic([x, y, z]: [f64; 3]) -> GeoPoint {
    let e2 = WGS84_F * (2.0 - WGS84_F);
    let lon = y.atan2(x);
    let p = x.hypot(y);
    let mut lat = z.atan2(p * (1.0 - e2));
    let mut alt_km = 0.0;
    for _ in 0..6 {
        let n = WGS84_A_KM / (1.0 - e2 * lat.sin().powi(2)).sqrt();
        // Near the poles cos(lat) → 0, so measure height along z instead.
        alt_km = if lat.cos().abs() > 1e-9 { p / lat.cos() - n } else { z.abs() - n * (1.0 - e2) };
        lat = z.atan2(p * (1.0 - e2 * n / (n + alt_km)));
    }
    GeoPoint { lat: lat.to_degrees(), lon: lon.to_degrees(), alt_m: Some(alt_km * 1_000.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ISS (ZARYA) element set, epoch 2024-03-08T12:00:00Z (illustrative but physical).
    const ISS_TLE: (&str, &str) = (
        "1 25544U 98067A   24068.50000000  .00016717  00000-0  30049-3 0  9996",
        "2 25544  51.6416 247.4627 0006703 130.5360 325.0288 15.50377579443931",
    );

    fn elements_from_tle() -> (SatelliteElements, sgp4::Elements) {
        let tle = sgp4::Elements::from_tle(
            Some("ISS (ZARYA)".to_owned()),
            ISS_TLE.0.as_bytes(),
            ISS_TLE.1.as_bytes(),
        )
        .unwrap();
        let epoch: Timestamp =
            format!("{}Z", tle.datetime.format("%Y-%m-%dT%H:%M:%S%.f")).parse().unwrap();
        let ours = SatelliteElements {
            norad_id: 25544,
            name: "ISS (ZARYA)".into(),
            intl_designator: tle.international_designator.clone(),
            group: "stations".into(),
            epoch,
            mean_motion: tle.mean_motion,
            eccentricity: tle.eccentricity,
            inclination_deg: tle.inclination,
            raan_deg: tle.right_ascension,
            arg_of_pericenter_deg: tle.argument_of_perigee,
            mean_anomaly_deg: tle.mean_anomaly,
            bstar: tle.drag_term,
            mean_motion_dot: tle.mean_motion_dot,
            mean_motion_ddot: tle.mean_motion_ddot,
            rev_at_epoch: None,
            classification: None,
        };
        (ours, tle)
    }

    #[test]
    fn matches_the_crates_own_construction() {
        let (ours, tle) = elements_from_tle();
        let reference = Constants::from_elements(&tle).unwrap();
        let prop = Propagator::new(&ours).unwrap();
        for minutes in [0.0, 30.0, 92.9, 600.0] {
            let expected = reference.propagate(MinutesSinceEpoch(minutes)).unwrap().position;
            let t = ours.epoch.checked_add(SignedDuration::from_secs_f64(minutes * 60.0)).unwrap();
            let got = prop.teme_km(t).unwrap();
            for i in 0..3 {
                assert!((got[i] - expected[i]).abs() < 1e-6, "{minutes}: {got:?} vs {expected:?}");
            }
        }
    }

    #[test]
    fn iss_altitude_and_latitude_are_plausible() {
        let (ours, _) = elements_from_tle();
        let prop = Propagator::new(&ours).unwrap();
        assert!((prop.period_min() - 92.88).abs() < 0.1, "{}", prop.period_min());
        for p in prop.ground_track(ours.epoch, 95, 60) {
            let alt_km = p.position.alt_m.unwrap() / 1_000.0;
            assert!((380.0..460.0).contains(&alt_km), "alt {alt_km}");
            // Geodetic latitude exceeds the inclination by up to ~0.19° (vs geocentric).
            assert!(p.position.lat.abs() <= 51.9, "lat {}", p.position.lat);
            assert!(p.position.lon.abs() <= 180.0);
        }
    }

    #[test]
    fn ecef_axes() {
        let p = ecef_to_geodetic([WGS84_A_KM, 0.0, 0.0]);
        assert!(p.lat.abs() < 1e-9 && p.lon.abs() < 1e-9 && p.alt_m.unwrap().abs() < 1e-3);
        let north_pole = ecef_to_geodetic([0.0, 0.0, 6_356.752_314_245]);
        assert!((north_pole.lat - 90.0).abs() < 1e-6);
    }

    #[test]
    fn rejects_hyperbolic_orbit() {
        let (mut ours, _) = elements_from_tle();
        ours.eccentricity = 1.2;
        assert!(Propagator::new(&ours).is_err());
    }
}
