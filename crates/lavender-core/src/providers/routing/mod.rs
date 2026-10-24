//! Paced routing provider and Open Source Routing Machine (OSRM) integration.
//!
//! Calculates turn-by-turn routes across travel profiles (`car`, `bike`, `foot`),
//! enforces outbound pacing (1 request/s max), bounds queue depth (429 when full),
//! coalesces in-flight duplicate requests, and formats English navigation steps.

pub mod client;
pub mod pacer;
pub mod steps;

pub use client::{
    DEFAULT_BIKE_ENDPOINT, DEFAULT_CAR_ENDPOINT, DEFAULT_FOOT_ENDPOINT, ROUTE_CACHE_MAX_ENTRIES,
    ROUTE_CACHE_TTL, ROUTE_MAX_LEG_KM, ROUTE_MAX_RESPONSE_BYTES, ROUTE_MAX_TOTAL_KM,
    ROUTE_REQUEST_TIMEOUT, RouteEngine, RouteProfile, parse_and_validate_coords,
};
pub use pacer::{
    QueueGuard, ROUTE_UPSTREAM_MIN_INTERVAL, ROUTE_UPSTREAM_QUEUE_MAX, RouteError, RoutePacer,
};
pub use steps::{
    ROUTE_STEPS_MAX, RawGeometry, RawManeuver, RawOsrmLeg, RawOsrmResponse, RawOsrmRoute,
    RawOsrmStep, format_route_distance, format_route_duration, instruction_for,
    normalize_osrm_steps, road_label,
};
