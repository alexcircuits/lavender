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

test "pointInPolygonWrapped matches plain ray-cast away from the antimeridian" {
    const box = [_]f64{ -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0 };

    try std.testing.expect(geometry.pointInPolygonWrapped(0.0, 0.0, &box, 4));
    try std.testing.expect(geometry.pointInPolygonWrapped(0.99, 0.99, &box, 4));
    try std.testing.expect(!geometry.pointInPolygonWrapped(1.5, 0.0, &box, 4));
    try std.testing.expect(!geometry.pointInPolygonWrapped(0.0, -1.5, &box, 4));
}

test "pointInPolygonWrapped handles rings written across the antimeridian" {
    // Ring spanning 170E -> -170W (a 20-degree-wide box straddling the antimeridian)
    const ring = [_]f64{ 170.0, 10.0, -170.0, 10.0, -170.0, -10.0, 170.0, -10.0 };

    // Points inside the wrapped box
    try std.testing.expect(geometry.pointInPolygonWrapped(179.0, 0.0, &ring, 4));
    try std.testing.expect(geometry.pointInPolygonWrapped(-179.0, 5.0, &ring, 4));

    // Points outside: west of 170E and east of -170W respectively
    try std.testing.expect(!geometry.pointInPolygonWrapped(160.0, 0.0, &ring, 4));
    try std.testing.expect(!geometry.pointInPolygonWrapped(-160.0, 0.0, &ring, 4));
    // Latitude band miss
    try std.testing.expect(!geometry.pointInPolygonWrapped(179.0, 20.0, &ring, 4));
}

test "pointInPolygonBatch classifies many points against one ring" {
    const box = [_]f64{ -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0 };
    const points = [_]f64{
        0.0,  0.0, // inside
        0.5,  0.5, // inside
        2.0,  0.0, // outside
        0.0,  2.0, // outside
        -0.5, -0.5, // inside
    };
    var hits: [5]u8 = undefined;

    const hit_count = geometry.pointInPolygonBatch(&points, 5, &box, 4, &hits);
    try std.testing.expectEqual(@as(usize, 3), hit_count);
    try std.testing.expectEqualSlices(u8, &[_]u8{ 1, 1, 0, 0, 1 }, &hits);
}

test "pointInPolygonBatch rejects degenerate inputs without writing" {
    const box = [_]f64{ -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0 };
    const points = [_]f64{ 0.0, 0.0 };
    var hits: [1]u8 = undefined;

    // Too few ring vertices
    try std.testing.expectEqual(@as(usize, 0), geometry.pointInPolygonBatch(&points, 1, &box, 2, &hits));
    // Too few points
    try std.testing.expectEqual(@as(usize, 0), geometry.pointInPolygonBatch(&points, 0, &box, 4, &hits));
}

test "pointInPolygons hit-tests one point against many geofence rings" {
    // Three disjoint 1-degree boxes around the globe
    const ring_a = [_]f64{ -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0 }; // London-ish
    const ring_b = [_]f64{ 100.0, 10.0, 102.0, 10.0, 102.0, 12.0, 100.0, 12.0 }; // SE Asia
    const ring_c = [_]f64{ 170.0, 10.0, -170.0, 10.0, -170.0, -10.0, 170.0, -10.0 }; // antimeridian

    var rings: [24]f64 = undefined;
    @memcpy(rings[0..8], &ring_a);
    @memcpy(rings[8..16], &ring_b);
    @memcpy(rings[16..24], &ring_c);
    const lengths = [_]u32{ 4, 4, 4 };

    var hits: [3]u8 = undefined;
    try std.testing.expectEqual(@as(usize, 1), geometry.pointInPolygons(0.5, 0.5, &rings, &lengths, 3, &hits));
    try std.testing.expectEqualSlices(u8, &[_]u8{ 1, 0, 0 }, &hits);

    try std.testing.expectEqual(@as(usize, 1), geometry.pointInPolygons(101.0, 11.0, &rings, &lengths, 3, &hits));
    try std.testing.expectEqualSlices(u8, &[_]u8{ 0, 1, 0 }, &hits);

    // Point on the antimeridian falls inside ring_c only
    try std.testing.expectEqual(@as(usize, 1), geometry.pointInPolygons(-179.0, 0.0, &rings, &lengths, 3, &hits));
    try std.testing.expectEqualSlices(u8, &[_]u8{ 0, 0, 1 }, &hits);

    // Point outside every ring
    try std.testing.expectEqual(@as(usize, 0), geometry.pointInPolygons(60.0, 60.0, &rings, &lengths, 3, &hits));
    try std.testing.expectEqualSlices(u8, &[_]u8{ 0, 0, 0 }, &hits);
}

test "pointInPolygons tolerates malformed ring lengths" {
    const ring_a = [_]f64{ -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0 };
    const lengths = [_]u32{ 4, 2 }; // second ring degenerate
    var hits: [2]u8 = undefined;

    try std.testing.expectEqual(@as(usize, 1), geometry.pointInPolygons(0.0, 0.0, &ring_a, &lengths, 2, &hits));
    try std.testing.expectEqualSlices(u8, &[_]u8{ 1, 0 }, &hits);
}
