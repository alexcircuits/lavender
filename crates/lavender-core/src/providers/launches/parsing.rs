//! Parsing and normalisation for Launch Library 2 responses.
//!
//! Text from the API is untrusted: every string is cleaned of control characters and
//! length-capped, and a launch keeps at most `MAX_PAYLOADS` payload names.

use jiff::Timestamp;
use lavender_domain::text::clean_opt;
use lavender_domain::{Launch, LaunchPad};
use serde_json::Value;

use crate::providers::ProviderError;

const SOURCE: &str = "Launch Library 2";
const MAX_ID_CHARS: usize = 64;
const MAX_NAME_CHARS: usize = 200;
const MAX_LABEL_CHARS: usize = 120;
const MAX_DESCRIPTION_CHARS: usize = 2_000;
/// Payload names kept per launch.
const MAX_PAYLOADS: usize = 64;

/// One page of a Launch Library 2 launch list.
#[derive(Debug, Clone, PartialEq)]
pub struct LaunchPage {
    pub launches: Vec<Launch>,
    /// Rows that could not be read and were skipped.
    pub dropped: usize,
    /// Rows on this page, readable or not.
    pub rows: usize,
    /// Launches the query matched that this page does not contain (LL2 pages hold at
    /// most 100).
    pub not_returned: usize,
}

/// Parses a Launch Library 2 document: a paginated `{ "count", "results": [...] }` answer
/// or a bare array. Malformed rows are skipped and counted.
///
/// # Errors
/// Returns [`ProviderError::Parse`] when the body is not JSON or has no `results` array.
pub fn parse(body: &[u8]) -> Result<LaunchPage, ProviderError> {
    let value: Value =
        serde_json::from_slice(body).map_err(|e| ProviderError::parse(SOURCE, &e))?;
    let (rows, count) = match &value {
        Value::Object(map) => match map.get("results") {
            Some(Value::Array(rows)) => (rows.as_slice(), map.get("count").and_then(Value::as_u64)),
            _ => return Err(ProviderError::parse(SOURCE, &"missing results array")),
        },
        Value::Array(rows) => (rows.as_slice(), None),
        _ => return Err(ProviderError::parse(SOURCE, &"expected a JSON object or array")),
    };
    let launches: Vec<Launch> = rows.iter().filter_map(normalize_launch).collect();
    let not_returned =
        count.map_or(0, |c| usize::try_from(c).unwrap_or(usize::MAX).saturating_sub(rows.len()));
    Ok(LaunchPage {
        dropped: rows.len() - launches.len(),
        rows: rows.len(),
        launches,
        not_returned,
    })
}

/// Normalises one launch; `None` when it has no usable identifier.
fn normalize_launch(row: &Value) -> Option<Launch> {
    if !row.is_object() {
        return None;
    }
    let id = launch_id(row)?;
    Some(Launch {
        id,
        name: text(row.get("name"), MAX_NAME_CHARS).unwrap_or_else(|| "Unnamed launch".to_owned()),
        status: text(row.pointer("/status/name").or_else(|| row.get("status")), MAX_LABEL_CHARS)
            .unwrap_or_else(|| "Unknown".to_owned()),
        net: timestamp(row.get("net")),
        window_start: timestamp(row.get("window_start")),
        window_end: timestamp(row.get("window_end")),
        launch_designator: launch_designator(row.get("launch_designator")),
        provider: text(
            row.pointer("/launch_service_provider/name").or_else(|| row.get("provider")),
            MAX_LABEL_CHARS,
        ),
        rocket: text(
            row.pointer("/rocket/configuration/full_name")
                .or_else(|| row.pointer("/rocket/configuration/name"))
                .or_else(|| row.pointer("/rocket/name")),
            MAX_LABEL_CHARS,
        ),
        mission: text(row.pointer("/mission/name"), MAX_NAME_CHARS),
        mission_description: text(row.pointer("/mission/description"), MAX_DESCRIPTION_CHARS),
        orbit: text(row.pointer("/mission/orbit/name"), MAX_LABEL_CHARS),
        pad: pad(row.get("pad")),
        payload_names: payload_names(row),
        matched_norad_ids: Vec::new(),
    })
}

/// Cleaned, capped text; `None` for anything that is not a non-empty string.
fn text(value: Option<&Value>, max_chars: usize) -> Option<String> {
    clean_opt(value.and_then(Value::as_str), max_chars)
}

/// The `id` (a UUID in LL2 2.3.0, a number in older shapes), else the slug, else the name.
fn launch_id(row: &Value) -> Option<String> {
    let id = match row.get("id") {
        Some(Value::Number(n)) => n.as_i64().map(|n| n.to_string()),
        other => text(other, MAX_ID_CHARS),
    };
    id.or_else(|| text(row.get("slug"), MAX_ID_CHARS))
        .or_else(|| text(row.get("name"), MAX_ID_CHARS))
}

fn timestamp(value: Option<&Value>) -> Option<Timestamp> {
    value?.as_str()?.trim().parse().ok()
}

/// A COSPAR launch designator: four-digit year, hyphen, three-digit launch number.
fn launch_designator(value: Option<&Value>) -> Option<String> {
    let designator = value?.as_str()?.trim();
    let (year, number) = designator.split_once('-')?;
    let digits =
        |part: &str, len: usize| part.len() == len && part.bytes().all(|b| b.is_ascii_digit());
    (digits(year, 4) && digits(number, 3)).then(|| designator.to_owned())
}

fn coordinate(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
    .filter(|v| v.is_finite())
}

/// Pad latitude/longitude, else the location's `"lon,lat"` coordinates string.
fn pad_coordinates(pad: &Value) -> Option<(f64, f64)> {
    let in_range = |(lat, lon): (f64, f64)| {
        ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)).then_some((lat, lon))
    };
    let direct = coordinate(pad.get("latitude")).zip(coordinate(pad.get("longitude")));
    direct.and_then(in_range).or_else(|| {
        let coordinates = pad.pointer("/location/coordinates")?.as_str()?;
        let (lon, lat) = coordinates.split_once(',')?;
        in_range((lat.trim().parse().ok()?, lon.trim().parse().ok()?))
    })
}

/// The launch pad, when it has valid coordinates.
fn pad(value: Option<&Value>) -> Option<LaunchPad> {
    let pad = value?;
    let (latitude, longitude) = pad_coordinates(pad)?;
    let location_name = text(pad.pointer("/location/name"), MAX_LABEL_CHARS);
    let name = text(pad.get("name"), MAX_LABEL_CHARS)
        .or_else(|| location_name.clone())
        .unwrap_or_else(|| "Unknown launch pad".to_owned());
    Some(LaunchPad { name, location_name, latitude, longitude })
}

/// Distinct payload names from the payload flights LL2 lists (2.3.0 detailed mode puts them
/// under `rocket.payloads`; older shapes used `payloads` or `mission.payloads`).
fn payload_names(row: &Value) -> Vec<String> {
    let flights = ["/rocket/payloads", "/payloads", "/mission/payloads"]
        .into_iter()
        .filter_map(|pointer| row.pointer(pointer)?.as_array())
        .flatten();
    let mut names: Vec<String> = Vec::new();
    for flight in flights {
        let name =
            text(flight.pointer("/payload/name").or_else(|| flight.get("name")), MAX_LABEL_CHARS);
        if let Some(name) = name
            && !names.contains(&name)
        {
            names.push(name);
            if names.len() == MAX_PAYLOADS {
                break;
            }
        }
    }
    names
}
