//! Mode-S ADS-B Extended Squitter (DF17 / DF18) Frame Decoder
//!
//! Implements CRC-24 checksum verification (polynomial 0xfff409),
//! bitfield unpacking, callsign extraction, altitude decoding,
//! velocity computation, and CPR payload separation.

const std = @import("std");
const cpr = @import("cpr.zig");

pub const MODE_S_POLYNOMIAL: u24 = 0xfff409;
pub const CALLSIGN_CHARSET: []const u8 = "#ABCDEFGHIJKLMNOPQRSTUVWXYZ#####_###############0123456789######";

pub const AdsbType = enum {
    callsign,
    surface_position,
    airborne_position,
    velocity,
    other,
};

pub const Velocity = struct {
    speed_kt: f64,
    heading_deg: f64,
    vertical_rate_fpm: ?i32,
};

pub const CprPayload = struct {
    odd: bool,
    surface: bool,
    latitude_raw: u32,
    longitude_raw: u32,
};

pub const DecodedMessage = struct {
    icao: [6]u8,
    type_code: u8,
    downlink_format: u8,
    msg_type: AdsbType,

    callsign: ?[8]u8 = null,
    emitter_category: ?[2]u8 = null,
    altitude_ft: ?i32 = null,
    on_ground: bool = false,
    velocity: ?Velocity = null,
    cpr: ?CprPayload = null,
};

/// Reads a single bit (0 or 1) from a 112-bit (14-byte) buffer at `index`.
pub inline fn getBit(bytes: *const [14]u8, index: usize) u1 {
    return @truncate((bytes[index >> 3] >> @intCast(7 - (index & 7))) & 1);
}

/// Reads `length` bits starting from `start` bit offset, packed big-endian into u32.
pub fn getBits(bytes: *const [14]u8, start: usize, length: usize) u32 {
    std.debug.assert(length <= 32);
    var val: u32 = 0;
    var i: usize = 0;
    while (i < length) : (i += 1) {
        val = (val << 1) | @as(u32, getBit(bytes, start + i));
    }
    return val;
}

/// Computes the 24-bit Mode S parity remainder.
/// Returns 0 for a valid extended squitter frame without transmission errors.
pub fn modeSChecksum(bytes: *const [14]u8) u24 {
    var remainder: u32 = 0;
    var index: usize = 0;
    const total_bits = bytes.len * 8;

    while (index < total_bits) : (index += 1) {
        const feedback = getBit(bytes, index) ^ @as(u1, @truncate((remainder >> 23) & 1));
        remainder = (remainder << 1) & 0xffffff;
        if (feedback != 0) {
            remainder ^= MODE_S_POLYNOMIAL;
        }
    }

    return @truncate(remainder);
}

/// Converts bytes 1..3 into a 6-character uppercase hex string (e.g., "4840D6").
pub fn extractIcao(bytes: *const [14]u8) [6]u8 {
    const hex_digits = "0123456789ABCDEF";
    var out: [6]u8 = undefined;
    for (0..3) |i| {
        const byte = bytes[1 + i];
        out[i * 2] = hex_digits[byte >> 4];
        out[i * 2 + 1] = hex_digits[byte & 0x0F];
    }
    return out;
}

/// Decodes an 8-character flight callsign from TypeCode 1..4.
pub fn decodeCallsign(bytes: *const [14]u8) [8]u8 {
    var out: [8]u8 = [_]u8{' '} ** 8;
    for (0..8) |i| {
        const char_code = getBits(bytes, 40 + i * 6, 6);
        const ch = CALLSIGN_CHARSET[char_code];
        out[i] = if (ch == '#' or ch == '_') ' ' else ch;
    }
    return out;
}

/// Decodes airborne barometric altitude in feet.
/// Bit 47 is Q-bit (1 = 25-ft increments, 0 = Gillham code).
pub fn decodeAltitude(bytes: *const [14]u8) ?i32 {
    const q_bit = getBit(bytes, 47);
    if (q_bit == 0) return null; // 100-ft Gillham code currently unmapped

    const high_part = getBits(bytes, 40, 7);
    const low_part = getBits(bytes, 48, 4);
    const encoded = (high_part << 4) | low_part;

    return @as(i32, @intCast(encoded)) * 25 - 1000;
}

/// Decodes ground speed, heading, and vertical rate from TypeCode 19 (Airborne Velocity).
pub fn decodeVelocity(bytes: *const [14]u8) ?Velocity {
    const subtype = getBits(bytes, 37, 3);
    if (subtype != 1 and subtype != 2) return null;
    const multiplier: f64 = if (subtype == 2) 4.0 else 1.0;

    const ew_raw = getBits(bytes, 46, 10);
    const ns_raw = getBits(bytes, 57, 10);
    if (ew_raw == 0 or ns_raw == 0) return null;

    const ew_dir: f64 = if (getBit(bytes, 45) != 0) -1.0 else 1.0;
    const ns_dir: f64 = if (getBit(bytes, 56) != 0) -1.0 else 1.0;

    const ew_vel = (@as(f64, @floatFromInt(ew_raw)) - 1.0) * multiplier * ew_dir;
    const ns_vel = (@as(f64, @floatFromInt(ns_raw)) - 1.0) * multiplier * ns_dir;

    const speed_kt = @sqrt(ew_vel * ew_vel + ns_vel * ns_vel);

    const rad_to_deg = 180.0 / std.math.pi;
    const heading_deg = cpr.positiveModulo(std.math.atan2(ew_vel, ns_vel) * rad_to_deg, 360.0);

    const vr_raw = getBits(bytes, 69, 9);
    var vr_fpm: ?i32 = null;
    if (vr_raw > 0) {
        const vr_dir: i32 = if (getBit(bytes, 68) != 0) -1 else 1;
        vr_fpm = (@as(i32, @intCast(vr_raw)) - 1) * 64 * vr_dir;
    }

    return Velocity{
        .speed_kt = speed_kt,
        .heading_deg = heading_deg,
        .vertical_rate_fpm = vr_fpm,
    };
}

/// Full decoder entry point for one raw 14-byte Mode-S frame.
/// Verifies CRC and unpacks fields into a `DecodedMessage`.
pub fn decodeAdsbFrame(bytes: *const [14]u8) ?DecodedMessage {
    const df: u8 = @truncate(getBits(bytes, 0, 5));
    if (df != 17 and df != 18) return null;
    if (modeSChecksum(bytes) != 0) return null;

    const icao = extractIcao(bytes);
    const type_code: u8 = @truncate(getBits(bytes, 32, 5));

    var msg = DecodedMessage{
        .icao = icao,
        .type_code = type_code,
        .downlink_format = df,
        .msg_type = .other,
    };

    if (type_code >= 1 and type_code <= 4) {
        msg.msg_type = .callsign;
        msg.callsign = decodeCallsign(bytes);
        const cat_sub = getBits(bytes, 37, 3);
        msg.emitter_category = [2]u8{ 69 - type_code, @as(u8, @intCast(cat_sub)) + '0' };
    } else if (type_code >= 5 and type_code <= 8) {
        msg.msg_type = .surface_position;
        msg.on_ground = true;
        msg.cpr = CprPayload{
            .surface = true,
            .odd = getBit(bytes, 53) != 0,
            .latitude_raw = getBits(bytes, 54, 17),
            .longitude_raw = getBits(bytes, 71, 17),
        };
    } else if (type_code >= 9 and type_code <= 18) {
        msg.msg_type = .airborne_position;
        msg.on_ground = false;
        msg.altitude_ft = decodeAltitude(bytes);
        msg.cpr = CprPayload{
            .surface = false,
            .odd = getBit(bytes, 53) != 0,
            .latitude_raw = getBits(bytes, 54, 17),
            .longitude_raw = getBits(bytes, 71, 17),
        };
    } else if (type_code == 19) {
        msg.msg_type = .velocity;
        msg.velocity = decodeVelocity(bytes);
    }

    return msg;
}
