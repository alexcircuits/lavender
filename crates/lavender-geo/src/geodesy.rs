//! Great-circle geodesy on a spherical Earth (mean radius). Accurate to ~0.5 %, which is
//! appropriate for distances, bearings and search radii; MGRS/UTM use the ellipsoid.

use lavender_domain::GeoPoint;

/// IUGG mean Earth radius in metres.
pub const EARTH_RADIUS_M: f64 = 6_371_008.8;
/// Metres per international nautical mile.
pub const METRES_PER_NM: f64 = 1_852.0;

/// Great-circle distance in metres (haversine).
#[must_use]
pub fn distance_m(a: &GeoPoint, b: &GeoPoint) -> f64 {
    let (lat1, lat2) = (a.lat.to_radians(), b.lat.to_radians());
    let dlat = lat2 - lat1;
    let dlon = (b.lon - a.lon).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().min(1.0).asin()
}

/// Initial bearing from `a` to `b`, degrees clockwise from true north (0..360).
#[must_use]
pub fn initial_bearing_deg(a: &GeoPoint, b: &GeoPoint) -> f64 {
    let (lat1, lat2) = (a.lat.to_radians(), b.lat.to_radians());
    let dlon = (b.lon - a.lon).to_radians();
    let y = dlon.sin() * lat2.cos();
    let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

/// The point reached from `start` after `distance_m` metres on `bearing_deg`.
#[must_use]
pub fn destination(start: &GeoPoint, bearing_deg: f64, distance_m: f64) -> GeoPoint {
    let delta = distance_m / EARTH_RADIUS_M;
    let theta = bearing_deg.to_radians();
    let lat1 = start.lat.to_radians();
    let lon1 = start.lon.to_radians();
    let lat2 = (lat1.sin() * delta.cos() + lat1.cos() * delta.sin() * theta.cos()).asin();
    let lon2 = lon1
        + (theta.sin() * delta.sin() * lat1.cos()).atan2(delta.cos() - lat1.sin() * lat2.sin());
    let lon = ((lon2.to_degrees() + 540.0) % 360.0) - 180.0;
    GeoPoint { lat: lat2.to_degrees(), lon, alt_m: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    #[test]
    fn helsinki_to_tallinn() {
        let d = distance_m(&p(60.1699, 24.9384), &p(59.4370, 24.7536));
        assert!((d - 82_147.0).abs() < 200.0, "{d}");
    }

    #[test]
    fn quarter_meridian() {
        let d = distance_m(&p(0.0, 0.0), &p(90.0, 0.0));
        assert!((d - std::f64::consts::FRAC_PI_2 * EARTH_RADIUS_M).abs() < 1.0);
    }

    #[test]
    fn bearings() {
        assert!((initial_bearing_deg(&p(0.0, 0.0), &p(10.0, 0.0)) - 0.0).abs() < 1e-9);
        assert!((initial_bearing_deg(&p(0.0, 0.0), &p(0.0, 10.0)) - 90.0).abs() < 1e-9);
        assert!((initial_bearing_deg(&p(0.0, 0.0), &p(0.0, -10.0)) - 270.0).abs() < 1e-9);
    }

    #[test]
    fn destination_round_trip() {
        let start = p(51.5, -0.12);
        let end = destination(&start, 60.0, 250_000.0);
        assert!((distance_m(&start, &end) - 250_000.0).abs() < 1.0);
        assert!((initial_bearing_deg(&start, &end) - 60.0).abs() < 1e-6);
    }

    #[test]
    fn destination_wraps_longitude() {
        let end = destination(&p(0.0, 179.5), 90.0, 200_000.0);
        assert!(end.lon < -178.0 && end.lon > -180.0, "{}", end.lon);
    }
}
