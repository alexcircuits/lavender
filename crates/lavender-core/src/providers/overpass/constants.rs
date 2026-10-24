//! Overpass API proxy constants and constraints.
//!
//! Ported directly from `legacy/server/providers/overpass/constants.js` to preserve
//! the identical security boundary and query limits.

/// Stable application identity for operator-configured Overpass instances.
pub const OVERPASS_USER_AGENT: &str =
    "gods-eye-view/0.1 (+https://github.com/bilawalsidhu/gods-eye-view)";

/// TTL for fresh cached Overpass responses in memory (24 hours).
pub const OVERPASS_CACHE_MS: u64 = 86_400_000;

/// Disk-cache TTL for Overpass responses (7 days).
pub const OVERPASS_DISK_TTL_MS: u64 = 7 * 86_400_000;

/// Disk-cache TTL for boundary-class queries (`is_in` / admin-relation pivots) — 30 days.
pub const OVERPASS_BOUNDARY_DISK_TTL_MS: u64 = 30 * 86_400_000;

/// Per-upstream fetch timeout in milliseconds (22 seconds).
pub const OVERPASS_TIMEOUT_MS: u64 = 22_000;

/// Max entries in the Overpass in-memory response cache.
pub const OVERPASS_CACHE_MAX_ENTRIES: usize = 120;

/// Max accepted POST body for the Overpass proxy (24 KB).
pub const OVERPASS_MAX_BODY_BYTES: usize = 24 * 1024;

/// Hard cap on a single Overpass upstream response buffered into memory (32 MB).
pub const OVERPASS_MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

/// Max concurrent in-flight upstream Overpass fetches across all queries.
pub const OVERPASS_MAX_CONCURRENT: usize = 6;

/// Server-side timeout ceiling (seconds) permitted inside an Overpass QL query.
pub const OVERPASS_MAX_QL_TIMEOUT: u32 = 30;

/// Max `around:` radius in meters (50,000 m).
pub const OVERPASS_MAX_AROUND_M: f64 = 50_000.0;

/// Max bbox span in degrees (12 degrees).
pub const OVERPASS_MAX_BBOX_DEG: f64 = 12.0;

/// Regex pattern for all Overpass element type specifiers including shortcuts.
pub const OVERPASS_ELEMENT_TYPES: &str = "node|way|relation|nwr|nw|nr|wr|rel";
