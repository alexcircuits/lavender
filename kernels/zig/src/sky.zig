//! Bright-star appearance kernel: catalogue pack to GPU vertex data.
//!
//! Port of legacy/src/sky/stars.js. Pure functions, zero heap allocations.
//! Catalogue packs are flat `[raDeg, decDeg, vmag, bv]` arrays in ICRF/J2000.

const std = @import("std");

/// Brightness reference; fainter catalogue stars are drawn like it.
pub const STAR_LIMIT_MAGNITUDE: f64 = 6.5;
/// Gaussian sigma of the faintest star, CSS pixels.
pub const STAR_MIN_SIGMA_PX: f64 = 0.55;
/// Gaussian sigma cap for the brightest star, CSS pixels.
pub const STAR_MAX_SIGMA_PX: f64 = 1.35;
/// Peak opacity of the faintest star.
pub const STAR_MIN_ALPHA: f64 = 0.22;
/// B-V for stars the catalogue gives no colour (a solar-type white).
pub const STAR_DEFAULT_BV: f64 = 0.6;
/// Floats per star in the vertex buffer: xyz, sigma, rgb, alpha.
pub const STAR_VERTEX_FLOATS: usize = 8;
/// Fraction of the colour kept when desaturating towards white.
pub const COLOR_SATURATION: f64 = 0.55;

const DEG: f64 = std.math.pi / 180.0;

pub const Appearance = struct { sigma_px: f64, alpha: f64 };

fn clamp(value: f64, lo: f64, hi: f64) f64 {
    return @max(lo, @min(hi, value));
}

/// ICRF unit vector for J2000 right ascension and declination in degrees.
pub fn raDecToUnitVector(ra_deg: f64, dec_deg: f64) [3]f64 {
    const ra = ra_deg * DEG;
    const dec = dec_deg * DEG;
    const cos_dec = @cos(dec);
    return .{ cos_dec * @cos(ra), cos_dec * @sin(ra), @sin(dec) };
}

/// Perceived brightness relative to the limit magnitude (`10^(-0.2 (m - limit))`).
pub fn starBrightness(vmag: f64) f64 {
    const magnitude = if (std.math.isFinite(vmag)) vmag else STAR_LIMIT_MAGNITUDE;
    return std.math.pow(f64, 10.0, -0.2 * (@min(magnitude, STAR_LIMIT_MAGNITUDE) - STAR_LIMIT_MAGNITUDE));
}

/// Sprite sigma (CSS px) and peak alpha for a visual magnitude.
pub fn starAppearance(vmag: f64) Appearance {
    const b = starBrightness(vmag);
    return .{
        .sigma_px = clamp(STAR_MIN_SIGMA_PX * std.math.pow(f64, b, 0.3), STAR_MIN_SIGMA_PX, STAR_MAX_SIGMA_PX),
        .alpha = clamp(STAR_MIN_ALPHA * b, STAR_MIN_ALPHA, 1.0),
    };
}

/// Effective temperature in kelvin from B-V (Ballesteros 2012).
pub fn bvToTemperature(bv: f64) f64 {
    const index = clamp(if (std.math.isFinite(bv)) bv else STAR_DEFAULT_BV, -0.4, 2.0);
    return 4600.0 * (1.0 / (0.92 * index + 1.7) + 1.0 / (0.92 * index + 0.62));
}

/// Black-body colour of a temperature, sRGB 0..1 (Helland's fit).
fn temperatureToRgb(kelvin: f64) [3]f64 {
    const t = clamp(kelvin, 1000.0, 40000.0) / 100.0;
    const red = if (t <= 66.0) 255.0 else 329.698727446 * std.math.pow(f64, t - 60.0, -0.1332047592);
    const green = if (t <= 66.0)
        99.4708025861 * @log(t) - 161.1195681661
    else
        288.1221695283 * std.math.pow(f64, t - 60.0, -0.0755148492);
    const blue = if (t >= 66.0)
        255.0
    else if (t <= 19.0)
        0.0
    else
        138.5177312231 * @log(t - 10.0) - 305.0447927307;
    return .{ clamp(red, 0, 255) / 255.0, clamp(green, 0, 255) / 255.0, clamp(blue, 0, 255) / 255.0 };
}

/// Display colour for a B-V index: desaturated black-body hue, peak channel = 1.
pub fn bvToRgb(bv: f64) [3]f64 {
    var rgb = temperatureToRgb(bvToTemperature(bv));
    for (&rgb) |*c| c.* = 1.0 - COLOR_SATURATION + COLOR_SATURATION * c.*;
    const peak = @max(rgb[0], @max(rgb[1], rgb[2]));
    for (&rgb) |*c| c.* /= peak;
    return rgb;
}

/// Decodes `stars` (`[ra, dec, vmag, bv] * N`) into `out` (8 floats per star).
/// Returns the number of stars written, bounded by both buffers.
pub fn buildStarVertices(stars: []const f32, out: []f32) usize {
    const count = @min(stars.len / 4, out.len / STAR_VERTEX_FLOATS);
    for (0..count) |i| {
        const s = stars[i * 4 ..][0..4];
        const app = starAppearance(s[2]);
        const dir = raDecToUnitVector(s[0], s[1]);
        const rgb = bvToRgb(s[3]);
        const o = out[i * STAR_VERTEX_FLOATS ..][0..STAR_VERTEX_FLOATS];
        o[0] = @floatCast(dir[0]);
        o[1] = @floatCast(dir[1]);
        o[2] = @floatCast(dir[2]);
        o[3] = @floatCast(app.sigma_px);
        o[4] = @floatCast(rgb[0]);
        o[5] = @floatCast(rgb[1]);
        o[6] = @floatCast(rgb[2]);
        o[7] = @floatCast(app.alpha);
    }
    return count;
}
