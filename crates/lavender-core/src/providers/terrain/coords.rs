//! Geographic coordinate parsing, validation, and canonicalization for terrain heights.

use super::error::TerrainError;

/// Number of decimal places used when formatting coordinates for Re:Earth queries and cache keys.
/// 5 decimal places corresponds to ~1.1m resolution at the equator.
pub const TERRAIN_POINT_PRECISION: usize = 5;

/// Validate a list of `[longitude, latitude]` coordinate pairs against bounds and length limits.
///
/// # Errors
/// - [`TerrainError::EmptyPoints`] if the list is empty.
/// - [`TerrainError::TooManyPoints`] if `points.len() > max_points`.
/// - [`TerrainError::InvalidCoordinate`] if any coordinate is non-finite or out of range.
pub fn validate_coordinates(points: &[[f64; 2]], max_points: usize) -> Result<(), TerrainError> {
    if points.is_empty() {
        return Err(TerrainError::EmptyPoints);
    }
    if points.len() > max_points {
        return Err(TerrainError::TooManyPoints { count: points.len(), max: max_points });
    }

    for (index, point) in points.iter().enumerate() {
        let lon = point[0];
        let lat = point[1];

        if !lon.is_finite() || !(-180.0..=180.0).contains(&lon) {
            return Err(TerrainError::InvalidCoordinate {
                index,
                reason: format!("longitude {lon} out of range [-180, 180]"),
            });
        }
        if !lat.is_finite() || !(-90.0..=90.0).contains(&lat) {
            return Err(TerrainError::InvalidCoordinate {
                index,
                reason: format!("latitude {lat} out of range [-90, 90]"),
            });
        }
    }

    Ok(())
}

/// Format a `[longitude, latitude]` pair into a canonical 5-decimal-place key string (`"lon,lat"`).
#[must_use]
pub fn canonical_key(point: [f64; 2]) -> String {
    format!(
        "{:.precision$},{:.precision$}",
        point[0],
        point[1],
        precision = TERRAIN_POINT_PRECISION
    )
}

/// Convert a point to its canonical 5-decimal-place rounded coordinates.
#[must_use]
pub fn canonical_point(point: [f64; 2]) -> [f64; 2] {
    let factor = 100_000.0;
    [(point[0] * factor).round() / factor, (point[1] * factor).round() / factor]
}

/// Parse query string format `points=lon,lat;lon,lat;...` into a list of `[lon, lat]` pairs.
///
/// # Errors
/// Returns [`TerrainError`] if format is invalid, empty, or exceeds `max_points`.
pub fn parse_terrain_points_query(
    raw: &str,
    max_points: usize,
) -> Result<Vec<[f64; 2]>, TerrainError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(TerrainError::EmptyPoints);
    }

    let mut points = Vec::new();
    for (index, segment) in trimmed.split(';').map(str::trim).filter(|s| !s.is_empty()).enumerate()
    {
        let parts: Vec<&str> = segment.split(',').map(str::trim).collect();
        if parts.len() != 2 {
            return Err(TerrainError::InvalidCoordinate {
                index,
                reason: format!("expected 'lon,lat' pair, found '{segment}'"),
            });
        }

        let lon = parts[0].parse::<f64>().map_err(|_| TerrainError::InvalidCoordinate {
            index,
            reason: format!("invalid longitude number '{}'", parts[0]),
        })?;

        let lat = parts[1].parse::<f64>().map_err(|_| TerrainError::InvalidCoordinate {
            index,
            reason: format!("invalid latitude number '{}'", parts[1]),
        })?;

        points.push([lon, lat]);
    }

    validate_coordinates(&points, max_points)?;
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_coordinates_pass_validation() {
        let pts = vec![[-97.7431, 30.2672], [139.6917, 35.6895], [0.0, 0.0]];
        assert!(validate_coordinates(&pts, 100).is_ok());
    }

    #[test]
    fn empty_points_rejected() {
        let pts: Vec<[f64; 2]> = Vec::new();
        assert_eq!(validate_coordinates(&pts, 100), Err(TerrainError::EmptyPoints));
    }

    #[test]
    fn too_many_points_rejected() {
        let pts = vec![[0.0, 0.0], [1.0, 1.0], [2.0, 2.0]];
        assert_eq!(
            validate_coordinates(&pts, 2),
            Err(TerrainError::TooManyPoints { count: 3, max: 2 })
        );
    }

    #[test]
    fn out_of_range_coordinates_rejected() {
        assert!(matches!(
            validate_coordinates(&[[185.0, 45.0]], 100),
            Err(TerrainError::InvalidCoordinate { index: 0, .. })
        ));
        assert!(matches!(
            validate_coordinates(&[[-185.0, 45.0]], 100),
            Err(TerrainError::InvalidCoordinate { index: 0, .. })
        ));
        assert!(matches!(
            validate_coordinates(&[[0.0, 95.0]], 100),
            Err(TerrainError::InvalidCoordinate { index: 0, .. })
        ));
        assert!(matches!(
            validate_coordinates(&[[0.0, -95.0]], 100),
            Err(TerrainError::InvalidCoordinate { index: 0, .. })
        ));
        assert!(matches!(
            validate_coordinates(&[[f64::NAN, 0.0]], 100),
            Err(TerrainError::InvalidCoordinate { index: 0, .. })
        ));
        assert!(matches!(
            validate_coordinates(&[[0.0, f64::INFINITY]], 100),
            Err(TerrainError::InvalidCoordinate { index: 0, .. })
        ));
    }

    #[test]
    fn canonical_key_formats_to_5_decimal_places() {
        assert_eq!(canonical_key([-97.743_123_45, 30.267_209]), "-97.74312,30.26721");
        assert_eq!(canonical_key([0.0, 0.0]), "0.00000,0.00000");
    }

    #[test]
    fn parses_query_string_semicolon_format() {
        let query = "-97.7431,30.2672; 139.6917,35.6895 ";
        let pts = parse_terrain_points_query(query, 10).unwrap();
        assert_eq!(pts.len(), 2);
        assert_eq!(pts[0], [-97.7431, 30.2672]);
        assert_eq!(pts[1], [139.6917, 35.6895]);
    }
}
