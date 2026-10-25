//! Geofence alerting rule engine, spatial containment evaluation, and event buffering.

pub mod engine;
pub mod geometry;
pub mod state;

pub use engine::GeofenceAlertEngine;
pub use geometry::{antimeridian_point_in_polygon, polygon_centroid};
pub use lavender_domain::{AlertEvent, AlertKind, Geofence, GeofenceRule};
pub use state::BoundedLru;
