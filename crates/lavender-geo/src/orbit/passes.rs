//! WGS84 observer geometry and bounded threshold-crossing/culmination search.

use super::{LookAngles, OrbitError, WGS84_A_KM, WGS84_F};
use jiff::{SignedDuration, Timestamp};
use lavender_domain::{GeoPoint, SatellitePass};

pub const MAX_PASS_COARSE_SAMPLES: usize = 8_641;
pub const MAX_PASS_REFINEMENT_SAMPLES: usize = 64 * 50;
pub const MAX_SATELLITE_PASSES: usize = 64;
const COARSE_STEP_S: u32 = 10;
const BOUNDARY_TOLERANCE_S: f64 = 0.1;
const PEAK_TOLERANCE_S: f64 = 0.05;

#[derive(Debug)]
pub(super) struct ObserverFrame {
    position: [f64; 3],
    sin_lat: f64,
    cos_lat: f64,
    sin_lon: f64,
    cos_lon: f64,
}

impl ObserverFrame {
    pub(super) fn new(observer: GeoPoint) -> Result<Self, OrbitError> {
        GeoPoint::new(observer.lat, observer.lon)
            .and_then(|p| p.with_altitude(observer.alt_m))
            .map_err(|e| OrbitError::InvalidRequest(e.to_string()))?;
        let altitude = observer.alt_m.unwrap_or(0.0);
        if !(-500.0..=10_000.0).contains(&altitude) {
            return Err(OrbitError::InvalidRequest(
                "Observer altitude must be -500–10000 metres.".to_owned(),
            ));
        }
        let (sin_lat, cos_lat) = observer.lat.to_radians().sin_cos();
        let (sin_lon, cos_lon) = observer.lon.to_radians().sin_cos();
        let e2 = WGS84_F * (2.0 - WGS84_F);
        let normal = WGS84_A_KM / (1.0 - e2 * sin_lat * sin_lat).sqrt();
        let altitude_km = altitude / 1_000.0;
        Ok(Self {
            position: [
                (normal + altitude_km) * cos_lat * cos_lon,
                (normal + altitude_km) * cos_lat * sin_lon,
                (normal * (1.0 - e2) + altitude_km) * sin_lat,
            ],
            sin_lat,
            cos_lat,
            sin_lon,
            cos_lon,
        })
    }

    pub(super) fn look(&self, satellite: [f64; 3]) -> Result<LookAngles, OrbitError> {
        let [dx, dy, dz] = std::array::from_fn(|i| satellite[i] - self.position[i]);
        let east = -self.sin_lon * dx + self.cos_lon * dy;
        let north = -self.sin_lat * (self.cos_lon * dx + self.sin_lon * dy) + self.cos_lat * dz;
        let up = self.cos_lat * (self.cos_lon * dx + self.sin_lon * dy) + self.sin_lat * dz;
        let horizontal = east.hypot(north);
        let range_km = horizontal.hypot(up);
        if !range_km.is_finite() || range_km <= 0.0 {
            return Err(OrbitError::Propagation(
                "Observer range is not finite and positive.".into(),
            ));
        }
        Ok(LookAngles {
            elevation_deg: up.atan2(horizontal).to_degrees(),
            azimuth_deg: east.atan2(north).to_degrees().rem_euclid(360.0),
            range_km,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct Sample {
    seconds: f64,
    elevation: f64,
}

/// The hard refinement limit also covers rejected local maxima, preventing high-frequency
/// or pathological input from doing unlimited work when the output happens to be empty.
struct Refinement<'a, F> {
    from: Timestamp,
    remaining: usize,
    elevation: &'a mut F,
}

impl<F: FnMut(Timestamp) -> Result<f64, OrbitError>> Refinement<'_, F> {
    fn sample(&mut self, seconds: f64) -> Result<Sample, OrbitError> {
        self.remaining = self.remaining.checked_sub(1).ok_or_else(|| {
            OrbitError::Limit("Pass refinement exceeds 3200 additional samples.".into())
        })?;
        let elevation = (self.elevation)(offset(self.from, seconds)?)?;
        finite_elevation(elevation)?;
        Ok(Sample { seconds, elevation })
    }

    fn peak(&mut self, left: Sample, center: Sample, right: Sample) -> Result<Sample, OrbitError> {
        const RATIO: f64 = 0.618_033_988_749_894_9;
        let mut best = [left, center, right]
            .into_iter()
            .max_by(|a, b| a.elevation.total_cmp(&b.elevation))
            .ok_or_else(|| OrbitError::Propagation("Peak bracket is empty.".into()))?;
        let mut low = left.seconds;
        let mut high = right.seconds;
        let mut a = self.sample(high - RATIO * (high - low))?;
        let mut b = self.sample(low + RATIO * (high - low))?;
        while high - low > PEAK_TOLERANCE_S {
            for point in [a, b] {
                if point.elevation > best.elevation {
                    best = point;
                }
            }
            if a.elevation > b.elevation {
                high = b.seconds;
                b = a;
                a = self.sample(high - RATIO * (high - low))?;
            } else {
                low = a.seconds;
                a = b;
                b = self.sample(low + RATIO * (high - low))?;
            }
        }
        for point in [a, b] {
            if point.elevation > best.elevation {
                best = point;
            }
        }
        Ok(best)
    }
}

pub(super) fn predict(
    from: Timestamp,
    hours: u32,
    min_elevation_deg: f64,
    mut elevation: impl FnMut(Timestamp) -> Result<f64, OrbitError>,
) -> Result<Vec<SatellitePass>, OrbitError> {
    if !(1..=24).contains(&hours) || !(0.0..=89.0).contains(&min_elevation_deg) {
        return Err(OrbitError::InvalidRequest(
            "hours must be 1–24 and minElevationDeg 0–89.".into(),
        ));
    }
    let duration_s = hours * 3_600;
    offset(from, f64::from(duration_s))?;
    let count = duration_s / COARSE_STEP_S + 1;
    let mut samples = Vec::with_capacity(usize::try_from(count).unwrap_or(MAX_PASS_COARSE_SAMPLES));
    for step in 0..count {
        let seconds = f64::from(step * COARSE_STEP_S);
        let value = elevation(offset(from, seconds)?)?;
        finite_elevation(value)?;
        samples.push(Sample { seconds, elevation: value });
    }
    let mut refinement =
        Refinement { from, remaining: MAX_PASS_REFINEMENT_SAMPLES, elevation: &mut elevation };
    let mut passes: Vec<SatellitePass> = Vec::new();
    for i in 0..samples.len() {
        let is_peak = if i == 0 {
            samples[i].elevation >= samples[i + 1].elevation
        } else if i == samples.len() - 1 {
            samples[i].elevation > samples[i - 1].elevation
        } else {
            samples[i].elevation > samples[i - 1].elevation
                && samples[i].elevation >= samples[i + 1].elevation
        };
        if !is_peak {
            continue;
        }
        let peak = refinement.peak(
            samples[i.saturating_sub(1)],
            samples[i],
            samples[(i + 1).min(samples.len() - 1)],
        )?;
        if peak.elevation < min_elevation_deg {
            continue;
        }
        let before = samples
            .iter()
            .rposition(|s| s.seconds < peak.seconds && s.elevation < min_elevation_deg);
        let after = samples
            .iter()
            .position(|s| s.seconds > peak.seconds && s.elevation < min_elevation_deg);
        let rise = match before {
            Some(index) => {
                let above = samples[index + 1];
                let above = if above.seconds < peak.seconds { above } else { peak };
                crossing(&mut refinement, samples[index], above, min_elevation_deg)?
            }
            None => 0.0,
        };
        let set = match after {
            Some(index) => {
                let above = samples[index - 1];
                let above = if above.seconds > peak.seconds { above } else { peak };
                crossing(&mut refinement, samples[index], above, min_elevation_deg)?
            }
            None => f64::from(duration_s),
        };
        let pass = SatellitePass {
            rise: offset(from, rise)?,
            peak: offset(from, peak.seconds)?,
            set: offset(from, set)?,
            max_elevation_deg: peak.elevation,
            already_above: before.is_none(),
            continues_after_window: after.is_none(),
        };
        if let Some(previous) = passes.last_mut()
            && pass.rise <= previous.set
        {
            previous.set = previous.set.max(pass.set);
            previous.continues_after_window |= pass.continues_after_window;
            if pass.max_elevation_deg > previous.max_elevation_deg {
                previous.peak = pass.peak;
                previous.max_elevation_deg = pass.max_elevation_deg;
            }
            continue;
        }
        if passes.len() == MAX_SATELLITE_PASSES {
            return Err(OrbitError::Limit("More than 64 passes occur in this window.".into()));
        }
        passes.push(pass);
    }
    Ok(passes)
}

fn crossing<F: FnMut(Timestamp) -> Result<f64, OrbitError>>(
    refinement: &mut Refinement<'_, F>,
    mut below: Sample,
    mut above: Sample,
    threshold: f64,
) -> Result<f64, OrbitError> {
    while (above.seconds - below.seconds).abs() > BOUNDARY_TOLERANCE_S {
        let middle = refinement.sample(above.seconds.midpoint(below.seconds))?;
        if middle.elevation >= threshold {
            above = middle;
        } else {
            below = middle;
        }
    }
    Ok(above.seconds.midpoint(below.seconds))
}

fn offset(from: Timestamp, seconds: f64) -> Result<Timestamp, OrbitError> {
    let delta = SignedDuration::try_from_secs_f64(seconds)
        .map_err(|e| OrbitError::InvalidRequest(e.to_string()))?;
    from.checked_add(delta).map_err(|e| OrbitError::InvalidRequest(e.to_string()))
}

fn finite_elevation(value: f64) -> Result<(), OrbitError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(OrbitError::Propagation("Elevation is not finite.".into()))
    }
}

#[cfg(test)]
mod tests;
