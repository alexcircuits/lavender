//! Lavender High-Performance Zig Kernel
//!
//! Provides ultra-low-latency signal demodulation, Mode-S CPR navigation decoding,
//! and satellite orbit kernels for WebAssembly and native edge targets.

pub const cpr = @import("cpr.zig");
pub const adsb = @import("adsb.zig");
pub const iq = @import("iq.zig");
pub const sgp4 = @import("sgp4.zig");
pub const wind = @import("wind.zig");
pub const labels = @import("labels.zig");
pub const geometry = @import("geometry.zig");
pub const cctv = @import("cctv.zig");

// Re-export core types
pub const GeoPoint = cpr.GeoPoint;
pub const DecodedMessage = adsb.DecodedMessage;
pub const Velocity = adsb.Velocity;
pub const AdsbType = adsb.AdsbType;
pub const SatRec = sgp4.SatRec;
pub const SatelliteElements = sgp4.SatelliteElements;
pub const WindGrid = wind.WindGrid;
pub const LabelCandidate = labels.LabelCandidate;
pub const CameraPose = cctv.CameraPose;
pub const FrustumGeometry = cctv.FrustumGeometry;

// Dummy entry for freestanding WASM runtimes
export fn _start() void {}

// C-ABI exports for WASM and dynamic linking

export fn lavender_cpr_nl(lat: f64) u32 {
    return cpr.cprNl(lat);
}

export fn lavender_modes_checksum(frame: *const [14]u8) u32 {
    return adsb.modeSChecksum(frame);
}

export fn lavender_decode_global_airborne_cpr(
    even_lat: u32,
    even_lon: u32,
    odd_lat: u32,
    odd_lon: u32,
    use_odd: bool,
    out_lat: *f64,
    out_lon: *f64,
) bool {
    if (cpr.decodeGlobalAirborneCpr(even_lat, even_lon, odd_lat, odd_lon, use_odd)) |res| {
        out_lat.* = res.latitude;
        out_lon.* = res.longitude;
        return true;
    }
    return false;
}

/// SGP4: Propagates satellite position at `tsince_minutes` to ECEF coordinates (metres).
export fn lavender_sgp4_propagate_ecf_m(
    bstar: f64,
    inclo_rad: f64,
    nodeo_rad: f64,
    ecco: f64,
    argpo_rad: f64,
    mo_rad: f64,
    no_kozai_rad_min: f64,
    tsince_min: f64,
    gmst_rad: f64,
    out_ecf_m: *[3]f64,
) bool {
    const el = SatelliteElements{
        .epoch_days = 0.0,
        .bstar = bstar,
        .inclination_rad = inclo_rad,
        .raan_rad = nodeo_rad,
        .eccentricity = ecco,
        .arg_perigee_rad = argpo_rad,
        .mean_anomaly_rad = mo_rad,
        .mean_motion_rad_min = no_kozai_rad_min,
    };
    const con = sgp4.GravitationalConstants{};
    const sat = sgp4.initSatrec(el, con) catch return false;
    const teme = sgp4.propagateTeme(&sat, tsince_min) catch return false;
    const ecf_km = sgp4.temeToEcf(teme, gmst_rad);

    out_ecf_m[0] = ecf_km.x * 1000.0;
    out_ecf_m[1] = ecf_km.y * 1000.0;
    out_ecf_m[2] = ecf_km.z * 1000.0;
    return true;
}

/// SGP4: Batch propagates an array of satellites directly into a contiguous float buffer for Cesium.
/// `elements_ptr`: array of (7 floats per satellite: bstar, incl, raan, ecc, argp, mo, no_kozai)
/// `tsince_array_ptr`: array of tsince (minutes) per satellite
/// `out_coords_m`: output flat buffer of (3 floats per satellite: x, y, z in metres)
export fn lavender_sgp4_batch_propagate(
    elements_ptr: [*]const f64,
    tsince_array_ptr: [*]const f64,
    count: usize,
    gmst_rad: f64,
    out_coords_m: [*]f32,
) usize {
    var success_count: usize = 0;
    const con = sgp4.GravitationalConstants{};

    for (0..count) |i| {
        const base = i * 7;
        const el = SatelliteElements{
            .epoch_days = 0.0,
            .bstar = elements_ptr[base + 0],
            .inclination_rad = elements_ptr[base + 1],
            .raan_rad = elements_ptr[base + 2],
            .eccentricity = elements_ptr[base + 3],
            .arg_perigee_rad = elements_ptr[base + 4],
            .mean_anomaly_rad = elements_ptr[base + 5],
            .mean_motion_rad_min = elements_ptr[base + 6],
        };
        const tsince = tsince_array_ptr[i];

        if (sgp4.initSatrec(el, con)) |sat| {
            if (sgp4.propagateTeme(&sat, tsince)) |teme| {
                const ecf = sgp4.temeToEcf(teme, gmst_rad);
                out_coords_m[i * 3 + 0] = @floatCast(ecf.x * 1000.0);
                out_coords_m[i * 3 + 1] = @floatCast(ecf.y * 1000.0);
                out_coords_m[i * 3 + 2] = @floatCast(ecf.z * 1000.0);
                success_count += 1;
                continue;
            } else |_| {}
        } else |_| {}

        out_coords_m[i * 3 + 0] = 0.0;
        out_coords_m[i * 3 + 1] = 0.0;
        out_coords_m[i * 3 + 2] = 0.0;
    }

    return success_count;
}

export fn lavender_delta_decode_coords(
    deltas_ptr: [*]const i32,
    deltas_len: usize,
    scale: f64,
    out_coords_ptr: [*]f64,
    max_coords: usize,
) usize {
    if (deltas_len < 2 or max_coords < 2) return 0;
    const deltas = deltas_ptr[0..deltas_len];
    const out_coords = out_coords_ptr[0..max_coords];
    return geometry.deltaDecodeCoordinates(deltas, scale, out_coords);
}

export fn lavender_simplify_rdp(
    coords_ptr: [*]const f64,
    num_points: usize,
    epsilon: f64,
    out_coords_ptr: [*]f64,
    max_out_points: usize,
) usize {
    if (num_points < 2 or max_out_points < 2) return 0;
    const coords = coords_ptr[0..(num_points * 2)];
    const out_coords = out_coords_ptr[0..(max_out_points * 2)];
    return geometry.simplifyRdp(coords, num_points, epsilon, out_coords);
}

export fn lavender_point_in_polygon(
    test_lon: f64,
    test_lat: f64,
    poly_coords_ptr: [*]const f64,
    num_points: usize,
) bool {
    if (num_points < 3) return false;
    const poly_coords = poly_coords_ptr[0..(num_points * 2)];
    return geometry.pointInPolygon(test_lon, test_lat, poly_coords, num_points);
}

// Force analysis of exported functions in submodules
comptime {
    _ = wind.lavender_wind_sample;
    _ = wind.lavender_wind_bake_streamline;
    _ = labels.lavender_arbitrate_labels;
    _ = cctv.lavender_cctv_compute_frustum;
    _ = cctv.lavender_cctv_batch_wireframe;
    _ = lavender_delta_decode_coords;
    _ = lavender_simplify_rdp;
    _ = lavender_point_in_polygon;
}

test {
    _ = @import("adsb_test.zig");
    _ = @import("sgp4_test.zig");
    _ = @import("wind_test.zig");
    _ = @import("labels_test.zig");
    _ = @import("geometry_test.zig");
    _ = @import("cctv_test.zig");
}
