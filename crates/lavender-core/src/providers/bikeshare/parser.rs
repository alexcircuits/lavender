//! GBFS specification versions 2 and 3 parser and normalizer.

use lavender_domain::{BikeshareStation, GeoPoint};
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;

/// Discovered endpoints from `gbfs.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFeeds {
    pub station_information_url: Option<String>,
    pub station_status_url: Option<String>,
    pub ttl: u32,
}

/// Raw parsed station information.
#[derive(Debug, Clone, PartialEq)]
pub struct RawStationInfo {
    pub station_id: String,
    pub name: String,
    pub lon: f64,
    pub lat: f64,
    pub capacity: Option<u32>,
    pub is_installed: bool,
    pub is_renting: bool,
    pub is_returning: bool,
}

/// Raw parsed station status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawStationStatus {
    pub station_id: String,
    pub bikes_available: Option<u32>,
    pub docks_available: Option<u32>,
    pub is_installed: bool,
    pub is_renting: bool,
    pub is_returning: bool,
    pub last_reported: Option<i64>,
}

/// Parse discovery feed `gbfs.json` across GBFS v2 and v3 schemas.
#[must_use]
pub fn parse_gbfs_discovery(json: &Value) -> DiscoveredFeeds {
    let ttl = json
        .get("ttl")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(300)
        .clamp(60, 86400);

    let mut station_information_url = None;
    let mut station_status_url = None;

    // Check data.feeds (v3) or data.<lang>.feeds (v2)
    if let Some(data) = json.get("data") {
        let feeds_slice = if let Some(feeds) = data.get("feeds").and_then(Value::as_array) {
            Some(feeds)
        } else if let Some(obj) = data.as_object() {
            // Find first language entry that has a "feeds" array (e.g. "en")
            obj.values().find_map(|v| v.get("feeds").and_then(Value::as_array))
        } else {
            None
        };

        if let Some(feeds) = feeds_slice {
            for feed in feeds {
                let name = feed.get("name").and_then(Value::as_str).unwrap_or("");
                let url = feed.get("url").and_then(Value::as_str).map(str::trim);
                if let Some(u) = url
                    && !u.is_empty()
                {
                    if name == "station_information" {
                        station_information_url = Some(u.to_string());
                    } else if name == "station_status" {
                        station_status_url = Some(u.to_string());
                    }
                }
            }
        }
    }

    DiscoveredFeeds { station_information_url, station_status_url, ttl }
}

/// Helper to extract stations array from multiple GBFS JSON structures.
fn extract_stations_array(payload: &Value) -> &[Value] {
    if let Some(data) = payload.get("data") {
        if let Some(stations) = data.get("stations").and_then(Value::as_array) {
            return stations.as_slice();
        }
        if let Some(stations) = data.as_array() {
            return stations.as_slice();
        }
        if let Some(obj) = data.as_object() {
            for v in obj.values() {
                if let Some(stations) = v.get("stations").and_then(Value::as_array) {
                    return stations.as_slice();
                }
            }
        }
    }
    &[]
}

/// Tolerant boolean coercion for inconsistent GBFS feeds.
fn parse_gbfs_bool(val: Option<&Value>, fallback: bool) -> bool {
    match val {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_i64().is_some_and(|i| i != 0),
        Some(Value::String(s)) => {
            let s = s.trim().to_ascii_lowercase();
            if s == "true" || s == "1" || s == "yes" {
                true
            } else if s == "false" || s == "0" || s == "no" {
                false
            } else {
                fallback
            }
        }
        _ => fallback,
    }
}

/// Parse non-negative integer.
fn parse_non_negative_u32(val: Option<&Value>) -> Option<u32> {
    match val {
        Some(Value::Number(n)) => n.as_u64().and_then(|u| u32::try_from(u).ok()),
        Some(Value::String(s)) => s.trim().parse::<u32>().ok(),
        _ => None,
    }
}

/// Parse coordinate float.
fn parse_coord_f64(val: Option<&Value>) -> Option<f64> {
    match val {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Parse station name across v2 (string) and v3 (localized array of objects).
fn parse_station_name(val: Option<&Value>) -> String {
    match val {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Array(arr)) => {
            // Find "en" text or take first
            arr.iter()
                .find(|item| {
                    item.get("language")
                        .and_then(Value::as_str)
                        .is_some_and(|l| l.eq_ignore_ascii_case("en"))
                })
                .or_else(|| arr.first())
                .and_then(|item| item.get("text").and_then(Value::as_str))
                .map_or(String::new(), |s| s.trim().to_string())
        }
        _ => String::new(),
    }
}

/// Parse timestamp to epoch seconds.
fn parse_timestamp_epoch_s(val: Option<&Value>) -> Option<i64> {
    match val {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => {
            if let Ok(ts) = s.parse::<i64>() {
                Some(ts)
            } else if let Ok(parsed) = jiff::Timestamp::from_str(s.trim()) {
                Some(parsed.as_second())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Parse `station_information.json` payload into a map of station info.
#[must_use]
pub fn parse_station_information(payload: &Value) -> HashMap<String, RawStationInfo> {
    let stations = extract_stations_array(payload);
    let mut map = HashMap::with_capacity(stations.len());

    for raw in stations {
        let station_id =
            raw.get("station_id").or_else(|| raw.get("id")).and_then(Value::as_str).map_or_else(
                || {
                    raw.get("station_id")
                        .or_else(|| raw.get("id"))
                        .and_then(Value::as_i64)
                        .map(|i| i.to_string())
                        .unwrap_or_default()
                },
                |s| s.trim().to_string(),
            );

        if station_id.is_empty() {
            continue;
        }

        let lat = parse_coord_f64(raw.get("lat").or_else(|| raw.get("latitude")));
        let lon = parse_coord_f64(raw.get("lon").or_else(|| raw.get("longitude")));

        let (Some(lat), Some(lon)) = (lat, lon) else {
            continue;
        };

        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            continue;
        }

        let mut name = parse_station_name(raw.get("name"));
        if name.is_empty() {
            name = parse_station_name(raw.get("short_name"));
        }
        if name.is_empty() {
            name = format!("Station {station_id}");
        }

        let capacity = parse_non_negative_u32(raw.get("capacity"));
        let is_installed = parse_gbfs_bool(raw.get("is_installed"), true);
        let is_renting = parse_gbfs_bool(raw.get("is_renting"), true);
        let is_returning = parse_gbfs_bool(raw.get("is_returning"), true);

        map.insert(
            station_id.clone(),
            RawStationInfo {
                station_id,
                name,
                lon,
                lat,
                capacity,
                is_installed,
                is_renting,
                is_returning,
            },
        );
    }

    map
}

/// Parse `station_status.json` payload into a map of live status.
#[must_use]
pub fn parse_station_status(payload: &Value) -> HashMap<String, RawStationStatus> {
    let stations = extract_stations_array(payload);
    let mut map = HashMap::with_capacity(stations.len());

    for raw in stations {
        let station_id =
            raw.get("station_id").or_else(|| raw.get("id")).and_then(Value::as_str).map_or_else(
                || {
                    raw.get("station_id")
                        .or_else(|| raw.get("id"))
                        .and_then(Value::as_i64)
                        .map(|i| i.to_string())
                        .unwrap_or_default()
                },
                |s| s.trim().to_string(),
            );

        if station_id.is_empty() {
            continue;
        }

        let bikes_available =
            parse_non_negative_u32(raw.get("num_bikes_available")).or_else(|| {
                // Some v3 feeds break down by vehicle_types_available
                raw.get("vehicle_types_available").and_then(Value::as_array).map(|arr| {
                    arr.iter().filter_map(|v| parse_non_negative_u32(v.get("count"))).sum()
                })
            });

        let docks_available = parse_non_negative_u32(raw.get("num_docks_available"));
        let is_installed = parse_gbfs_bool(raw.get("is_installed"), true);
        let is_renting = parse_gbfs_bool(raw.get("is_renting"), true);
        let is_returning = parse_gbfs_bool(raw.get("is_returning"), true);
        let last_reported = parse_timestamp_epoch_s(raw.get("last_reported"));

        map.insert(
            station_id.clone(),
            RawStationStatus {
                station_id,
                bikes_available,
                docks_available,
                is_installed,
                is_renting,
                is_returning,
                last_reported,
            },
        );
    }

    map
}

/// Merge static station information and dynamic status into unified `BikeshareStation` entities.
#[must_use]
pub fn merge_stations<S1: std::hash::BuildHasher, S2: std::hash::BuildHasher>(
    system_id: &str,
    info_map: &HashMap<String, RawStationInfo, S1>,
    status_map: &HashMap<String, RawStationStatus, S2>,
) -> Vec<BikeshareStation> {
    let mut stations = Vec::with_capacity(info_map.len());

    for info in info_map.values() {
        let status = status_map.get(&info.station_id);

        let bikes_available = status.and_then(|s| s.bikes_available);
        let docks_available = status.and_then(|s| s.docks_available);

        // Derive capacity if omitted in station_information
        let capacity = info.capacity.or_else(|| match (bikes_available, docks_available) {
            (Some(b), Some(d)) => Some(b + d),
            _ => None,
        });

        let is_installed = status.map_or(info.is_installed, |s| s.is_installed);
        let is_renting = status.map_or(info.is_renting, |s| s.is_renting);
        let is_returning = status.map_or(info.is_returning, |s| s.is_returning);
        let last_reported = status.and_then(|s| s.last_reported);

        let Ok(position) = GeoPoint::new(info.lat, info.lon) else {
            continue;
        };

        stations.push(BikeshareStation {
            id: format!("{system_id}:{}", info.station_id),
            station_id: info.station_id.clone(),
            system_id: system_id.to_string(),
            name: info.name.clone(),
            position,
            capacity,
            bikes_available,
            docks_available,
            is_installed,
            is_renting,
            is_returning,
            last_reported,
        });
    }

    stations.sort_by(|a, b| a.id.cmp(&b.id));
    stations
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_v2_discovery_and_stations() {
        let gbfs_v2 = json!({
            "last_updated": 1_600_000_000,
            "ttl": 120,
            "version": "2.3",
            "data": {
                "en": {
                    "feeds": [
                        { "name": "station_information", "url": "https://example.com/gbfs/2.3/station_information.json" },
                        { "name": "station_status", "url": "https://example.com/gbfs/2.3/station_status.json" }
                    ]
                }
            }
        });

        let discovery = parse_gbfs_discovery(&gbfs_v2);
        assert_eq!(discovery.ttl, 120);
        assert_eq!(
            discovery.station_information_url.as_deref(),
            Some("https://example.com/gbfs/2.3/station_information.json")
        );
        assert_eq!(
            discovery.station_status_url.as_deref(),
            Some("https://example.com/gbfs/2.3/station_status.json")
        );

        let info_v2 = json!({
            "data": {
                "stations": [
                    {
                        "station_id": "st-1",
                        "name": "Central Park S",
                        "lat": 40.765,
                        "lon": -73.976,
                        "capacity": 30,
                        "is_installed": 1,
                        "is_renting": 1,
                        "is_returning": 1
                    }
                ]
            }
        });

        let status_v2 = json!({
            "data": {
                "stations": [
                    {
                        "station_id": "st-1",
                        "num_bikes_available": 14,
                        "num_docks_available": 16,
                        "is_installed": 1,
                        "is_renting": 1,
                        "is_returning": 1,
                        "last_reported": 1_600_000_050
                    }
                ]
            }
        });

        let info_map = parse_station_information(&info_v2);
        let status_map = parse_station_status(&status_v2);
        assert_eq!(info_map.len(), 1);
        assert_eq!(status_map.len(), 1);

        let merged = merge_stations("nyc-citibike", &info_map, &status_map);
        assert_eq!(merged.len(), 1);
        let s = &merged[0];
        assert_eq!(s.id, "nyc-citibike:st-1");
        assert_eq!(s.name, "Central Park S");
        assert_eq!(s.bikes_available, Some(14));
        assert_eq!(s.docks_available, Some(16));
        assert_eq!(s.capacity, Some(30));
        assert!(s.is_installed);
        assert!(s.is_renting);
        assert!(s.is_returning);
        assert_eq!(s.last_reported, Some(1_600_000_050));
    }

    #[test]
    fn parse_v3_discovery_and_localized_stations() {
        let gbfs_v3 = json!({
            "last_updated": "2024-05-01T12:00:00Z",
            "ttl": 60,
            "version": "3.0",
            "data": {
                "feeds": [
                    { "name": "station_information", "url": "https://example.com/gbfs/3.0/station_information.json" },
                    { "name": "station_status", "url": "https://example.com/gbfs/3.0/station_status.json" }
                ]
            }
        });

        let discovery = parse_gbfs_discovery(&gbfs_v3);
        assert_eq!(discovery.ttl, 60);
        assert_eq!(
            discovery.station_information_url.as_deref(),
            Some("https://example.com/gbfs/3.0/station_information.json")
        );

        let info_v3 = json!({
            "data": {
                "stations": [
                    {
                        "station_id": "st-v3-100",
                        "name": [
                            { "text": "Place de la Concorde", "language": "fr" },
                            { "text": "Concorde Square", "language": "en" }
                        ],
                        "lat": 48.8656,
                        "lon": 2.3212,
                        "capacity": 45,
                        "is_installed": true,
                        "is_renting": true,
                        "is_returning": true
                    }
                ]
            }
        });

        let status_v3 = json!({
            "data": {
                "stations": [
                    {
                        "station_id": "st-v3-100",
                        "num_bikes_available": 20,
                        "num_docks_available": 25,
                        "is_installed": true,
                        "is_renting": true,
                        "is_returning": true,
                        "last_reported": "2024-05-01T12:00:10Z"
                    }
                ]
            }
        });

        let info_map = parse_station_information(&info_v3);
        let status_map = parse_station_status(&status_v3);
        let merged = merge_stations("velib-paris", &info_map, &status_map);
        assert_eq!(merged.len(), 1);
        let s = &merged[0];
        assert_eq!(s.id, "velib-paris:st-v3-100");
        assert_eq!(s.name, "Concorde Square");
        assert_eq!(s.bikes_available, Some(20));
        assert_eq!(s.docks_available, Some(25));
        assert!(s.last_reported.is_some());
    }
}
