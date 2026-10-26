const std = @import("std");

test {
    _ = @import("gnss_test.zig");
}

test "arraylist" {
    var list = std.ArrayList(u32).init(std.testing.allocator);
    defer list.deinit();
    try list.append(123);
    try std.testing.expectEqual(@as(usize, 1), list.items.len);
}
