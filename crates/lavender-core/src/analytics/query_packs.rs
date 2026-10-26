//! Analyst Query Packs Engine (`LB-121` — porting `legacy/src/tools/queries/`).
//!
//! Provides a catalog of multi-domain OSINT query packs (`aircraft_in_area`,
//! `military_aircraft_watch`, `vessels_in_area`, `maritime_dark_targets`,
//! `satellites_overhead`, `hazards_in_area`, `gnss_interference_watch`,
//! `alpr_surveillance_scan`, `submarine_cables_in_area`, `theater_situation_brief`)
//! with ICAO airline designator extraction, ITU-R M.1371 ship type family
//! normalization, category breakdowns, numeric summary statistics (`min`,
//! `median`, `max`), and parameterized `DuckDB` Spatial SQL generation.

use std::collections::HashMap;

use jiff::{Span, Timestamp};
use lavender_domain::{
    Aircraft, AlprCamera, BBox, Cyclone, Earthquake, FireCell, GeoPoint, GnssCell, GnssLevel,
    QueryPackBreakdownBucket, QueryPackDefinition, QueryPackDomain, QueryPackExecution,
    QueryPackNumericStats, QueryPackRequest, QueryPackRow, SatelliteElements, Vessel,
};
use lavender_geo::distance_m;

use crate::analytics::queries::{
    DarkTargetQuery, TrajectoryQuery, build_dark_target_query, build_trajectory_query,
};
use crate::error::CoreError;
use crate::providers::cables::SubmarineCable;

/// Maximum rows returned by any single query pack execution.
pub const MAX_QUERY_PACK_LIMIT: usize = 500;
/// Default rows returned when caller omits `limit`.
pub const DEFAULT_QUERY_PACK_LIMIT: usize = 50;

/// Extracts the 3-letter ICAO airline designator from a flight callsign (`UAL123` -> `Some("UAL")`).
///
/// Mirrors `airlineDesignator` in `legacy/src/tools/queries/aviation.js`:
/// - Three uppercase ASCII letters followed by a digit and up to 3 alphanumeric characters (`3..=7` chars total).
/// - Excludes Philippine `RPC\d{4}` and Cuban `CUT\d{4}` tail registrations that mimic airline callsigns when unhyphenated.
#[must_use]
pub fn airline_designator(callsign: &str) -> Option<String> {
    let text = callsign.trim().to_ascii_uppercase();
    let bytes = text.as_bytes();
    if bytes.len() < 4 || bytes.len() > 7 {
        return None;
    }
    if bytes.len() == 7
        && (&bytes[0..3] == b"RPC" || &bytes[0..3] == b"CUT")
        && bytes[3..7].iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    if !bytes[0..3].iter().all(u8::is_ascii_uppercase) {
        return None;
    }
    if !bytes[3].is_ascii_digit() {
        return None;
    }
    if !bytes[4..].iter().all(u8::is_ascii_alphanumeric) {
        return None;
    }
    Some(text[0..3].to_owned())
}

/// Normalizes an AIS vessel type string or ITU-R M.1371 numeric code into a canonical ship family.
///
/// Mirrors `vesselType` in `legacy/src/tools/queries/maritime.js`.
#[must_use]
pub fn normalize_vessel_type_family(raw: Option<&str>) -> String {
    let Some(trimmed) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return "UNKNOWN".to_owned();
    };
    if let Ok(code) = trimmed.parse::<u16>() {
        return match code {
            0 => "UNKNOWN".to_owned(),
            30 => "FISHING".to_owned(),
            31 | 32 | 52 => "TUG".to_owned(),
            35 => "MILITARY".to_owned(),
            36 | 37 => "PLEASURE".to_owned(),
            40..=49 => "HIGH_SPEED".to_owned(),
            50..=59 => "SPECIAL".to_owned(),
            60..=69 => "PASSENGER".to_owned(),
            70..=79 => "CARGO".to_owned(),
            80..=89 => "TANKER".to_owned(),
            _ => "OTHER".to_owned(),
        };
    }
    let upper = trimmed.to_ascii_uppercase();
    if upper.contains("CARGO") || upper.contains("CONTAINER") || upper.contains("BULK") {
        "CARGO".to_owned()
    } else if upper.contains("TANKER") {
        "TANKER".to_owned()
    } else if upper.contains("PASSENGER") || upper.contains("FERRY") || upper.contains("CRUISE") {
        "PASSENGER".to_owned()
    } else if upper.contains("TUG") || upper.contains("TOW") {
        "TUG".to_owned()
    } else if upper.contains("FISH") {
        "FISHING".to_owned()
    } else if upper.contains("MILITARY") || upper.contains("NAVAL") || upper.contains("WARSHIP") {
        "MILITARY".to_owned()
    } else if upper.contains("YACHT") || upper.contains("SAIL") || upper.contains("PLEASURE") {
        "PLEASURE".to_owned()
    } else {
        upper
    }
}

/// Classifies a satellite's orbital regime from its mean motion (revolutions/day) and eccentricity.
#[must_use]
pub fn classify_orbital_regime(mean_motion_rev_day: f64, eccentricity: f64) -> &'static str {
    if eccentricity >= 0.25 {
        "HEO"
    } else if mean_motion_rev_day >= 11.25 {
        "LEO"
    } else if (0.9..=1.1).contains(&mean_motion_rev_day) {
        "GEO"
    } else {
        "MEO"
    }
}

/// Computes `min`, `median` (P50), and `max` summary statistics across finite values.
#[must_use]
pub fn compute_numeric_stats(metric: &str, values: &[f64]) -> Option<QueryPackNumericStats> {
    let mut finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return None;
    }
    finite.sort_by(f64::total_cmp);
    let count = finite.len();
    let min = round_1(*finite.first()?);
    let max = round_1(*finite.last()?);
    let mid = count / 2;
    let raw_median =
        if count % 2 == 1 { finite[mid] } else { f64::midpoint(finite[mid - 1], finite[mid]) };
    Some(QueryPackNumericStats {
        metric: metric.to_owned(),
        count,
        min,
        median: round_1(raw_median),
        max,
    })
}

/// Counts occurrences per group key and returns up to `max_buckets` sorted by count descending.
#[must_use]
pub fn compute_breakdown<I>(keys: I, max_buckets: usize) -> Vec<QueryPackBreakdownBucket>
where
    I: IntoIterator<Item = String>,
{
    let mut counts: HashMap<String, usize> = HashMap::new();
    for key in keys {
        if !key.is_empty() {
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    let mut buckets: Vec<QueryPackBreakdownBucket> =
        counts.into_iter().map(|(key, count)| QueryPackBreakdownBucket { key, count }).collect();
    buckets.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));
    buckets.truncate(max_buckets);
    buckets
}

fn round_1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn bbox_center(bbox: BBox) -> GeoPoint {
    let lat = f64::midpoint(bbox.south, bbox.north);
    let lon = if bbox.crosses_antimeridian() {
        let mut mid = f64::midpoint(bbox.west, bbox.east + 360.0);
        if mid > 180.0 {
            mid -= 360.0;
        }
        mid
    } else {
        f64::midpoint(bbox.west, bbox.east)
    };
    GeoPoint { lat, lon, alt_m: None }
}

fn distance_from_center_km(center: Option<GeoPoint>, pos: GeoPoint) -> Option<f64> {
    center.map(|c| round_1(distance_m(&c, &pos) / 1000.0))
}

/// Snapshot inputs passed to the query pack execution engine.
#[derive(Debug, Default)]
pub struct QueryPackWorld<'a> {
    pub aircraft: &'a [Aircraft],
    pub vessels: &'a [Vessel],
    pub satellites: &'a [SatelliteElements],
    pub earthquakes: &'a [Earthquake],
    pub cyclones: &'a [Cyclone],
    pub fires: &'a [FireCell],
    pub gnss_cells: &'a [GnssCell],
    pub alpr_cameras: &'a [AlprCamera],
    pub cables: &'a [SubmarineCable],
}

/// Returns the static catalog of registered analyst query packs.
#[must_use]
pub fn query_pack_catalog() -> Vec<QueryPackDefinition> {
    vec![
        QueryPackDefinition {
            id: "aircraft_in_area".to_owned(),
            title: "Aircraft in theater".to_owned(),
            domain: QueryPackDomain::Aviation,
            description: "Live ADS-B/MLAT aircraft contacts inside the target area with ICAO airline breakdown and airborne altitude statistics.".to_owned(),
            default_limit: DEFAULT_QUERY_PACK_LIMIT,
        },
        QueryPackDefinition {
            id: "military_aircraft_watch".to_owned(),
            title: "Military & special-mission aviation watch".to_owned(),
            domain: QueryPackDomain::Aviation,
            description: "Military and special-mission transponders sorted by proximity with aircraft type and squawk breakdown.".to_owned(),
            default_limit: DEFAULT_QUERY_PACK_LIMIT,
        },
        QueryPackDefinition {
            id: "vessels_in_area".to_owned(),
            title: "AIS maritime vessels in area".to_owned(),
            domain: QueryPackDomain::Maritime,
            description: "Active AIS ships inside the target area grouped by ITU-R M.1371 ship family with speed-over-ground statistics.".to_owned(),
            default_limit: DEFAULT_QUERY_PACK_LIMIT,
        },
        QueryPackDefinition {
            id: "maritime_dark_targets".to_owned(),
            title: "Maritime dark-vessel & loiter watch".to_owned(),
            domain: QueryPackDomain::Maritime,
            description: "Detects slow-loitering or anomalous AIS contacts and generates a DuckDB LAG() transponder-gap SQL query plan.".to_owned(),
            default_limit: DEFAULT_QUERY_PACK_LIMIT,
        },
        QueryPackDefinition {
            id: "satellites_overhead".to_owned(),
            title: "Orbital catalog & regime breakdown".to_owned(),
            domain: QueryPackDomain::Space,
            description: "Tracked CelesTrak orbital elements classified into LEO, MEO, GEO, and HEO regimes with inclination statistics.".to_owned(),
            default_limit: DEFAULT_QUERY_PACK_LIMIT,
        },
        QueryPackDefinition {
            id: "hazards_in_area".to_owned(),
            title: "Multi-hazard seismic, cyclone & thermal scan".to_owned(),
            domain: QueryPackDomain::Hazards,
            description: "Correlates USGS earthquakes, NOAA tropical cyclones, and NASA FIRMS wildfire radiative power clusters.".to_owned(),
            default_limit: DEFAULT_QUERY_PACK_LIMIT,
        },
        QueryPackDefinition {
            id: "gnss_interference_watch".to_owned(),
            title: "GNSS navigation integrity & jamming cells".to_owned(),
            domain: QueryPackDomain::Surveillance,
            description: "DO-260A/B navigation accuracy anomalies (NIC < 7 or NACp < 8) across 0.5° cells using the 1-aircraft false-positive discount.".to_owned(),
            default_limit: DEFAULT_QUERY_PACK_LIMIT,
        },
        QueryPackDefinition {
            id: "alpr_surveillance_scan".to_owned(),
            title: "ALPR surveillance camera density".to_owned(),
            domain: QueryPackDomain::Surveillance,
            description: "Automated license plate reader (ALPR) cameras mapped in OpenStreetMap grouped by operator and manufacturer.".to_owned(),
            default_limit: DEFAULT_QUERY_PACK_LIMIT,
        },
        QueryPackDefinition {
            id: "submarine_cables_in_area".to_owned(),
            title: "Submarine telecommunications cables".to_owned(),
            domain: QueryPackDomain::Infrastructure,
            description: "Undersea fiber-optic cable corridors intersecting the target maritime bounding box.".to_owned(),
            default_limit: DEFAULT_QUERY_PACK_LIMIT,
        },
        QueryPackDefinition {
            id: "theater_situation_brief".to_owned(),
            title: "Composite theater situation brief".to_owned(),
            domain: QueryPackDomain::Briefing,
            description: "Cross-domain situation report combining military aviation, maritime traffic, natural hazards, and GNSS integrity anomalies.".to_owned(),
            default_limit: DEFAULT_QUERY_PACK_LIMIT,
        },
    ]
}

/// Executes a named analyst query pack against the current world state.
///
/// # Errors
/// Returns [`CoreError::NotFound`] if `pack_id` does not match a registered query pack,
/// or [`CoreError::BadRequest`] if `limit` is 0 or exceeds [`MAX_QUERY_PACK_LIMIT`].
#[allow(clippy::too_many_lines)]
pub fn execute_query_pack(
    pack_id: &str,
    request: &QueryPackRequest,
    world: &QueryPackWorld<'_>,
    now: Timestamp,
) -> Result<QueryPackExecution, CoreError> {
    let catalog = query_pack_catalog();
    let Some(def) = catalog.into_iter().find(|d| d.id == pack_id) else {
        return Err(CoreError::NotFound(format!("Unknown analyst query pack '{pack_id}'.")));
    };

    let limit = request.limit.unwrap_or(def.default_limit);
    if limit == 0 || limit > MAX_QUERY_PACK_LIMIT {
        return Err(CoreError::BadRequest(format!(
            "Query pack limit must be between 1 and {MAX_QUERY_PACK_LIMIT}."
        )));
    }

    let bbox = request.bbox;
    let center = bbox.map(bbox_center);
    let filter_upper = request
        .filter
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_ascii_uppercase);

    let start_time = now.checked_sub(Span::new().hours(24)).unwrap_or(now);

    match pack_id {
        "aircraft_in_area" | "military_aircraft_watch" => {
            let military_only = pack_id == "military_aircraft_watch";
            let matched: Vec<&Aircraft> = world
                .aircraft
                .iter()
                .filter(|a| !military_only || a.is_military)
                .filter(|a| bbox.is_none_or(|b| b.contains(&a.position)))
                .filter(|a| {
                    filter_upper.as_ref().is_none_or(|f| {
                        a.callsign.as_deref().is_some_and(|c| c.to_ascii_uppercase().contains(f))
                            || a.type_code
                                .as_deref()
                                .is_some_and(|t| t.to_ascii_uppercase().contains(f))
                            || a.hex.to_ascii_uppercase().contains(f)
                    })
                })
                .collect();

            let total_matches = matched.len();
            let altitudes_m: Vec<f64> = matched
                .iter()
                .filter(|a| !a.on_ground)
                .filter_map(|a| a.altitude_baro_ft.or(a.altitude_geom_ft))
                .map(|ft| ft * 0.3048)
                .collect();
            let stats = compute_numeric_stats("altitude_m", &altitudes_m);

            let breakdown = compute_breakdown(
                matched.iter().map(|a| {
                    if military_only {
                        a.type_code.clone().unwrap_or_else(|| "MILITARY".to_owned())
                    } else {
                        a.callsign
                            .as_deref()
                            .and_then(airline_designator)
                            .or_else(|| a.type_code.clone())
                            .unwrap_or_else(|| "GENERAL".to_owned())
                    }
                }),
                8,
            );

            let mut rows: Vec<QueryPackRow> = matched
                .into_iter()
                .map(|a| {
                    let alt_m =
                        a.altitude_baro_ft.or(a.altitude_geom_ft).map(|ft| round_1(ft * 0.3048));
                    let airline = a.callsign.as_deref().and_then(airline_designator);
                    let subtitle = match (airline, a.type_code.as_deref()) {
                        (Some(air), Some(tc)) => Some(format!("{air} · {tc}")),
                        (Some(air), None) => Some(air),
                        (None, Some(tc)) => Some(tc.to_owned()),
                        (None, None) => Some(a.source.clone()),
                    };
                    QueryPackRow {
                        id: a.hex.clone(),
                        kind: "aircraft".to_owned(),
                        label: a.callsign.clone().unwrap_or_else(|| a.hex.to_ascii_uppercase()),
                        subtitle,
                        position: Some(a.position),
                        distance_km: distance_from_center_km(center, a.position),
                        primary_metric: alt_m,
                        primary_unit: alt_m.map(|_| "m".to_owned()),
                    }
                })
                .collect();

            rows.sort_by(|a, b| {
                a.distance_km
                    .unwrap_or(f64::MAX)
                    .total_cmp(&b.distance_km.unwrap_or(f64::MAX))
                    .then_with(|| a.id.cmp(&b.id))
            });
            let truncated = rows.len() > limit;
            rows.truncate(limit);

            let duckdb_sql = build_trajectory_query(&TrajectoryQuery {
                parquet_pattern: "data/analytics/tracks/aircraft/**/*.parquet",
                entity_ids: &[],
                bbox,
                start_time,
                end_time: now,
                limit,
            });

            let summary = if military_only {
                format!("{total_matches} military aircraft reported in theater.")
            } else {
                format!("{total_matches} aircraft contacts reported in theater.")
            };

            Ok(QueryPackExecution {
                pack_id: def.id,
                title: def.title,
                domain: def.domain,
                executed_at: now,
                bbox,
                total_matches,
                truncated,
                summary,
                duckdb_sql,
                breakdown,
                stats,
                rows,
            })
        }
        "vessels_in_area" | "maritime_dark_targets" => {
            let dark_watch = pack_id == "maritime_dark_targets";
            let matched: Vec<&Vessel> = world
                .vessels
                .iter()
                .filter(|v| bbox.is_none_or(|b| b.contains(&v.position)))
                .filter(|v| {
                    if dark_watch {
                        // Flag vessels loitering at <= 2.5 knots or missing voyage destination
                        v.speed_knots.is_none_or(|s| s <= 2.5) || v.destination.is_none()
                    } else {
                        true
                    }
                })
                .filter(|v| {
                    filter_upper.as_ref().is_none_or(|f| {
                        normalize_vessel_type_family(v.vessel_type.as_deref()).contains(f)
                            || v.name.as_deref().is_some_and(|n| n.to_ascii_uppercase().contains(f))
                            || v.mmsi.contains(f)
                    })
                })
                .collect();

            let total_matches = matched.len();
            let speeds: Vec<f64> = matched.iter().filter_map(|v| v.speed_knots).collect();
            let stats = compute_numeric_stats("speed_knots", &speeds);
            let breakdown = compute_breakdown(
                matched.iter().map(|v| normalize_vessel_type_family(v.vessel_type.as_deref())),
                8,
            );

            let mut rows: Vec<QueryPackRow> = matched
                .into_iter()
                .map(|v| {
                    let family = normalize_vessel_type_family(v.vessel_type.as_deref());
                    let subtitle = match v.destination.as_deref() {
                        Some(dest) if !dest.trim().is_empty() => {
                            Some(format!("{family} → {}", dest.trim()))
                        }
                        _ => Some(family),
                    };
                    QueryPackRow {
                        id: v.mmsi.clone(),
                        kind: "vessels".to_owned(),
                        label: v.name.clone().unwrap_or_else(|| format!("MMSI {}", v.mmsi)),
                        subtitle,
                        position: Some(v.position),
                        distance_km: distance_from_center_km(center, v.position),
                        primary_metric: v.speed_knots.map(round_1),
                        primary_unit: v.speed_knots.map(|_| "kt".to_owned()),
                    }
                })
                .collect();

            rows.sort_by(|a, b| {
                a.distance_km
                    .unwrap_or(f64::MAX)
                    .total_cmp(&b.distance_km.unwrap_or(f64::MAX))
                    .then_with(|| a.id.cmp(&b.id))
            });
            let truncated = rows.len() > limit;
            rows.truncate(limit);

            let duckdb_sql = if dark_watch {
                build_dark_target_query(&DarkTargetQuery {
                    parquet_pattern: "data/analytics/tracks/vessels/**/*.parquet",
                    bbox,
                    start_time,
                    end_time: now,
                    min_gap_seconds: 1800,
                    min_speed_knots: 3.0,
                    limit,
                })
            } else {
                build_trajectory_query(&TrajectoryQuery {
                    parquet_pattern: "data/analytics/tracks/vessels/**/*.parquet",
                    entity_ids: &[],
                    bbox,
                    start_time,
                    end_time: now,
                    limit,
                })
            };

            let summary = if dark_watch {
                format!("{total_matches} loitering or dark-candidate vessels identified.")
            } else {
                format!("{total_matches} AIS vessels reported in area.")
            };

            Ok(QueryPackExecution {
                pack_id: def.id,
                title: def.title,
                domain: def.domain,
                executed_at: now,
                bbox,
                total_matches,
                truncated,
                summary,
                duckdb_sql,
                breakdown,
                stats,
                rows,
            })
        }
        "satellites_overhead" => {
            let matched: Vec<&SatelliteElements> = world
                .satellites
                .iter()
                .filter(|s| {
                    filter_upper.as_ref().is_none_or(|f| {
                        s.name.to_ascii_uppercase().contains(f)
                            || s.group.to_ascii_uppercase().contains(f)
                            || classify_orbital_regime(s.mean_motion, s.eccentricity) == f
                    })
                })
                .collect();

            let total_matches = matched.len();
            let inclinations: Vec<f64> = matched.iter().map(|s| s.inclination_deg).collect();
            let stats = compute_numeric_stats("inclination_deg", &inclinations);
            let breakdown = compute_breakdown(
                matched
                    .iter()
                    .map(|s| classify_orbital_regime(s.mean_motion, s.eccentricity).to_owned()),
                6,
            );

            let mut rows: Vec<QueryPackRow> = matched
                .into_iter()
                .map(|s| {
                    let regime = classify_orbital_regime(s.mean_motion, s.eccentricity);
                    QueryPackRow {
                        id: s.norad_id.to_string(),
                        kind: "satellites".to_owned(),
                        label: s.name.clone(),
                        subtitle: Some(format!("{regime} · {} · NORAD {}", s.group, s.norad_id)),
                        position: None,
                        distance_km: None,
                        primary_metric: Some(round_1(s.inclination_deg)),
                        primary_unit: Some("deg".to_owned()),
                    }
                })
                .collect();

            let truncated = rows.len() > limit;
            rows.truncate(limit);

            Ok(QueryPackExecution {
                pack_id: def.id,
                title: def.title,
                domain: def.domain,
                executed_at: now,
                bbox,
                total_matches,
                truncated,
                summary: format!("{total_matches} tracked orbital objects in catalog."),
                duckdb_sql: format!(
                    "SELECT norad_id, name, group_id, inclination_deg, mean_motion FROM satellites LIMIT {limit};"
                ),
                breakdown,
                stats,
                rows,
            })
        }
        "hazards_in_area" => {
            let mut rows = Vec::new();
            let mut bucket_keys = Vec::new();
            let mut magnitudes = Vec::new();

            for eq in world.earthquakes {
                if bbox.is_none_or(|b| b.contains(&eq.position)) {
                    bucket_keys.push("EARTHQUAKE".to_owned());
                    magnitudes.push(eq.magnitude);
                    rows.push(QueryPackRow {
                        id: eq.id.clone(),
                        kind: "earthquakes".to_owned(),
                        label: format!(
                            "M{:.1} — {}",
                            eq.magnitude,
                            eq.place.as_deref().unwrap_or("Seismic event")
                        ),
                        subtitle: Some(format!("Depth {:.1} km", eq.depth_km)),
                        position: Some(eq.position),
                        distance_km: distance_from_center_km(center, eq.position),
                        primary_metric: Some(round_1(eq.magnitude)),
                        primary_unit: Some("M".to_owned()),
                    });
                }
            }

            for cy in world.cyclones {
                if bbox.is_none_or(|b| b.contains(&cy.position)) {
                    bucket_keys.push("CYCLONE".to_owned());
                    let wind_kt = cy.intensity_kt.unwrap_or(0.0);
                    rows.push(QueryPackRow {
                        id: cy.id.clone(),
                        kind: "cyclones".to_owned(),
                        label: cy.name.clone(),
                        subtitle: Some(format!(
                            "{} · {:.0} kt max winds",
                            cy.classification, wind_kt
                        )),
                        position: Some(cy.position),
                        distance_km: distance_from_center_km(center, cy.position),
                        primary_metric: cy.intensity_kt.map(round_1),
                        primary_unit: cy.intensity_kt.map(|_| "kt".to_owned()),
                    });
                }
            }

            for fire in world.fires {
                let pos = GeoPoint { lat: fire.lat, lon: fire.lon, alt_m: None };
                if bbox.is_none_or(|b| b.contains(&pos)) {
                    bucket_keys.push("WILDFIRE".to_owned());
                    rows.push(QueryPackRow {
                        id: format!("fire:{:.3}:{:.3}", fire.lat, fire.lon),
                        kind: "fires".to_owned(),
                        label: format!(
                            "{} thermal cluster ({} detections)",
                            fire.satellite, fire.count
                        ),
                        subtitle: Some(format!("Confidence {}", fire.confidence)),
                        position: Some(pos),
                        distance_km: distance_from_center_km(center, pos),
                        primary_metric: Some(round_1(fire.frp)),
                        primary_unit: Some("MW".to_owned()),
                    });
                }
            }

            let total_matches = rows.len();
            let stats = compute_numeric_stats("magnitude", &magnitudes);
            let breakdown = compute_breakdown(bucket_keys, 6);

            rows.sort_by(|a, b| {
                a.distance_km
                    .unwrap_or(f64::MAX)
                    .total_cmp(&b.distance_km.unwrap_or(f64::MAX))
                    .then_with(|| a.id.cmp(&b.id))
            });
            let truncated = rows.len() > limit;
            rows.truncate(limit);

            Ok(QueryPackExecution {
                pack_id: def.id,
                title: def.title,
                domain: def.domain,
                executed_at: now,
                bbox,
                total_matches,
                truncated,
                summary: format!("{total_matches} active hazard detections in area."),
                duckdb_sql: format!(
                    "SELECT id, hazard_kind, severity, ST_X(geom) AS lon, ST_Y(geom) AS lat FROM hazards LIMIT {limit};"
                ),
                breakdown,
                stats,
                rows,
            })
        }
        "gnss_interference_watch" => {
            let matched: Vec<&GnssCell> = world
                .gnss_cells
                .iter()
                .filter(|c| bbox.is_none_or(|b| b.contains(&c.center)))
                .collect();

            let total_matches = matched.len();
            let rates: Vec<f64> = matched.iter().map(|c| c.percent_degraded).collect();
            let stats = compute_numeric_stats("percent_degraded", &rates);
            let breakdown = compute_breakdown(
                matched.iter().map(|c| match c.level {
                    GnssLevel::High => "HIGH".to_owned(),
                    GnssLevel::Medium => "MEDIUM".to_owned(),
                    GnssLevel::Low => "LOW".to_owned(),
                }),
                3,
            );

            let mut rows: Vec<QueryPackRow> = matched
                .into_iter()
                .map(|c| QueryPackRow {
                    id: c.id.clone(),
                    kind: "gnss".to_owned(),
                    label: format!("Cell {} ({:.1}% degraded)", c.id, c.percent_degraded),
                    subtitle: Some(format!(
                        "{}/{} aircraft degraded · {}",
                        c.degraded, c.aircraft, c.state
                    )),
                    position: Some(c.center),
                    distance_km: distance_from_center_km(center, c.center),
                    primary_metric: Some(round_1(c.percent_degraded)),
                    primary_unit: Some("%".to_owned()),
                })
                .collect();

            rows.sort_by(|a, b| {
                b.primary_metric.unwrap_or(0.0).total_cmp(&a.primary_metric.unwrap_or(0.0))
            });
            let truncated = rows.len() > limit;
            rows.truncate(limit);

            Ok(QueryPackExecution {
                pack_id: def.id,
                title: def.title,
                domain: def.domain,
                executed_at: now,
                bbox,
                total_matches,
                truncated,
                summary: format!("{total_matches} GNSS integrity cells evaluated."),
                duckdb_sql: format!(
                    "SELECT cell_id, aircraft, degraded, percent_degraded, level FROM gnss_cells ORDER BY percent_degraded DESC LIMIT {limit};"
                ),
                breakdown,
                stats,
                rows,
            })
        }
        "alpr_surveillance_scan" => {
            let matched: Vec<&AlprCamera> = world
                .alpr_cameras
                .iter()
                .filter(|c| bbox.is_none_or(|b| b.contains(&c.position)))
                .collect();

            let total_matches = matched.len();
            let breakdown = compute_breakdown(
                matched.iter().map(|c| {
                    c.manufacturer
                        .clone()
                        .or_else(|| c.operator.clone())
                        .unwrap_or_else(|| "UNSPECIFIED".to_owned())
                }),
                8,
            );

            let mut rows: Vec<QueryPackRow> = matched
                .into_iter()
                .map(|c| QueryPackRow {
                    id: c.id.clone(),
                    kind: "alpr".to_owned(),
                    label: c
                        .operator
                        .clone()
                        .unwrap_or_else(|| format!("ALPR Camera #{}", c.osm_id)),
                    subtitle: c.manufacturer.clone().or_else(|| c.zone.clone()),
                    position: Some(c.position),
                    distance_km: distance_from_center_km(center, c.position),
                    primary_metric: c.direction_deg.map(round_1),
                    primary_unit: c.direction_deg.map(|_| "deg".to_owned()),
                })
                .collect();

            rows.sort_by(|a, b| {
                a.distance_km
                    .unwrap_or(f64::MAX)
                    .total_cmp(&b.distance_km.unwrap_or(f64::MAX))
                    .then_with(|| a.id.cmp(&b.id))
            });
            let truncated = rows.len() > limit;
            rows.truncate(limit);

            Ok(QueryPackExecution {
                pack_id: def.id,
                title: def.title,
                domain: def.domain,
                executed_at: now,
                bbox,
                total_matches,
                truncated,
                summary: format!("{total_matches} ALPR surveillance cameras mapped in area."),
                duckdb_sql: format!(
                    "SELECT id, osm_id, operator, manufacturer, direction_deg FROM alpr_cameras LIMIT {limit};"
                ),
                breakdown,
                stats: None,
                rows,
            })
        }
        "submarine_cables_in_area" => {
            let matched: Vec<&SubmarineCable> = world
                .cables
                .iter()
                .filter(|c| {
                    bbox.is_none_or(|b| {
                        c.coordinates
                            .iter()
                            .flat_map(|seg| seg.iter())
                            .any(|pt| b.contains(&GeoPoint { lat: pt[1], lon: pt[0], alt_m: None }))
                    })
                })
                .collect();

            let total_matches = matched.len();
            let breakdown = vec![QueryPackBreakdownBucket {
                key: "SUBMARINE_CABLE".to_owned(),
                count: total_matches,
            }];

            let mut rows: Vec<QueryPackRow> = matched
                .into_iter()
                .map(|c| {
                    let first_pt = c
                        .coordinates
                        .first()
                        .and_then(|seg| seg.first())
                        .map(|pt| GeoPoint { lat: pt[1], lon: pt[0], alt_m: None });
                    let segments = c.coordinates.len();
                    let segments_f64 = u32::try_from(segments).map_or(0.0, f64::from);
                    QueryPackRow {
                        id: c.id.clone(),
                        kind: "cables".to_owned(),
                        label: c.name.clone(),
                        subtitle: Some(format!("{segments} cable segment(s)")),
                        position: first_pt,
                        distance_km: first_pt.and_then(|p| distance_from_center_km(center, p)),
                        primary_metric: Some(segments_f64),
                        primary_unit: Some("seg".to_owned()),
                    }
                })
                .collect();

            let truncated = rows.len() > limit;
            rows.truncate(limit);

            Ok(QueryPackExecution {
                pack_id: def.id,
                title: def.title,
                domain: def.domain,
                executed_at: now,
                bbox,
                total_matches,
                truncated,
                summary: format!("{total_matches} submarine telecommunications cables in area."),
                duckdb_sql: format!(
                    "SELECT id, name, ST_NumGeometries(geom) AS segments FROM submarine_cables LIMIT {limit};"
                ),
                breakdown,
                stats: None,
                rows,
            })
        }
        "theater_situation_brief" => {
            let mut rows = Vec::new();
            let mut bucket_keys = Vec::new();

            for a in world
                .aircraft
                .iter()
                .filter(|a| bbox.is_none_or(|b| b.contains(&a.position)))
                .take(15)
            {
                bucket_keys.push(if a.is_military {
                    "MILITARY_AIR".to_owned()
                } else {
                    "CIVIL_AIR".to_owned()
                });
                rows.push(QueryPackRow {
                    id: a.hex.clone(),
                    kind: "aircraft".to_owned(),
                    label: a.callsign.clone().unwrap_or_else(|| a.hex.to_ascii_uppercase()),
                    subtitle: Some(if a.is_military {
                        "Military aircraft".to_owned()
                    } else {
                        "Aircraft".to_owned()
                    }),
                    position: Some(a.position),
                    distance_km: distance_from_center_km(center, a.position),
                    primary_metric: a.ground_speed_kt.map(round_1),
                    primary_unit: a.ground_speed_kt.map(|_| "kt".to_owned()),
                });
            }

            for v in world
                .vessels
                .iter()
                .filter(|v| bbox.is_none_or(|b| b.contains(&v.position)))
                .take(15)
            {
                bucket_keys.push("MARITIME".to_owned());
                rows.push(QueryPackRow {
                    id: v.mmsi.clone(),
                    kind: "vessels".to_owned(),
                    label: v.name.clone().unwrap_or_else(|| format!("MMSI {}", v.mmsi)),
                    subtitle: Some(normalize_vessel_type_family(v.vessel_type.as_deref())),
                    position: Some(v.position),
                    distance_km: distance_from_center_km(center, v.position),
                    primary_metric: v.speed_knots.map(round_1),
                    primary_unit: v.speed_knots.map(|_| "kt".to_owned()),
                });
            }

            for eq in world
                .earthquakes
                .iter()
                .filter(|e| bbox.is_none_or(|b| b.contains(&e.position)))
                .take(10)
            {
                bucket_keys.push("HAZARD".to_owned());
                rows.push(QueryPackRow {
                    id: eq.id.clone(),
                    kind: "earthquakes".to_owned(),
                    label: format!("M{:.1} Earthquake", eq.magnitude),
                    subtitle: eq.place.clone(),
                    position: Some(eq.position),
                    distance_km: distance_from_center_km(center, eq.position),
                    primary_metric: Some(round_1(eq.magnitude)),
                    primary_unit: Some("M".to_owned()),
                });
            }

            for cell in world
                .gnss_cells
                .iter()
                .filter(|c| c.level != GnssLevel::Low && bbox.is_none_or(|b| b.contains(&c.center)))
                .take(10)
            {
                bucket_keys.push("GNSS_ANOMALY".to_owned());
                rows.push(QueryPackRow {
                    id: cell.id.clone(),
                    kind: "gnss".to_owned(),
                    label: format!("GNSS Anomaly Cell {}", cell.id),
                    subtitle: Some(format!("{:.1}% degraded", cell.percent_degraded)),
                    position: Some(cell.center),
                    distance_km: distance_from_center_km(center, cell.center),
                    primary_metric: Some(round_1(cell.percent_degraded)),
                    primary_unit: Some("%".to_owned()),
                });
            }

            let total_matches = rows.len();
            let breakdown = compute_breakdown(bucket_keys, 8);
            let truncated = rows.len() > limit;
            rows.truncate(limit);

            Ok(QueryPackExecution {
                pack_id: def.id,
                title: def.title,
                domain: def.domain,
                executed_at: now,
                bbox,
                total_matches,
                truncated,
                summary: format!(
                    "Composite theater brief compiled with {total_matches} multi-domain contacts."
                ),
                duckdb_sql: build_trajectory_query(&TrajectoryQuery {
                    parquet_pattern: "data/analytics/tracks/**/*.parquet",
                    entity_ids: &[],
                    bbox,
                    start_time,
                    end_time: now,
                    limit,
                }),
                breakdown,
                stats: None,
                rows,
            })
        }
        _ => Err(CoreError::NotFound(format!("Unknown analyst query pack '{pack_id}'."))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airline_designator_matches_icao_callsigns_and_rejects_registrations() {
        assert_eq!(airline_designator("UAL123"), Some("UAL".to_owned()));
        assert_eq!(airline_designator("swa1234"), Some("SWA".to_owned()));
        assert_eq!(airline_designator("BAW12A"), Some("BAW".to_owned()));
        // Tail registrations and excluded prefixes
        assert_eq!(airline_designator("N123AB"), None);
        assert_eq!(airline_designator("RPC1234"), None);
        assert_eq!(airline_designator("CUT1240"), None);
        assert_eq!(airline_designator("AB12"), None);
    }

    #[test]
    fn vessel_type_family_maps_itu_codes_and_labels() {
        assert_eq!(normalize_vessel_type_family(Some("70")), "CARGO");
        assert_eq!(normalize_vessel_type_family(Some("79")), "CARGO");
        assert_eq!(normalize_vessel_type_family(Some("82")), "TANKER");
        assert_eq!(normalize_vessel_type_family(Some("60")), "PASSENGER");
        assert_eq!(normalize_vessel_type_family(Some("35")), "MILITARY");
        assert_eq!(normalize_vessel_type_family(Some("0")), "UNKNOWN");
        assert_eq!(normalize_vessel_type_family(Some("Container Ship")), "CARGO");
        assert_eq!(normalize_vessel_type_family(None), "UNKNOWN");
    }

    #[test]
    fn execute_aircraft_and_maritime_query_packs() {
        let now = Timestamp::from_second(1_760_000_000).unwrap();
        let aircraft = vec![
            Aircraft {
                hex: "a1b2c3".to_owned(),
                callsign: Some("UAL451".to_owned()),
                registration: Some("N123UA".to_owned()),
                type_code: Some("B738".to_owned()),
                category: Some("A3".to_owned()),
                position: GeoPoint { lat: 37.62, lon: -122.38, alt_m: None },
                altitude_baro_ft: Some(32_000.0),
                altitude_geom_ft: None,
                on_ground: false,
                ground_speed_kt: Some(455.0),
                track_deg: Some(90.0),
                vertical_rate_fpm: Some(0.0),
                squawk: Some("1200".to_owned()),
                emergency: None,
                is_military: false,
                seen_pos_s: 1.0,
                source: "adsb.lol".to_owned(),
            },
            Aircraft {
                hex: "ae0102".to_owned(),
                callsign: Some("RCH401".to_owned()),
                registration: None,
                type_code: Some("C17".to_owned()),
                category: Some("A5".to_owned()),
                position: GeoPoint { lat: 37.75, lon: -122.42, alt_m: None },
                altitude_baro_ft: Some(28_000.0),
                altitude_geom_ft: None,
                on_ground: false,
                ground_speed_kt: Some(430.0),
                track_deg: Some(180.0),
                vertical_rate_fpm: None,
                squawk: None,
                emergency: None,
                is_military: true,
                seen_pos_s: 0.5,
                source: "adsb.lol".to_owned(),
            },
        ];

        let vessels = vec![Vessel {
            mmsi: "366999712".to_owned(),
            name: Some("PACIFIC HORIZON".to_owned()),
            imo: Some("9123456".to_owned()),
            vessel_type: Some("74".to_owned()),
            destination: Some("OAKLAND".to_owned()),
            position: GeoPoint { lat: 37.80, lon: -122.35, alt_m: None },
            speed_knots: Some(1.2),
            course_deg: Some(110.0),
            heading_deg: Some(112.0),
            last_position_utc: None,
            last_position_epoch: None,
        }];

        let world =
            QueryPackWorld { aircraft: &aircraft, vessels: &vessels, ..QueryPackWorld::default() };

        let bbox = BBox::new(-123.0, 37.0, -122.0, 38.0).unwrap();
        let req = QueryPackRequest { bbox: Some(bbox), limit: Some(10), filter: None };

        let air_res = execute_query_pack("aircraft_in_area", &req, &world, now).unwrap();
        assert_eq!(air_res.total_matches, 2);
        assert_eq!(air_res.breakdown.len(), 2);
        assert!(air_res.stats.is_some());

        let mil_res = execute_query_pack("military_aircraft_watch", &req, &world, now).unwrap();
        assert_eq!(mil_res.total_matches, 1);
        assert_eq!(mil_res.rows[0].label, "RCH401");

        let dark_res = execute_query_pack("maritime_dark_targets", &req, &world, now).unwrap();
        assert_eq!(dark_res.total_matches, 1);
        assert_eq!(dark_res.rows[0].label, "PACIFIC HORIZON");
        assert!(dark_res.duckdb_sql.contains("LAG(timestamp)"));
    }
}
