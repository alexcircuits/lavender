//! Unit tests for Screen-Space Label Collision Arbiter (ENG-979)

const std = @import("std");
const labels = @import("labels.zig");
const LabelCandidate = labels.LabelCandidate;
const arbitrateLabels = labels.arbitrateLabels;
const lavender_arbitrate_labels = labels.lavender_arbitrate_labels;

test "LabelCandidate struct layout is C-ABI compatible" {
    // 4*f32 (16) + 1*u32 (4) + 2*bool (2) + 2 padding = 24 bytes, 4-byte aligned
    try std.testing.expectEqual(@as(usize, 24), @sizeOf(LabelCandidate));
    try std.testing.expectEqual(@as(usize, 4), @alignOf(LabelCandidate));
    try std.testing.expectEqual(@as(usize, 0), @offsetOf(LabelCandidate, "x"));
    try std.testing.expectEqual(@as(usize, 4), @offsetOf(LabelCandidate, "y"));
    try std.testing.expectEqual(@as(usize, 8), @offsetOf(LabelCandidate, "w"));
    try std.testing.expectEqual(@as(usize, 12), @offsetOf(LabelCandidate, "h"));
    try std.testing.expectEqual(@as(usize, 16), @offsetOf(LabelCandidate, "priority"));
    try std.testing.expectEqual(@as(usize, 20), @offsetOf(LabelCandidate, "is_selected"));
    try std.testing.expectEqual(@as(usize, 21), @offsetOf(LabelCandidate, "is_tracked"));
}

test "non-overlapping labels are all accepted" {
    const candidates = [_]LabelCandidate{
        .{ .x = 10.0, .y = 10.0, .w = 40.0, .h = 20.0, .priority = 10, .is_selected = false, .is_tracked = false },
        .{ .x = 100.0, .y = 10.0, .w = 50.0, .h = 20.0, .priority = 20, .is_selected = false, .is_tracked = false },
        .{ .x = 10.0, .y = 80.0, .w = 40.0, .h = 20.0, .priority = 30, .is_selected = false, .is_tracked = false },
        .{ .x = 200.0, .y = 150.0, .w = 60.0, .h = 25.0, .priority = 40, .is_selected = false, .is_tracked = false },
    };
    var mask: [candidates.len]u8 = undefined;

    const visible_count = arbitrateLabels(&candidates, 800.0, 600.0, 10, &mask);

    try std.testing.expectEqual(@as(usize, 4), visible_count);
    for (mask) |m| {
        try std.testing.expectEqual(@as(u8, 1), m);
    }
}

test "overlapping labels: higher priority wins, lower priority is suppressed" {
    const candidates = [_]LabelCandidate{
        // Candidate 0: lower priority (10)
        .{ .x = 100.0, .y = 100.0, .w = 60.0, .h = 30.0, .priority = 10, .is_selected = false, .is_tracked = false },
        // Candidate 1: higher priority (50), overlaps with candidate 0
        .{ .x = 120.0, .y = 110.0, .w = 60.0, .h = 30.0, .priority = 50, .is_selected = false, .is_tracked = false },
        // Candidate 2: distinct location, independent
        .{ .x = 300.0, .y = 300.0, .w = 50.0, .h = 20.0, .priority = 5, .is_selected = false, .is_tracked = false },
    };
    var mask: [candidates.len]u8 = undefined;

    const visible_count = arbitrateLabels(&candidates, 1920.0, 1080.0, 10, &mask);

    try std.testing.expectEqual(@as(usize, 2), visible_count);
    // Candidate 1 wins over Candidate 0
    try std.testing.expectEqual(@as(u8, 0), mask[0]);
    try std.testing.expectEqual(@as(u8, 1), mask[1]);
    // Candidate 2 does not collide with anything
    try std.testing.expectEqual(@as(u8, 1), mask[2]);
}

test "selected labels always win even against larger overlapping labels" {
    const candidates = [_]LabelCandidate{
        // Candidate 0: Giant unselected bounding box with huge priority
        .{ .x = 50.0, .y = 50.0, .w = 300.0, .h = 200.0, .priority = 999999, .is_selected = false, .is_tracked = false },
        // Candidate 1: Small selected candidate inside candidate 0 with priority 1
        .{ .x = 100.0, .y = 100.0, .w = 40.0, .h = 20.0, .priority = 1, .is_selected = true, .is_tracked = false },
    };
    var mask: [candidates.len]u8 = undefined;

    const visible_count = arbitrateLabels(&candidates, 1920.0, 1080.0, 10, &mask);

    try std.testing.expectEqual(@as(usize, 1), visible_count);
    try std.testing.expectEqual(@as(u8, 0), mask[0]); // Giant label suppressed
    try std.testing.expectEqual(@as(u8, 1), mask[1]); // Selected label placed
}

test "tracked labels take precedence over unselected untracked labels" {
    const candidates = [_]LabelCandidate{
        // Candidate 0: High numerical priority (1000), but not tracked
        .{ .x = 50.0, .y = 50.0, .w = 80.0, .h = 30.0, .priority = 1000, .is_selected = false, .is_tracked = false },
        // Candidate 1: Low numerical priority (5), but is_tracked = true
        .{ .x = 70.0, .y = 60.0, .w = 80.0, .h = 30.0, .priority = 5, .is_selected = false, .is_tracked = true },
    };
    var mask: [candidates.len]u8 = undefined;

    const visible_count = arbitrateLabels(&candidates, 1920.0, 1080.0, 10, &mask);

    try std.testing.expectEqual(@as(usize, 1), visible_count);
    try std.testing.expectEqual(@as(u8, 0), mask[0]);
    try std.testing.expectEqual(@as(u8, 1), mask[1]);
}

test "selected beats tracked when both collide" {
    const candidates = [_]LabelCandidate{
        // Candidate 0: Tracked
        .{ .x = 50.0, .y = 50.0, .w = 60.0, .h = 30.0, .priority = 100, .is_selected = false, .is_tracked = true },
        // Candidate 1: Selected
        .{ .x = 60.0, .y = 55.0, .w = 60.0, .h = 30.0, .priority = 10, .is_selected = true, .is_tracked = false },
    };
    var mask: [candidates.len]u8 = undefined;

    const visible_count = arbitrateLabels(&candidates, 1920.0, 1080.0, 10, &mask);

    try std.testing.expectEqual(@as(usize, 1), visible_count);
    try std.testing.expectEqual(@as(u8, 0), mask[0]);
    try std.testing.expectEqual(@as(u8, 1), mask[1]);
}

test "max_labels budget cap" {
    const candidates = [_]LabelCandidate{
        .{ .x = 10.0, .y = 10.0, .w = 40.0, .h = 20.0, .priority = 10, .is_selected = false, .is_tracked = false },
        .{ .x = 60.0, .y = 10.0, .w = 40.0, .h = 20.0, .priority = 50, .is_selected = false, .is_tracked = false },
        .{ .x = 110.0, .y = 10.0, .w = 40.0, .h = 20.0, .priority = 30, .is_selected = false, .is_tracked = false },
        .{ .x = 160.0, .y = 10.0, .w = 40.0, .h = 20.0, .priority = 40, .is_selected = false, .is_tracked = false },
        .{ .x = 210.0, .y = 10.0, .w = 40.0, .h = 20.0, .priority = 20, .is_selected = false, .is_tracked = false },
    };
    var mask: [candidates.len]u8 = undefined;

    // Cap at 2 visible labels. Priorities are 50, 40, 30, 20, 10.
    // The top 2 (candidates 1 with prio 50, and candidate 3 with prio 40) should be placed.
    const visible_count = arbitrateLabels(&candidates, 1920.0, 1080.0, 2, &mask);

    try std.testing.expectEqual(@as(usize, 2), visible_count);
    try std.testing.expectEqual(@as(u8, 0), mask[0]); // prio 10
    try std.testing.expectEqual(@as(u8, 1), mask[1]); // prio 50
    try std.testing.expectEqual(@as(u8, 0), mask[2]); // prio 30
    try std.testing.expectEqual(@as(u8, 1), mask[3]); // prio 40
    try std.testing.expectEqual(@as(u8, 0), mask[4]); // prio 20
}

test "screen boundary handling" {
    const candidates = [_]LabelCandidate{
        // 0: Off-screen left (x + w <= 0)
        .{ .x = -100.0, .y = 50.0, .w = 50.0, .h = 20.0, .priority = 100, .is_selected = true, .is_tracked = false },
        // 1: Off-screen top (y + h <= 0)
        .{ .x = 50.0, .y = -50.0, .w = 50.0, .h = 20.0, .priority = 100, .is_selected = true, .is_tracked = false },
        // 2: Off-screen right (x >= screen_w)
        .{ .x = 800.0, .y = 50.0, .w = 50.0, .h = 20.0, .priority = 100, .is_selected = true, .is_tracked = false },
        // 3: Off-screen bottom (y >= screen_h)
        .{ .x = 50.0, .y = 600.0, .w = 50.0, .h = 20.0, .priority = 100, .is_selected = true, .is_tracked = false },
        // 4: Partially on-screen left (x = -20, w = 50 -> extends from -20 to 30)
        .{ .x = -20.0, .y = 50.0, .w = 50.0, .h = 20.0, .priority = 50, .is_selected = false, .is_tracked = false },
        // 5: Partially on-screen right (x = 780, w = 40 -> extends from 780 to 820)
        .{ .x = 780.0, .y = 50.0, .w = 40.0, .h = 20.0, .priority = 50, .is_selected = false, .is_tracked = false },
        // 6: Invalid non-positive dimensions
        .{ .x = 100.0, .y = 100.0, .w = 0.0, .h = 20.0, .priority = 100, .is_selected = true, .is_tracked = false },
        .{ .x = 100.0, .y = 100.0, .w = 50.0, .h = -10.0, .priority = 100, .is_selected = true, .is_tracked = false },
        // 7: Non-finite coordinate (NaN)
        .{ .x = std.math.nan(f32), .y = 100.0, .w = 50.0, .h = 20.0, .priority = 100, .is_selected = true, .is_tracked = false },
    };
    var mask: [candidates.len]u8 = undefined;

    const visible_count = arbitrateLabels(&candidates, 800.0, 600.0, 10, &mask);

    try std.testing.expectEqual(@as(usize, 2), visible_count);
    try std.testing.expectEqual(@as(u8, 0), mask[0]); // Off-screen left
    try std.testing.expectEqual(@as(u8, 0), mask[1]); // Off-screen top
    try std.testing.expectEqual(@as(u8, 0), mask[2]); // Off-screen right
    try std.testing.expectEqual(@as(u8, 0), mask[3]); // Off-screen bottom
    try std.testing.expectEqual(@as(u8, 1), mask[4]); // Partially on-screen left accepted
    try std.testing.expectEqual(@as(u8, 1), mask[5]); // Partially on-screen right accepted
    try std.testing.expectEqual(@as(u8, 0), mask[6]); // Zero width suppressed
    try std.testing.expectEqual(@as(u8, 0), mask[7]); // Negative height suppressed
    try std.testing.expectEqual(@as(u8, 0), mask[8]); // NaN coordinate suppressed
}

test "zero or invalid screen dimensions return 0" {
    const candidates = [_]LabelCandidate{
        .{ .x = 10.0, .y = 10.0, .w = 40.0, .h = 20.0, .priority = 10, .is_selected = false, .is_tracked = false },
    };
    var mask: [candidates.len]u8 = undefined;

    try std.testing.expectEqual(@as(usize, 0), arbitrateLabels(&candidates, 0.0, 600.0, 10, &mask));
    try std.testing.expectEqual(@as(u8, 0), mask[0]);

    try std.testing.expectEqual(@as(usize, 0), arbitrateLabels(&candidates, 800.0, -10.0, 10, &mask));
    try std.testing.expectEqual(@as(u8, 0), mask[0]);

    try std.testing.expectEqual(@as(usize, 0), arbitrateLabels(&candidates, 800.0, 600.0, 0, &mask));
    try std.testing.expectEqual(@as(u8, 0), mask[0]);
}

test "touching boundary labels do not collide" {
    const candidates = [_]LabelCandidate{
        // Candidate 0: [0, 50] x [0, 20]
        .{ .x = 0.0, .y = 0.0, .w = 50.0, .h = 20.0, .priority = 10, .is_selected = false, .is_tracked = false },
        // Candidate 1: exactly touches right edge of candidate 0: [50, 100] x [0, 20]
        .{ .x = 50.0, .y = 0.0, .w = 50.0, .h = 20.0, .priority = 10, .is_selected = false, .is_tracked = false },
        // Candidate 2: exactly touches bottom edge of candidate 0: [0, 50] x [20, 40]
        .{ .x = 0.0, .y = 20.0, .w = 50.0, .h = 20.0, .priority = 10, .is_selected = false, .is_tracked = false },
    };
    var mask: [candidates.len]u8 = undefined;

    const visible_count = arbitrateLabels(&candidates, 800.0, 600.0, 10, &mask);

    try std.testing.expectEqual(@as(usize, 3), visible_count);
    for (mask) |m| {
        try std.testing.expectEqual(@as(u8, 1), m);
    }
}

test "deterministic tie breaking by original index" {
    const candidates = [_]LabelCandidate{
        // Both candidates have identical properties and overlap
        .{ .x = 100.0, .y = 100.0, .w = 50.0, .h = 20.0, .priority = 10, .is_selected = false, .is_tracked = false },
        .{ .x = 110.0, .y = 105.0, .w = 50.0, .h = 20.0, .priority = 10, .is_selected = false, .is_tracked = false },
    };
    var mask: [candidates.len]u8 = undefined;

    const visible_count = arbitrateLabels(&candidates, 800.0, 600.0, 10, &mask);

    try std.testing.expectEqual(@as(usize, 1), visible_count);
    // Earlier candidate 0 wins tie, candidate 1 suppressed
    try std.testing.expectEqual(@as(u8, 1), mask[0]);
    try std.testing.expectEqual(@as(u8, 0), mask[1]);
}

test "C-ABI lavender_arbitrate_labels export" {
    const candidates = [_]LabelCandidate{
        .{ .x = 10.0, .y = 10.0, .w = 40.0, .h = 20.0, .priority = 10, .is_selected = false, .is_tracked = false },
        .{ .x = 15.0, .y = 15.0, .w = 40.0, .h = 20.0, .priority = 20, .is_selected = false, .is_tracked = false },
        .{ .x = 100.0, .y = 100.0, .w = 40.0, .h = 20.0, .priority = 5, .is_selected = false, .is_tracked = false },
    };
    var mask: [candidates.len]u8 = undefined;

    const visible_count = lavender_arbitrate_labels(
        &candidates,
        candidates.len,
        800.0,
        600.0,
        10,
        &mask,
    );

    try std.testing.expectEqual(@as(usize, 2), visible_count);
    try std.testing.expectEqual(@as(u8, 0), mask[0]); // Lower priority candidate 0 suppressed
    try std.testing.expectEqual(@as(u8, 1), mask[1]); // Higher priority candidate 1 placed
    try std.testing.expectEqual(@as(u8, 1), mask[2]); // Non-overlapping candidate 2 placed
}
