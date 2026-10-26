//! Unit tests for Optics, Sensor Metrics & Cyber Sonar Kernel

const std = @import("std");
const testing = std.testing;
const optics = @import("optics.zig");

test "GSD and NIIRS calculation bounds" {
    // Street level (1000m)
    const gsd_street = optics.calculateGsd(1000.0);
    try testing.expectApproxEqAbs(0.375, gsd_street, 0.001);

    const niirs_street = optics.calculateNiirs(gsd_street);
    // 0.375m = 14.76 inches. 10.25 - 3.32 * log10(14.76) ≈ 10.25 - 3.32 * 1.169 ≈ 6.37
    try testing.expect(niirs_street > 6.0 and niirs_street < 6.8);

    // High satellite altitude (800,000m)
    const gsd_sat = optics.calculateGsd(800_000.0);
    try testing.expectApproxEqAbs(300.0, gsd_sat, 1.0);

    const niirs_sat = optics.calculateNiirs(gsd_sat);
    // Low NIIRS at high altitude
    try testing.expect(niirs_sat >= 0.0 and niirs_sat < 1.0);

    // Negative altitude clamps to minimum GSD
    const gsd_neg = optics.calculateGsd(-100.0);
    try testing.expectEqual(@as(f64, 0.01), gsd_neg);
    try testing.expect(optics.calculateNiirs(gsd_neg) <= 9.0);
}

test "Off-nadir angle (ONA)" {
    // Nadir pitch is -90 degrees -> ONA = 0
    try testing.expectApproxEqAbs(0.0, optics.calculateOna(-90.0), 0.001);

    // Tilted 30 degrees forward -> pitch = -60 -> ONA = 30
    try testing.expectApproxEqAbs(30.0, optics.calculateOna(-60.0), 0.001);

    // Looking at the horizon -> pitch = 0 -> ONA = 90
    try testing.expectApproxEqAbs(90.0, optics.calculateOna(0.0), 0.001);

    // Looking straight up -> pitch = 90 -> ONA = 180
    try testing.expectApproxEqAbs(180.0, optics.calculateOna(90.0), 0.001);
}

test "Sun elevation estimation" {
    // Equator at solar noon around equinox (March 21, 2026 ~ 1774180800 Unix sec)
    // Longitude 0 at 12:00 UTC
    const unix_sec: f64 = 1774180800.0;
    const elev = optics.estimateSunElevation(0.0, 0.0, unix_sec);
    // Sun should be nearly directly overhead (elevation close to 85..90 deg)
    try testing.expect(elev > 80.0 and elev <= 90.0);

    // Midnight at same location (12 hours later)
    const midnight_elev = optics.estimateSunElevation(0.0, 0.0, unix_sec + 43200.0);
    // Sun should be below horizon
    try testing.expect(midnight_elev < -70.0 and midnight_elev >= -90.0);
}

test "Cyber Sonar sweep gain" {
    const sweep_head = 90.0;
    const sweep_width = 20.0;
    const feather = 10.0;

    // Inside center of sweep (e.g. 100 deg, delta = +10, half-width = 10)
    const gain_center = optics.calculateSonarSweepGain(100.0, sweep_head, sweep_width, feather);
    try testing.expectApproxEqAbs(1.0, gain_center, 0.001);

    // Right on the leading edge (110 deg, delta = +20)
    const gain_edge = optics.calculateSonarSweepGain(110.0, sweep_head, sweep_width, feather);
    try testing.expectApproxEqAbs(1.0, gain_edge, 0.001);

    // In the feather zone (115 deg)
    const gain_feather = optics.calculateSonarSweepGain(115.0, sweep_head, sweep_width, feather);
    try testing.expect(gain_feather > 0.1 and gain_feather < 0.9);

    // Outside the beam (130 deg)
    const gain_outside = optics.calculateSonarSweepGain(130.0, sweep_head, sweep_width, feather);
    try testing.expectEqual(@as(f64, 0.0), gain_outside);

    // Antimeridian wrap: sweep around 355 deg, contact at 5 deg
    const gain_wrap = optics.calculateSonarSweepGain(5.0, 355.0, 20.0, 10.0);
    try testing.expectApproxEqAbs(1.0, gain_wrap, 0.001);
}

test "Scope mask terminus alpha quantization" {
    // High altitude (>= 10 Mm)
    try testing.expectEqual(@as(f64, 0.94), optics.calculateScopeTerminusAlpha(12_000_000.0));
    try testing.expectEqual(@as(f64, 0.94), optics.calculateScopeTerminusAlpha(10_000_000.0));

    // Low altitude (<= 7 Mm)
    try testing.expectEqual(@as(f64, 1.0), optics.calculateScopeTerminusAlpha(7_000_000.0));
    try testing.expectEqual(@as(f64, 1.0), optics.calculateScopeTerminusAlpha(2_000_000.0));

    // Mid altitude (8.5 Mm) -> halfway between 1.0 and 0.94 -> 0.97
    const alpha_mid = optics.calculateScopeTerminusAlpha(8_500_000.0);
    try testing.expectEqual(@as(f64, 0.97), alpha_mid);
}

test "Keyhole aperture geometry" {
    const geo = optics.calculateKeyholeGeometry(1920.0, 1080.0, 0.11);
    try testing.expectApproxEqAbs(960.0, geo[0], 0.001); // cx
    try testing.expectApproxEqAbs(540.0, geo[1], 0.001); // cy
    // radius = min(1920, 1080) * 0.61 * 0.5 = 1080 * 0.305 = 329.4
    try testing.expectApproxEqAbs(329.4, geo[2], 0.001);
    // feather = 329.4 * 0.11 ≈ 36.234
    try testing.expectApproxEqAbs(36.234, geo[3], 0.01);
}
