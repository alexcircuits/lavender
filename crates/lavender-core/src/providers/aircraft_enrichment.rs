//! Aircraft metadata and flight route enrichment via adsbdb.com.
//!
//! Provides cached lookups for callsign routes (origin/destination airports and airline)
//! and transponder hex details (ICAO aircraft type designator, type name, registration).
//! Employs bounded in-memory caching with negative caching for 404s.

use std::sync::Arc;
use std::time::Duration;

use lavender_domain::{
    AircraftRouteEnrichment, AircraftTypeEnrichment, AirportInfo, FeedId, LicenceMode,
};
use moka::future::Cache;
use serde::Deserialize;

use crate::config::AdsbdbConfig;
use crate::error::CoreError;
use crate::fetch::{FetchError, FetchOptions, Fetcher, HostPolicy};
use crate::sources;

const MAX_BODY: usize = 1024 * 1024; // 1 MB cap for metadata JSON

#[derive(Debug)]
pub struct AircraftEnrichmentService {
    config: AdsbdbConfig,
    fetcher: Arc<Fetcher>,
    policy: HostPolicy,
    licence_mode: LicenceMode,
    licence_acknowledged: Vec<String>,
    routes: Cache<String, Option<AircraftRouteEnrichment>>,
    aircraft: Cache<String, Option<AircraftTypeEnrichment>>,
}

#[derive(Debug, Deserialize)]
struct AdsbdbAirport {
    #[serde(default)]
    iata_code: Option<String>,
    #[serde(default)]
    icao_code: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    municipality: Option<String>,
    #[serde(default)]
    latitude: Option<f64>,
    #[serde(default)]
    longitude: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct AdsbdbAirline {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AdsbdbFlightroute {
    #[serde(default)]
    airline: Option<AdsbdbAirline>,
    #[serde(default)]
    origin: Option<AdsbdbAirport>,
    #[serde(default)]
    destination: Option<AdsbdbAirport>,
}

#[derive(Debug, Deserialize)]
struct AdsbdbRouteResponse {
    #[serde(default)]
    response: Option<AdsbdbRouteContainer>,
}

#[derive(Debug, Deserialize)]
struct AdsbdbRouteContainer {
    #[serde(default)]
    flightroute: Option<AdsbdbFlightroute>,
}

#[derive(Debug, Deserialize)]
struct AdsbdbAircraftDetails {
    #[serde(default)]
    icao_type: Option<String>,
    #[serde(default)]
    manufacturer: Option<String>,
    #[serde(default)]
    #[serde(rename = "type")]
    type_name: Option<String>,
    #[serde(default)]
    registration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AdsbdbAircraftResponse {
    #[serde(default)]
    response: Option<AdsbdbAircraftContainer>,
}

#[derive(Debug, Deserialize)]
struct AdsbdbAircraftContainer {
    #[serde(default)]
    aircraft: Option<AdsbdbAircraftDetails>,
}

impl AircraftEnrichmentService {
    #[must_use]
    pub fn new(
        config: AdsbdbConfig,
        fetcher: Arc<Fetcher>,
        licence_mode: LicenceMode,
        licence_acknowledged: Vec<String>,
    ) -> Self {
        let policy = HostPolicy::for_url(&config.base_url)
            .with_min_interval(Duration::from_millis(config.min_request_interval_ms));
        let routes = Cache::builder()
            .max_capacity(config.cache_max_entries)
            .time_to_live(Duration::from_secs(config.cache_ttl_s))
            .build();
        let aircraft = Cache::builder()
            .max_capacity(config.cache_max_entries)
            .time_to_live(Duration::from_secs(config.cache_ttl_s))
            .build();
        Self { config, fetcher, policy, licence_mode, licence_acknowledged, routes, aircraft }
    }

    fn check_licence(&self) -> Result<(), CoreError> {
        if self.licence_mode == LicenceMode::Commercial
            && !self.licence_acknowledged.iter().any(|s| s == sources::ADS_B_DB)
        {
            return Err(CoreError::DisabledByLicence { feed: FeedId::Aircraft });
        }
        Ok(())
    }

    /// Looks up airline and origin/destination airports by flight callsign.
    ///
    /// # Errors
    /// Returns `CoreError::BadRequest` if callsign is invalid,
    /// or `CoreError::DisabledByLicence` if commercial mode is not acknowledged.
    pub async fn lookup_route(&self, callsign: &str) -> Result<AircraftRouteEnrichment, CoreError> {
        self.check_licence()?;
        let cs = callsign.trim().to_ascii_uppercase();
        if cs.len() < 2 || cs.len() > 8 || !cs.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(CoreError::BadRequest(
                "Callsign must be 2 to 8 alphanumeric characters".to_owned(),
            ));
        }

        if let Some(cached) = self.routes.get(&cs).await {
            return Ok(cached.unwrap_or(AircraftRouteEnrichment {
                callsign: cs,
                found: false,
                airline: None,
                origin: None,
                destination: None,
            }));
        }

        let url = match self.config.base_url.join(&format!("callsign/{cs}")) {
            Ok(u) => u,
            Err(e) => return Err(CoreError::BadRequest(format!("Invalid adsbdb URL: {e}"))),
        };

        let opts = FetchOptions { max_body_bytes: Some(MAX_BODY), ..FetchOptions::default() };

        let result = match self.fetcher.get(&url, &self.policy, &opts).await {
            Ok(resp) => {
                let parsed: AdsbdbRouteResponse = serde_json::from_slice(&resp.body)
                    .map_err(|e| CoreError::Upstream(format!("adsbdb route parse error: {e}")))?;
                if let Some(fr) = parsed.response.and_then(|r| r.flightroute) {
                    if let (Some(orig), Some(dest)) = (fr.origin, fr.destination) {
                        let to_airport = |a: AdsbdbAirport| AirportInfo {
                            code: a.iata_code.or(a.icao_code).unwrap_or_default(),
                            name: a.municipality.or(a.name).unwrap_or_default(),
                            lat: a.latitude.filter(|v| v.is_finite()),
                            lon: a.longitude.filter(|v| v.is_finite()),
                        };
                        Some(AircraftRouteEnrichment {
                            callsign: cs.clone(),
                            found: true,
                            airline: fr.airline.and_then(|a| a.name),
                            origin: Some(to_airport(orig)),
                            destination: Some(to_airport(dest)),
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Err(FetchError::Status { status: 404, .. }) => None,
            Err(e) => return Err(CoreError::Upstream(format!("adsbdb fetch failed: {e}"))),
        };

        self.routes.insert(cs.clone(), result.clone()).await;

        Ok(result.unwrap_or(AircraftRouteEnrichment {
            callsign: cs,
            found: false,
            airline: None,
            origin: None,
            destination: None,
        }))
    }

    /// Looks up ICAO type designator, type name and registration by hex.
    ///
    /// # Errors
    /// Returns `CoreError::BadRequest` if hex is invalid,
    /// or `CoreError::DisabledByLicence` if commercial mode is not acknowledged.
    pub async fn lookup_type(&self, hex: &str) -> Result<AircraftTypeEnrichment, CoreError> {
        self.check_licence()?;
        let hex_norm = hex.trim().to_ascii_lowercase();
        let body = hex_norm.strip_prefix('~').unwrap_or(&hex_norm);
        if body.len() != 6 || !body.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(CoreError::BadRequest(
                "Aircraft hex must be 6 hexadecimal digits".to_owned(),
            ));
        }

        if let Some(cached) = self.aircraft.get(body).await {
            return Ok(cached.unwrap_or_else(|| AircraftTypeEnrichment {
                hex: body.to_owned(),
                found: false,
                type_code: None,
                type_name: None,
                registration: None,
            }));
        }

        let url = match self.config.base_url.join(&format!("aircraft/{body}")) {
            Ok(u) => u,
            Err(e) => return Err(CoreError::BadRequest(format!("Invalid adsbdb URL: {e}"))),
        };

        let opts = FetchOptions { max_body_bytes: Some(MAX_BODY), ..FetchOptions::default() };

        let result = match self.fetcher.get(&url, &self.policy, &opts).await {
            Ok(resp) => {
                let parsed: AdsbdbAircraftResponse =
                    serde_json::from_slice(&resp.body).map_err(|e| {
                        CoreError::Upstream(format!("adsbdb aircraft parse error: {e}"))
                    })?;
                if let Some(ac) = parsed.response.and_then(|r| r.aircraft) {
                    let type_name = match (ac.manufacturer, ac.type_name) {
                        (Some(m), Some(t)) if !m.is_empty() && !t.is_empty() => {
                            Some(format!("{m} {t}"))
                        }
                        (_, Some(t)) if !t.is_empty() => Some(t),
                        _ => None,
                    };
                    Some(AircraftTypeEnrichment {
                        hex: body.to_owned(),
                        found: true,
                        type_code: ac.icao_type,
                        type_name,
                        registration: ac.registration,
                    })
                } else {
                    None
                }
            }
            Err(FetchError::Status { status: 404, .. }) => None,
            Err(e) => return Err(CoreError::Upstream(format!("adsbdb fetch failed: {e}"))),
        };

        self.aircraft.insert(body.to_owned(), result.clone()).await;

        Ok(result.unwrap_or_else(|| AircraftTypeEnrichment {
            hex: body.to_owned(),
            found: false,
            type_code: None,
            type_name: None,
            registration: None,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_adsbdb_route_fixture() {
        let raw = br#"{
            "response": {
                "flightroute": {
                    "callsign": "BAW123",
                    "airline": { "name": "British Airways" },
                    "origin": {
                        "iata_code": "LHR",
                        "municipality": "London",
                        "latitude": 51.47,
                        "longitude": -0.454
                    },
                    "destination": {
                        "iata_code": "JFK",
                        "municipality": "New York",
                        "latitude": 40.64,
                        "longitude": -73.78
                    }
                }
            }
        }"#;

        let parsed: AdsbdbRouteResponse = serde_json::from_slice(raw).unwrap();
        let fr = parsed.response.unwrap().flightroute.unwrap();
        assert_eq!(fr.airline.unwrap().name.as_deref(), Some("British Airways"));
        assert_eq!(fr.origin.unwrap().iata_code.as_deref(), Some("LHR"));
        assert_eq!(fr.destination.unwrap().iata_code.as_deref(), Some("JFK"));
    }

    #[test]
    fn parse_adsbdb_aircraft_fixture() {
        let raw = br#"{
            "response": {
                "aircraft": {
                    "icao_type": "B738",
                    "manufacturer": "Boeing",
                    "type": "737-800",
                    "registration": "EI-ABC"
                }
            }
        }"#;

        let parsed: AdsbdbAircraftResponse = serde_json::from_slice(raw).unwrap();
        let ac = parsed.response.unwrap().aircraft.unwrap();
        assert_eq!(ac.icao_type.as_deref(), Some("B738"));
        assert_eq!(ac.manufacturer.as_deref(), Some("Boeing"));
        assert_eq!(ac.type_name.as_deref(), Some("737-800"));
        assert_eq!(ac.registration.as_deref(), Some("EI-ABC"));
    }
}
