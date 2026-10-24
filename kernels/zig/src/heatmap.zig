//! Density Heatmap Binning Micro-Kernel
//!
//! Projects weighted lat/lon points onto a fixed equirectangular grid and
//! accumulates two f32 planes — weighted sum and sample count — for density
//! rendering. Zero allocation: callers provide the output planes.

const std = @import("std");

/// Bins weighted geographic points into an equirectangular `width` x `height` grid.
///
/// `points` is interleaved [lon0, lat0, weight0, lon1, lat1, weight1, ...].
/// Row 0 of the output planes corresponds to `lat_max` (north at the top, ready
/// for direct texture upload); column 0 corresponds to `lon_min`.
/// Points outside the bounds, or with non-finite coordinates/weights, are skipped.
/// Accumulation is additive so multiple batches can feed the same grid.
///
/// Returns the number of points binned.
pub fn binEquirect(
    points: []const f64,
    num_points: usize,
    width: usize,
    height: usize,
    lon_min: f64,
    lon_max: f64,
    lat_min: f64,
    lat_max: f64,
    out_weight_sum: []f32,
    out_count: []f32,
) usize {
    if (num_points == 0 or width == 0 or height == 0) return 0;
    if (points.len < num_points * 3) return 0;
    const cells = width * height;
    if (out_weight_sum.len < cells or out_count.len < cells) return 0;

    const lon_span = lon_max - lon_min;
    const lat_span = lat_max - lat_min;
    if (!std.math.isFinite(lon_span) or !std.math.isFinite(lat_span)) return 0;
    if (lon_span <= 0.0 or lat_span <= 0.0) return 0;

    var binned: usize = 0;

    for (0..num_points) |i| {
        const lon = points[i * 3];
        const lat = points[i * 3 + 1];
        const weight = points[i * 3 + 2];

        if (!std.math.isFinite(lon) or !std.math.isFinite(lat) or !std.math.isFinite(weight)) {
            continue;
        }
        if (lon < lon_min or lon > lon_max or lat < lat_min or lat > lat_max) {
            continue;
        }

        const col_f = (lon - lon_min) / lon_span * @as(f64, @floatFromInt(width));
        const row_f = (lat_max - lat) / lat_span * @as(f64, @floatFromInt(height));
        // Clamp guards against rounding pushing exact-boundary points out of range.
        const col = @min(@as(usize, @intFromFloat(col_f)), width - 1);
        const row = @min(@as(usize, @intFromFloat(row_f)), height - 1);

        const cell = row * width + col;
        out_weight_sum[cell] += @floatCast(weight);
        out_count[cell] += 1.0;
        binned += 1;
    }

    return binned;
}

/// Convenience wrapper binning the full equirectangular extent
/// (lon -180..180, lat -90..90).
pub fn binWorld(
    points: []const f64,
    num_points: usize,
    width: usize,
    height: usize,
    out_weight_sum: []f32,
    out_count: []f32,
) usize {
    return binEquirect(
        points,
        num_points,
        width,
        height,
        -180.0,
        180.0,
        -90.0,
        90.0,
        out_weight_sum,
        out_count,
    );
}
