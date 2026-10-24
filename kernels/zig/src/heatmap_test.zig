const std = @import("std");
const heatmap = @import("heatmap.zig");

test "binEquirect accumulates weight and count planes" {
    // 2x2 grid over the full world: quadrants NE, NW, SW, SE
    const points = [_]f64{
        90.0,  45.0,  2.0, // NE quadrant
        -90.0, 45.0,  3.0, // NW quadrant
        -90.0, -45.0, 5.0, // SW quadrant
        90.0,  -45.0, 7.0, // SE quadrant
        90.0,  45.0,  1.0, // NE again (accumulate)
    };
    var weight_sum: [4]f32 = undefined;
    var count: [4]f32 = undefined;
    @memset(&weight_sum, 0.0);
    @memset(&count, 0.0);

    const binned = heatmap.binEquirect(
        &points,
        5,
        2,
        2,
        -180.0,
        180.0,
        -90.0,
        90.0,
        &weight_sum,
        &count,
    );
    try std.testing.expectEqual(@as(usize, 5), binned);

    // Grid layout (row 0 = north): [NW, NE, SW, SE]
    try std.testing.expectApproxEqAbs(@as(f32, 3.0), weight_sum[0], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f32, 1.0), count[0], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f32, 3.0), weight_sum[1], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f32, 2.0), count[1], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f32, 5.0), weight_sum[2], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f32, 1.0), count[2], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f32, 7.0), weight_sum[3], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f32, 1.0), count[3], 1e-6);
}

test "binEquirect skips out-of-bounds and non-finite points" {
    const points = [_]f64{
        0.0,   0.0,   1.0, // inside
        200.0, 0.0,   1.0, // lon out of bounds
        0.0,   95.0,  1.0, // lat out of bounds
        std.math.nan(f64), 0.0, 1.0, // non-finite lon
        0.0,   0.0,   std.math.inf(f64), // non-finite weight
    };
    var weight_sum: [4]f32 = undefined;
    var count: [4]f32 = undefined;
    @memset(&weight_sum, 0.0);
    @memset(&count, 0.0);

    const binned = heatmap.binEquirect(&points, 5, 2, 2, -180.0, 180.0, -90.0, 90.0, &weight_sum, &count);
    try std.testing.expectEqual(@as(usize, 1), binned);

    // Only the first point lands in a cell (SE quadrant centre for lon 0 is row 1, col 1)
    var total: f32 = 0.0;
    for (weight_sum) |w| total += w;
    try std.testing.expectApproxEqAbs(@as(f32, 1.0), total, 1e-6);
    try std.testing.expectApproxEqAbs(@as(f32, 1.0), count[3], 1e-6);
}

test "binEquirect places boundary points via clamping" {
    // Exactly on each corner of the extent: clamped into edge cells, never skipped
    const points = [_]f64{
        -180.0, 90.0,  1.0,
        180.0,  90.0,  1.0,
        -180.0, -90.0, 1.0,
        180.0,  -90.0, 1.0,
    };
    var weight_sum: [4]f32 = undefined;
    var count: [4]f32 = undefined;
    @memset(&weight_sum, 0.0);
    @memset(&count, 0.0);

    const binned = heatmap.binEquirect(&points, 4, 2, 2, -180.0, 180.0, -90.0, 90.0, &weight_sum, &count);
    try std.testing.expectEqual(@as(usize, 4), binned);
    // One point per quadrant corner cell
    for (count) |c| try std.testing.expectApproxEqAbs(@as(f32, 1.0), c, 1e-6);
}

test "binEquirect rejects degenerate grids and buffers" {
    const points = [_]f64{ 0.0, 0.0, 1.0 };
    var weight_sum: [4]f32 = undefined;
    var count: [4]f32 = undefined;

    // Zero dimensions
    try std.testing.expectEqual(@as(usize, 0), heatmap.binEquirect(&points, 1, 0, 2, -180.0, 180.0, -90.0, 90.0, &weight_sum, &count));
    try std.testing.expectEqual(@as(usize, 0), heatmap.binEquirect(&points, 1, 2, 0, -180.0, 180.0, -90.0, 90.0, &weight_sum, &count));
    // Zero span extent
    try std.testing.expectEqual(@as(usize, 0), heatmap.binEquirect(&points, 1, 2, 2, 0.0, 0.0, -90.0, 90.0, &weight_sum, &count));
    // Undersized output planes
    var small_weight: [3]f32 = undefined;
    try std.testing.expectEqual(@as(usize, 0), heatmap.binEquirect(&points, 1, 2, 2, -180.0, 180.0, -90.0, 90.0, &small_weight, &count));
}

test "binWorld covers the full equirectangular extent" {
    const points = [_]f64{ 179.9, -89.9, 4.0 };
    var weight_sum: [16]f32 = undefined;
    var count: [16]f32 = undefined;
    @memset(&weight_sum, 0.0);
    @memset(&count, 0.0);

    const binned = heatmap.binWorld(&points, 1, 4, 4, &weight_sum, &count);
    try std.testing.expectEqual(@as(usize, 1), binned);
    // Far-east far-south corner: last cell of the last row
    try std.testing.expectApproxEqAbs(@as(f32, 4.0), weight_sum[15], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f32, 1.0), count[15], 1e-6);
}
