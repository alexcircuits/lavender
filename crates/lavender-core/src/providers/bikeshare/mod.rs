//! Bike-share provider managing GBFS feeds across versions 2 and 3.

pub mod client;
pub mod parser;
pub mod registry;

pub use client::{BikeshareEngine, BikeshareError, GBFS_MAX_BODY_BYTES, GBFS_REQUEST_TIMEOUT};
pub use parser::{
    DiscoveredFeeds, merge_stations, parse_gbfs_discovery, parse_station_information,
    parse_station_status,
};
pub use registry::default_approved_systems;
