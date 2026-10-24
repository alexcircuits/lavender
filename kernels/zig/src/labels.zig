//! Screen-Space Label Collision Arbiter (ENG-979)
//!
//! Provides ultra-fast screen-space bounding box overlap detection (AABB)
//! using a spatial hash grid (32px cells) and priority arbitration for
//! WebAssembly and native edge targets.

const std = @import("std");

/// Spatial grid cell dimension in pixels, matching Lavender legacy specification.
pub const CELL_SIZE: f32 = 32.0;

/// Number of hash buckets for the spatial hash table.
pub const SPATIAL_HASH_BUCKETS: usize = 2048;

/// Sentinel indicating an empty bucket link in the spatial hash.
pub const EMPTY_BUCKET: u32 = std.math.maxInt(u32);

/// Screen-space label placement candidate.
/// Uses C-ABI compatible memory layout for direct memory mapping from JavaScript/WASM.
pub const LabelCandidate = extern struct {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    priority: u32,
    is_selected: bool,
    is_tracked: bool,

    /// Returns true if bounding box has finite, positive dimensions.
    pub fn isFinite(self: LabelCandidate) bool {
        return std.math.isFinite(self.x) and
            std.math.isFinite(self.y) and
            std.math.isFinite(self.w) and
            std.math.isFinite(self.h) and
            self.w > 0.0 and
            self.h > 0.0;
    }

    /// Tests if this candidate has any visible overlap with the screen rectangle [0, screen_w] x [0, screen_h].
    pub fn isOnScreen(self: LabelCandidate, screen_w: f32, screen_h: f32) bool {
        if (!self.isFinite()) return false;
        if (screen_w <= 0.0 or screen_h <= 0.0) return false;
        if (self.x + self.w <= 0.0 or self.x >= screen_w) return false;
        if (self.y + self.h <= 0.0 or self.y >= screen_h) return false;
        return true;
    }

    /// Strict axis-aligned bounding box (AABB) intersection check.
    pub fn overlaps(self: LabelCandidate, other: LabelCandidate) bool {
        return (self.x < other.x + other.w) and
            (self.x + self.w > other.x) and
            (self.y < other.y + other.h) and
            (self.y + self.h > other.y);
    }
};

/// A node in the spatial hash chained bucket list.
const CellNode = struct {
    cx: i32,
    cy: i32,
    candidate_idx: usize,
    next: u32,
};

/// Fast hash function for 2D grid coordinates.
fn hashCell(cx: i32, cy: i32) u32 {
    const ux: u32 = @bitCast(cx);
    const uy: u32 = @bitCast(cy);
    var h: u32 = ux *% 73856093 ^ uy *% 19349663;
    h ^= h >> 16;
    h *%= 0x85ebca6b;
    h ^= h >> 13;
    return h;
}

/// Context for sorting candidate indices by descending priority order.
/// Priority hierarchy:
/// 1. `is_selected: bool` (selected items always take highest priority)
/// 2. `is_tracked: bool` (tracked contacts take precedence over unselected/untracked)
/// 3. `priority: u32` (descending: higher numerical priority wins)
/// 4. Stable tie-breaker: original candidate index (earlier candidate in input array wins)
const CandidateSortContext = struct {
    candidates: []const LabelCandidate,

    pub fn lessThan(ctx: @This(), a_idx: usize, b_idx: usize) bool {
        const a = ctx.candidates[a_idx];
        const b = ctx.candidates[b_idx];

        // 1. Selected candidates win over non-selected
        if (a.is_selected != b.is_selected) {
            return a.is_selected;
        }

        // 2. Tracked candidates win over non-tracked
        if (a.is_tracked != b.is_tracked) {
            return a.is_tracked;
        }

        // 3. Higher numerical priority wins
        if (a.priority != b.priority) {
            return a.priority > b.priority;
        }

        // 4. Stable tie-breaker: original index
        return a_idx < b_idx;
    }
};

/// Screen-space label collision arbiter.
///
/// Arbitrates bounding box placement among `candidates` over `screen_w * screen_h`:
/// - Iterates candidates in descending priority order.
/// - Performs AABB overlap detection against already placed labels using a 32px spatial hash grid.
/// - Writes 1 to `out_visible[i]` if candidate `i` is accepted, 0 if suppressed.
/// - Caps total visible labels at `max_labels`.
/// - Returns total number of visible labels accepted.
pub fn arbitrateLabels(
    candidates: []const LabelCandidate,
    screen_w: f32,
    screen_h: f32,
    max_labels: usize,
    out_visible: []u8,
) usize {
    const out_len = @min(candidates.len, out_visible.len);
    @memset(out_visible[0..out_len], 0);

    if (candidates.len == 0 or max_labels == 0 or screen_w <= 0.0 or screen_h <= 0.0) {
        return 0;
    }

    // Set up arena allocator backed by a 32KB stack buffer with page allocator fallback
    var stack_buf: [32768]u8 = undefined;
    var fallback = std.heap.stackFallback(stack_buf.len, std.heap.page_allocator);
    var arena = std.heap.ArenaAllocator.init(fallback.get());
    defer arena.deinit();
    const allocator = arena.allocator();

    // 1. Initialize candidate order array
    const order = allocator.alloc(usize, candidates.len) catch return 0;
    for (order, 0..) |*item, idx| {
        item.* = idx;
    }

    // 2. Sort candidate indices into descending priority order
    std.mem.sort(
        usize,
        order,
        CandidateSortContext{ .candidates = candidates },
        CandidateSortContext.lessThan,
    );

    // 3. Set up Spatial Hash Grid
    const buckets = allocator.alloc(u32, SPATIAL_HASH_BUCKETS) catch return 0;
    @memset(buckets, EMPTY_BUCKET);

    var nodes: std.ArrayList(CellNode) = .empty;

    var visible_count: usize = 0;

    const max_cx = @as(i32, @intFromFloat(@ceil(screen_w / CELL_SIZE)));
    const max_cy = @as(i32, @intFromFloat(@ceil(screen_h / CELL_SIZE)));

    // 4. Arbitrate candidates in priority order
    for (order) |cand_idx| {
        if (visible_count >= max_labels) break;

        const cand = candidates[cand_idx];

        // Screen boundary & validity check
        if (!cand.isOnScreen(screen_w, screen_h)) {
            continue; // Suppressed: out_visible[cand_idx] remains 0
        }

        // Compute cell coverage clamped to screen bounds
        const cx0 = @max(0, @min(max_cx, @as(i32, @intFromFloat(@floor(cand.x / CELL_SIZE)))));
        const cy0 = @max(0, @min(max_cy, @as(i32, @intFromFloat(@floor(cand.y / CELL_SIZE)))));
        const cx1 = @max(0, @min(max_cx, @as(i32, @intFromFloat(@floor((cand.x + cand.w) / CELL_SIZE)))));
        const cy1 = @max(0, @min(max_cy, @as(i32, @intFromFloat(@floor((cand.y + cand.h) / CELL_SIZE)))));

        // Test AABB intersection with previously placed labels in covering and adjacent spatial buckets
        var collides = false;
        check_cells: {
            var cy = cy0;
            while (cy <= cy1) : (cy += 1) {
                var cx = cx0;
                while (cx <= cx1) : (cx += 1) {
                    const b_idx = hashCell(cx, cy) & (SPATIAL_HASH_BUCKETS - 1);
                    var node_idx = buckets[b_idx];
                    while (node_idx != EMPTY_BUCKET) {
                        const node = nodes.items[node_idx];
                        if (node.cx == cx and node.cy == cy) {
                            const other = candidates[node.candidate_idx];
                            if (cand.overlaps(other)) {
                                collides = true;
                                break :check_cells;
                            }
                        }
                        node_idx = node.next;
                    }
                }
            }
        }

        if (collides) {
            continue; // Suppressed
        }

        // Accepted: Place candidate into spatial hash cells
        var place_cy = cy0;
        while (place_cy <= cy1) : (place_cy += 1) {
            var place_cx = cx0;
            while (place_cx <= cx1) : (place_cx += 1) {
                const b_idx = hashCell(place_cx, place_cy) & (SPATIAL_HASH_BUCKETS - 1);
                const new_node_idx: u32 = @intCast(nodes.items.len);
                nodes.append(allocator, CellNode{
                    .cx = place_cx,
                    .cy = place_cy,
                    .candidate_idx = cand_idx,
                    .next = buckets[b_idx],
                }) catch return visible_count;
                buckets[b_idx] = new_node_idx;
            }
        }

        if (cand_idx < out_visible.len) {
            out_visible[cand_idx] = 1;
        }
        visible_count += 1;
    }

    return visible_count;
}

// C-ABI exports for WASM and dynamic linking

/// WASM C-ABI: Arbitrates screen-space labels for collision-free display.
///
/// `labels_ptr`: Pointer to contiguous array of `LabelCandidate` structures.
/// `count`: Number of candidates in the array.
/// `screen_w`: Width of the screen/viewport in pixels.
/// `screen_h`: Height of the screen/viewport in pixels.
/// `max_labels`: Maximum visible labels to accept.
/// `out_mask_ptr`: Output byte buffer of size `count` (1 = visible, 0 = suppressed).
///
/// Returns total number of visible labels placed.
pub export fn lavender_arbitrate_labels(
    labels_ptr: [*]const LabelCandidate,
    count: usize,
    screen_w: f32,
    screen_h: f32,
    max_labels: usize,
    out_mask_ptr: [*]u8,
) usize {
    if (count == 0 or max_labels == 0) return 0;
    const candidates = labels_ptr[0..count];
    const out_visible = out_mask_ptr[0..count];
    return arbitrateLabels(candidates, screen_w, screen_h, max_labels, out_visible);
}
