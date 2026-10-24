//! `DuckDB` + Spatial SQL query builders for spatiotemporal OSINT analytics.
//!
//! Generates parameterized and validated SQL queries executed by the in-process
//! `DuckDB` engine over partitioned Parquet track stores (`$LAVENDER_DATA_DIR/analytics/tracks/`).

use jiff::Timestamp;
use lavender_domain::BBox;

/// Parameters for querying historical trajectories of specific entities.
#[derive(Debug, Clone, PartialEq)]
pub struct TrajectoryQuery<'a> {
    pub parquet_pattern: &'a str,
    pub entity_ids: &'a [&'a str],
    pub bbox: Option<BBox>,
    pub start_time: Timestamp,
    pub end_time: Timestamp,
    pub limit: usize,
}

/// Parameters for dark target (AIS / ADS-B transponder gap) detection.
#[derive(Debug, Clone, PartialEq)]
pub struct DarkTargetQuery<'a> {
    pub parquet_pattern: &'a str,
    pub bbox: Option<BBox>,
    pub start_time: Timestamp,
    pub end_time: Timestamp,
    /// Minimum transponder silence in seconds to qualify as a "dark" event (e.g., 1800s).
    pub min_gap_seconds: u32,
    /// Minimum speed in knots before disappearance to filter out docked/parked targets.
    pub min_speed_knots: f64,
    pub limit: usize,
}

/// Generates a `DuckDB` Spatial SQL query to fetch chronological track points.
/// Uses `ST_Point` and `ST_Within` when a bounding box filter is present.
#[must_use]
pub fn build_trajectory_query(q: &TrajectoryQuery<'_>) -> String {
    let mut where_clauses = Vec::new();

    // Time window
    let start_iso = q.start_time.to_string();
    let end_iso = q.end_time.to_string();
    where_clauses.push(format!(
        "timestamp >= '{start_iso}'::TIMESTAMP AND timestamp <= '{end_iso}'::TIMESTAMP"
    ));

    // Entity IDs filter
    if !q.entity_ids.is_empty() {
        let quoted: Vec<String> =
            q.entity_ids.iter().map(|id| format!("'{}'", id.replace('\'', "''"))).collect();
        where_clauses.push(format!("entity_id IN ({})", quoted.join(", ")));
    }

    // Bounding box filter (OpenGIS ST_MakeEnvelope: xmin, ymin, xmax, ymax)
    if let Some(bbox) = q.bbox {
        if bbox.crosses_antimeridian() {
            // Split into eastern and western hemispheres across 180° meridian
            where_clauses.push(format!(
                "(ST_Within(geom, ST_MakeEnvelope({}, {}, 180.0, {})) OR ST_Within(geom, ST_MakeEnvelope(-180.0, {}, {}, {})))",
                bbox.west, bbox.south, bbox.north,
                bbox.south, bbox.east, bbox.north
            ));
        } else {
            where_clauses.push(format!(
                "ST_Within(geom, ST_MakeEnvelope({}, {}, {}, {}))",
                bbox.west, bbox.south, bbox.east, bbox.north
            ));
        }
    }

    let where_str = where_clauses.join(" AND ");
    let limit_str = if q.limit > 0 { format!("LIMIT {}", q.limit) } else { String::new() };

    format!(
        r"SELECT 
    entity_id,
    timestamp,
    ST_X(geom) AS lon,
    ST_Y(geom) AS lat,
    altitude_m,
    speed_knots,
    heading_deg,
    source_id
FROM read_parquet('{}')
WHERE {}
ORDER BY entity_id, timestamp ASC
{};",
        q.parquet_pattern.replace('\'', "''"),
        where_str,
        limit_str
    )
}

/// Generates a vectorized `DuckDB` Spatial query detecting dark target events.
/// Uses window functions `LAG()` to compute temporal gaps and spatial drift.
#[must_use]
pub fn build_dark_target_query(q: &DarkTargetQuery<'_>) -> String {
    let start_iso = q.start_time.to_string();
    let end_iso = q.end_time.to_string();

    let mut spatial_filter = String::new();
    if let Some(bbox) = q.bbox {
        spatial_filter = format!(
            "AND ST_Within(geom, ST_MakeEnvelope({}, {}, {}, {}))",
            bbox.west, bbox.south, bbox.east, bbox.north
        );
    }

    format!(
        r"WITH ranked_points AS (
    SELECT 
        entity_id,
        timestamp,
        geom,
        speed_knots,
        LAG(timestamp) OVER (PARTITION BY entity_id ORDER BY timestamp) AS prev_timestamp,
        LAG(geom) OVER (PARTITION BY entity_id ORDER BY timestamp) AS prev_geom,
        LAG(speed_knots) OVER (PARTITION BY entity_id ORDER BY timestamp) AS prev_speed
    FROM read_parquet('{}')
    WHERE timestamp >= '{start_iso}'::TIMESTAMP 
      AND timestamp <= '{end_iso}'::TIMESTAMP
      {spatial_filter}
)
SELECT 
    entity_id,
    prev_timestamp AS gap_start,
    timestamp AS gap_end,
    date_diff('second', prev_timestamp, timestamp) AS gap_duration_s,
    ST_X(prev_geom) AS last_seen_lon,
    ST_Y(prev_geom) AS last_seen_lat,
    ST_X(geom) AS reacquired_lon,
    ST_Y(geom) AS reacquired_lat,
    ST_Distance_Sphere(prev_geom, geom) AS drift_distance_m,
    prev_speed AS speed_before_gap_knots
FROM ranked_points
WHERE date_diff('second', prev_timestamp, timestamp) >= {}
  AND (prev_speed IS NULL OR prev_speed >= {})
ORDER BY gap_duration_s DESC
LIMIT {};",
        q.parquet_pattern.replace('\'', "''"),
        q.min_gap_seconds,
        q.min_speed_knots,
        q.limit
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trajectory_query_generates_valid_duckdb_sql() {
        let t1 = Timestamp::from_second(1_700_000_000).unwrap();
        let t2 = Timestamp::from_second(1_700_086_400).unwrap();
        let bbox = BBox::new(-5.0, 50.0, 2.0, 55.0).unwrap();

        let query = build_trajectory_query(&TrajectoryQuery {
            parquet_pattern: "data/analytics/tracks/**/*.parquet",
            entity_ids: &["4840D6", "A1B2C3"],
            bbox: Some(bbox),
            start_time: t1,
            end_time: t2,
            limit: 500,
        });

        assert!(query.contains("read_parquet('data/analytics/tracks/**/*.parquet')"));
        assert!(query.contains("entity_id IN ('4840D6', 'A1B2C3')"));
        assert!(query.contains("ST_Within(geom, ST_MakeEnvelope(-5, 50, 2, 55))"));
        assert!(query.contains("LIMIT 500"));
    }

    #[test]
    fn trajectory_query_handles_antimeridian_crossing() {
        let t1 = Timestamp::from_second(1_700_000_000).unwrap();
        let t2 = Timestamp::from_second(1_700_086_400).unwrap();
        // West = 170°, East = -170° (crosses 180° meridian in Pacific)
        let bbox = BBox::new(170.0, -20.0, -170.0, 20.0).unwrap();

        let query = build_trajectory_query(&TrajectoryQuery {
            parquet_pattern: "tracks/*.parquet",
            entity_ids: &[],
            bbox: Some(bbox),
            start_time: t1,
            end_time: t2,
            limit: 100,
        });

        // Must split into two envelopes across the antimeridian
        assert!(query.contains("ST_MakeEnvelope(170, -20, 180.0, 20)"));
        assert!(query.contains("ST_MakeEnvelope(-180.0, -20, -170, 20)"));
    }

    #[test]
    fn dark_target_query_includes_lag_window_function() {
        let t1 = Timestamp::from_second(1_700_000_000).unwrap();
        let t2 = Timestamp::from_second(1_700_086_400).unwrap();

        let query = build_dark_target_query(&DarkTargetQuery {
            parquet_pattern: "tracks/vessels/**/*.parquet",
            bbox: None,
            start_time: t1,
            end_time: t2,
            min_gap_seconds: 1800,
            min_speed_knots: 3.5,
            limit: 50,
        });

        assert!(query.contains("LAG(timestamp) OVER (PARTITION BY entity_id ORDER BY timestamp)"));
        assert!(query.contains("date_diff('second', prev_timestamp, timestamp) >= 1800"));
        assert!(query.contains("prev_speed >= 3.5"));
        assert!(query.contains("ST_Distance_Sphere(prev_geom, geom) AS drift_distance_m"));
    }
}
