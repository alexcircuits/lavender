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
pub const heatmap = @import("heatmap.zig");
pub const gnss = @import("gnss.zig");
pub const sky = @import("sky.zig");
pub const kinematics = @import("kinematics.zig");

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

/// Batch geofence geometry: tests `num_points` interleaved [lon, lat] points
/// against one polygon ring. Writes 1/0 per point into `out_hits_ptr` and
/// returns the number of points inside the ring. Antimeridian-safe.
export fn lavender_point_in_polygon_batch(
    points_ptr: [*]const f64,
    num_points: usize,
    poly_coords_ptr: [*]const f64,
    num_vertices: usize,
    out_hits_ptr: [*]u8,
) usize {
    if (num_points == 0 or num_vertices < 3) return 0;
    return geometry.pointInPolygonBatch(
        points_ptr[0 .. num_points * 2],
        num_points,
        poly_coords_ptr[0 .. num_vertices * 2],
        num_vertices,
        out_hits_ptr[0..num_points],
    );
}

/// Geofence hit-testing: tests one point against `num_rings` packed polygon
/// rings. `rings_ptr` holds every ring's interleaved [lon, lat] vertices
/// back-to-back; `ring_lengths_ptr` holds each ring's vertex count. Writes 1/0
/// per ring into `out_hits_ptr` and returns the number of rings hit. Antimeridian-safe.
export fn lavender_point_in_polygons(
    test_lon: f64,
    test_lat: f64,
    rings_ptr: [*]const f64,
    ring_lengths_ptr: [*]const u32,
    num_rings: usize,
    out_hits_ptr: [*]u8,
) usize {
    if (num_rings == 0) return 0;
    var total_vertices: usize = 0;
    for (ring_lengths_ptr[0..num_rings]) |len| total_vertices += len;
    return geometry.pointInPolygons(
        test_lon,
        test_lat,
        rings_ptr[0 .. total_vertices * 2],
        ring_lengths_ptr[0..num_rings],
        num_rings,
        out_hits_ptr[0..num_rings],
    );
}

/// Heatmap binning: projects `num_points` interleaved [lon, lat, weight] points
/// onto a `width` x `height` equirectangular grid over [lon_min, lon_max] x
/// [lat_min, lat_max], accumulating weighted sums and counts into two f32
/// planes (row 0 = lat_max, i.e. north at the top). Returns the number of
/// points binned; out-of-bounds and non-finite points are skipped.
export fn lavender_heatmap_bin(
    points_ptr: [*]const f64,
    num_points: usize,
    width: usize,
    height: usize,
    lon_min: f64,
    lon_max: f64,
    lat_min: f64,
    lat_max: f64,
    out_weight_sum_ptr: [*]f32,
    out_count_ptr: [*]f32,
) usize {
    if (num_points == 0 or width == 0 or height == 0) return 0;
    const cells = width * height;
    return heatmap.binEquirect(
        points_ptr[0 .. num_points * 3],
        num_points,
        width,
        height,
        lon_min,
        lon_max,
        lat_min,
        lat_max,
        out_weight_sum_ptr[0..cells],
        out_count_ptr[0..cells],
    );
}

/// GNSS DO-260A/B navigation integrity observation classifier.
export fn lavender_gnss_classify_observation(
    nic: i32,
    nac_p: i32,
    gps_lost: bool,
) bool {
    return gnss.classifyObservation(nic, nac_p, gps_lost);
}

/// GNSS 1-aircraft false-positive discount percentage calculator (gpsjam.org formula).
export fn lavender_gnss_discount_percent(
    aircraft: u32,
    degraded: u32,
) f64 {
    return gnss.discountDegradedPercent(aircraft, degraded);
}

/// GNSS severity band classifier (`0` = Low, `1` = Medium, `2` = High).
export fn lavender_gnss_classify_level(
    percent_degraded: f64,
) u8 {
    return gnss.classifyLevel(percent_degraded);
}

/// GNSS spatial cell binning kernel.
/// Bins `num_obs` aircraft observations into a world grid of `cell_deg` resolution
/// and writes all active cells (`aircraft >= min_aircraft`) into the output arrays.
export fn lavender_gnss_bin_cells(
    lons_ptr: [*]const f64,
    lats_ptr: [*]const f64,
    degraded_flags_ptr: [*]const u8,
    num_obs: usize,
    cell_deg: f64,
    min_aircraft: u32,
    cell_aircraft_ptr: [*]u32,
    cell_degraded_ptr: [*]u32,
    scratch_len: usize,
    out_row_ptr: [*]u16,
    out_col_ptr: [*]u16,
    out_aircraft_ptr: [*]u32,
    out_degraded_ptr: [*]u32,
    out_percent_ptr: [*]f32,
    out_level_ptr: [*]u8,
    max_out_cells: usize,
) usize {
    if (num_obs == 0 or scratch_len == 0 or max_out_cells == 0) return 0;
    return gnss.binCells(
        lons_ptr[0..num_obs],
        lats_ptr[0..num_obs],
        degraded_flags_ptr[0..num_obs],
        num_obs,
        cell_deg,
        min_aircraft,
        cell_aircraft_ptr[0..scratch_len],
        cell_degraded_ptr[0..scratch_len],
        out_row_ptr[0..max_out_cells],
        out_col_ptr[0..max_out_cells],
        out_aircraft_ptr[0..max_out_cells],
        out_degraded_ptr[0..max_out_cells],
        out_percent_ptr[0..max_out_cells],
        out_level_ptr[0..max_out_cells],
    );
}

/// SDR 2 Msps I/Q demodulator: extracts valid 112-bit (14-byte) Mode-S frames
/// from unsigned 8-bit interleaved I/Q samples.
export fn lavender_iq_extract_adsb_frames(
    iq_ptr: [*]const u8,
    iq_len: usize,
    power_scratch_ptr: [*]f32,
    power_scratch_len: usize,
    out_frames_ptr: [*][14]u8,
    max_frames: usize,
) usize {
    if (iq_len == 0 or power_scratch_len == 0 or max_frames == 0) return 0;
    return iq.extractAdsbFrames(
        iq_ptr[0..iq_len],
        power_scratch_ptr[0..power_scratch_len],
        out_frames_ptr[0..max_frames],
    );
}

/// Mode-S Extended Squitter (DF17/DF18) frame decoder C-ABI export.
/// `out_msg_type`: 0 = callsign, 1 = surface_position, 2 = airborne_position, 3 = velocity, 4 = other.
export fn lavender_adsb_decode_frame(
    frame_ptr: *const [14]u8,
    out_icao: *[6]u8,
    out_type_code: *u8,
    out_msg_type: *u8,
    out_callsign: *[8]u8,
    out_altitude_ft: *i32,
    out_has_altitude: *u8,
    out_on_ground: *u8,
    out_speed_kt: *f64,
    out_heading_deg: *f64,
    out_vr_fpm: *i32,
    out_has_velocity: *u8,
    out_cpr_odd: *u8,
    out_cpr_surface: *u8,
    out_cpr_lat: *u32,
    out_cpr_lon: *u32,
    out_has_cpr: *u8,
) bool {
    const msg = adsb.decodeAdsbFrame(frame_ptr) orelse return false;

    out_icao.* = msg.icao;
    out_type_code.* = msg.type_code;
    out_msg_type.* = switch (msg.msg_type) {
        .callsign => 0,
        .surface_position => 1,
        .airborne_position => 2,
        .velocity => 3,
        .other => 4,
    };

    if (msg.callsign) |cs| {
        out_callsign.* = cs;
    } else {
        out_callsign.* = [_]u8{' '} ** 8;
    }

    if (msg.altitude_ft) |alt| {
        out_altitude_ft.* = alt;
        out_has_altitude.* = 1;
    } else {
        out_altitude_ft.* = 0;
        out_has_altitude.* = 0;
    }

    out_on_ground.* = if (msg.on_ground) 1 else 0;

    if (msg.velocity) |vel| {
        out_speed_kt.* = vel.speed_kt;
        out_heading_deg.* = vel.heading_deg;
        out_vr_fpm.* = vel.vertical_rate_fpm orelse 0;
        out_has_velocity.* = 1;
    } else {
        out_speed_kt.* = 0.0;
        out_heading_deg.* = 0.0;
        out_vr_fpm.* = 0;
        out_has_velocity.* = 0;
    }

    if (msg.cpr) |cpr_payload| {
        out_cpr_odd.* = if (cpr_payload.odd) 1 else 0;
        out_cpr_surface.* = if (cpr_payload.surface) 1 else 0;
        out_cpr_lat.* = cpr_payload.latitude_raw;
        out_cpr_lon.* = cpr_payload.longitude_raw;
        out_has_cpr.* = 1;
    } else {
        out_cpr_odd.* = 0;
        out_cpr_surface.* = 0;
        out_cpr_lat.* = 0;
        out_cpr_lon.* = 0;
        out_has_cpr.* = 0;
    }

    return true;
}

// Force analysis of exported functions in submodules

/// Sky: decodes `star_count` catalogue stars (`[ra, dec, vmag, bv]` f32 each) into
/// `out_vertices` (8 f32 per star: xyz, sigma_px, rgb, alpha). Returns stars written.
export fn lavender_sky_build_star_vertices(
    stars_ptr: [*]const f32,
    star_count: usize,
    out_vertices: [*]f32,
) usize {
    return sky.buildStarVertices(stars_ptr[0 .. star_count * 4], out_vertices[0 .. star_count * sky.STAR_VERTEX_FLOATS]);
}

/// Kinematics: advances a heading (deg) toward target along the shortest arc.
export fn lavender_kinematics_slew_heading(current_deg: f64, target_deg: f64, max_step_deg: f64) f64 {
    return kinematics.slewHeading(current_deg, target_deg, max_step_deg);
}

/// Kinematics: bounded cockpit anchor correction (metres) for one render step.
export fn lavender_kinematics_anchor_correction(distance_m: f64, speed_mps: f64, dt_sec: f64) f64 {
    return kinematics.cockpitAnchorCorrectionStep(distance_m, speed_mps, dt_sec);
}

/// Director: samples a camera move. `from_ptr`/`to_ptr`/`out_pose` are 6 f64
/// `[lat, lon, alt, heading, pitch, roll]`. `linear` selects linear easing.
export fn lavender_director_sample_camera_move(
    from_ptr: [*]const f64,
    to_ptr: [*]const f64,
    linear: bool,
    progress: f64,
    out_pose: [*]f64,
) void {
    const pose = kinematics.sampleCameraMove(from_ptr[0..6].*, to_ptr[0..6].*, linear, progress);
    for (pose, 0..) |v, i| out_pose[i] = v;
}

/// Weather: derives effect strengths into `out_profile` (9 f32: cloud, rain, snow,
/// fog, haze, droplets, storm, wind, windDirDeg). NaN `weather_code` means missing
/// weather; returns false (profile zeroed) when unavailable.
export fn lavender_weather_derive_effects(
    weather_code: f64,
    cloud_cover_pct: f64,
    precipitation_mm: f64,
    wind_kph: f64,
    visibility_m: f64,
    wind_direction_deg: f64,
    out_profile: [*]f32,
) bool {
    const p = kinematics.deriveWeatherEffectProfile(weather_code, cloud_cover_pct, precipitation_mm, wind_kph, visibility_m, wind_direction_deg);
    kinematics.writeProfile(p, out_profile[0..kinematics.WEATHER_PROFILE_FLOATS]);
    return p.available;
}

comptime {
    _ = wind.lavender_wind_sample;
    _ = wind.lavender_wind_bake_streamline;
    _ = labels.lavender_arbitrate_labels;
    _ = cctv.lavender_cctv_compute_frustum;
    _ = cctv.lavender_cctv_batch_wireframe;
    _ = lavender_delta_decode_coords;
    _ = lavender_simplify_rdp;
    _ = lavender_point_in_polygon;
    _ = lavender_point_in_polygon_batch;
    _ = lavender_point_in_polygons;
    _ = lavender_heatmap_bin;
    _ = lavender_gnss_classify_observation;
    _ = lavender_gnss_discount_percent;
    _ = lavender_gnss_classify_level;
    _ = lavender_gnss_bin_cells;
    _ = lavender_sky_build_star_vertices;
    _ = lavender_kinematics_slew_heading;
    _ = lavender_kinematics_anchor_correction;
    _ = lavender_director_sample_camera_move;
    _ = lavender_weather_derive_effects;
    _ = lavender_iq_extract_adsb_frames;
    _ = lavender_adsb_decode_frame;
}

test {
    _ = @import("adsb_test.zig");
    _ = @import("sgp4_test.zig");
    _ = @import("wind_test.zig");
    _ = @import("labels_test.zig");
    _ = @import("geometry_test.zig");
    _ = @import("cctv_test.zig");
    _ = @import("heatmap_test.zig");
    _ = @import("gnss_test.zig");
    _ = @import("sky_test.zig");
    _ = @import("kinematics_test.zig");
}
