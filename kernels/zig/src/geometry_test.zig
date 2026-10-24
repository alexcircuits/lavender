const std = @import("std");
const geometry = @import("geometry.zig");

test "deltaDecodeCoordinates decodes cumulative coordinates accurately" {
    // [x0, y0, dx1, dy1, dx2, dy2]
    const deltas = [_]i32{ 10000, 20000, 100, -200, -50, 300 };
    var out_coords: [6]f64 = undefined;

    const count = geometry.deltaDecodeCoordinates(&deltas, 1000.0, &out_coords);
    try std.testing.expectEqual(@as(usize, 6), count);

    // Point 0: 10000 / 1000 = 10.0, 20000 / 1000 = 20.0
    try std.testing.expectApproxEqAbs(@as(f64, 10.0), out_coords[0], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f64, 20.0), out_coords[1], 1e-6);

    // Point 1: (10000 + 100) / 1000 = 10.1, (20000 - 200) / 1000 = 19.8
    try std.testing.expectApproxEqAbs(@as(f64, 10.1), out_coords[2], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f64, 19.8), out_coords[3], 1e-6);

    // Point 2: (10100 - 50) / 1000 = 10.05, (19800 + 300) / 1000 = 20.1
    try std.testing.expectApproxEqAbs(@as(f64, 10.05), out_coords[4], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f64, 20.1), out_coords[5], 1e-6);
}

test "perpendicularDistanceSq computes correct distance to segment" {
    // Point (0, 1) to segment from (0, 0) to (10, 0)
    const d_sq = geometry.perpendicularDistanceSq(0.0, 1.0, 0.0, 0.0, 10.0, 0.0);
    try std.testing.expectApproxEqAbs(@as(f64, 1.0), d_sq, 1e-7);

    // Point (5, 0) directly on segment
    const on_segment = geometry.perpendicularDistanceSq(5.0, 0.0, 0.0, 0.0, 10.0, 0.0);
    try std.testing.expectApproxEqAbs(@as(f64, 0.0), on_segment, 1e-7);
}

test "simplifyRdp simplifies collinear vertices" {
    // Collinear points along diagonal: (0, 0), (1, 1), (2, 2), (3, 3), (4, 4)
    const coords = [_]f64{ 0.0, 0.0, 1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0 };
    var out_coords: [10]f64 = undefined;

    const out_pts = geometry.simplifyRdp(&coords, 5, 0.1, &out_coords);
    try std.testing.expectEqual(@as(usize, 2), out_pts);
    try std.testing.expectApproxEqAbs(@as(f64, 0.0), out_coords[0], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f64, 0.0), out_coords[1], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f64, 4.0), out_coords[2], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f64, 4.0), out_coords[3], 1e-6);
}

test "simplifyRdp preserves sharp spikes" {
    // (0, 0), (1, 0), (2, 5), (3, 0), (4, 0)
    const coords = [_]f64{ 0.0, 0.0, 1.0, 0.0, 2.0, 5.0, 3.0, 0.0, 4.0, 0.0 };
    var out_coords: [10]f64 = undefined;

    const out_pts = geometry.simplifyRdp(&coords, 5, 1.0, &out_coords);
    try std.testing.expectEqual(@as(usize, 3), out_pts);
    try std.testing.expectApproxEqAbs(@as(f64, 0.0), out_coords[0], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f64, 0.0), out_coords[1], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f64, 2.0), out_coords[2], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f64, 5.0), out_coords[3], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f64, 4.0), out_coords[4], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f64, 0.0), out_coords[5], 1e-6);
}

test "pointInPolygon correctly identifies inside and outside points" {
    // CCW box from (-1, -1) to (1, 1)
    const box = [_]f64{ -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0 };

    // Inside
    try std.testing.expect(geometry.pointInPolygon(0.0, 0.0, &box, 4));
    try std.testing.expect(geometry.pointInPolygon(0.5, 0.5, &box, 4));

    // Outside
    try std.testing.expect(!geometry.pointInPolygon(2.0, 0.0, &box, 4));
    try std.testing.expect(!geometry.pointInPolygon(0.0, 2.0, &box, 4));
    try std.testing.expect(!geometry.pointInPolygon(-3.0, -3.0, &box, 4));
}
