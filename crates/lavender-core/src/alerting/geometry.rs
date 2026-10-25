//! Antimeridian-aware point-in-polygon and polygon centroid calculations.

use lavender_domain::GeoPoint;

/// Tests whether a point `(lon, lat)` lies inside the given polygon ring,
/// correctly handling polygons that cross the antimeridian (180° / -180°).
#[must_use]
pub fn antimeridian_point_in_polygon(lon: f64, lat: f64, polygon: &[GeoPoint]) -> bool {
    let n = polygon.len();
    if n < 3 || !lon.is_finite() || !lat.is_finite() {
        return false;
    }

    // Check if any edge crosses the antimeridian (a longitude jump > 180 degrees)
    let crosses_antimeridian = polygon.windows(2).any(|w| (w[0].lon - w[1].lon).abs() > 180.0)
        || ((polygon[0].lon - polygon[n - 1].lon).abs() > 180.0);

    if !crosses_antimeridian {
        return standard_point_in_polygon(lon, lat, polygon);
    }

    // Unwrap vertices along longitude relative to polygon[0]
    let mut unwrapped: Vec<(f64, f64)> = Vec::with_capacity(n);
    let mut prev_lon = polygon[0].lon;
    unwrapped.push((prev_lon, polygon[0].lat));

    for pt in &polygon[1..] {
        let mut cur_lon = pt.lon;
        while cur_lon - prev_lon > 180.0 {
            cur_lon -= 360.0;
        }
        while cur_lon - prev_lon < -180.0 {
            cur_lon += 360.0;
        }
        unwrapped.push((cur_lon, pt.lat));
        prev_lon = cur_lon;
    }

    // Test candidate longitudes matching the unwrapped coordinate frame
    raycast_unwrapped(lon, lat, &unwrapped)
        || raycast_unwrapped(lon + 360.0, lat, &unwrapped)
        || raycast_unwrapped(lon - 360.0, lat, &unwrapped)
}

/// Standard raycasting (even-odd rule) point-in-polygon for non-crossing polygons.
#[must_use]
fn standard_point_in_polygon(lon: f64, lat: f64, polygon: &[GeoPoint]) -> bool {
    let n = polygon.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let xi = polygon[i].lon;
        let yi = polygon[i].lat;
        let xj = polygon[j].lon;
        let yj = polygon[j].lat;

        if ((yi > lat) != (yj > lat)) && (lon < (xj - xi) * (lat - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Raycasts a candidate longitude against unwrapped polygon vertices.
#[must_use]
fn raycast_unwrapped(lon: f64, lat: f64, unwrapped: &[(f64, f64)]) -> bool {
    let n = unwrapped.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = unwrapped[i];
        let (xj, yj) = unwrapped[j];

        if ((yi > lat) != (yj > lat)) && (lon < (xj - xi) * (lat - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Computes the geographic centroid of a polygon ring, unwrapping across the antimeridian
/// if necessary.
#[must_use]
pub fn polygon_centroid(polygon: &[GeoPoint]) -> Option<GeoPoint> {
    if polygon.is_empty() {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let count = polygon.len() as f64;

    let mut sum_lat = 0.0;
    let mut sum_lon = 0.0;
    let mut prev_lon = polygon[0].lon;

    for pt in polygon {
        let mut cur_lon = pt.lon;
        while cur_lon - prev_lon > 180.0 {
            cur_lon -= 360.0;
        }
        while cur_lon - prev_lon < -180.0 {
            cur_lon += 360.0;
        }
        sum_lat += pt.lat;
        sum_lon += cur_lon;
        prev_lon = cur_lon;
    }

    let avg_lat = (sum_lat / count).clamp(-90.0, 90.0);
    let mut avg_lon = (sum_lon / count).rem_euclid(360.0);
    if avg_lon > 180.0 {
        avg_lon -= 360.0;
    }

    GeoPoint::new(avg_lat, avg_lon).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_polygon_containment() {
        // A simple square in northern Europe [10..20°E, 50..60°N]
        let square = vec![
            GeoPoint::new(50.0, 10.0).unwrap(),
            GeoPoint::new(60.0, 10.0).unwrap(),
            GeoPoint::new(60.0, 20.0).unwrap(),
            GeoPoint::new(50.0, 20.0).unwrap(),
        ];

        // Center is inside
        assert!(antimeridian_point_in_polygon(15.0, 55.0, &square));
        // Outside west
        assert!(!antimeridian_point_in_polygon(5.0, 55.0, &square));
        // Outside east
        assert!(!antimeridian_point_in_polygon(25.0, 55.0, &square));
        // Outside north
        assert!(!antimeridian_point_in_polygon(15.0, 65.0, &square));
        // Outside south
        assert!(!antimeridian_point_in_polygon(15.0, 45.0, &square));
    }

    #[test]
    fn antimeridian_crossing_polygon() {
        // A box across the antimeridian [170°E..170°W (190°E), 0..10°N]
        let pacific_box = vec![
            GeoPoint::new(0.0, 170.0).unwrap(),
            GeoPoint::new(10.0, 170.0).unwrap(),
            GeoPoint::new(10.0, -170.0).unwrap(),
            GeoPoint::new(0.0, -170.0).unwrap(),
        ];

        // Point inside on the western side of the dateline (175°E)
        assert!(antimeridian_point_in_polygon(175.0, 5.0, &pacific_box));
        // Point inside on the eastern side of the dateline (175°W / -175°E)
        assert!(antimeridian_point_in_polygon(-175.0, 5.0, &pacific_box));
        // Exactly on the antimeridian (180° / -180°)
        assert!(antimeridian_point_in_polygon(180.0, 5.0, &pacific_box));
        assert!(antimeridian_point_in_polygon(-180.0, 5.0, &pacific_box));

        // Outside to the west (165°E)
        assert!(!antimeridian_point_in_polygon(165.0, 5.0, &pacific_box));
        // Outside to the east (-165°E)
        assert!(!antimeridian_point_in_polygon(-165.0, 5.0, &pacific_box));
        // Outside to the north (lat 15°N)
        assert!(!antimeridian_point_in_polygon(175.0, 15.0, &pacific_box));
        // Outside in the prime meridian
        assert!(!antimeridian_point_in_polygon(0.0, 5.0, &pacific_box));
    }

    #[test]
    fn centroid_antimeridian_calculation() {
        let pacific_box = vec![
            GeoPoint::new(0.0, 170.0).unwrap(),
            GeoPoint::new(10.0, 170.0).unwrap(),
            GeoPoint::new(10.0, -170.0).unwrap(),
            GeoPoint::new(0.0, -170.0).unwrap(),
        ];
        let centroid = polygon_centroid(&pacific_box).expect("valid centroid");
        assert_eq!(centroid.lat, 5.0);
        assert!((centroid.lon.abs() - 180.0).abs() < 1e-6);
    }
}
