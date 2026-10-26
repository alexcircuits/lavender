//! Unit tests for the Zig kinematics / director / weather kernels.

const std = @import("std");
const root = @import("root.zig");
const k = root.kinematics;

const eps = 1e-9;

test "normalizeHeading wraps and rejects non-finite" {
    try std.testing.expectEqual(@as(f64, 0), k.normalizeHeading(360));
    try std.testing.expectEqual(@as(f64, 350), k.normalizeHeading(-10));
    try std.testing.expectEqual(@as(f64, 10), k.normalizeHeading(730));
    try std.testing.expectEqual(@as(f64, 0), k.normalizeHeading(std.math.nan(f64)));
    try std.testing.expectEqual(@as(f64, 0), k.normalizeHeading(std.math.inf(f64)));
}

test "slewHeading takes shortest arc with rate limit" {
    try std.testing.expectApproxEqAbs(@as(f64, 5), k.slewHeading(355, 20, 10), eps);
    try std.testing.expectApproxEqAbs(@as(f64, 355), k.slewHeading(5, 340, 10), eps);
    try std.testing.expectApproxEqAbs(@as(f64, 20), k.slewHeading(15, 20, 10), eps);
    try std.testing.expectEqual(@as(f64, 15), k.slewHeading(15, 20, 0));
    try std.testing.expectEqual(@as(f64, 15), k.slewHeading(15, 20, std.math.nan(f64)));
}

test "cockpitAnchorCorrectionStep is bounded" {
    try std.testing.expectEqual(@as(f64, 0), k.cockpitAnchorCorrectionStep(0, 100, 0.016));
    try std.testing.expectEqual(@as(f64, 0), k.cockpitAnchorCorrectionStep(10, 100, 0));
    try std.testing.expectEqual(@as(f64, 0), k.cockpitAnchorCorrectionStep(std.math.nan(f64), 100, 0.1));
    // dt capped to 0.1; speed 200 m/s -> rate 44 m/s -> cap 4.4 m; eased = 1000*(1-e^-0.125)
    try std.testing.expectApproxEqAbs(@as(f64, 4.4), k.cockpitAnchorCorrectionStep(1000, 200, 5), eps);
    // small distance limited by easing
    const eased = 1.0 * (1.0 - @exp(-1.25 * 0.1));
    try std.testing.expectApproxEqAbs(eased, k.cockpitAnchorCorrectionStep(1, 200, 0.1), eps);
    // minimum rate 0.75 m/s with unknown speed
    try std.testing.expectApproxEqAbs(@as(f64, 0.075), k.cockpitAnchorCorrectionStep(1000, std.math.nan(f64), 0.1), eps);
}

test "sampleCameraMove endpoints, easing and short arcs" {
    const from = k.CameraPose{ 0, 170, 1000, 350, -10, 10 };
    const to = k.CameraPose{ 10, -170, 2000, 10, -30, 350 };
    try std.testing.expectEqual(from, k.sampleCameraMove(from, to, false, 0));
    try std.testing.expectEqual(to, k.sampleCameraMove(from, to, false, 1));
    try std.testing.expectEqual(from, k.sampleCameraMove(from, to, false, std.math.nan(f64)));
    try std.testing.expectEqual(to, k.sampleCameraMove(from, to, true, 7));

    const mid = k.sampleCameraMove(from, to, true, 0.5);
    try std.testing.expectApproxEqAbs(@as(f64, 5), mid[0], eps);
    try std.testing.expectApproxEqAbs(@as(f64, -180), mid[1], eps);
    try std.testing.expectApproxEqAbs(@as(f64, 1500), mid[2], eps);
    try std.testing.expectApproxEqAbs(@as(f64, 360), mid[3], eps);
    try std.testing.expectApproxEqAbs(@as(f64, -20), mid[4], eps);
    try std.testing.expectApproxEqAbs(@as(f64, 0), mid[5], eps);

    // Cubic ease-in-out: t=0.25 -> 4*t^3 = 0.0625; t=0.75 -> 1-(0.5)^3/2 = 0.9375
    const a = k.sampleCameraMove(from, to, false, 0.25);
    try std.testing.expectApproxEqAbs(@as(f64, 0.625), a[0], eps);
    const b = k.sampleCameraMove(from, to, false, 0.75);
    try std.testing.expectApproxEqAbs(@as(f64, 9.375), b[0], eps);
}

test "normalizeCameraLongitude wraps" {
    try std.testing.expectApproxEqAbs(@as(f64, -170), k.normalizeCameraLongitude(190), eps);
    try std.testing.expectApproxEqAbs(@as(f64, -180), k.normalizeCameraLongitude(180), eps);
    try std.testing.expectApproxEqAbs(@as(f64, 170), k.normalizeCameraLongitude(-190), eps);
}

test "deriveWeatherEffectProfile families" {
    const nan = std.math.nan(f64);
    const missing = k.deriveWeatherEffectProfile(nan, 100, 5, 50, 100, 90);
    try std.testing.expect(!missing.available);
    try std.testing.expectEqual(@as(f64, 0), missing.cloud);

    const clear = k.deriveWeatherEffectProfile(0, 8, 0, 4, 20000, 370);
    try std.testing.expect(clear.available);
    try std.testing.expectEqual(@as(f64, 0), clear.cloud);
    try std.testing.expectEqual(@as(f64, 0), clear.fog);
    try std.testing.expectApproxEqAbs(@as(f64, 10), clear.wind_dir_deg, eps);

    const drizzle = k.deriveWeatherEffectProfile(53, nan, nan, nan, nan, nan);
    try std.testing.expectApproxEqAbs(@as(f64, 0.12), drizzle.rain, eps);
    try std.testing.expectApproxEqAbs(@as(f64, 0.7), drizzle.cloud, eps);
    try std.testing.expectEqual(@as(f64, 0), drizzle.wind_dir_deg);

    const snow = k.deriveWeatherEffectProfile(73, 0, 8, 90, 12000, 0);
    try std.testing.expectApproxEqAbs(@as(f64, 1), snow.snow, eps);
    try std.testing.expectEqual(@as(f64, 0), snow.rain);
    try std.testing.expectApproxEqAbs(@as(f64, 1), snow.wind, eps);

    const fog = k.deriveWeatherEffectProfile(45, 0, 0, 0, 12000, 0);
    try std.testing.expectApproxEqAbs(@as(f64, 0.78), fog.fog, eps);
    try std.testing.expectApproxEqAbs(@as(f64, 0.55), fog.cloud, eps);
    try std.testing.expectApproxEqAbs(@as(f64, 0.78 * 0.78), fog.haze, eps);

    const storm = k.deriveWeatherEffectProfile(95, 0, 0, 0, nan, 0);
    try std.testing.expectApproxEqAbs(@as(f64, 0.68), storm.rain, eps);
    try std.testing.expectApproxEqAbs(@as(f64, 0.55), storm.storm, eps);
    try std.testing.expectApproxEqAbs(@as(f64, 0.92), storm.cloud, eps);
    try std.testing.expectApproxEqAbs(@as(f64, 0.68 * (0.58 + 0.55 * 0.42)), storm.droplets, eps);

    // Poor visibility alone raises fog under a clear code.
    const vis = k.deriveWeatherEffectProfile(0, 0, 0, 0, 2000, 0);
    try std.testing.expectApproxEqAbs(@as(f64, 10000.0 / 11500.0), vis.fog, eps);
}

test "weatherAltitudeFactors fade with height" {
    const ground = k.weatherAltitudeFactors(-5);
    try std.testing.expectEqual(@as(f64, 1), ground.precipitation);
    try std.testing.expectEqual(@as(f64, 1), ground.cloud);
    const mid = k.weatherAltitudeFactors(10000);
    try std.testing.expectApproxEqAbs(@as(f64, 0.5), mid.precipitation, eps);
    try std.testing.expectApproxEqAbs(@as(f64, 1), mid.cloud, eps);
    const high = k.weatherAltitudeFactors(30000);
    try std.testing.expectEqual(@as(f64, 0), high.precipitation);
    try std.testing.expectEqual(@as(f64, 0), high.cloud);
    try std.testing.expectEqual(@as(f64, 0), high.haze);
}

test "writeProfile layout" {
    const p = k.deriveWeatherEffectProfile(95, 0, 0, 90, nan_vis, 450);
    var out: [k.WEATHER_PROFILE_FLOATS]f32 = undefined;
    k.writeProfile(p, &out);
    try std.testing.expectApproxEqAbs(@as(f32, 0.92), out[0], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f32, 0.55), out[6], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f32, 1), out[7], 1e-6);
    try std.testing.expectApproxEqAbs(@as(f32, 90), out[8], 1e-4);
}

const nan_vis = std.math.nan(f64);
