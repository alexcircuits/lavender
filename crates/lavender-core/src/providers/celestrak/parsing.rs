//! Parses CelesTrak OMM documents into validated satellite elements, without network or cache access.

use crate::providers::{ProviderError, lenient, parse_rows};
use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use lavender_domain::SatelliteElements;
use lavender_domain::text::{clean, clean_opt};
use serde::Deserialize;

// Every field is lenient: a wrong type reads as missing, and `normalize_elements` drops the element
// set when a required field is missing (for example `"BSTAR": null`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct Omm {
    #[serde(default, deserialize_with = "lenient")]
    object_name: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    object_id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    epoch: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    mean_motion: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    eccentricity: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    inclination: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    ra_of_asc_node: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    arg_of_pericenter: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    mean_anomaly: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    classification_type: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    norad_cat_id: Option<u32>,
    #[serde(default, deserialize_with = "lenient")]
    rev_at_epoch: Option<u32>,
    #[serde(default, deserialize_with = "lenient")]
    bstar: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    mean_motion_dot: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    mean_motion_ddot: Option<f64>,
}

fn parse_epoch(s: &str) -> Option<Timestamp> {
    if let Ok(ts) = s.parse::<Timestamp>() {
        return Some(ts);
    }
    s.parse::<DateTime>().ok()?.to_zoned(TimeZone::UTC).ok().map(|z| z.timestamp())
}

fn normalize_elements(o: &Omm, group: &str) -> Option<SatelliteElements> {
    let finite = |v: Option<f64>| v.filter(|x| x.is_finite());
    let mean_motion = finite(o.mean_motion).filter(|m| *m > 0.0)?;
    let eccentricity = finite(o.eccentricity).filter(|e| (0.0..1.0).contains(e))?;
    let inclination = finite(o.inclination).filter(|i| (0.0..=180.0).contains(i))?;
    let norad_id = o.norad_cat_id.filter(|n| *n != 0)?;
    Some(SatelliteElements {
        norad_id,
        name: clean(o.object_name.as_deref().unwrap_or("Unnamed object"), 48),
        intl_designator: clean_opt(o.object_id.as_deref(), 16),
        group: group.to_owned(),
        epoch: parse_epoch(o.epoch.as_deref()?)?,
        mean_motion,
        eccentricity,
        inclination_deg: inclination,
        raan_deg: finite(o.ra_of_asc_node)?,
        arg_of_pericenter_deg: finite(o.arg_of_pericenter)?,
        mean_anomaly_deg: finite(o.mean_anomaly)?,
        bstar: finite(o.bstar)?,
        mean_motion_dot: finite(o.mean_motion_dot)?,
        mean_motion_ddot: finite(o.mean_motion_ddot)?,
        rev_at_epoch: o.rev_at_epoch,
        classification: clean_opt(o.classification_type.as_deref(), 1),
    })
}

/// Parses one group's OMM JSON array. Invalid element sets are skipped; returns the valid
/// ones and how many were skipped.
///
/// # Errors
/// Returns an error when the document is not a JSON array (CelesTrak answers plain text
/// for unknown groups and rate-limit refusals).
pub fn parse(body: &[u8], group: &str) -> Result<(Vec<SatelliteElements>, usize), ProviderError> {
    let rows: Vec<serde_json::Value> =
        serde_json::from_slice(body).map_err(|e| ProviderError::parse("CelesTrak", &e))?;
    let total = rows.len();
    let (records, _) = parse_rows::<Omm>(rows);
    let elements: Vec<SatelliteElements> =
        records.iter().filter_map(|o| normalize_elements(o, group)).collect();
    let dropped = total - elements.len();
    Ok((elements, dropped))
}
