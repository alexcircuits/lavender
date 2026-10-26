//! Unit tests for the Zig bright-star appearance kernel.

const std = @import("std");
const root = @import("root.zig");
const sky = root.sky;

test "raDecToUnitVector axes and unit length" {
    const x = sky.raDecToUnitVector(0, 0);
    try std.testing.expectApproxEqAbs(@as(f64, 1), x[0], 1e-12);
    const y = sky.raDecToUnitVector(90, 0);
    try std.testing.expectApproxEqAbs(@as(f64, 1), y[1], 1e-12);
    const z = sky.raDecToUnitVector(123, 90);
    try std.testing.expectApproxEqAbs(@as(f64, 1), z[2], 1e-12);
    const v = sky.raDecToUnitVector(101.287, -16.716);
    try std.testing.expectApproxEqAbs(@as(f64, 1), v[0] * v[0] + v[1] * v[1] + v[2] * v[2], 1e-12);
}

test "starBrightness and appearance bounds" {
    try std.testing.expectApproxEqAbs(@as(f64, 1), sky.starBrightness(6.5), 1e-12);
    try std.testing.expectApproxEqAbs(@as(f64, 1), sky.starBrightness(9.0), 1e-12);
    try std.testing.expectApproxEqAbs(@as(f64, 1), sky.starBrightness(std.math.nan(f64)), 1e-12);
    // Sirius (-1.46) is ~ 10^1.592
    try std.testing.expectApproxEqAbs(std.math.pow(f64, 10.0, 1.592), sky.starBrightness(-1.46), 1e-9);
    const faint = sky.starAppearance(6.5);
    try std.testing.expectApproxEqAbs(@as(f64, 0.55), faint.sigma_px, 1e-12);
    try std.testing.expectApproxEqAbs(@as(f64, 0.22), faint.alpha, 1e-12);
    const bright = sky.starAppearance(-1.46);
    try std.testing.expectApproxEqAbs(@as(f64, 1.35), bright.sigma_px, 1e-12);
    try std.testing.expectApproxEqAbs(@as(f64, 1.0), bright.alpha, 1e-12);
}

test "bvToTemperature Ballesteros" {
    // Sun-like B-V 0.65 ~ 5.8 kK; clamps outside [-0.4, 2.0]; NaN uses 0.6.
    const t = sky.bvToTemperature(0.65);
    try std.testing.expect(t > 5500 and t < 6000);
    try std.testing.expectEqual(sky.bvToTemperature(-5), sky.bvToTemperature(-0.4));
    try std.testing.expectEqual(sky.bvToTemperature(9), sky.bvToTemperature(2.0));
    try std.testing.expectEqual(sky.bvToTemperature(std.math.nan(f64)), sky.bvToTemperature(0.6));
}

test "bvToRgb peak is one and hue follows colour" {
    const blue = sky.bvToRgb(-0.3);
    try std.testing.expect(blue[2] > blue[0]);
    try std.testing.expectApproxEqAbs(@as(f64, 1), @max(blue[0], @max(blue[1], blue[2])), 1e-12);
    const red = sky.bvToRgb(1.8);
    try std.testing.expect(red[0] > red[2]);
    try std.testing.expectApproxEqAbs(@as(f64, 1), red[0], 1e-12);
    // Desaturation keeps every channel above 1 - 0.55 before normalizing.
    for (red) |c| try std.testing.expect(c >= 0.45 - 1e-9);
}

test "buildStarVertices layout and bounds" {
    const stars = [_]f32{ 0, 0, 6.5, 0.6, 90, 0, -1.46, 0.0, 10, 10, 3, 1 };
    var out: [24]f32 = undefined;
    try std.testing.expectEqual(@as(usize, 3), sky.buildStarVertices(&stars, &out));
    try std.testing.expectApproxEqAbs(@as(f32, 1), out[0], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f32, 0.55), out[3], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f32, 0.22), out[7], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f32, 1), out[8 + 1], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f32, 1.35), out[8 + 3], 1e-6);
    var small: [10]f32 = undefined;
    try std.testing.expectEqual(@as(usize, 1), sky.buildStarVertices(&stars, &small));
    try std.testing.expectEqual(@as(usize, 0), sky.buildStarVertices(stars[0..3], &out));
}
