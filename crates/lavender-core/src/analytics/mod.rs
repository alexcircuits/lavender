//! Embedded `DuckDB` Spatial analytical storage and spatiotemporal query engine.
//!
//! Provides high-throughput historical trajectory retrieval, dark target detection,
//! partitioned Parquet track ingestion sink, and Nim CEP tactical alerting rules.

pub mod awareness;
pub mod cep;
pub mod queries;
pub mod query_packs;
pub mod sink;

pub use cep::{AlertResultC, EvaluateFn, KinematicEventC, NimRuleEngine, TacticalAlert};
pub use queries::{
    DarkTargetQuery, TrajectoryQuery, build_dark_target_query, build_trajectory_query,
};
pub use query_packs::{
    DEFAULT_QUERY_PACK_LIMIT, MAX_QUERY_PACK_LIMIT, QueryPackWorld, airline_designator,
    classify_orbital_regime, compute_breakdown, compute_numeric_stats, execute_query_pack,
    normalize_vessel_type_family, query_pack_catalog,
};
pub use sink::{AnalyticsError, TrackBatchBuffer, TrackObservation, partition_path};
