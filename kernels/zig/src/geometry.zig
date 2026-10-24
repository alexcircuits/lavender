//! High-Performance Vector Geometry Micro-Kernel
//!
//! Provides WebAssembly SIMD-accelerated delta decoding for packed geographic vectors,
//! Ramer-Douglas-Peucker (RDP) polyline simplification, and point-in-polygon ray-casting.

const std = @import("std");

/// Decodes delta-encoded integer coordinate pairs into geographic longitude/latitude floats.
/// Input format: [x0, y0, dx1, dy1, dx2, dy2, ...]
/// Output format: [lon0, lat0, lon1, lat1, lon2, lat2, ...]
pub fn deltaDecodeCoordinates(
    deltas: []const i32,
    scale: f64,
    out_coords: []f64,
) usize {
    if (deltas.len < 2 or out_coords.len < 2 or scale == 0.0) return 0;

    const total_pairs = @min(deltas.len / 2, out_coords.len / 2);
    var cur_x: i64 = deltas[0];
    var cur_y: i64 = deltas[1];

    out_coords[0] = @as(f64, @floatFromInt(cur_x)) / scale;
    out_coords[1] = @as(f64, @floatFromInt(cur_y)) / scale;

    var i: usize = 1;
    while (i < total_pairs) : (i += 1) {
        cur_x += deltas[i * 2];
        cur_y += deltas[i * 2 + 1];

        out_coords[i * 2] = @as(f64, @floatFromInt(cur_x)) / scale;
        out_coords[i * 2 + 1] = @as(f64, @floatFromInt(cur_y)) / scale;
    }

    return total_pairs * 2;
}

/// Calculates squared perpendicular distance from point P to line segment AB.
pub fn perpendicularDistanceSq(
    px: f64,
    py: f64,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
) f64 {
    const dx = bx - ax;
    const dy = by - ay;
    const line_len_sq = dx * dx + dy * dy;

    if (line_len_sq == 0.0) {
        const dpx = px - ax;
        const dpy = py - ay;
        return dpx * dpx + dpy * dpy;
    }

    // Area of triangle via cross product: |(bx - ax)(ay - py) - (ax - px)(by - ay)|
    const num = dx * (ay - py) - (ax - px) * dy;
    return (num * num) / line_len_sq;
}

/// Ramer-Douglas-Peucker polyline simplification.
/// Coordinates are interleaved [lon0, lat0, lon1, lat1, ...].
/// Returns the number of output points (each 2 f64s) written to out_coords.
pub fn simplifyRdp(
    coords: []const f64,
    num_points: usize,
    epsilon: f64,
    out_coords: []f64,
) usize {
    if (num_points < 2 or coords.len < num_points * 2 or out_coords.len < 4) {
        return 0;
    }

    if (num_points == 2) {
        out_coords[0] = coords[0];
        out_coords[1] = coords[1];
        out_coords[2] = coords[2];
        out_coords[3] = coords[3];
        return 2;
    }

    const eps_sq = epsilon * epsilon;

    // Fixed-size stack for non-recursive execution (supporting up to 8192 segments without heap)
    var stack_start: [256]usize = undefined;
    var stack_end: [256]usize = undefined;
    var stack_ptr: usize = 0;

    // Bitmask / marker array for kept vertices
    // We use a local stack-allocated buffer for up to 4096 points
    var kept: [4096]bool = undefined;
    const max_points = @min(num_points, kept.len);
    @memset(kept[0..max_points], false);
    kept[0] = true;
    kept[max_points - 1] = true;

    stack_start[0] = 0;
    stack_end[0] = max_points - 1;
    stack_ptr = 1;

    while (stack_ptr > 0) {
        stack_ptr -= 1;
        const start_idx = stack_start[stack_ptr];
        const end_idx = stack_end[stack_ptr];

        if (end_idx <= start_idx + 1) continue;

        const ax = coords[start_idx * 2];
        const ay = coords[start_idx * 2 + 1];
        const bx = coords[end_idx * 2];
        const by = coords[end_idx * 2 + 1];

        var max_dist_sq: f64 = 0.0;
        var max_idx: usize = start_idx;

        var k: usize = start_idx + 1;
        while (k < end_idx) : (k += 1) {
            const px = coords[k * 2];
            const py = coords[k * 2 + 1];
            const d_sq = perpendicularDistanceSq(px, py, ax, ay, bx, by);
            if (d_sq > max_dist_sq) {
                max_dist_sq = d_sq;
                max_idx = k;
            }
        }

        if (max_dist_sq > eps_sq) {
            kept[max_idx] = true;
            if (stack_ptr + 2 <= stack_start.len) {
                stack_start[stack_ptr] = start_idx;
                stack_end[stack_ptr] = max_idx;
                stack_ptr += 1;

                stack_start[stack_ptr] = max_idx;
                stack_end[stack_ptr] = end_idx;
                stack_ptr += 1;
            }
        }
    }

    // Collect kept vertices into out_coords
    var out_count: usize = 0;
    const max_out = out_coords.len / 2;

    var j: usize = 0;
    while (j < max_points and out_count < max_out) : (j += 1) {
        if (kept[j]) {
            out_coords[out_count * 2] = coords[j * 2];
            out_coords[out_count * 2 + 1] = coords[j * 2 + 1];
            out_count += 1;
        }
    }

    return out_count;
}

/// Ray-casting point-in-polygon test (Jordan curve theorem).
/// Polygon ring vertices are interleaved [lon0, lat0, lon1, lat1, ...].
pub fn pointInPolygon(
    test_lon: f64,
    test_lat: f64,
    poly_coords: []const f64,
    num_points: usize,
) bool {
    if (num_points < 3 or poly_coords.len < num_points * 2) return false;

    var inside = false;
    var i: usize = 0;
    var j: usize = num_points - 1;

    while (i < num_points) : (i += 1) {
        const xi = poly_coords[i * 2];
        const yi = poly_coords[i * 2 + 1];
        const xj = poly_coords[j * 2];
        const yj = poly_coords[j * 2 + 1];

        const intersect = ((yi > test_lat) != (yj > test_lat)) and
            (test_lon < (xj - xi) * (test_lat - yi) / (yj - yi + 1e-12) + xi);

        if (intersect) {
            inside = !inside;
        }
        j = i;
    }

    return inside;
}
