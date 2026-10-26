//! Unit tests for Zig GNSS Navigation Integrity & Spatial Cell Binning Kernel

const std = @import("std");
const root = @import("root.zig");
const gnss = root.gnss;

test "classifyObservation evaluates DO-260A/B NIC, NACp, and gps_lost rules" {
    // Nominal high-integrity observations
    try std.testing.expectEqual(false, gnss.classifyObservation(7, 8, false));
    try std.testing.expectEqual(false, gnss.classifyObservation(8, 9, false));
    try std.testing.expectEqual(false, gnss.classifyObservation(11, 11, false));

    // Unreported indicators (negative) without GPS loss
    try std.testing.expectEqual(false, gnss.classifyObservation(-1, -1, false));
    try std.testing.expectEqual(false, gnss.classifyObservation(8, -1, false));
    try std.testing.expectEqual(false, gnss.classifyObservation(-1, 9, false));

    // Degraded NIC (0 <= nic < 7)
    try std.testing.expectEqual(true, gnss.classifyObservation(6, 9, false));
    try std.testing.expectEqual(true, gnss.classifyObservation(0, 10, false));
    try std.testing.expectEqual(true, gnss.classifyObservation(4, -1, false));

    // Degraded NACp (0 <= nac_p < 8)
    try std.testing.expectEqual(true, gnss.classifyObservation(8, 7, false));
    try std.testing.expectEqual(true, gnss.classifyObservation(9, 0, false));
    try std.testing.expectEqual(true, gnss.classifyObservation(-1, 5, false));

    // GPS loss flag overrides all indicators
    try std.testing.expectEqual(true, gnss.classifyObservation(9, 10, true));
    try std.testing.expectEqual(true, gnss.classifyObservation(-1, -1, true));
}

test "discountDegradedPercent applies gpsjam 1-aircraft false-positive discount" {
    try std.testing.expectApproxEqAbs(@as(f64, 0.0), gnss.discountDegradedPercent(0, 0), 1e-9);
    try std.testing.expectApproxEqAbs(@as(f64, 0.0), gnss.discountDegradedPercent(0, 5), 1e-9);
    try std.testing.expectApproxEqAbs(@as(f64, 0.0), gnss.discountDegradedPercent(10, 0), 1e-9);
    // 1 degraded aircraft is discounted to 0.0%
    try std.testing.expectApproxEqAbs(@as(f64, 0.0), gnss.discountDegradedPercent(10, 1), 1e-9);
    try std.testing.expectApproxEqAbs(@as(f64, 0.0), gnss.discountDegradedPercent(3, 1), 1e-9);

    // 50 aircraft, 2 degraded -> 100 * (2 - 1) / 50 = 2.0%
    try std.testing.expectApproxEqAbs(@as(f64, 2.0), gnss.discountDegradedPercent(50, 2), 1e-9);
    // 20 aircraft, 4 degraded -> 100 * (4 - 1) / 20 = 15.0%
    try std.testing.expectApproxEqAbs(@as(f64, 15.0), gnss.discountDegradedPercent(20, 4), 1e-9);
}

test "classifyLevel maps discounted percentage to Low, Medium, and High bands" {
    try std.testing.expectEqual(@as(u8, 0), gnss.classifyLevel(0.0));
    try std.testing.expectEqual(@as(u8, 0), gnss.classifyLevel(1.99));
    try std.testing.expectEqual(@as(u8, 0), gnss.classifyLevel(std.math.nan(f64)));

    try std.testing.expectEqual(@as(u8, 1), gnss.classifyLevel(2.0));
    try std.testing.expectEqual(@as(u8, 1), gnss.classifyLevel(6.5));
    try std.testing.expectEqual(@as(u8, 1), gnss.classifyLevel(10.0));

    try std.testing.expectEqual(@as(u8, 2), gnss.classifyLevel(10.01));
    try std.testing.expectEqual(@as(u8, 2), gnss.classifyLevel(33.33));
}

test "binCells enforces min_aircraft, 1-aircraft discount, and medium/high classification" {
    // Use 45.0 deg cells -> rows = 4, cols = 8 (32 total cells) for compact stack scratch
    const cell_deg: f64 = 45.0;
    var cell_aircraft: [32]u32 = undefined;
    var cell_degraded: [32]u32 = undefined;

    var out_row: [8]u16 = undefined;
    var out_col: [8]u16 = undefined;
    var out_aircraft: [8]u32 = undefined;
    var out_degraded: [8]u32 = undefined;
    var out_percent: [8]f32 = undefined;
    var out_level: [8]u8 = undefined;

    // Cell A (lon 10..20, lat 10..20 -> row 2, col 4): 3 aircraft, 1 degraded -> 0.0% (Low)
    // Cell B (lon 60..70, lat 10..20 -> row 2, col 5): 10 aircraft, 2 degraded -> 10.0% (Medium)
    // Cell C (lon 110..120, lat 10..20 -> row 2, col 6): 4 aircraft, 2 degraded -> 25.0% (High)
    // Cell D (lon -60..-50, lat -20..-10 -> row 1, col 2): 2 aircraft (< min_aircraft=3, skipped)
    const lons = [_]f64{
        // Cell A (3 aircraft)
        10.0, 12.0, 14.0,
        // Cell B (10 aircraft)
        60.0, 61.0, 62.0, 63.0, 64.0, 65.0, 66.0, 67.0, 68.0, 69.0,
        // Cell C (4 aircraft)
        110.0, 112.0, 114.0, 116.0,
        // Cell D (2 aircraft, below threshold)
        -60.0, -55.0,
    };
    const lats = [_]f64{
        // Cell A
        10.0, 12.0, 14.0,
        // Cell B
        10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 19.0,
        // Cell C
        10.0, 12.0, 14.0, 16.0,
        // Cell D
        -20.0, -15.0,
    };
    const degraded = [_]u8{
        // Cell A: 1 degraded
        1, 0, 0,
        // Cell B: 2 degraded
        1, 1, 0, 0, 0, 0, 0, 0, 0, 0,
        // Cell C: 2 degraded
        1, 1, 0, 0,
        // Cell D: 2 degraded
        1, 1,
    };

    const written = gnss.binCells(
        &lons,
        &lats,
        &degraded,
        lons.len,
        cell_deg,
        3,
        &cell_aircraft,
        &cell_degraded,
        &out_row,
        &out_col,
        &out_aircraft,
        &out_degraded,
        &out_percent,
        &out_level,
    );

    try std.testing.expectEqual(@as(usize, 3), written);

    // Cell A: row 2, col 4 -> 3 aircraft, 1 degraded -> 0% -> Low (0)
    try std.testing.expectEqual(@as(u16, 2), out_row[0]);
    try std.testing.expectEqual(@as(u16, 4), out_col[0]);
    try std.testing.expectEqual(@as(u32, 3), out_aircraft[0]);
    try std.testing.expectEqual(@as(u32, 1), out_degraded[0]);
    try std.testing.expectApproxEqAbs(@as(f32, 0.0), out_percent[0], 1e-4);
    try std.testing.expectEqual(@as(u8, 0), out_level[0]);

    // Cell B: row 2, col 5 -> 10 aircraft, 2 degraded -> 10.0% -> Medium (1)
    try std.testing.expectEqual(@as(u16, 2), out_row[1]);
    try std.testing.expectEqual(@as(u16, 5), out_col[1]);
    try std.testing.expectEqual(@as(u32, 10), out_aircraft[1]);
    try std.testing.expectEqual(@as(u32, 2), out_degraded[1]);
    try std.testing.expectApproxEqAbs(@as(f32, 10.0), out_percent[1], 1e-4);
    try std.testing.expectEqual(@as(u8, 1), out_level[1]);

    // Cell C: row 2, col 6 -> 4 aircraft, 2 degraded -> 25.0% -> High (2)
    try std.testing.expectEqual(@as(u16, 2), out_row[2]);
    try std.testing.expectEqual(@as(u16, 6), out_col[2]);
    try std.testing.expectEqual(@as(u32, 4), out_aircraft[2]);
    try std.testing.expectEqual(@as(u32, 2), out_degraded[2]);
    try std.testing.expectApproxEqAbs(@as(f32, 25.0), out_percent[2], 1e-4);
    try std.testing.expectEqual(@as(u8, 2), out_level[2]);
}

test "binCells skips out-of-bounds and non-finite coordinates" {
    const cell_deg: f64 = 90.0; // rows = 2, cols = 4 (8 cells)
    var cell_aircraft: [8]u32 = undefined;
    var cell_degraded: [8]u32 = undefined;

    var out_row: [4]u16 = undefined;
    var out_col: [4]u16 = undefined;
    var out_aircraft: [4]u32 = undefined;
    var out_degraded: [4]u32 = undefined;
    var out_percent: [4]f32 = undefined;
    var out_level: [4]u8 = undefined;

    const lons = [_]f64{
        10.0,               12.0,              14.0, // valid (3 aircraft in row 1, col 2)
        -185.0,             185.0,             0.0,  // lon out of bounds, lat out of bounds below
        std.math.nan(f64),  std.math.inf(f64),
    };
    const lats = [_]f64{
        10.0,               12.0,              14.0,
        10.0,               10.0,              95.0,
        10.0,               10.0,
    };
    const degraded = [_]u8{ 1, 1, 1, 1, 1, 1, 1, 1 };

    const written = gnss.binCells(
        &lons,
        &lats,
        &degraded,
        lons.len,
        cell_deg,
        3,
        &cell_aircraft,
        &cell_degraded,
        &out_row,
        &out_col,
        &out_aircraft,
        &out_degraded,
        &out_percent,
        &out_level,
    );

    try std.testing.expectEqual(@as(usize, 1), written);
    try std.testing.expectEqual(@as(u32, 3), out_aircraft[0]);
    try std.testing.expectEqual(@as(u32, 3), out_degraded[0]);
}
