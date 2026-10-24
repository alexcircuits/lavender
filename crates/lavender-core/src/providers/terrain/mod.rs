//! Terrain point-height lookup provider backed by Re:Earth.
//!
//! Provides cached, chunked, bounded lookups of ellipsoidal heights (m)
//! for sets of `[longitude, latitude]` points with exact input order preservation.

pub mod cache;
pub mod client;
pub mod coords;
pub mod error;

pub use cache::TerrainLruCache;
pub use client::TerrainEngine;
pub use coords::{
    canonical_key, canonical_point, parse_terrain_points_query, validate_coordinates,
};
pub use error::TerrainError;
