//! Unit tests for Zig SGP4 Satellite Orbit Propagator

const std = @import("std");
const sgp4 = @import("sgp4.zig");

test "Vanguard 1 (NORAD 00005) matches Vallado reference at t=0 and t=360 min" {
    const deg_to_rad = std.math.pi / 180.0;
    const rev_day_to_rad_min = (2.0 * std.math.pi) / 1440.0;

    const vanguard_elements = sgp4.SatelliteElements{
        .epoch_days = 0.0, // relative time test
        .bstar = 0.28098e-4,
        .inclination_rad = 34.2682 * deg_to_rad,
        .raan_rad = 348.7242 * deg_to_rad,
        .eccentricity = 0.1859667,
        .arg_perigee_rad = 331.7664 * deg_to_rad,
        .mean_anomaly_rad = 19.3264 * deg_to_rad,
        .mean_motion_rad_min = 10.82419157 * rev_day_to_rad_min,
    };

    const con = sgp4.GravitationalConstants{};
    const sat = try sgp4.initSatrec(vanguard_elements, con);

    // Test case 1: t = 0 min
    // Expected TEME km: [7022.465, -1400.083, 0.040]
    const teme_0 = try sgp4.propagateTeme(&sat, 0.0);
    std.debug.print("\nteme_0: [{d}, {d}, {d}]\n", .{ teme_0.x, teme_0.y, teme_0.z });
    try std.testing.expect(@abs(teme_0.x - 7022.465) < 1.0);
    try std.testing.expect(@abs(teme_0.y - (-1400.083)) < 1.0);
    try std.testing.expect(@abs(teme_0.z - 0.040) < 1.0);

    // Test case 2: t = 360 min
    // Expected TEME km: [-7154.03120202, -3783.17682504, -3536.19412294]
    const teme_360 = try sgp4.propagateTeme(&sat, 360.0);
    std.debug.print("\nteme_360: [{d}, {d}, {d}]\n", .{ teme_360.x, teme_360.y, teme_360.z });
    try std.testing.expect(@abs(teme_360.x - (-7154.03120202)) < 0.01);
    try std.testing.expect(@abs(teme_360.y - (-3783.17682504)) < 0.01);
    try std.testing.expect(@abs(teme_360.z - (-3536.19412294)) < 0.01);
}

test "ECEF to Geodetic conversion accuracy" {
    // Test point on Equator (Null Island at altitude 0)
    const ecf_null_island = sgp4.Vec3{ .x = 6378.137, .y = 0.0, .z = 0.0 };
    const geo = sgp4.ecfToGeodetic(ecf_null_island);

    try std.testing.expect(@abs(geo.latitude_deg) < 0.0001);
    try std.testing.expect(@abs(geo.longitude_deg) < 0.0001);
    try std.testing.expect(@abs(geo.altitude_m) < 1.0);
}
