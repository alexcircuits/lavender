//! Unit tests for Zig Mode-S ADS-B and CPR Decoding Kernel

const std = @import("std");
const root = @import("root.zig");
const adsb = root.adsb;
const cpr = root.cpr;
const iq = root.iq;

fn parseHex(comptime hex: []const u8) [hex.len / 2]u8 {
    var bytes: [hex.len / 2]u8 = undefined;
    for (0..hex.len / 2) |i| {
        bytes[i] = std.fmt.parseInt(u8, hex[i * 2 .. i * 2 + 2], 16) catch unreachable;
    }
    return bytes;
}

test "decodes standard callsign message" {
    const raw = parseHex("8D4840D6202CC371C32CE0576098");
    try std.testing.expectEqual(@as(u24, 0), adsb.modeSChecksum(&raw));

    const msg = adsb.decodeAdsbFrame(&raw) orelse return error.DecodeFailed;
    try std.testing.expectEqualStrings("4840D6", &msg.icao);
    try std.testing.expectEqual(adsb.AdsbType.callsign, msg.msg_type);
    try std.testing.expectEqualStrings("KLM1023 ", &msg.callsign.?);
}

test "decodes airborne velocity message" {
    const raw = parseHex("8D485020994409940838175B284F");
    try std.testing.expectEqual(@as(u24, 0), adsb.modeSChecksum(&raw));

    const msg = adsb.decodeAdsbFrame(&raw) orelse return error.DecodeFailed;
    try std.testing.expectEqual(adsb.AdsbType.velocity, msg.msg_type);
    const vel = msg.velocity orelse return error.MissingVelocity;

    try std.testing.expect(@abs(vel.speed_kt - 159.2) < 0.1);
    try std.testing.expect(@abs(vel.heading_deg - 182.88) < 0.1);
    try std.testing.expectEqual(@as(?i32, -832), vel.vertical_rate_fpm);
}

test "decodes airborne position altitude" {
    const raw = parseHex("8D40621D58C382D690C8AC2863A7");
    try std.testing.expectEqual(@as(u24, 0), adsb.modeSChecksum(&raw));

    const msg = adsb.decodeAdsbFrame(&raw) orelse return error.DecodeFailed;
    try std.testing.expectEqual(adsb.AdsbType.airborne_position, msg.msg_type);
    try std.testing.expectEqual(@as(?i32, 38000), msg.altitude_ft);
    try std.testing.expectEqual(false, msg.cpr.?.odd);
}

test "globally decodes valid even/odd CPR pair to geographic fix" {
    const even_raw = parseHex("8D40621D58C382D690C8AC2863A7");
    const odd_raw = parseHex("8D40621D58C386435CC412692AD6");

    const even_msg = adsb.decodeAdsbFrame(&even_raw) orelse return error.DecodeEvenFailed;
    const odd_msg = adsb.decodeAdsbFrame(&odd_raw) orelse return error.DecodeOddFailed;

    const even_cpr = even_msg.cpr orelse return error.MissingEvenCpr;
    const odd_cpr = odd_msg.cpr orelse return error.MissingOddCpr;

    try std.testing.expectEqual(false, even_cpr.odd);
    try std.testing.expectEqual(true, odd_cpr.odd);

    const fix = cpr.decodeGlobalAirborneCpr(
        even_cpr.latitude_raw,
        even_cpr.longitude_raw,
        odd_cpr.latitude_raw,
        odd_cpr.longitude_raw,
        true, // use_odd
    ) orelse return error.GlobalCprFailed;

    // Expected: lat ~52.26578, lon ~3.93891
    try std.testing.expect(@abs(fix.latitude - 52.26578) < 0.0001);
    try std.testing.expect(@abs(fix.longitude - 3.93891) < 0.0001);
}

test "local CPR decoding agrees with global fix" {
    const odd_raw = parseHex("8D40621D58C386435CC412692AD6");
    const odd_msg = adsb.decodeAdsbFrame(&odd_raw) orelse return error.DecodeOddFailed;
    const odd_cpr = odd_msg.cpr orelse return error.MissingOddCpr;

    // Reference near Amsterdam: 52.3, 4.0
    const local_fix = cpr.decodeLocalAirborneCpr(
        odd_cpr.latitude_raw,
        odd_cpr.longitude_raw,
        true,
        52.3,
        4.0,
    ) orelse return error.LocalCprFailed;

    try std.testing.expect(@abs(local_fix.latitude - 52.26578) < 0.001);
    try std.testing.expect(@abs(local_fix.longitude - 3.93891) < 0.001);
}
