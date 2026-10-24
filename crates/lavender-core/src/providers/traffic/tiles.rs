//! Pure tile mathematics and slippy map coordinate validation for traffic flow tiles.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::doc_markdown,
    clippy::must_use_candidate
)]

use std::f64::consts::PI;

/// Minimum supported TomTom flow-tile zoom level.
pub const MIN_TILE_ZOOM: u32 = 8;

/// Maximum supported TomTom flow-tile zoom level.
pub const MAX_TILE_ZOOM: u32 = 16;

/// Web Mercator latitude limit in degrees.
pub const MERCATOR_LAT_LIMIT: f64 = 85.051_128_78;

/// Validate a `(z, x, y)` tile coordinate.
#[must_use]
pub fn is_valid_tile_coord(z: u32, x: u32, y: u32, min_zoom: u32, max_zoom: u32) -> bool {
    if z < min_zoom || z > max_zoom || min_zoom > max_zoom || max_zoom > 30 {
        return false;
    }
    let n = 1u64 << z;
    (u64::from(x) < n) && (u64::from(y) < n)
}

/// Convert longitude/latitude in degrees to containing tile column and row at zoom `z`.
#[must_use]
pub fn lon_lat_to_tile(lon: f64, lat: f64, z: u32) -> (u32, u32) {
    let n = 2.0_f64.powi(z as i32);
    let clamped_lat = lat.clamp(-MERCATOR_LAT_LIMIT, MERCATOR_LAT_LIMIT);
    let lat_rad = clamped_lat.to_radians();

    let x_raw = ((lon + 180.0) / 360.0) * n;
    let y_raw = ((1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / PI) / 2.0) * n;

    let max_idx = (n as u64).saturating_sub(1);
    let x = (x_raw.floor() as i64).clamp(0, max_idx as i64) as u32;
    let y = (y_raw.floor() as i64).clamp(0, max_idx as i64) as u32;

    (x, y)
}

/// Compute the geographic bounding box of a tile in degrees: `(west, south, east, north)`.
#[must_use]
pub fn tile_to_bbox(z: u32, x: u32, y: u32) -> (f64, f64, f64, f64) {
    let n = 2.0_f64.powi(z as i32);

    let lon_at = |col: f64| (col / n) * 360.0 - 180.0;
    let lat_at = |row: f64| {
        let sinh_val = (PI * (1.0 - (2.0 * row) / n)).sinh();
        sinh_val.atan().to_degrees()
    };

    let west = lon_at(f64::from(x));
    let east = lon_at(f64::from(x + 1));
    let north = lat_at(f64::from(y));
    let south = lat_at(f64::from(y + 1));

    (west, south, east, north)
}

/// Returns the tiles covering a bounding box at zoom level `zoom`, up to `max_tiles`.
#[must_use]
pub fn tiles_for_bounds(
    west: f64,
    south: f64,
    east: f64,
    north: f64,
    zoom: u32,
    max_tiles: usize,
) -> Vec<(u32, u32, u32)> {
    if !west.is_finite() || !south.is_finite() || !east.is_finite() || !north.is_finite() {
        return Vec::new();
    }

    let min_lon = west.min(east);
    let max_lon = west.max(east);
    let min_lat = south.min(north);
    let max_lat = south.max(north);

    let (nw_x, nw_y) = lon_lat_to_tile(min_lon, max_lat, zoom);
    let (se_x, se_y) = lon_lat_to_tile(max_lon, min_lat, zoom);

    let mut tiles = Vec::new();
    for y in nw_y..=se_y {
        for x in nw_x..=se_x {
            if tiles.len() >= max_tiles {
                return tiles;
            }
            tiles.push((zoom, x, y));
        }
    }

    tiles
}

/// Produce a UTC calendar day string (`"YYYY-MM-DD"`) for budget bucketing.
#[must_use]
pub fn utc_day_key(epoch_s: i64) -> String {
    let dt = jiff::Timestamp::from_second(epoch_s)
        .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
        .to_zoned(jiff::tz::TimeZone::UTC);
    format!("{:04}-{:02}-{:02}", dt.year(), dt.month(), dt.day())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_tile_coord() {
        assert!(is_valid_tile_coord(12, 935, 1686, 8, 16));
        assert!(!is_valid_tile_coord(7, 935, 1686, 8, 16));
        assert!(!is_valid_tile_coord(17, 935, 1686, 8, 16));
        assert!(!is_valid_tile_coord(12, 4096, 1686, 8, 16));
        assert!(!is_valid_tile_coord(12, 935, 4096, 8, 16));
    }

    #[test]
    fn test_lon_lat_to_tile_austin() {
        // Austin, TX: ~ -97.7431, 30.2672 at zoom 12
        let (x, y) = lon_lat_to_tile(-97.7431, 30.2672, 12);
        assert_eq!(x, 935);
        assert_eq!(y, 1686);
    }

    #[test]
    fn test_tile_to_bbox() {
        let (west, south, east, north) = tile_to_bbox(12, 935, 1686);
        assert!(west < east);
        assert!(south < north);
        assert!(-97.9 < west && west < -97.8);
        assert!(30.2 < north && north < 30.3);
    }

    #[test]
    fn test_utc_day_key() {
        // 1774000000 -> 2026-03-20
        let key = utc_day_key(1_774_000_000);
        assert_eq!(key, "2026-03-20");
    }
}
