//! Embedded `DuckDB` Spatial analytical storage and spatiotemporal query engine.
//!
//! Provides high-throughput historical trajectory retrieval, dark target detection,
//! partitioned Parquet track ingestion sink, and Nim CEP tactical alerting rules.

pub mod cep;
pub mod queries;
pub mod sink;

pub use cep::{AlertResultC, EvaluateFn, KinematicEventC, NimRuleEngine, TacticalAlert};
pub use queries::{
    DarkTargetQuery, TrajectoryQuery, build_dark_target_query, build_trajectory_query,
};
pub use sink::{AnalyticsError, TrackBatchBuffer, TrackObservation, partition_path};
