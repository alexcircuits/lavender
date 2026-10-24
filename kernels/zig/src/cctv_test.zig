const std = @import("std");
const cctv = @import("cctv.zig");

test "cctv: projectPoint north, east, south, west" {
    const lat = 51.5074;
    const lon = -0.1278; // London

    // 1000m North
    const north = cctv.projectPoint(lat, lon, 0.0, 1000.0);
    try std.testing.expect(north[1] > lat);
    try std.testing.expectApproxEqAbs(lon, north[0], 0.0001);

    // 1000m East
    const east = cctv.projectPoint(lat, lon, 90.0, 1000.0);
    try std.testing.expectApproxEqAbs(lat, east[1], 0.0001);
    try std.testing.expect(east[0] > lon);

    // 1000m South
    const south = cctv.projectPoint(lat, lon, 180.0, 1000.0);
    try std.testing.expect(south[1] < lat);
    try std.testing.expectApproxEqAbs(lon, south[0], 0.0001);

    // 1000m West
    const west = cctv.projectPoint(lat, lon, 270.0, 1000.0);
    try std.testing.expectApproxEqAbs(lat, west[1], 0.0001);
    try std.testing.expect(west[0] < lon);
}

test "cctv: computeFrustum geometry sanity" {
    const pose = cctv.CameraPose{
        .lat = 37.7749,
        .lon = -122.4194, // SF
        .ground_alt_m = 10.0,
        .mount_height_m = 25.0,
        .heading_deg = 45.0, // North-East
        .pitch_deg = -15.0, // Pitched down toward ground
        .fov_deg = 60.0,
        .range_m = 800.0,
    };

    const g = cctv.computeFrustum(pose);

    // Mount altitude = 10 + 25 = 35m
    try std.testing.expectEqual(@as(f64, 35.0), g.mount.alt);

    // Pitched down, so cap_center.alt should be lower than mount.alt
    try std.testing.expect(g.cap_center.alt < g.mount.alt);

    // Horizontal distance should be range * cos(-15 deg) ~ 800 * 0.9659 = ~772.7m
    // Cap center should be northeast of mount
    try std.testing.expect(g.cap_center.lat > g.mount.lat);
    try std.testing.expect(g.cap_center.lon > g.mount.lon);

    // Corners should flank cap_center
    try std.testing.expect(g.tl.alt > g.bl.alt);
    try std.testing.expect(g.tr.alt > g.br.alt);
}

test "cctv: C-ABI export lavender_cctv_compute_frustum" {
    var buffer: [18]f64 = undefined;
    const ok = cctv.lavender_cctv_compute_frustum(
        40.7128,
        -74.0060, // NYC
        5.0,
        20.0,
        180.0, // South
        -10.0,
        74.0,
        500.0,
        &buffer,
    );
    try std.testing.expect(ok);

    // Check mount [lon, lat, alt]
    try std.testing.expectApproxEqAbs(@as(f64, -74.0060), buffer[0], 0.0001);
    try std.testing.expectApproxEqAbs(@as(f64, 40.7128), buffer[1], 0.0001);
    try std.testing.expectEqual(@as(f64, 25.0), buffer[2]);

    // Check that top-left is above bottom-left
    try std.testing.expect(buffer[8] > buffer[17]); // tl.alt > bl.alt
}

test "cctv: C-ABI batch wireframe generation" {
    const cameras = [_]f64{
        40.7128, -74.0060, 5.0, 20.0, 180.0, -10.0, 74.0, 500.0,
        34.0522, -118.2437, 100.0, 15.0, 90.0, -5.0, 60.0, 1000.0,
    };

    var wireframe: [96]f32 = undefined; // 2 cameras * 48 floats
    const count = cctv.lavender_cctv_batch_wireframe(
        &cameras,
        2,
        &wireframe,
        2,
    );

    try std.testing.expectEqual(@as(usize, 2), count);

    // First camera mount point should be approx (-74.006, 40.7128, 25.0)
    try std.testing.expectApproxEqAbs(@as(f32, -74.006), wireframe[0], 0.01);
    try std.testing.expectApproxEqAbs(@as(f32, 40.7128), wireframe[1], 0.01);
    try std.testing.expectApproxEqAbs(@as(f32, 25.0), wireframe[2], 0.01);

    // Second camera mount point starts at offset 48
    try std.testing.expectApproxEqAbs(@as(f32, -118.2437), wireframe[48], 0.01);
    try std.testing.expectApproxEqAbs(@as(f32, 34.0522), wireframe[49], 0.01);
    try std.testing.expectApproxEqAbs(@as(f32, 115.0), wireframe[50], 0.01);
}
