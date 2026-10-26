//! GNSS Navigation Integrity Classification & Spatial Cell Binning Micro-Kernel
//!
//! Evaluates DO-260A/B navigation accuracy indicators (NIC and NACp) and GPS
//! loss flags per 14 CFR 91.227(c) performance minima, discounts single-aircraft
//! avionics faults using the gpsjam.org formula, and bins observations into
//! equirectangular spatial cells with zero heap allocations.

const std = @import("std");

/// Navigation Integrity Category threshold. NIC < 7 indicates containment radius >= 0.2 NM.
pub const NIC_THRESHOLD: i32 = 7;

/// Navigation Accuracy Category for Position threshold. NACp < 8 indicates uncertainty >= 93 m.
pub const NACP_THRESHOLD: i32 = 8;

/// Band threshold for Medium anomaly severity (percent degraded after 1-aircraft discount).
pub const MEDIUM_PERCENT_THRESHOLD: f64 = 2.0;

/// Band threshold for High anomaly severity (percent degraded after 1-aircraft discount).
pub const HIGH_PERCENT_THRESHOLD: f64 = 10.0;

/// Severity level codes written to output cell buffers.
pub const LEVEL_LOW: u8 = 0;
pub const LEVEL_MEDIUM: u8 = 1;
pub const LEVEL_HIGH: u8 = 2;

/// Classifies an aircraft observation as degraded (`true`) or nominal (`false`)
/// according to DO-260A/B navigation integrity rules:
/// - `gps_lost` is true -> `true`
/// - `nic >= 0 and nic < 7` -> `true`
/// - `nac_p >= 0 and nac_p < 8` -> `true`
/// - `nic < 0 and nac_p < 0` (both unreported and not gps_lost) -> `false`
/// - otherwise -> `false`
pub fn classifyObservation(nic: i32, nac_p: i32, gps_lost: bool) bool {
    if (gps_lost) return true;
    if (nic >= 0 and nic < NIC_THRESHOLD) return true;
    if (nac_p >= 0 and nac_p < NACP_THRESHOLD) return true;
    if (nic < 0 and nac_p < 0) return false;
    return false;
}

/// Computes the discounted degraded aircraft percentage using the gpsjam.org
/// 1-aircraft false-positive discount formula:
/// `100.0 * max(0, degraded - 1) / aircraft`.
pub fn discountDegradedPercent(aircraft: u32, degraded: u32) f64 {
    if (aircraft == 0 or degraded <= 1) return 0.0;
    return 100.0 * @as(f64, @floatFromInt(degraded - 1)) / @as(f64, @floatFromInt(aircraft));
}

/// Classifies a cell's discounted degraded percentage into an anomaly level:
/// - `0` (`Low`) if `percent_degraded < 2.0`
/// - `1` (`Medium`) if `percent_degraded >= 2.0 and percent_degraded <= 10.0`
/// - `2` (`High`) if `percent_degraded > 10.0`
pub fn classifyLevel(percent_degraded: f64) u8 {
    if (!std.math.isFinite(percent_degraded) or percent_degraded < MEDIUM_PERCENT_THRESHOLD) {
        return LEVEL_LOW;
    }
    if (percent_degraded <= HIGH_PERCENT_THRESHOLD) {
        return LEVEL_MEDIUM;
    }
    return LEVEL_HIGH;
}

/// Bins `num_obs` aircraft observations into a world grid of `cell_deg` resolution
/// (`rows = ceil(180 / cell_deg)`, `cols = ceil(360 / cell_deg)`).
///
/// Caller provides scratch buffers `cell_aircraft` and `cell_degraded` of length
/// at least `rows * cols`, and output buffers for active cells meeting
/// `aircraft >= min_aircraft`.
///
/// Out-of-bounds or non-finite coordinates are skipped.
/// Returns the number of active cells written to the output buffers.
pub fn binCells(
    lons: []const f64,
    lats: []const f64,
    degraded_flags: []const u8,
    num_obs: usize,
    cell_deg: f64,
    min_aircraft: u32,
    cell_aircraft: []u32,
    cell_degraded: []u32,
    out_row: []u16,
    out_col: []u16,
    out_aircraft: []u32,
    out_degraded: []u32,
    out_percent: []f32,
    out_level: []u8,
) usize {
    if (!std.math.isFinite(cell_deg) or cell_deg <= 0.0 or cell_deg > 180.0) return 0;

    const rows: usize = @intFromFloat(@ceil(180.0 / cell_deg));
    const cols: usize = @intFromFloat(@ceil(360.0 / cell_deg));
    if (rows == 0 or cols == 0) return 0;

    const total_cells = rows * cols;
    if (cell_aircraft.len < total_cells or cell_degraded.len < total_cells) return 0;

    @memset(cell_aircraft[0..total_cells], 0);
    @memset(cell_degraded[0..total_cells], 0);

    if (num_obs == 0) return 0;
    if (lons.len < num_obs or lats.len < num_obs or degraded_flags.len < num_obs) return 0;

    const max_out = @min(
        @min(@min(out_row.len, out_col.len), @min(out_aircraft.len, out_degraded.len)),
        @min(out_percent.len, out_level.len),
    );
    if (max_out == 0) return 0;

    for (0..num_obs) |i| {
        const lon = lons[i];
        const lat = lats[i];

        if (!std.math.isFinite(lon) or !std.math.isFinite(lat)) continue;
        if (lon < -180.0 or lon > 180.0 or lat < -90.0 or lat > 90.0) continue;

        const clamped_lat = @min(lat, 89.999999);
        const clamped_lon = @min(lon, 179.999999);
        const row_f = @floor((clamped_lat + 90.0) / cell_deg);
        const col_f = @floor((clamped_lon + 180.0) / cell_deg);

        const row = @min(@as(usize, @intFromFloat(@max(0.0, row_f))), rows - 1);
        const col = @min(@as(usize, @intFromFloat(@max(0.0, col_f))), cols - 1);

        const cell_idx = row * cols + col;
        cell_aircraft[cell_idx] +|= 1;
        if (degraded_flags[i] != 0) {
            cell_degraded[cell_idx] +|= 1;
        }
    }

    const threshold = @max(@as(u32, 1), min_aircraft);
    var written: usize = 0;

    for (0..rows) |r| {
        for (0..cols) |c| {
            const cell_idx = r * cols + c;
            const ac = cell_aircraft[cell_idx];
            if (ac < threshold) continue;
            if (written >= max_out) return written;

            const deg = cell_degraded[cell_idx];
            const pct = discountDegradedPercent(ac, deg);
            const lvl = classifyLevel(pct);

            out_row[written] = @intCast(r);
            out_col[written] = @intCast(c);
            out_aircraft[written] = ac;
            out_degraded[written] = deg;
            out_percent[written] = @floatCast(pct);
            out_level[written] = lvl;
            written += 1;
        }
    }

    return written;
}
