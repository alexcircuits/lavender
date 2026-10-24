//! Compact Position Reporting (CPR) decoding (DO-260B / ICAO Annex 10)
//!
//! Provides zero-allocation, high-precision latitude/longitude recovery
//! for Mode-S airborne and surface extended squitter frames.

const std = @import("std");

pub const CPR_SCALE: f64 = 131072.0; // 2^17
pub const CPR_NZ: u32 = 15;

pub const GeoPoint = struct {
    latitude: f64,
    longitude: f64,
};

/// Positive modulo: handles negative dividends correctly.
pub fn positiveModulo(value: f64, divisor: f64) f64 {
    const m = @mod(value, divisor);
    return @mod(m + divisor, divisor);
}

/// Computes the number of longitude zones NL(latitude) for a given latitude.
/// At high latitudes (>=87 degrees), zones collapse towards poles.
pub fn cprNl(latitude: f64) u32 {
    const abs_lat = @abs(latitude);
    if (abs_lat >= 87.0) {
        if (abs_lat == 87.0) return 2;
        return 1;
    }
    const pi = std.math.pi;
    const numerator = 1.0 - @cos(pi / (2.0 * @as(f64, @floatFromInt(CPR_NZ))));
    const cos_lat = @cos((pi / 180.0) * abs_lat);
    const denominator = cos_lat * cos_lat;
    const acos_val = 1.0 - numerator / denominator;
    if (acos_val < -1.0 or acos_val > 1.0) return 1;
    const nl = @floor((2.0 * pi) / std.math.acos(acos_val));
    return @intFromFloat(nl);
}

/// Normalizes longitude to [-180, +180] degrees.
pub fn normalizeLongitude(longitude: f64) f64 {
    var lon = longitude;
    while (lon > 180.0) lon -= 360.0;
    while (lon < -180.0) lon += 360.0;
    return lon;
}

/// Global CPR decoding from an even/odd airborne frame pair.
/// Returns null if the pair falls into different latitude zones or is invalid.
pub fn decodeGlobalAirborneCpr(
    even_lat_raw: u32,
    even_lon_raw: u32,
    odd_lat_raw: u32,
    odd_lon_raw: u32,
    use_odd: bool,
) ?GeoPoint {
    const even_lat_f = @as(f64, @floatFromInt(even_lat_raw));
    const even_lon_f = @as(f64, @floatFromInt(even_lon_raw));
    const odd_lat_f = @as(f64, @floatFromInt(odd_lat_raw));
    const odd_lon_f = @as(f64, @floatFromInt(odd_lon_raw));

    const lat_idx = @floor((59.0 * even_lat_f - 60.0 * odd_lat_f) / CPR_SCALE + 0.5);

    var even_lat_deg = 6.0 * (positiveModulo(lat_idx, 60.0) + even_lat_f / CPR_SCALE);
    var odd_lat_deg = (360.0 / 59.0) * (positiveModulo(lat_idx, 59.0) + odd_lat_f / CPR_SCALE);

    if (even_lat_deg >= 270.0) even_lat_deg -= 360.0;
    if (odd_lat_deg >= 270.0) odd_lat_deg -= 360.0;

    if (cprNl(even_lat_deg) != cprNl(odd_lat_deg)) return null;

    const lat = if (use_odd) odd_lat_deg else even_lat_deg;
    if (lat < -90.0 or lat > 90.0) return null;

    const nl = cprNl(lat);
    const nl_f = @as(f64, @floatFromInt(nl));
    const parity_sub: f64 = if (use_odd) 1.0 else 0.0;
    const lon_zones = if (nl_f - parity_sub > 1.0) nl_f - parity_sub else 1.0;

    const lon_idx = @floor((even_lon_f * (nl_f - 1.0) - odd_lon_f * nl_f) / CPR_SCALE + 0.5);
    const frame_lon = if (use_odd) odd_lon_f else even_lon_f;

    const lon = normalizeLongitude(
        (360.0 / lon_zones) * (positiveModulo(lon_idx, lon_zones) + frame_lon / CPR_SCALE),
    );

    return GeoPoint{ .latitude = lat, .longitude = lon };
}

/// Local CPR decoding relative to a known reference fix (e.g., receiver or previous track fix).
pub fn decodeLocalAirborneCpr(
    lat_raw: u32,
    lon_raw: u32,
    is_odd: bool,
    ref_lat: f64,
    ref_lon: f64,
) ?GeoPoint {
    const parity: f64 = if (is_odd) 1.0 else 0.0;
    const d_lat = 360.0 / (4.0 * @as(f64, @floatFromInt(CPR_NZ)) - parity);

    const lat_raw_f = @as(f64, @floatFromInt(lat_raw));
    const lon_raw_f = @as(f64, @floatFromInt(lon_raw));

    const lat_idx = @floor(ref_lat / d_lat) +
        @floor(0.5 + positiveModulo(ref_lat, d_lat) / d_lat - lat_raw_f / CPR_SCALE);

    var lat = d_lat * (lat_idx + lat_raw_f / CPR_SCALE);
    if (lat >= 270.0) lat -= 360.0;
    if (lat < -90.0 or lat > 90.0) return null;

    const nl = cprNl(lat);
    const nl_f = @as(f64, @floatFromInt(nl));
    const lon_zones = if (nl_f - parity > 1.0) nl_f - parity else 1.0;
    const d_lon = 360.0 / lon_zones;

    const lon_idx = @floor(ref_lon / d_lon) +
        @floor(0.5 + positiveModulo(ref_lon, d_lon) / d_lon - lon_raw_f / CPR_SCALE);

    const lon = normalizeLongitude(d_lon * (lon_idx + lon_raw_f / CPR_SCALE));
    return GeoPoint{ .latitude = lat, .longitude = lon };
}
