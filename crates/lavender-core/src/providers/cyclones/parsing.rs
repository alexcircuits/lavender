//! Strict bounded projection of NOAA's status and GeoJSON query documents.

use std::collections::HashSet;

use jiff::Timestamp;
use lavender_domain::{
    Cyclone, CycloneConeGeometry, CycloneForecastPoint, CycloneGeometryStatus, CycloneMovement,
    CycloneTrackGeometry, GeoPoint,
};
use serde_json::Value;
use url::Url;

use super::ProviderError;

pub(super) fn invalid() -> ProviderError {
    ProviderError::parse("NOAA NHC/CPHC", &"invalid or unbounded cyclone data")
}

fn text(value: &Value, max: usize) -> Result<String, ProviderError> {
    let s = value.as_str().ok_or_else(invalid)?;
    if s.trim().is_empty()
        || s.len() > max
        || s.chars().any(|c| c.is_control() || c == '<' || c == '>')
    {
        return Err(invalid());
    }
    Ok(s.trim().to_owned())
}

fn number(value: &Value, low: f64, high: f64) -> Option<f64> {
    let n = value.as_f64().or_else(|| value.as_str()?.parse::<f64>().ok())?;
    (n.is_finite() && (low..=high).contains(&n)).then_some(n)
}

fn advisory(value: &Value) -> Result<String, ProviderError> {
    let s = value.as_str().ok_or_else(invalid)?;
    let digits = s.strip_suffix(|c: char| c.is_ascii_alphabetic()).unwrap_or(s);
    if !(1..=3).contains(&digits.len())
        || !digits.bytes().all(|b| b.is_ascii_digit())
        || s.len() > digits.len() + 1
    {
        return Err(invalid());
    }
    let n = digits.parse::<u16>().map_err(|_| invalid())?;
    Ok(format!("{n}{}", s[digits.len()..].to_ascii_uppercase()))
}

fn storm_id(id: &str) -> bool {
    id.len() == 8
        && ["al", "ep", "cp"].iter().any(|prefix| id.starts_with(prefix))
        && id[2..].bytes().all(|b| b.is_ascii_digit())
}

fn time(value: &Value, now: Timestamp) -> Result<Timestamp, ProviderError> {
    let s = value.as_str().ok_or_else(invalid)?;
    // NOAA uses UTC ISO timestamps; require whole seconds or exactly milliseconds.
    if !(s.len() == 20 || s.len() == 24) || !s.ends_with('Z') || !s.is_ascii() {
        return Err(invalid());
    }
    let bytes = s.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        let expected = match i {
            4 | 7 => Some(b'-'),
            10 => Some(b'T'),
            13 | 16 => Some(b':'),
            19 if bytes.len() == 24 => Some(b'.'),
            19 | 23 => Some(b'Z'),
            _ => None,
        };
        if expected.map_or(!b.is_ascii_digit(), |e| *b != e) {
            return Err(invalid());
        }
    }
    let stamp = s.parse::<Timestamp>().map_err(|_| invalid())?;
    let age = now.duration_since(stamp).as_secs();
    if !(-300..=43_200).contains(&age) {
        return Err(invalid());
    }
    // Parsing rejects impossible calendar dates rather than allowing Date normalisation.
    Ok(stamp)
}

fn position(value: &Value) -> Result<GeoPoint, ProviderError> {
    let pair = value.as_array().filter(|p| p.len() == 2).ok_or_else(invalid)?;
    let lon = pair[0].as_f64().ok_or_else(invalid)?;
    let lat = pair[1].as_f64().ok_or_else(invalid)?;
    GeoPoint::new(lat, lon).map_err(|_| invalid())
}

fn official_link(value: &Value) -> Option<String> {
    let s = value.as_str().filter(|s| s.len() <= 256)?;
    let url = Url::parse(s).ok()?;
    let name = url.path().strip_prefix("/text/")?.strip_suffix(".shtml")?;
    (url.origin().ascii_serialization() == "https://www.nhc.noaa.gov"
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && !name.is_empty()
        && name.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()))
    .then(|| url.to_string())
}

/// Validates the status document as a whole. One malformed storm cannot turn a partly
/// corrupt advisory into an apparently healthy season.
///
/// # Errors
/// Rejects malformed, oversized, duplicate, expired or future status records.
pub fn parse_status(body: &[u8], now: Timestamp) -> Result<Vec<Cyclone>, ProviderError> {
    if body.len() > 128 * 1024 {
        return Err(invalid());
    }
    let document: Value = serde_json::from_slice(body).map_err(|_| invalid())?;
    let rows = document["activeStorms"].as_array().filter(|v| v.len() <= 32).ok_or_else(invalid)?;
    let mut seen = HashSet::new();
    rows.iter()
        .map(|raw| {
            let id = raw["id"].as_str().filter(|id| storm_id(id)).ok_or_else(invalid)?;
            if !seen.insert(id) {
                return Err(invalid());
            }
            let basin = &id[..2];
            Ok(Cyclone {
                id: id.to_owned(),
                name: text(&raw["name"], 80)?,
                classification: text(&raw["classification"], 16)?,
                basin: basin.to_ascii_uppercase(),
                position: position(&serde_json::json!([
                    raw["longitudeNumeric"],
                    raw["latitudeNumeric"]
                ]))?,
                position_at: time(&raw["lastUpdate"], now)?,
                advisory_number: advisory(&raw["forecastAdvisory"]["advNum"])?,
                issued_at: time(&raw["forecastAdvisory"]["issuance"], now)?,
                intensity_kt: number(&raw["intensity"], 0.0, 300.0),
                pressure_mb: number(&raw["pressure"], 800.0, 1100.0),
                movement: CycloneMovement {
                    direction_degrees: number(&raw["movementDir"], 0.0, 360.0),
                    speed_kt: number(&raw["movementSpeed"], 0.0, 200.0),
                },
                advisory_url: official_link(&raw["forecastAdvisory"]["url"]),
                outlook_url: format!(
                    "https://www.nhc.noaa.gov/gtwo.php?basin={}&fdays=7",
                    match basin {
                        "al" => "atlc",
                        "cp" => "cpac",
                        _ => "epac",
                    }
                ),
                geometry_status: CycloneGeometryStatus::Pending,
                geometry_advisory_number: None,
                forecast_points: Vec::new(),
                track: None,
                cone: None,
            })
        })
        .collect()
}

#[derive(Debug)]
struct GeometryRow {
    id: String,
    advisory: String,
    geometry: Value,
    tau: Option<f64>,
    wind: Option<f64>,
    gust: Option<f64>,
}

fn coordinate(value: &Value, budget: &mut usize) -> Result<[f64; 2], ProviderError> {
    let p = position(value)?;
    *budget += 1;
    if *budget > 25_000 {
        return Err(invalid());
    }
    Ok([p.lon, p.lat])
}

fn list(value: &Value, low: usize, high: usize) -> Result<&Vec<Value>, ProviderError> {
    value.as_array().filter(|v| (low..=high).contains(&v.len())).ok_or_else(invalid)
}

fn line(value: &Value, ring: bool, budget: &mut usize) -> Result<Vec<[f64; 2]>, ProviderError> {
    let result: Vec<_> = list(value, if ring { 4 } else { 2 }, 10_000)?
        .iter()
        .map(|p| coordinate(p, budget))
        .collect::<Result<_, _>>()?;
    if ring && result.first() != result.last() {
        return Err(invalid());
    }
    Ok(result)
}

fn polygon(value: &Value, budget: &mut usize) -> Result<Vec<Vec<[f64; 2]>>, ProviderError> {
    list(value, 1, 128)?.iter().map(|r| line(r, true, budget)).collect()
}

fn track(value: &Value, budget: &mut usize) -> Result<CycloneTrackGeometry, ProviderError> {
    match value["type"].as_str() {
        Some("LineString") => {
            Ok(CycloneTrackGeometry::LineString(line(&value["coordinates"], false, budget)?))
        }
        Some("MultiLineString") => Ok(CycloneTrackGeometry::MultiLineString(
            list(&value["coordinates"], 1, 128)?
                .iter()
                .map(|l| line(l, false, budget))
                .collect::<Result<_, _>>()?,
        )),
        _ => Err(invalid()),
    }
}

fn cone(value: &Value, budget: &mut usize) -> Result<CycloneConeGeometry, ProviderError> {
    match value["type"].as_str() {
        Some("Polygon") => {
            Ok(CycloneConeGeometry::Polygon(polygon(&value["coordinates"], budget)?))
        }
        Some("MultiPolygon") => Ok(CycloneConeGeometry::MultiPolygon(
            list(&value["coordinates"], 1, 128)?
                .iter()
                .map(|p| polygon(p, budget))
                .collect::<Result<_, _>>()?,
        )),
        _ => Err(invalid()),
    }
}

fn collection(
    body: &[u8],
    index: usize,
    budget: &mut usize,
) -> Result<Vec<GeometryRow>, ProviderError> {
    if body.len() > (if index == 2 { 2 * 1024 * 1024 } else { 512 * 1024 }) {
        return Err(invalid());
    }
    let document: Value = serde_json::from_slice(body).map_err(|_| invalid())?;
    if document["type"] != "FeatureCollection"
        || document.get("exceededTransferLimit").is_some_and(|v| v.as_bool() != Some(false))
    {
        return Err(invalid());
    }
    list(&document["features"], 0, if index == 0 { 500 } else { 32 })?
        .iter()
        .map(|feature| {
            if feature["type"] != "Feature" {
                return Err(invalid());
            }
            let p = &feature["properties"];
            let source = p["idp_source"].as_str().ok_or_else(invalid)?.to_ascii_lowercase();
            let (id, tail) = source.split_once('-').ok_or_else(invalid)?;
            let (adv, kind) = tail.split_once("_5day_").ok_or_else(invalid)?;
            if !storm_id(id) || kind != ["pts", "lin", "pgn"][index] {
                return Err(invalid());
            }
            let advisory = advisory(&p["advisnum"])?;
            if advisory != self::advisory(&Value::String(adv.to_owned()))? {
                return Err(invalid());
            }
            let geometry = &feature["geometry"];
            match index {
                0 => {
                    if geometry["type"] != "Point" {
                        return Err(invalid());
                    }
                    coordinate(&geometry["coordinates"], budget)?;
                }
                1 => {
                    track(geometry, budget)?;
                }
                _ => {
                    cone(geometry, budget)?;
                }
            }
            Ok(GeometryRow {
                id: id.to_owned(),
                advisory,
                geometry: geometry.clone(),
                tau: number(&p["tau"], 0.0, 168.0),
                wind: number(&p["maxwind"], 0.0, 300.0),
                gust: number(&p["gust"], 0.0, 350.0),
            })
        })
        .collect()
}

/// Attaches only geometry for the exact status advisory. Earlier GIS geometry is
/// never relabelled with a newer advisory; malformed collections are rejected whole.
///
/// # Errors
/// Rejects malformed features, mismatched advisory metadata, duplicate forecast leads,
/// open polygon rings and documents exceeding the geometry budget.
pub fn attach_geometry(storms: &mut [Cyclone], bodies: [&[u8]; 3]) -> Result<(), ProviderError> {
    if storms.len() > 32 {
        return Err(invalid());
    }
    let mut budget = 0;
    let points = collection(bodies[0], 0, &mut budget)?;
    let tracks = collection(bodies[1], 1, &mut budget)?;
    let cones = collection(bodies[2], 2, &mut budget)?;
    // Mutate only after the complete projection succeeds.
    let mut projected = storms.to_vec();
    for storm in &mut projected {
        let matches =
            |row: &&GeometryRow| row.id == storm.id && row.advisory == storm.advisory_number;
        let ps: Vec<_> = points.iter().filter(matches).collect();
        let ts: Vec<_> = tracks.iter().filter(matches).collect();
        let cs: Vec<_> = cones.iter().filter(matches).collect();
        if ts.len() > 1 || cs.len() > 1 {
            return Err(invalid());
        }
        if ps.is_empty() || ts.len() != 1 || cs.len() != 1 {
            continue;
        }
        let mut taus = HashSet::new();
        storm.forecast_points = ps
            .into_iter()
            .map(|p| {
                let tau = p.tau.ok_or_else(invalid)?;
                if !taus.insert(if tau == 0.0 { 0 } else { tau.to_bits() }) {
                    return Err(invalid());
                }
                Ok(CycloneForecastPoint {
                    position: position(&p.geometry["coordinates"])?,
                    tau_hours: tau,
                    wind_kt: p.wind,
                    gust_kt: p.gust,
                })
            })
            .collect::<Result<_, _>>()?;
        storm.forecast_points.sort_by(|a, b| a.tau_hours.total_cmp(&b.tau_hours));
        // Coordinates were validated and bounded above; avoid counting them twice.
        let mut unused_budget = 0;
        storm.track = Some(track(&ts[0].geometry, &mut unused_budget)?);
        unused_budget = 0;
        storm.cone = Some(cone(&cs[0].geometry, &mut unused_budget)?);
        storm.geometry_status = CycloneGeometryStatus::Current;
        storm.geometry_advisory_number = Some(storm.advisory_number.clone());
    }
    storms.clone_from_slice(&projected);
    Ok(())
}
