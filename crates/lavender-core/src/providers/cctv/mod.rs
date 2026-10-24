//! Public camera (CCTV) layer provider.
//!
//! Provides:
//! - 14 agency loaders with strict origin pinning and recorded fixture parsers.
//! - SSRF-protected still frame relay: validates destination against camera's registered origin, refuses cross-origin redirects.
//! - Live stream proxy with concurrency limiting and cancellation on client disconnect.
//! - Deterministic synthetic SVG billboard fallback (no unchecked Street View).
//! - Bounded catalog and camera health tracking.
//! - Privacy: sensitive rating and ADR 0008 licence enforcement.

pub mod cap;
pub mod catalog;
pub mod constants;
pub mod loaders;
pub mod media;
pub mod normalize;
pub mod vision_ipc;

pub use cap::{allocate_source_cap, prioritize_sources};
pub use catalog::CctvCatalog;
pub use constants::*;
pub use loaders::*;
pub use media::{
    CctvMediaError, LIVE_STREAM_SEMAPHORE, build_synthetic_cctv_svg, fetch_txdot_snapshot,
    fetch_within_host, upstream_user_agent,
};
pub use normalize::{
    camera_display_code, direction_to_heading, fallback_heading_from_id, hash_seed,
};
pub use vision_ipc::VisionPerceptionClient;
