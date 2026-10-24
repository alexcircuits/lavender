//! CCTV Camera Viewshed and Frustum Geometry Micro-Kernel
//!
//! Provides high-speed, zero-allocation 3D spherical trigonometry to compute
//! camera viewing frustums, ground projections, and wireframe vertices for Cesium.
//! Matches legacy projection.js / geometry.js with zero JavaScript GC overhead.

const std = @import("std");

const WGS84_A: f64 = 6378137.0; // Earth semi-major axis (metres)
const PI: f64 = std.math.pi;
const DEG_TO_RAD: f64 = PI / 180.0;
const RAD_TO_DEG: f64 = 180.0 / PI;

pub const FrustumPoint = struct {
    lon: f64,
    lat: f64,
    alt: f64,
};

pub const CameraPose = struct {
    lat: f64,
    lon: f64,
    ground_alt_m: f64,
    mount_height_m: f64,
    heading_deg: f64,
    pitch_deg: f64,
    fov_deg: f64,
    range_m: f64,
};

pub const FrustumGeometry = struct {
    mount: FrustumPoint,
    cap_center: FrustumPoint,
    tl: FrustumPoint,
    tr: FrustumPoint,
    br: FrustumPoint,
    bl: FrustumPoint,
    half_w: f64,
    half_h: f64,
    range_m: f64,
};

/// Great-circle spherical forward projection.
/// Projects from (lat, lon) along bearing (degrees) for a horizontal distance (metres).
pub fn projectPoint(lat_deg: f64, lon_deg: f64, heading_deg: f64, distance_m: f64) [2]f64 {
    if (distance_m == 0.0) {
        return .{ lon_deg, lat_deg };
    }

    const d = distance_m / WGS84_A;
    const lat_rad = lat_deg * DEG_TO_RAD;
    const lon_rad = lon_deg * DEG_TO_RAD;
    const brg_rad = heading_deg * DEG_TO_RAD;

    const sin_lat = @sin(lat_rad);
    const cos_lat = @cos(lat_rad);
    const sin_d = @sin(d);
    const cos_d = @cos(d);

    const dest_lat_rad = std.math.asin(sin_lat * cos_d + cos_lat * sin_d * @cos(brg_rad));
    const dest_lon_rad = lon_rad + std.math.atan2(
        @sin(brg_rad) * sin_d * cos_lat,
        cos_d - sin_lat * @sin(dest_lat_rad),
    );

    var dest_lon = dest_lon_rad * RAD_TO_DEG;
    while (dest_lon > 180.0) dest_lon -= 360.0;
    while (dest_lon < -180.0) dest_lon += 360.0;

    return .{ dest_lon, dest_lat_rad * RAD_TO_DEG };
}

/// Computes the complete 3D viewing frustum from camera pose.
pub fn computeFrustum(pose: CameraPose) FrustumGeometry {
    const pitch_clamped = std.math.clamp(pose.pitch_deg, -89.0, 89.0);
    const fov_clamped = std.math.clamp(pose.fov_deg, 8.0, 160.0);
    const range = @max(1.0, pose.range_m);
    const mount_alt = pose.ground_alt_m + pose.mount_height_m;

    const pitch_rad = pitch_clamped * DEG_TO_RAD;
    const fov_rad = fov_clamped * DEG_TO_RAD;

    const horiz_dist = range * @cos(pitch_rad);
    const vert_dist = range * @sin(pitch_rad);

    const cap_center_coords = projectPoint(pose.lat, pose.lon, pose.heading_deg, horiz_dist);
    const cap_center_alt = mount_alt + vert_dist;

    const half_w = range * @tan(fov_rad / 2.0);
    // 16:9 vertical FOV derivation:
    const v_fov_rad = 2.0 * std.math.atan(@tan(fov_rad / 2.0) / (16.0 / 9.0));
    const half_h = range * @tan(v_fov_rad / 2.0);

    const left_brg = pose.heading_deg - 90.0;
    const right_brg = pose.heading_deg + 90.0;

    const up_vert = @cos(pitch_rad) * half_h;
    const up_horiz = -@sin(pitch_rad) * half_h;

    // tl: +up_horiz along heading, +half_w along left_brg, alt = cap_center_alt + up_vert
    const tl_center = projectPoint(cap_center_coords[1], cap_center_coords[0], pose.heading_deg, up_horiz);
    const tl_coords = projectPoint(tl_center[1], tl_center[0], left_brg, half_w);

    // tr: +up_horiz along heading, +half_w along right_brg, alt = cap_center_alt + up_vert
    const tr_coords = projectPoint(tl_center[1], tl_center[0], right_brg, half_w);

    // br: -up_horiz along heading, +half_w along right_brg, alt = cap_center_alt - up_vert
    const br_center = projectPoint(cap_center_coords[1], cap_center_coords[0], pose.heading_deg, -up_horiz);
    const br_coords = projectPoint(br_center[1], br_center[0], right_brg, half_w);

    // bl: -up_horiz along heading, +half_w along left_brg, alt = cap_center_alt - up_vert
    const bl_coords = projectPoint(br_center[1], br_center[0], left_brg, half_w);

    return FrustumGeometry{
        .mount = .{ .lon = pose.lon, .lat = pose.lat, .alt = mount_alt },
        .cap_center = .{ .lon = cap_center_coords[0], .lat = cap_center_coords[1], .alt = cap_center_alt },
        .tl = .{ .lon = tl_coords[0], .lat = tl_coords[1], .alt = cap_center_alt + up_vert },
        .tr = .{ .lon = tr_coords[0], .lat = tr_coords[1], .alt = cap_center_alt + up_vert },
        .br = .{ .lon = br_coords[0], .lat = br_coords[1], .alt = cap_center_alt - up_vert },
        .bl = .{ .lon = bl_coords[0], .lat = bl_coords[1], .alt = cap_center_alt - up_vert },
        .half_w = half_w,
        .half_h = half_h,
        .range_m = range,
    };
}

// ============================================================================
// C-ABI Exports for WASM
// ============================================================================

/// Computes a single camera's 3D frustum points.
/// `out_coords_ptr`: must point to buffer of at least 18 f64 values (6 points * [lon, lat, alt]):
/// [mount, cap_center, tl, tr, br, bl]
pub export fn lavender_cctv_compute_frustum(
    lat: f64,
    lon: f64,
    ground_alt_m: f64,
    mount_height_m: f64,
    heading_deg: f64,
    pitch_deg: f64,
    fov_deg: f64,
    range_m: f64,
    out_coords_ptr: [*]f64,
) bool {
    const pose = CameraPose{
        .lat = lat,
        .lon = lon,
        .ground_alt_m = ground_alt_m,
        .mount_height_m = mount_height_m,
        .heading_deg = heading_deg,
        .pitch_deg = pitch_deg,
        .fov_deg = fov_deg,
        .range_m = range_m,
    };
    const g = computeFrustum(pose);

    // Write mount
    out_coords_ptr[0] = g.mount.lon;
    out_coords_ptr[1] = g.mount.lat;
    out_coords_ptr[2] = g.mount.alt;

    // Write cap_center
    out_coords_ptr[3] = g.cap_center.lon;
    out_coords_ptr[4] = g.cap_center.lat;
    out_coords_ptr[5] = g.cap_center.alt;

    // Write tl
    out_coords_ptr[6] = g.tl.lon;
    out_coords_ptr[7] = g.tl.lat;
    out_coords_ptr[8] = g.tl.alt;

    // Write tr
    out_coords_ptr[9] = g.tr.lon;
    out_coords_ptr[10] = g.tr.lat;
    out_coords_ptr[11] = g.tr.alt;

    // Write br
    out_coords_ptr[12] = g.br.lon;
    out_coords_ptr[13] = g.br.lat;
    out_coords_ptr[14] = g.br.alt;

    // Write bl
    out_coords_ptr[15] = g.bl.lon;
    out_coords_ptr[16] = g.bl.lat;
    out_coords_ptr[17] = g.bl.alt;

    return true;
}

/// Batch computes wireframe segments for an array of cameras.
/// Each camera generates 8 line segments (16 vertices * 3 coords [lon, lat, alt] = 48 f32 floats):
/// - 4 rays from mount to tl, tr, br, bl
/// - 4 perimeter edges tl->tr, tr->br, br->bl, bl->tl
/// `cameras_ptr`: flat f64 array of 8 fields per camera (lat, lon, ground, mount, heading, pitch, fov, range)
/// `out_wireframe_ptr`: flat f32 array receiving 48 floats per camera
pub export fn lavender_cctv_batch_wireframe(
    cameras_ptr: [*]const f64,
    camera_count: usize,
    out_wireframe_ptr: [*]f32,
    max_cameras: usize,
) usize {
    const count = @min(camera_count, max_cameras);
    var out_idx: usize = 0;

    for (0..count) |i| {
        const base = i * 8;
        const pose = CameraPose{
            .lat = cameras_ptr[base + 0],
            .lon = cameras_ptr[base + 1],
            .ground_alt_m = cameras_ptr[base + 2],
            .mount_height_m = cameras_ptr[base + 3],
            .heading_deg = cameras_ptr[base + 4],
            .pitch_deg = cameras_ptr[base + 5],
            .fov_deg = cameras_ptr[base + 6],
            .range_m = cameras_ptr[base + 7],
        };

        const g = computeFrustum(pose);

        // Ray 1: mount -> tl
        out_wireframe_ptr[out_idx + 0] = @floatCast(g.mount.lon);
        out_wireframe_ptr[out_idx + 1] = @floatCast(g.mount.lat);
        out_wireframe_ptr[out_idx + 2] = @floatCast(g.mount.alt);
        out_wireframe_ptr[out_idx + 3] = @floatCast(g.tl.lon);
        out_wireframe_ptr[out_idx + 4] = @floatCast(g.tl.lat);
        out_wireframe_ptr[out_idx + 5] = @floatCast(g.tl.alt);

        // Ray 2: mount -> tr
        out_wireframe_ptr[out_idx + 6] = @floatCast(g.mount.lon);
        out_wireframe_ptr[out_idx + 7] = @floatCast(g.mount.lat);
        out_wireframe_ptr[out_idx + 8] = @floatCast(g.mount.alt);
        out_wireframe_ptr[out_idx + 9] = @floatCast(g.tr.lon);
        out_wireframe_ptr[out_idx + 10] = @floatCast(g.tr.lat);
        out_wireframe_ptr[out_idx + 11] = @floatCast(g.tr.alt);

        // Ray 3: mount -> br
        out_wireframe_ptr[out_idx + 12] = @floatCast(g.mount.lon);
        out_wireframe_ptr[out_idx + 13] = @floatCast(g.mount.lat);
        out_wireframe_ptr[out_idx + 14] = @floatCast(g.mount.alt);
        out_wireframe_ptr[out_idx + 15] = @floatCast(g.br.lon);
        out_wireframe_ptr[out_idx + 16] = @floatCast(g.br.lat);
        out_wireframe_ptr[out_idx + 17] = @floatCast(g.br.alt);

        // Ray 4: mount -> bl
        out_wireframe_ptr[out_idx + 18] = @floatCast(g.mount.lon);
        out_wireframe_ptr[out_idx + 19] = @floatCast(g.mount.lat);
        out_wireframe_ptr[out_idx + 20] = @floatCast(g.mount.alt);
        out_wireframe_ptr[out_idx + 21] = @floatCast(g.bl.lon);
        out_wireframe_ptr[out_idx + 22] = @floatCast(g.bl.lat);
        out_wireframe_ptr[out_idx + 23] = @floatCast(g.bl.alt);

        // Perimeter 1: tl -> tr
        out_wireframe_ptr[out_idx + 24] = @floatCast(g.tl.lon);
        out_wireframe_ptr[out_idx + 25] = @floatCast(g.tl.lat);
        out_wireframe_ptr[out_idx + 26] = @floatCast(g.tl.alt);
        out_wireframe_ptr[out_idx + 27] = @floatCast(g.tr.lon);
        out_wireframe_ptr[out_idx + 28] = @floatCast(g.tr.lat);
        out_wireframe_ptr[out_idx + 29] = @floatCast(g.tr.alt);

        // Perimeter 2: tr -> br
        out_wireframe_ptr[out_idx + 30] = @floatCast(g.tr.lon);
        out_wireframe_ptr[out_idx + 31] = @floatCast(g.tr.lat);
        out_wireframe_ptr[out_idx + 32] = @floatCast(g.tr.alt);
        out_wireframe_ptr[out_idx + 33] = @floatCast(g.br.lon);
        out_wireframe_ptr[out_idx + 34] = @floatCast(g.br.lat);
        out_wireframe_ptr[out_idx + 35] = @floatCast(g.br.alt);

        // Perimeter 3: br -> bl
        out_wireframe_ptr[out_idx + 36] = @floatCast(g.br.lon);
        out_wireframe_ptr[out_idx + 37] = @floatCast(g.br.lat);
        out_wireframe_ptr[out_idx + 38] = @floatCast(g.br.alt);
        out_wireframe_ptr[out_idx + 39] = @floatCast(g.bl.lon);
        out_wireframe_ptr[out_idx + 40] = @floatCast(g.bl.lat);
        out_wireframe_ptr[out_idx + 41] = @floatCast(g.bl.alt);

        // Perimeter 4: bl -> tl
        out_wireframe_ptr[out_idx + 42] = @floatCast(g.bl.lon);
        out_wireframe_ptr[out_idx + 43] = @floatCast(g.bl.lat);
        out_wireframe_ptr[out_idx + 44] = @floatCast(g.bl.alt);
        out_wireframe_ptr[out_idx + 45] = @floatCast(g.tl.lon);
        out_wireframe_ptr[out_idx + 46] = @floatCast(g.tl.lat);
        out_wireframe_ptr[out_idx + 47] = @floatCast(g.tl.alt);

        out_idx += 48;
    }

    return count;
}
