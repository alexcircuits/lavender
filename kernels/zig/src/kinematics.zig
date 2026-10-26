//! Cockpit / director / weather kinematics micro-kernels.
//!
//! Ports of legacy/src/cockpitMath.js (heading + anchor correction),
//! legacy/src/director/camera.js (`sampleCameraMove`) and
//! legacy/src/weatherEffectsMath.js (`deriveWeatherEffectProfile`).
//! Angles are degrees, heights metres; zero heap allocations.

const std = @import("std");

fn isFinite(v: f64) bool {
    return std.math.isFinite(v);
}

/// Normalizes a heading into [0, 360). Non-finite input yields 0.
pub fn normalizeHeading(value: f64) f64 {
    if (!isFinite(value)) return 0;
    return @rem(@rem(value, 360.0) + 360.0, 360.0);
}

/// Advances `current` toward `target` along the shortest arc by at most `max_step_deg`.
pub fn slewHeading(current: f64, target: f64, max_step_deg: f64) f64 {
    const from = normalizeHeading(current);
    const to = normalizeHeading(target);
    if (!isFinite(max_step_deg) or max_step_deg <= 0) return from;
    const delta = @rem(to - from + 540.0, 360.0) - 180.0;
    const step = @max(-max_step_deg, @min(max_step_deg, delta));
    return normalizeHeading(from + step);
}

/// Bounded cockpit-anchor correction (metres) for one render step.
pub fn cockpitAnchorCorrectionStep(distance_m: f64, speed_mps: f64, dt_sec: f64) f64 {
    if (!isFinite(distance_m) or distance_m <= 0 or !isFinite(dt_sec) or dt_sec <= 0) return 0;
    const dt = @min(0.1, dt_sec);
    const speed = if (isFinite(speed_mps)) @max(0.0, speed_mps) else 0.0;
    const eased = distance_m * (1.0 - @exp(-1.25 * dt));
    const rate = @max(0.75, speed * 0.22);
    return @min(distance_m, @min(eased, rate * dt));
}

/// Camera pose: lat, lon, alt, heading, pitch, roll.
pub const CameraPose = [6]f64;

/// Wraps a longitude into [-180, 180).
pub fn normalizeCameraLongitude(longitude: f64) f64 {
    return @rem(@rem(longitude + 180.0, 360.0) + 360.0, 360.0) - 180.0;
}

/// Samples an authored camera move at `progress` (clamped 0..1), short-arc for lon/heading/roll.
pub fn sampleCameraMove(from: CameraPose, to: CameraPose, linear: bool, progress: f64) CameraPose {
    const t = if (isFinite(progress)) @max(0.0, @min(1.0, progress)) else 0.0;
    if (t == 0) return from;
    if (t == 1) return to;
    const eased = if (linear) t else if (t < 0.5) 4.0 * t * t * t else blk: {
        const u = -2.0 * t + 2.0;
        break :blk 1.0 - u * u * u / 2.0;
    };
    return .{
        lerp(from[0], to[0], eased),
        normalizeCameraLongitude(angle(from[1], to[1], eased)),
        lerp(from[2], to[2], eased),
        angle(from[3], to[3], eased),
        lerp(from[4], to[4], eased),
        angle(from[5], to[5], eased),
    };
}

fn lerp(a: f64, b: f64, e: f64) f64 {
    return a + (b - a) * e;
}

fn angle(a: f64, b: f64, e: f64) f64 {
    return a + (@rem(@rem(b - a + 540.0, 360.0) + 360.0, 360.0) - 180.0) * e;
}

/// Number of f32 values written by `deriveWeatherEffectProfile`.
pub const WEATHER_PROFILE_FLOATS: usize = 9;

pub const WeatherProfile = struct {
    available: bool = false,
    cloud: f64 = 0,
    rain: f64 = 0,
    snow: f64 = 0,
    fog: f64 = 0,
    haze: f64 = 0,
    droplets: f64 = 0,
    storm: f64 = 0,
    wind: f64 = 0,
    wind_dir_deg: f64 = 0,
};

fn clamp01(value: f64) f64 {
    if (!isFinite(value)) return 0;
    return @min(1.0, @max(0.0, value));
}

fn range01(value: f64, min: f64, max: f64) f64 {
    if (!isFinite(value) or max <= min) return 0;
    return clamp01((value - min) / (max - min));
}

/// Converts observations to bounded visual strengths. A NaN `weather_code` means
/// missing weather and yields an unavailable all-zero profile. Other NaN inputs
/// are treated as absent.
pub fn deriveWeatherEffectProfile(
    weather_code: f64,
    cloud_cover_pct: f64,
    precipitation_mm: f64,
    wind_kph: f64,
    visibility_m: f64,
    wind_direction_deg: f64,
) WeatherProfile {
    if (!isFinite(weather_code)) return .{};
    const code = weather_code;
    const cover = range01(cloud_cover_pct, 8, 100);
    const precipitation = if (isFinite(precipitation_mm)) @max(0.0, precipitation_mm) else 0.0;
    const wind = range01(wind_kph, 4, 90);
    const visibility_fog = if (isFinite(visibility_m)) range01(12000.0 - visibility_m, 0, 11500) else 0.0;

    const drizzle = code >= 51 and code <= 57;
    const rain_code = (code >= 61 and code <= 67) or (code >= 80 and code <= 82) or code >= 95;
    const snow_code = (code >= 71 and code <= 77) or (code >= 85 and code <= 86);
    const fog_code = code == 45 or code == 48;
    const storm_code = code >= 95;

    const observed = range01(precipitation, 0.05, 8);
    const rain_floor: f64 = if (drizzle) 0.12 else if (rain_code) (if (storm_code) 0.68 else 0.28) else 0.0;
    const snow_floor: f64 = if (snow_code) 0.3 else 0.0;
    const rain = if (drizzle or rain_code) @max(rain_floor, observed) else 0.0;
    const snow = if (snow_code) @max(snow_floor, observed) else 0.0;
    const fog = @max(if (fog_code) @as(f64, 0.78) else 0.0, visibility_fog);
    const storm = if (storm_code) @max(0.55, observed) else 0.0;
    const forced_cloud: f64 = if (storm_code) 0.92 else if (rain != 0 or snow != 0) 0.7 else if (fog_code) 0.55 else 0.0;
    const cloud = @max(cover, forced_cloud);

    return .{
        .available = true,
        .cloud = clamp01(cloud),
        .rain = clamp01(rain),
        .snow = clamp01(snow),
        .fog = clamp01(fog),
        .haze = clamp01(@max(fog * 0.78, cloud * 0.14 + (rain + snow) * 0.2)),
        .droplets = clamp01(rain * (0.58 + storm * 0.42)),
        .storm = clamp01(storm),
        .wind = wind,
        .wind_dir_deg = if (isFinite(wind_direction_deg)) @rem(@rem(wind_direction_deg, 360.0) + 360.0, 360.0) else 0.0,
    };
}

pub const AltitudeFactors = struct { precipitation: f64, cloud: f64, haze: f64 };

/// Fades ground-weather overlays as the camera climbs above their plausible layer.
pub fn weatherAltitudeFactors(altitude_m: f64) AltitudeFactors {
    const altitude = if (isFinite(altitude_m)) @max(0.0, altitude_m) else 0.0;
    return .{
        .precipitation = 1.0 - range01(altitude, 6000, 14000),
        .cloud = 1.0 - range01(altitude, 14000, 24000),
        .haze = 1.0 - range01(altitude, 9000, 20000),
    };
}

/// Writes the profile as [cloud, rain, snow, fog, haze, droplets, storm, wind, windDirDeg].
pub fn writeProfile(p: WeatherProfile, out: *[WEATHER_PROFILE_FLOATS]f32) void {
    out[0] = @floatCast(p.cloud);
    out[1] = @floatCast(p.rain);
    out[2] = @floatCast(p.snow);
    out[3] = @floatCast(p.fog);
    out[4] = @floatCast(p.haze);
    out[5] = @floatCast(p.droplets);
    out[6] = @floatCast(p.storm);
    out[7] = @floatCast(p.wind);
    out[8] = @floatCast(p.wind_dir_deg);
}
