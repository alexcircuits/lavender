//! Resampling decoded fields to the frontend's grid and packing the served `.bin` body.
//!
//! # Binary grid format (`GET /api/v1/wind/grid/{id}.bin`, desktop `wind_grid`)
//!
//! The body is a sequence of IEEE-754 binary32 values, **little-endian**, with no header:
//! the U block, then the V block, then the scalar block when the manifest has `scalar`.
//! Each block holds `ny` rows of `nx` values (row-major). Row 0 is latitude `la1` (90° N)
//! and rows step south by `dy` to 90° S; column 0 is longitude `lo1` (0° E) and columns
//! step east by `dx`. Value `(row, column)` of block `b` starts at byte
//! `4 × (b × nx × ny + row × nx + column)`. U and V are m/s (eastward / northward);
//! the scalar is °C (2 m temperature) or hPa (mean sea level pressure).

use lavender_domain::WeatherOverlay;

use super::grib::DecodedMessage;
use crate::providers::ProviderError;

/// Largest resampled grid (points per block): the global 0.25° grid, 1440 × 721, which is
/// the finest spacing `providers.wind.target_dx` accepts. At that spacing one served grid
/// with a scalar is about 12.5 MB (the default 1° grid: about 0.8 MB).
pub const MAX_OUTPUT_POINTS: usize = 1440 * 721;

/// A resampled forecast: U, V and an optional scalar on one equirectangular grid.
#[derive(Debug, Clone)]
pub struct ResampledGrid {
    pub nx: usize,
    pub ny: usize,
    pub lo1: f64,
    pub la1: f64,
    pub dx: f64,
    pub dy: f64,
    pub u: Vec<f32>,
    pub v: Vec<f32>,
    pub scalar: Option<Vec<f32>>,
}

fn same_geometry(a: &DecodedMessage, b: &DecodedMessage) -> bool {
    a.ni == b.ni
        && a.nj == b.nj
        && (a.lo1 - b.lo1).abs() <= 1e-4
        && (a.la1 - b.la1).abs() <= 1e-4
        && (a.di - b.di).abs() <= 1e-4
        && (a.dj - b.dj).abs() <= 1e-4
}

fn invalid(reason: &str) -> ProviderError {
    ProviderError::Parse { source_name: "wind", reason: reason.to_owned() }
}

/// Resample GRIB2 fields to equirectangular grid with bilinear interpolation.
///
/// Longitudes wrap around, so the source must cover all 360° of longitude; rows must run
/// north to south (as [`super::decode_grib_message`] guarantees).
///
/// # Errors
/// Returns [`ProviderError::Parse`] if grid geometries mismatch, the source is not global
/// in longitude, or point budgets are exceeded.
// All dimensions/indices are bounded below by nonzero grid dimensions and above by the
// input/output point budgets. Interpolation fractions are in [0, 1]; narrowing them to f32
// matches the decoded field precision.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
pub fn resample_wind_grid(
    u: &DecodedMessage,
    v: &DecodedMessage,
    scalar: Option<&DecodedMessage>,
    overlay: WeatherOverlay,
    dx: f64,
    dy: f64,
) -> Result<ResampledGrid, ProviderError> {
    if !same_geometry(u, v) {
        return Err(invalid("mismatched U and V grid geometry"));
    }
    if scalar.is_some_and(|s| !same_geometry(u, s)) {
        return Err(invalid("mismatched scalar grid geometry"));
    }
    let source_count = u.ni.saturating_mul(u.nj);
    if u.ni == 0
        || u.nj == 0
        || source_count > super::MAX_GRID_POINTS
        || u.values.len() != source_count
        || v.values.len() != source_count
        || scalar.is_some_and(|field| field.values.len() != source_count)
        || ![u.lo1, u.la1, u.di, u.dj, dx, dy].iter().all(|value| value.is_finite())
        || u.di <= 0.0
        || u.dj <= 0.0
        || !(0.25..=180.0).contains(&dx)
        || !(0.25..=180.0).contains(&dy)
    {
        return Err(invalid("invalid grid geometry"));
    }
    if ((u.ni as f64) * u.di - 360.0).abs() > u.di / 2.0 {
        return Err(invalid("source grid does not cover 360° of longitude"));
    }
    let nx = (360.0 / dx).round() as usize;
    let ny = ((180.0 / dy).round() as usize) + 1;
    let total = nx * ny;
    if total > MAX_OUTPUT_POINTS {
        return Err(invalid("resampled wind grid exceeds point budget"));
    }

    let sample = |source: &[f32], lon: f64, lat: f64| -> f32 {
        let x_coord = ((lon - u.lo1) / u.di).rem_euclid(u.ni as f64);
        let y_coord = ((u.la1 - lat) / u.dj).clamp(0.0, (u.nj - 1) as f64);
        let x0 = (x_coord.floor() as usize).min(u.ni - 1);
        let x1 = (x0 + 1) % u.ni;
        let y0 = y_coord.floor() as usize;
        let y1 = (y0 + 1).min(u.nj - 1);
        let fx = (x_coord - (x0 as f64)) as f32;
        let fy = (y_coord - (y0 as f64)) as f32;
        let top = source[y0 * u.ni + x0] * (1.0 - fx) + source[y0 * u.ni + x1] * fx;
        let bottom = source[y1 * u.ni + x0] * (1.0 - fx) + source[y1 * u.ni + x1] * fx;
        top * (1.0 - fy) + bottom * fy
    };

    let (scalar_scale, scalar_offset) = match overlay {
        WeatherOverlay::Temperature => (1.0f32, -273.15f32),
        WeatherOverlay::Pressure => (0.01f32, 0.0f32),
        WeatherOverlay::None => (1.0f32, 0.0f32),
    };
    let points = || {
        (0..ny).flat_map(move |y| {
            let lat = 90.0 - (y as f64) * dy;
            (0..nx).map(move |x| ((x as f64) * dx, lat))
        })
    };
    let out_u = points().map(|(lon, lat)| sample(&u.values, lon, lat)).collect();
    let out_v = points().map(|(lon, lat)| sample(&v.values, lon, lat)).collect();
    let out_scalar = scalar.map(|s| {
        points()
            .map(|(lon, lat)| sample(&s.values, lon, lat) * scalar_scale + scalar_offset)
            .collect()
    });

    Ok(ResampledGrid {
        nx,
        ny,
        lo1: 0.0,
        la1: 90.0,
        dx,
        dy,
        u: out_u,
        v: out_v,
        scalar: out_scalar,
    })
}

/// Packs the grid as documented in the module docs: little-endian f32, U then V then the
/// optional scalar.
#[must_use]
pub fn pack_grid_binary(grid: &ResampledGrid) -> Vec<u8> {
    let blocks = if grid.scalar.is_some() { 3 } else { 2 };
    let mut bytes = Vec::with_capacity(grid.nx * grid.ny * 4 * blocks);
    let scalar = grid.scalar.iter().flatten();
    for value in grid.u.iter().chain(&grid.v).chain(scalar) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}
