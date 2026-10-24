//! GRIB2 Wind Grid Resampling and Streamline Integrator
//!
//! Provides ultra-fast bilinear wind field interpolation, Runge-Kutta 2nd-order (midpoint)
//! streamline integration, and WebAssembly C-ABI exports for geographic weather visualizations.

const std = @import("std");

/// One degree of latitude in metres (WGS-84 / spherical Earth approximation).
pub const DEGREE_METERS: f32 = 111320.0;

/// Default step limits from Lavender streamline specification.
pub const WIND_PATH_STEPS: usize = 16;
pub const WIND_PATH_LIMIT: usize = 7200;

/// GRIB2 equirectangular wind grid descriptor.
/// Source grids are north-to-south (`la1` at top, `dj` increasing southward)
/// and start at `lo1`.
pub const WindGrid = struct {
    ni: usize,
    nj: usize,
    lo1: f32,
    la1: f32,
    di: f32,
    dj: f32,
    u: []const f32,
    v: []const f32,
};

/// Normalizes a longitude to the half-open world interval [-180, 180).
pub fn normalizeLongitude(lon: f32) f32 {
    const norm = @mod(lon + 180.0, 360.0) - 180.0;
    return if (norm >= 180.0) -180.0 else norm;
}

/// Metres represented by one longitude degree at a given latitude.
pub fn metersPerDegreeLon(lat: f32) f32 {
    const lat_rad = (lat * std.math.pi) / 180.0;
    return DEGREE_METERS * @cos(lat_rad);
}

/// Bilinear interpolation of U and V wind vectors across grid bounds with longitude seam wrapping.
/// Longitude wraps circularly; latitude clamps to the pole edges.
pub fn sampleWind(grid: WindGrid, lon: f32, lat: f32) [2]f32 {
    if (!std.math.isFinite(lon) or !std.math.isFinite(lat)) {
        return .{ 0.0, 0.0 };
    }
    if (grid.ni == 0 or grid.nj == 0 or grid.di <= 0.0 or grid.dj <= 0.0) {
        return .{ 0.0, 0.0 };
    }
    if (grid.u.len < grid.ni * grid.nj or grid.v.len < grid.ni * grid.nj) {
        return .{ 0.0, 0.0 };
    }

    const norm_lon_diff = normalizeLongitude(lon - grid.lo1);
    var lon_offset = @mod(norm_lon_diff + 360.0, 360.0);
    if (lon_offset >= 360.0) lon_offset = 0.0;

    var x = lon_offset / grid.di;
    const ni_f: f32 = @floatFromInt(grid.ni);
    if (x >= ni_f) x = 0.0;
    const x0 = @floor(x);
    const tx = x - x0;

    const y = (grid.la1 - lat) / grid.dj;
    const nj_f: f32 = @floatFromInt(grid.nj - 1);
    const y_clamped = std.math.clamp(y, 0.0, nj_f);
    const y0_f = @floor(y_clamped);
    const ty = y_clamped - y0_f;

    const left: usize = @intFromFloat(x0);
    const right: usize = (left + 1) % grid.ni;

    const y0: usize = @intFromFloat(y0_f);
    const y1: usize = @min(y0 + 1, grid.nj - 1);

    const a = y0 * grid.ni + left;
    const b = y0 * grid.ni + right;
    const c = y1 * grid.ni + left;
    const d = y1 * grid.ni + right;

    const top_u = grid.u[a] * (1.0 - tx) + grid.u[b] * tx;
    const bottom_u = grid.u[c] * (1.0 - tx) + grid.u[d] * tx;
    const u = top_u * (1.0 - ty) + bottom_u * ty;

    const top_v = grid.v[a] * (1.0 - tx) + grid.v[b] * tx;
    const bottom_v = grid.v[c] * (1.0 - tx) + grid.v[d] * tx;
    const v = top_v * (1.0 - ty) + bottom_v * ty;

    return .{ u, v };
}

/// Runge-Kutta 2nd-order midpoint integration step following wind direction.
/// Returns null if particle hits calm air (<0.15 m/s), invalid coordinates,
/// polar limits (|lat| > 88.5°), or crosses the longitude seam (|Δlon| > 180°).
pub fn advance(grid: WindGrid, point: [2]f32, seconds: f32) ?[2]f32 {
    if (!std.math.isFinite(point[0]) or !std.math.isFinite(point[1])) return null;
    if (@abs(point[1]) > 88.5) return null;

    const sample0 = sampleWind(grid, point[0], point[1]);
    const u_start = sample0[0];
    const v_start = sample0[1];

    if (!std.math.isFinite(u_start) or !std.math.isFinite(v_start)) return null;
    const speed0 = std.math.hypot(u_start, v_start);
    if (speed0 < 0.15) return null;

    const m_per_deg_lon0 = metersPerDegreeLon(point[1]);
    if (@abs(m_per_deg_lon0) < 1e-6) return null;

    const lon_rate = u_start / m_per_deg_lon0;
    const lat_rate = v_start / DEGREE_METERS;

    const sign_sec: f32 = if (seconds > 0.0) 1.0 else if (seconds < 0.0) -1.0 else 0.0;
    const abs_sec = @abs(seconds);
    const max_rate = @max(@max(@abs(lon_rate), @abs(lat_rate)), 1e-12);
    const dt = sign_sec * @min(abs_sec, 0.75 / max_rate);

    const mid_lat = point[1] + lat_rate * dt * 0.5;
    if (@abs(mid_lat) > 88.5) return null;

    const mid_lon = point[0] + lon_rate * dt * 0.5;
    const sample_mid = sampleWind(grid, mid_lon, mid_lat);
    const u_mid = sample_mid[0];
    const v_mid = sample_mid[1];

    if (!std.math.isFinite(u_mid) or !std.math.isFinite(v_mid)) return null;

    const m_per_deg_lon_mid = metersPerDegreeLon(mid_lat);
    if (@abs(m_per_deg_lon_mid) < 1e-6) return null;

    const p_lon = normalizeLongitude(point[0]);
    const lon = normalizeLongitude(p_lon + (u_mid * dt) / m_per_deg_lon_mid);
    const lat = point[1] + (v_mid * dt) / DEGREE_METERS;

    if (@abs(lat) > 88.5 or @abs(lon - p_lon) > 180.0) return null;
    if (@abs(lon - p_lon) + @abs(lat - point[1]) < 1e-8) return null;

    return .{ lon, lat };
}

/// Traces a streamline trajectory from a seed coordinate both backward and forward along wind vectors.
/// Output points are stored in upstream-to-downstream order:
/// [backward_reversed..., seed, ...forward].
/// Returns the number of points written into `out_points` (0 if calm, invalid, or fewer than 3 points).
pub fn bakeStreamline(
    grid: WindGrid,
    seed_lon: f32,
    seed_lat: f32,
    steps: usize,
    step_seconds: f32,
    out_points: [][2]f32,
) usize {
    if (!std.math.isFinite(seed_lon) or !std.math.isFinite(seed_lat)) return 0;
    if (@abs(seed_lat) > 88.5) return 0;
    if (out_points.len < 3 or steps == 0) return 0;

    const norm_seed_lon = normalizeLongitude(seed_lon);
    const center = [2]f32{ norm_seed_lon, seed_lat };

    const w = sampleWind(grid, center[0], center[1]);
    const speed = std.math.hypot(w[0], w[1]);
    if (!std.math.isFinite(speed) or speed < 0.15) return 0;

    const interval = if (std.math.isFinite(step_seconds) and step_seconds > 0.0)
        std.math.clamp(step_seconds, 1.0, 3600.0)
    else
        1800.0;

    const half_steps = @min(steps, (out_points.len - 1) / 2);
    if (half_steps == 0) return 0;

    // Backward tracing (against wind)
    var b_count: usize = 0;
    var cur = center;
    while (b_count < half_steps) {
        cur = advance(grid, cur, -interval) orelse break;
        out_points[b_count] = cur;
        b_count += 1;
    }

    // Reverse backward points in place so earliest upstream point is at index 0
    if (b_count > 0) {
        var left: usize = 0;
        var right: usize = b_count - 1;
        while (left < right) {
            const tmp = out_points[left];
            out_points[left] = out_points[right];
            out_points[right] = tmp;
            left += 1;
            right -= 1;
        }
    }

    // Seed center point
    out_points[b_count] = center;

    // Forward tracing (with wind)
    var f_count: usize = 0;
    cur = center;
    while (f_count < half_steps and (b_count + 1 + f_count) < out_points.len) {
        cur = advance(grid, cur, interval) orelse break;
        out_points[b_count + 1 + f_count] = cur;
        f_count += 1;
    }

    const total = b_count + 1 + f_count;
    if (total < 3) return 0;

    return total;
}

// C-ABI exports for WASM and dynamic linking

/// WASM C-ABI: Sample U and V wind components at (lon, lat).
/// Output is written to `out_uv[0] = u, out_uv[1] = v`.
pub export fn lavender_wind_sample(
    ni: usize,
    nj: usize,
    lo1: f32,
    la1: f32,
    di: f32,
    dj: f32,
    u_ptr: [*]const f32,
    v_ptr: [*]const f32,
    lon: f32,
    lat: f32,
    out_uv: [*]f32,
) void {
    const grid = WindGrid{
        .ni = ni,
        .nj = nj,
        .lo1 = lo1,
        .la1 = la1,
        .di = di,
        .dj = dj,
        .u = u_ptr[0 .. ni * nj],
        .v = v_ptr[0 .. ni * nj],
    };
    const uv = sampleWind(grid, lon, lat);
    out_uv[0] = uv[0];
    out_uv[1] = uv[1];
}

/// WASM C-ABI: Bake a streamline trajectory from a seed coordinate.
/// `out_coords` must have room for at least `max_points * 2` floats (lon, lat interleaved).
/// Returns the number of points written.
pub export fn lavender_wind_bake_streamline(
    ni: usize,
    nj: usize,
    lo1: f32,
    la1: f32,
    di: f32,
    dj: f32,
    u_ptr: [*]const f32,
    v_ptr: [*]const f32,
    seed_lon: f32,
    seed_lat: f32,
    steps: usize,
    step_seconds: f32,
    out_coords: [*]f32,
    max_points: usize,
) usize {
    const grid = WindGrid{
        .ni = ni,
        .nj = nj,
        .lo1 = lo1,
        .la1 = la1,
        .di = di,
        .dj = dj,
        .u = u_ptr[0 .. ni * nj],
        .v = v_ptr[0 .. ni * nj],
    };
    const points_ptr: [*][2]f32 = @ptrCast(@alignCast(out_coords));
    return bakeStreamline(grid, seed_lon, seed_lat, steps, step_seconds, points_ptr[0..max_points]);
}
