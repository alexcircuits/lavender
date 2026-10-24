//! `TomTom` traffic flow vector tile proxy, daily request budget governor,
//! and `OpenFreeMap` road matching engine.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::module_name_repetitions
)]

pub mod budget;
pub mod cache;
pub mod client;
pub mod error;
pub mod matcher;
pub mod mvt;
pub mod proto;
pub mod tiles;

pub use budget::BudgetGovernor;
pub use cache::{collect_cache_files, sweep_disk_cache};
pub use client::{CacheStatus, TILE_TTL, TrafficEngine};
pub use error::TrafficError;
pub use matcher::{MatchResult, match_flow_to_roads, median};
pub use mvt::{FlowSegment, RoadFeature, decode_openfreemap_roads, decode_tomtom_flow_tile};
pub use tiles::{
    MAX_TILE_ZOOM, MIN_TILE_ZOOM, is_valid_tile_coord, lon_lat_to_tile, tile_to_bbox,
    tiles_for_bounds, utc_day_key,
};
