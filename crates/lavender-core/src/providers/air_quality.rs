//! Environment and Climate Change Canada (ECCC) AQHI air quality provider.
//!
//! Fetches Canadian AQHI stations (`aqhi-stations`) and realtime observations
//! (`aqhi-observations-realtime`) from MSC `GeoMet` OGC API.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jiff::Timestamp;
use lavender_domain::{AirQualityBand, AirQualityReading, AirQualitySnapshot, Freshness};
use tokio::sync::RwLock;
use url::Url;

use super::ProviderError;
use crate::fetch::{FetchOptions, Fetcher, HostPolicy};

/// ECCC MSC `GeoMet` collection items URL for AQHI station metadata.
pub const STATIONS_URL: &str =
    "https://api.weather.gc.ca/collections/aqhi-stations/items?f=json&limit=500";

/// ECCC MSC `GeoMet` collection items URL for latest AQHI realtime observations.
pub const OBSERVATIONS_URL: &str = "https://api.weather.gc.ca/collections/aqhi-observations-realtime/items?f=json&latest=true&limit=500";

/// Station metadata cache time-to-live (12 hours).
pub const STATION_TTL: Duration = Duration::from_hours(12);

/// Observations older than 6 hours are considered too old and dropped.
pub const OBSERVATION_MAX_AGE: Duration = Duration::from_hours(6);

/// Observations older than 2 hours render the snapshot stale.
pub const STALE_AFTER: Duration = Duration::from_hours(2);

/// Maximum accepted response size for ECCC GeoJSON payloads (4 MiB).
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// Cached station information parsed from the station catalog.
#[derive(Debug, Clone, PartialEq)]
pub struct StationInfo {
    pub id: String,
    pub name: String,
    pub zone: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
}

/// Convert a raw AQHI value from the upstream API into an integer scale (1..).
#[must_use]
pub fn aqhi_display_value(raw: &serde_json::Value) -> Option<u32> {
    let num = match raw {
        serde_json::Value::Number(n) => n.as_f64()?,
        serde_json::Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return None;
            }
            trimmed.parse::<f64>().ok()?
        }
        _ => return None,
    };

    if !num.is_finite() || num < 0.0 {
        return None;
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let rounded = num.round().max(1.0) as u32;
    Some(rounded)
}

/// Parse and normalize station features into an id-keyed catalog.
///
/// Discards individual features with missing or invalid coordinates or IDs without
/// dropping the rest of the dataset.
///
/// # Errors
/// Returns [`ProviderError::Parse`] if the top-level payload is structurally invalid.
pub fn normalize_aqhi_stations(
    json: &serde_json::Value,
) -> Result<HashMap<String, StationInfo>, ProviderError> {
    let features = json
        .get("features")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| ProviderError::parse("eccc-aqhi", &"missing or invalid features array"))?;

    let mut stations = HashMap::new();

    for feature in features {
        let Some(properties) = feature.get("properties").and_then(serde_json::Value::as_object)
        else {
            continue;
        };

        let Some(coordinates) = feature
            .get("geometry")
            .and_then(|g| g.get("coordinates"))
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };

        if coordinates.len() < 2 {
            continue;
        }

        let (Some(lon), Some(lat)) = (coordinates[0].as_f64(), coordinates[1].as_f64()) else {
            continue;
        };

        if !lon.is_finite() || !lat.is_finite() || lat.abs() > 90.0 || lon.abs() > 180.0 {
            continue;
        }

        let id = properties
            .get("location_id")
            .and_then(serde_json::Value::as_str)
            .or_else(|| feature.get("id").and_then(serde_json::Value::as_str))
            .map(str::trim)
            .unwrap_or_default();

        if id.is_empty() {
            continue;
        }

        let name = properties
            .get("location_name_en")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(id)
            .to_owned();

        let zone = properties
            .get("eccc_administrative-zone")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);

        stations.insert(
            id.to_owned(),
            StationInfo { id: id.to_owned(), name, zone, latitude: lat, longitude: lon },
        );
    }

    Ok(stations)
}

/// Normalizes observations and joins station coordinates into domain reading rows.
///
/// Observations older than [`OBSERVATION_MAX_AGE`] or without a matching station are dropped.
///
/// # Errors
/// Returns [`ProviderError::Parse`] if the top-level payload is structurally invalid.
pub fn normalize_aqhi_observations<S: ::std::hash::BuildHasher>(
    json: &serde_json::Value,
    stations: &HashMap<String, StationInfo, S>,
    now: Timestamp,
) -> Result<Vec<AirQualityReading>, ProviderError> {
    let features = json
        .get("features")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| ProviderError::parse("eccc-aqhi", &"missing or invalid features array"))?;

    let mut newest: HashMap<String, AirQualityReading> = HashMap::new();

    for feature in features {
        let Some(properties) = feature.get("properties").and_then(serde_json::Value::as_object)
        else {
            continue;
        };

        let station_id = properties
            .get("location_id")
            .and_then(serde_json::Value::as_str)
            .or_else(|| feature.get("id").and_then(serde_json::Value::as_str))
            .map(str::trim)
            .unwrap_or_default();

        if station_id.is_empty() {
            continue;
        }

        let Some(station) = stations.get(station_id) else {
            continue;
        };

        let Some(val_node) = properties.get("aqhi") else {
            continue;
        };
        let Some(value) = aqhi_display_value(val_node) else {
            continue;
        };

        let Some(dt_str) =
            properties.get("observation_datetime").and_then(serde_json::Value::as_str)
        else {
            continue;
        };

        let Ok(observed_at) = dt_str.trim().parse::<Timestamp>() else {
            continue;
        };

        let age_secs = now.duration_since(observed_at).as_secs();
        #[allow(clippy::cast_possible_wrap)]
        if age_secs > OBSERVATION_MAX_AGE.as_secs() as i64 {
            continue;
        }

        if let Some(prev) = newest.get(station_id)
            && prev.observed_at >= observed_at
        {
            continue;
        }

        let band = AirQualityBand::from_aqhi(value);
        let label = if value > 10 { "10+".to_owned() } else { value.to_string() };

        let note = properties
            .get("special_notes_en")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);

        let reading = AirQualityReading {
            id: format!("eccc:{station_id}"),
            provider: "eccc".to_owned(),
            scale: "AQHI".to_owned(),
            value,
            band,
            station_id: station_id.to_owned(),
            name: station.name.clone(),
            zone: station.zone.clone(),
            latitude: station.latitude,
            longitude: station.longitude,
            observed_at,
            color: band.color().to_owned(),
            label,
            risk: band.risk().to_owned(),
            note,
        };

        newest.insert(station_id.to_owned(), reading);
    }

    let mut readings: Vec<AirQualityReading> = newest.into_values().collect();
    readings.sort_by(|a, b| b.value.cmp(&a.value).then_with(|| a.station_id.cmp(&b.station_id)));

    Ok(readings)
}

/// Provider client for ECCC AQHI realtime stations and observations.
#[derive(Debug)]
pub struct EcccAirQualityProvider {
    fetcher: Arc<Fetcher>,
    stations: RwLock<HashMap<String, StationInfo>>,
    last_stations_fetch: RwLock<Option<Instant>>,
    host_policy: HostPolicy,
    stations_url: Url,
    observations_url: Url,
}

impl EcccAirQualityProvider {
    /// Builds a new provider with default upstream ECCC URLs.
    ///
    /// # Panics
    /// Panics if the hardcoded constant URLs are invalid.
    #[must_use]
    #[allow(clippy::expect_used, clippy::missing_panics_doc)]
    pub fn new(fetcher: Arc<Fetcher>) -> Self {
        let stations_url = Url::parse(STATIONS_URL).expect("valid STATIONS_URL");
        let observations_url = Url::parse(OBSERVATIONS_URL).expect("valid OBSERVATIONS_URL");
        Self::with_urls(fetcher, stations_url, observations_url)
    }

    /// Builds a new provider with custom URLs (for testing or mirroring).
    #[must_use]
    pub fn with_urls(fetcher: Arc<Fetcher>, stations_url: Url, observations_url: Url) -> Self {
        let hosts: Vec<&str> =
            [stations_url.host_str(), observations_url.host_str()].into_iter().flatten().collect();
        let host_policy = HostPolicy::new(&hosts);
        Self {
            fetcher,
            stations: RwLock::new(HashMap::new()),
            last_stations_fetch: RwLock::new(None),
            host_policy,
            stations_url,
            observations_url,
        }
    }

    /// Refreshes the station catalog if expired or empty.
    async fn refresh_stations(&self) -> Result<(), ProviderError> {
        let need_fetch = {
            let stations = self.stations.read().await;
            let last = self.last_stations_fetch.read().await;
            stations.is_empty() || last.is_none_or(|t| t.elapsed() > STATION_TTL)
        };

        if !need_fetch {
            return Ok(());
        }

        let opts = FetchOptions {
            max_body_bytes: Some(MAX_RESPONSE_BYTES),
            accept: "application/json",
            ..Default::default()
        };

        match self.fetcher.get(&self.stations_url, &self.host_policy, &opts).await {
            Ok(resp) => {
                let json: serde_json::Value = serde_json::from_slice(&resp.body)
                    .map_err(|e| ProviderError::parse("eccc-aqhi", &e))?;
                let new_stations = normalize_aqhi_stations(&json)?;
                if !new_stations.is_empty() {
                    let mut stations = self.stations.write().await;
                    let mut last = self.last_stations_fetch.write().await;
                    *stations = new_stations;
                    *last = Some(Instant::now());
                }
                Ok(())
            }
            Err(err) => {
                let stations = self.stations.read().await;
                if stations.is_empty() {
                    Err(ProviderError::from(err))
                } else {
                    tracing::warn!(error = %err, "Failed to refresh AQHI stations; using cached catalog");
                    Ok(())
                }
            }
        }
    }

    /// Fetches the latest air quality readings snapshot evaluated at current time.
    ///
    /// # Errors
    /// Returns [`ProviderError`] on network failure, parse error, or when no stations are available.
    pub async fn get_snapshot(&self) -> Result<AirQualitySnapshot, ProviderError> {
        self.get_snapshot_at(Timestamp::now()).await
    }

    /// Fetches the latest air quality readings snapshot evaluated at `now`.
    ///
    /// # Errors
    /// Returns [`ProviderError`] on network failure, parse error, or when no stations are available.
    pub async fn get_snapshot_at(
        &self,
        now: Timestamp,
    ) -> Result<AirQualitySnapshot, ProviderError> {
        self.refresh_stations().await?;

        let stations = self.stations.read().await.clone();
        if stations.is_empty() {
            return Err(ProviderError::Unavailable("No AQHI stations available".to_owned()));
        }

        let opts = FetchOptions {
            max_body_bytes: Some(MAX_RESPONSE_BYTES),
            accept: "application/json",
            ..Default::default()
        };

        let resp = self.fetcher.get(&self.observations_url, &self.host_policy, &opts).await?;
        let json: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| ProviderError::parse("eccc-aqhi", &e))?;

        let readings = normalize_aqhi_observations(&json, &stations, now)?;

        let newest_ts = readings.iter().map(|r| r.observed_at).max();
        let freshness = match newest_ts {
            Some(ts) => {
                let age_s =
                    u32::try_from(now.duration_since(ts).as_secs().max(0)).unwrap_or(u32::MAX);
                let stale_limit = u32::try_from(STALE_AFTER.as_secs()).unwrap_or(u32::MAX);
                if age_s > stale_limit { Freshness::Stale { age_s } } else { Freshness::Live }
            }
            None => Freshness::Stale { age_s: 0 },
        };

        Ok(AirQualitySnapshot { readings, freshness, updated_at: now })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_aqhi_display_values() {
        assert_eq!(aqhi_display_value(&json!(null)), None);
        assert_eq!(aqhi_display_value(&json!("")), None);
        assert_eq!(aqhi_display_value(&json!("   ")), None);
        assert_eq!(aqhi_display_value(&json!(-1)), None);
        assert_eq!(aqhi_display_value(&json!("invalid")), None);

        // Floored to 1
        assert_eq!(aqhi_display_value(&json!(0.2)), Some(1));
        assert_eq!(aqhi_display_value(&json!(0.0)), Some(1));
        assert_eq!(aqhi_display_value(&json!(0.9)), Some(1));
        assert_eq!(aqhi_display_value(&json!("0.4")), Some(1));

        // Rounded values
        assert_eq!(aqhi_display_value(&json!(2.4)), Some(2));
        assert_eq!(aqhi_display_value(&json!(2.5)), Some(3));
        assert_eq!(aqhi_display_value(&json!("7.8")), Some(8));
        assert_eq!(aqhi_display_value(&json!(11.2)), Some(11));
    }

    #[test]
    fn normalizes_valid_stations_and_discards_invalid() {
        let payload = json!({
            "type": "FeatureCollection",
            "features": [
                {
                    "type": "Feature",
                    "id": "60104",
                    "geometry": {
                        "type": "Point",
                        "coordinates": [-79.38, 43.65]
                    },
                    "properties": {
                        "location_id": "60104",
                        "location_name_en": "Toronto Downtown",
                        "eccc_administrative-zone": "ont"
                    }
                },
                {
                    "type": "Feature",
                    "geometry": {
                        "type": "Point",
                        "coordinates": [-123.12, 49.28]
                    },
                    "properties": {
                        "location_id": "80100",
                        "location_name_en": "Vancouver Kitsilano",
                        "eccc_administrative-zone": "pyr"
                    }
                },
                // Invalid: out-of-range coordinates
                {
                    "type": "Feature",
                    "geometry": {
                        "type": "Point",
                        "coordinates": [-200.0, 95.0]
                    },
                    "properties": {
                        "location_id": "INVALID_GEO"
                    }
                },
                // Invalid: missing coordinates
                {
                    "type": "Feature",
                    "geometry": {
                        "type": "Point",
                        "coordinates": []
                    },
                    "properties": {
                        "location_id": "NO_COORDS"
                    }
                },
                // Invalid: missing ID
                {
                    "type": "Feature",
                    "geometry": {
                        "type": "Point",
                        "coordinates": [-75.0, 45.0]
                    },
                    "properties": {}
                }
            ]
        });

        let stations = normalize_aqhi_stations(&payload).unwrap();
        assert_eq!(stations.len(), 2);

        let toronto = stations.get("60104").unwrap();
        assert_eq!(toronto.name, "Toronto Downtown");
        assert_eq!(toronto.zone, Some("ont".to_owned()));
        assert_eq!(toronto.latitude, 43.65);
        assert_eq!(toronto.longitude, -79.38);

        let vancouver = stations.get("80100").unwrap();
        assert_eq!(vancouver.name, "Vancouver Kitsilano");
        assert_eq!(vancouver.zone, Some("pyr".to_owned()));
    }

    #[test]
    fn normalizes_observations_drops_old_and_deduplicates() {
        let now: Timestamp = "2026-10-25T18:00:00Z".parse().unwrap();

        let mut stations = HashMap::new();
        stations.insert(
            "60104".to_owned(),
            StationInfo {
                id: "60104".to_owned(),
                name: "Toronto Downtown".to_owned(),
                zone: Some("ont".to_owned()),
                latitude: 43.65,
                longitude: -79.38,
            },
        );
        stations.insert(
            "80100".to_owned(),
            StationInfo {
                id: "80100".to_owned(),
                name: "Vancouver Kitsilano".to_owned(),
                zone: Some("pyr".to_owned()),
                latitude: 49.28,
                longitude: -123.12,
            },
        );

        let payload = json!({
            "type": "FeatureCollection",
            "features": [
                // Valid observation 1h old
                {
                    "type": "Feature",
                    "properties": {
                        "location_id": "60104",
                        "aqhi": 3.2,
                        "observation_datetime": "2026-10-25T17:00:00Z",
                        "special_notes_en": null
                    }
                },
                // Newer observation for 60104 (30m old) - should supersede
                {
                    "type": "Feature",
                    "properties": {
                        "location_id": "60104",
                        "aqhi": 4.1,
                        "observation_datetime": "2026-10-25T17:30:00Z",
                        "special_notes_en": null
                    }
                },
                // High AQHI observation (> 10)
                {
                    "type": "Feature",
                    "properties": {
                        "location_id": "80100",
                        "aqhi": 11.4,
                        "observation_datetime": "2026-10-25T17:00:00Z",
                        "special_notes_en": "Wildfire smoke advisory"
                    }
                },
                // Dropped: older than 6 hours
                {
                    "type": "Feature",
                    "properties": {
                        "location_id": "60104",
                        "aqhi": 2.0,
                        "observation_datetime": "2026-10-25T11:00:00Z"
                    }
                },
                // Dropped: unknown station
                {
                    "type": "Feature",
                    "properties": {
                        "location_id": "UNKNOWN_STN",
                        "aqhi": 3.0,
                        "observation_datetime": "2026-10-25T17:00:00Z"
                    }
                }
            ]
        });

        let readings = normalize_aqhi_observations(&payload, &stations, now).unwrap();
        assert_eq!(readings.len(), 2);

        // Sorted by value descending: 80100 (value 11) then 60104 (value 4)
        assert_eq!(readings[0].station_id, "80100");
        assert_eq!(readings[0].value, 11);
        assert_eq!(readings[0].band, AirQualityBand::VeryHigh);
        assert_eq!(readings[0].label, "10+");
        assert_eq!(readings[0].risk, "Very High health risk");
        assert_eq!(readings[0].color, "#c1121f");
        assert_eq!(readings[0].note, Some("Wildfire smoke advisory".to_owned()));

        assert_eq!(readings[1].station_id, "60104");
        assert_eq!(readings[1].value, 4);
        assert_eq!(readings[1].band, AirQualityBand::Moderate);
        assert_eq!(readings[1].label, "4");
        assert_eq!(readings[1].risk, "Moderate health risk");
        assert_eq!(readings[1].color, "#ffd43b");
        assert_eq!(readings[1].note, None);
    }

    #[test]
    fn evaluates_freshness_correctly() {
        let now: Timestamp = "2026-10-25T18:00:00Z".parse().unwrap();

        let make_reading = |dt: &str| AirQualityReading {
            id: "eccc:1".to_owned(),
            provider: "eccc".to_owned(),
            scale: "AQHI".to_owned(),
            value: 2,
            band: AirQualityBand::Low,
            station_id: "1".to_owned(),
            name: "Test".to_owned(),
            zone: None,
            latitude: 45.0,
            longitude: -75.0,
            observed_at: dt.parse().unwrap(),
            color: "#4cc9f0".to_owned(),
            label: "2".to_owned(),
            risk: "Low health risk".to_owned(),
            note: None,
        };

        // Live: observation 1 hour old (< 2 hours)
        let readings_live = [make_reading("2026-10-25T17:00:00Z")];
        let newest_ts = readings_live.iter().map(|r| r.observed_at).max().unwrap();
        let age_s = u32::try_from(now.duration_since(newest_ts).as_secs()).unwrap();
        let stale_limit = u32::try_from(STALE_AFTER.as_secs()).unwrap();
        assert!(age_s <= stale_limit);

        // Stale: observation 3 hours old (> 2 hours)
        let readings_stale = [make_reading("2026-10-25T15:00:00Z")];
        let newest_ts = readings_stale.iter().map(|r| r.observed_at).max().unwrap();
        let age_s = u32::try_from(now.duration_since(newest_ts).as_secs()).unwrap();
        assert!(age_s > stale_limit);
    }
}
