//! Pure geospatial computation for Lavender. Nothing here performs I/O, so every function
//! is deterministic and directly testable; providers, search and the feed hub build on it.

pub mod coords;
pub mod geodesy;
pub mod index;
pub mod mgrs;
pub mod orbit;
pub mod region;

pub use coords::{ParsedCoordinate, format_decimal, format_dms, parse_coordinates};
pub use geodesy::{destination, distance_m, initial_bearing_deg};
pub use index::PointIndex;
pub use orbit::{LookAngles, OrbitError, Propagator};
pub use region::{Cell, RegionPlan, plan_cells};
