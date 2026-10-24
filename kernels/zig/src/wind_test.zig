//! Unit tests for GRIB2 Wind Grid Resampling and Streamline Integrator

const std = @import("std");
const wind = @import("wind.zig");

test "normalizeLongitude handles standard and seam boundary angles" {
    const eps = 1e-5;
    try std.testing.expectApproxEqAbs(@as(f32, -180.0), wind.normalizeLongitude(180.0), eps);
    try std.testing.expectApproxEqAbs(@as(f32, -180.0), wind.normalizeLongitude(-180.0), eps);
    try std.testing.expectApproxEqAbs(@as(f32, 0.0), wind.normalizeLongitude(0.0), eps);
    try std.testing.expectApproxEqAbs(@as(f32, 0.0), wind.normalizeLongitude(360.0), eps);
    try std.testing.expectApproxEqAbs(@as(f32, -90.0), wind.normalizeLongitude(270.0), eps);
    try std.testing.expectApproxEqAbs(@as(f32, 90.0), wind.normalizeLongitude(-270.0), eps);
    try std.testing.expectApproxEqAbs(@as(f32, -170.0), wind.normalizeLongitude(190.0), eps);
    try std.testing.expectApproxEqAbs(@as(f32, 170.0), wind.normalizeLongitude(-190.0), eps);
    try std.testing.expectApproxEqAbs(@as(f32, -180.0), wind.normalizeLongitude(540.0), eps);
}

test "sampleWind bilinear interpolation matches known reference values" {
    const u_data = [_]f32{ 1.0, 2.0, 3.0, 4.0 };
    const v_data = [_]f32{ 5.0, 6.0, 7.0, 8.0 };

    const grid = wind.WindGrid{
        .ni = 2,
        .nj = 2,
        .lo1 = 0.0,
        .la1 = 90.0,
        .di = 180.0,
        .dj = 90.0,
        .u = &u_data,
        .v = &v_data,
    };

    const eps = 1e-4;

    // Corner (0, 90): top-left
    const p_top_left = wind.sampleWind(grid, 0.0, 90.0);
    try std.testing.expectApproxEqAbs(@as(f32, 1.0), p_top_left[0], eps);
    try std.testing.expectApproxEqAbs(@as(f32, 5.0), p_top_left[1], eps);

    // Corner (180, 90): top-right
    const p_top_right = wind.sampleWind(grid, 180.0, 90.0);
    try std.testing.expectApproxEqAbs(@as(f32, 2.0), p_top_right[0], eps);
    try std.testing.expectApproxEqAbs(@as(f32, 6.0), p_top_right[1], eps);

    // Midpoint (90, 45): interpolated center
    const p_mid = wind.sampleWind(grid, 90.0, 45.0);
    try std.testing.expectApproxEqAbs(@as(f32, 2.5), p_mid[0], eps);
    try std.testing.expectApproxEqAbs(@as(f32, 6.5), p_mid[1], eps);

    // Bottom-left (0, 0)
    const p_bot_left = wind.sampleWind(grid, 0.0, 0.0);
    try std.testing.expectApproxEqAbs(@as(f32, 3.0), p_bot_left[0], eps);
    try std.testing.expectApproxEqAbs(@as(f32, 7.0), p_bot_left[1], eps);

    // Bottom-right (180, 0)
    const p_bot_right = wind.sampleWind(grid, 180.0, 0.0);
    try std.testing.expectApproxEqAbs(@as(f32, 4.0), p_bot_right[0], eps);
    try std.testing.expectApproxEqAbs(@as(f32, 8.0), p_bot_right[1], eps);

    // Latitude clamping beyond poles
    const p_clamp_north = wind.sampleWind(grid, 0.0, 100.0);
    try std.testing.expectApproxEqAbs(@as(f32, 1.0), p_clamp_north[0], eps);
    try std.testing.expectApproxEqAbs(@as(f32, 5.0), p_clamp_north[1], eps);

    const p_clamp_south = wind.sampleWind(grid, 0.0, -100.0);
    try std.testing.expectApproxEqAbs(@as(f32, 3.0), p_clamp_south[0], eps);
    try std.testing.expectApproxEqAbs(@as(f32, 7.0), p_clamp_south[1], eps);

    // Rejection of non-finite inputs
    const p_inf = wind.sampleWind(grid, std.math.inf(f32), 0.0);
    try std.testing.expectEqual(@as(f32, 0.0), p_inf[0]);
    try std.testing.expectEqual(@as(f32, 0.0), p_inf[1]);

    const p_nan = wind.sampleWind(grid, 0.0, std.math.nan(f32));
    try std.testing.expectEqual(@as(f32, 0.0), p_nan[0]);
    try std.testing.expectEqual(@as(f32, 0.0), p_nan[1]);
}

test "sampleWind longitude wrapping across the seam" {
    const u_data = [_]f32{ 1.0, 2.0, 3.0, 4.0 };
    const v_data = [_]f32{ 5.0, 6.0, 7.0, 8.0 };

    const grid = wind.WindGrid{
        .ni = 2,
        .nj = 2,
        .lo1 = 0.0,
        .la1 = 90.0,
        .di = 180.0,
        .dj = 90.0,
        .u = &u_data,
        .v = &v_data,
    };

    const eps = 1e-3;

    // In a 0..360 grid, 359.99° and -0.01° must produce matching interpolated values
    const p_pos = wind.sampleWind(grid, 359.99, 90.0);
    const p_neg = wind.sampleWind(grid, -0.01, 90.0);
    try std.testing.expectApproxEqAbs(p_pos[0], p_neg[0], eps);
    try std.testing.expectApproxEqAbs(p_pos[1], p_neg[1], eps);

    // Grid starting at -180: columns at -180, -90, 0, 90
    const u_4 = [_]f32{ 10.0, 20.0, 30.0, 40.0, 10.0, 20.0, 30.0, 40.0 };
    const v_4 = [_]f32{ 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0 };
    const grid180 = wind.WindGrid{
        .ni = 4,
        .nj = 2,
        .lo1 = -180.0,
        .la1 = 90.0,
        .di = 90.0,
        .dj = 90.0,
        .u = &u_4,
        .v = &v_4,
    };

    // lon = -180 and lon = +180 are the exact same date-line meridian
    const s_neg180 = wind.sampleWind(grid180, -180.0, 45.0);
    const s_pos180 = wind.sampleWind(grid180, 180.0, 45.0);
    try std.testing.expectApproxEqAbs(s_neg180[0], s_pos180[0], 1e-4);
    try std.testing.expectApproxEqAbs(s_neg180[1], s_pos180[1], 1e-4);
    try std.testing.expectApproxEqAbs(@as(f32, 10.0), s_neg180[0], 1e-4);

    // Wrapping continuity near the seam: 179.9° and -179.9°
    const s_east = wind.sampleWind(grid180, 179.9, 90.0);
    const s_west = wind.sampleWind(grid180, -179.9, 90.0);
    try std.testing.expectApproxEqAbs(@as(f32, 10.0), s_east[0], 0.1);
    try std.testing.expectApproxEqAbs(@as(f32, 10.0), s_west[0], 0.1);
}

test "advance performs midpoint integration and validates bounds" {
    var u_field = [_]f32{12.0} ** 12;
    var v_field = [_]f32{0.0} ** 12;

    const grid = wind.WindGrid{
        .ni = 4,
        .nj = 3,
        .lo1 = -180.0,
        .la1 = 90.0,
        .di = 90.0,
        .dj = 90.0,
        .u = &u_field,
        .v = &v_field,
    };

    // Forward step in uniform eastward wind at Equator
    const start = [2]f32{ -180.0, 0.0 };
    const next_pt = wind.advance(grid, start, 1800.0);
    try std.testing.expect(next_pt != null);
    const p = next_pt.?;

    // Moved east: lon must increase, lat must remain 0
    try std.testing.expect(p[0] > start[0]);
    try std.testing.expectApproxEqAbs(@as(f32, 0.0), p[1], 1e-5);
    try std.testing.expect(@abs(p[0] - start[0]) < 1.0);

    // Backward step in uniform eastward wind
    const prev_pt = wind.advance(grid, [2]f32{ 0.0, 0.0 }, -1800.0);
    try std.testing.expect(prev_pt != null);
    try std.testing.expect(prev_pt.?[0] < 0.0);

    // Calm wind rejection (< 0.15 m/s)
    var calm_u = [_]f32{0.05} ** 12;
    var calm_v = [_]f32{0.05} ** 12;
    const calm_grid = wind.WindGrid{
        .ni = 4,
        .nj = 3,
        .lo1 = -180.0,
        .la1 = 90.0,
        .di = 90.0,
        .dj = 90.0,
        .u = &calm_u,
        .v = &calm_v,
    };
    try std.testing.expect(wind.advance(calm_grid, [2]f32{ 0.0, 0.0 }, 1800.0) == null);

    // Polar limit rejection (|lat| > 88.5)
    try std.testing.expect(wind.advance(grid, [2]f32{ 0.0, 89.0 }, 1800.0) == null);
}

test "bakeStreamline traces streamline following vector field" {
    // 1. Uniform eastward wind: u = 12, v = 0
    const u_east = [_]f32{12.0} ** 12;
    const v_zero = [_]f32{0.0} ** 12;
    const east_grid = wind.WindGrid{
        .ni = 4,
        .nj = 3,
        .lo1 = -180.0,
        .la1 = 90.0,
        .di = 90.0,
        .dj = 90.0,
        .u = &u_east,
        .v = &v_zero,
    };

    var points: [33][2]f32 = undefined;
    const count = wind.bakeStreamline(east_grid, 0.0, 0.0, 16, 1800.0, &points);
    try std.testing.expectEqual(@as(usize, 33), count);

    // Center point is at index 16
    try std.testing.expectApproxEqAbs(@as(f32, 0.0), points[16][0], 1e-4);
    try std.testing.expectApproxEqAbs(@as(f32, 0.0), points[16][1], 1e-4);

    // Coordinates must monotonically progress eastward along the wind
    for (1..count) |i| {
        try std.testing.expect(points[i][0] > points[i - 1][0]);
        try std.testing.expect(@abs(points[i][0] - points[i - 1][0]) < 1.0);
        try std.testing.expectApproxEqAbs(@as(f32, 0.0), points[i][1], 1e-5);
    }

    // 2. Field with changing northward component (bends streamlines)
    const u_bend = [_]f32{20.0} ** 12;
    const v_bend = [_]f32{
        -20.0, -10.0, 0.0, 10.0,
        -20.0, -10.0, 0.0, 10.0,
        -20.0, -10.0, 0.0, 10.0,
    };
    const bend_grid = wind.WindGrid{
        .ni = 4,
        .nj = 3,
        .lo1 = -180.0,
        .la1 = 90.0,
        .di = 90.0,
        .dj = 90.0,
        .u = &u_bend,
        .v = &v_bend,
    };

    var bend_points: [33][2]f32 = undefined;
    const bend_count = wind.bakeStreamline(bend_grid, 0.0, 0.0, 16, 1800.0, &bend_points);
    try std.testing.expectEqual(@as(usize, 33), bend_count);

    const middle = bend_points[16];
    try std.testing.expectApproxEqAbs(@as(f32, 0.0), middle[0], 1e-4);
    // Both ends must bend northward (lat > middle lat)
    try std.testing.expect(bend_points[0][1] > middle[1]);
    try std.testing.expect(bend_points[bend_count - 1][1] > middle[1]);

    // 3. Calm field produces 0 points
    const u_calm = [_]f32{0.0} ** 12;
    const calm_g = wind.WindGrid{
        .ni = 4,
        .nj = 3,
        .lo1 = -180.0,
        .la1 = 90.0,
        .di = 90.0,
        .dj = 90.0,
        .u = &u_calm,
        .v = &u_calm,
    };
    try std.testing.expectEqual(@as(usize, 0), wind.bakeStreamline(calm_g, 0.0, 0.0, 16, 1800.0, &points));
}

test "C-ABI WASM exports lavender_wind_sample and lavender_wind_bake_streamline" {
    const u_data = [_]f32{ 1.0, 2.0, 3.0, 4.0 };
    const v_data = [_]f32{ 5.0, 6.0, 7.0, 8.0 };

    var out_uv: [2]f32 = undefined;
    wind.lavender_wind_sample(
        2,
        2,
        0.0,
        90.0,
        180.0,
        90.0,
        &u_data,
        &v_data,
        90.0,
        45.0,
        &out_uv,
    );

    try std.testing.expectApproxEqAbs(@as(f32, 2.5), out_uv[0], 1e-4);
    try std.testing.expectApproxEqAbs(@as(f32, 6.5), out_uv[1], 1e-4);

    const u_east = [_]f32{12.0} ** 12;
    const v_zero = [_]f32{0.0} ** 12;
    var out_coords: [66]f32 = undefined; // 33 points * 2 coords

    const pts = wind.lavender_wind_bake_streamline(
        4,
        3,
        -180.0,
        90.0,
        90.0,
        90.0,
        &u_east,
        &v_zero,
        0.0,
        0.0,
        16,
        1800.0,
        &out_coords,
        33,
    );

    try std.testing.expectEqual(@as(usize, 33), pts);
    // Center point at index 16
    const center_lon = out_coords[16 * 2 + 0];
    const center_lat = out_coords[16 * 2 + 1];
    try std.testing.expectApproxEqAbs(@as(f32, 0.0), center_lon, 1e-4);
    try std.testing.expectApproxEqAbs(@as(f32, 0.0), center_lat, 1e-4);
}
