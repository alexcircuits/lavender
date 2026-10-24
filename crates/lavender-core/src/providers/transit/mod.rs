//! GTFS-Realtime transit vehicle positions, feed registry, and historical trails.

pub mod client;
pub mod decoder;
pub mod error;
pub mod history;
pub mod proto;
pub mod redirect;
pub mod registry;

pub use client::{TRANSIT_MAX_BODY_BYTES, TRANSIT_STALE_MAX_SECS, TRANSIT_TTL_SECS, TransitEngine};
pub use decoder::{
    DecodedTransitSnapshot, decode_vehicle_positions, is_plausible_vehicle_position,
};
pub use error::TransitError;
pub use history::{TransitHistoryDiagnostics, TransitHistoryStore, valid_transit_identifier};
pub use redirect::transit_redirect_decision;
pub use registry::{default_approved_feeds, resolve_route_mode, valid_transit_feed_id};
