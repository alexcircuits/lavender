//! GNSS navigation integrity and GPS interference detection engine.
//!
//! Evaluates aircraft-reported navigation accuracy (NIC and `NACp`) and transponder
//! GPS loss indicators across rolling 30-minute time windows, clustering observations
//! into 0.5-degree geographic cells using the gpsjam.org 1-aircraft false-positive
//! discount formula.

pub mod records;
pub mod store;

use std::sync::RwLock;

use jiff::Timestamp;
use lavender_domain::{Aircraft, BBox, GeoPoint, GnssProvenance, GnssSnapshot};
use serde::Deserialize;

use crate::error::CoreError;
use records::{GnssAircraftObservation, MAX_POSITION_AGE_S, default_gnss_provenance, is_degraded};
use store::GnssObservationStore;

/// Thread-safe GNSS integrity engine.
#[derive(Debug, Default)]
pub struct GnssEngine {
    store: RwLock<GnssObservationStore>,
}

#[derive(Debug, Deserialize)]
struct ReadsbSnapshot {
    #[serde(default)]
    now: Option<f64>,
    #[serde(default)]
    ac: Option<Vec<RawGnssAircraft>>,
}

#[derive(Debug, Deserialize)]
struct RawGnssAircraft {
    hex: Option<String>,
    #[serde(rename = "type")]
    type_code: Option<String>,
    version: Option<u8>,
    alt_baro: Option<serde_json::Value>,
    lat: Option<f64>,
    lon: Option<f64>,
    seen_pos: Option<f64>,
    nic: Option<u8>,
    nac_p: Option<u8>,
    #[serde(rename = "gpsOkBefore")]
    gps_ok_before: Option<f64>,
    #[serde(rename = "gpsOkLat")]
    gps_ok_lat: Option<f64>,
    #[serde(rename = "gpsOkLon")]
    gps_ok_lon: Option<f64>,
    #[serde(default)]
    tisb: Option<Vec<String>>,
    #[serde(default)]
    mlat: Option<Vec<String>>,
}

impl GnssEngine {
    /// Creates a new GNSS integrity engine with default 30-minute window and 0.5° resolution.
    #[must_use]
    pub fn new() -> Self {
        Self { store: RwLock::new(GnssObservationStore::default()) }
    }

    /// Ingests normalized GNSS aircraft observations into the store.
    pub fn ingest_observations(&self, records: &[GnssAircraftObservation], now: Timestamp) {
        if let Ok(mut store) = self.store.write() {
            store.accumulate(records, now);
        }
    }

    /// Ingests active `Aircraft` records from the live feed hub.
    pub fn ingest_aircraft_feed(&self, aircraft: &[Aircraft], now: Timestamp) {
        let mut observations = Vec::with_capacity(aircraft.len());
        for plane in aircraft {
            if plane.on_ground || plane.seen_pos_s > MAX_POSITION_AGE_S {
                continue;
            }
            observations.push(GnssAircraftObservation {
                hex: plane.hex.clone(),
                position: plane.position,
                nic: None,
                nac_p: None,
                gps_lost: false,
                degraded: false,
                observed_at: now,
            });
        }
        self.ingest_observations(&observations, now);
    }

    /// Ingests a raw readsb/adsb.lol JSON payload containing transponder quality indicators.
    ///
    /// # Errors
    /// Returns an error if the payload cannot be parsed as a valid readsb document.
    #[allow(clippy::cast_possible_truncation)]
    pub fn ingest_readsb_payload(
        &self,
        payload_bytes: &[u8],
        now: Timestamp,
    ) -> Result<usize, CoreError> {
        let snapshot: ReadsbSnapshot = serde_json::from_slice(payload_bytes)
            .map_err(|e| CoreError::BadRequest(format!("malformed readsb payload: {e}")))?;

        let Some(aircraft_list) = snapshot.ac else {
            return Ok(0);
        };

        let observed_at = snapshot
            .now
            .and_then(|secs| {
                if !secs.is_finite() || secs <= 0.0 {
                    None
                } else if secs > 10_000_000_000.0 {
                    Timestamp::from_millisecond((secs * 1000.0) as i64).ok()
                } else {
                    Timestamp::from_second(secs as i64).ok()
                }
            })
            .unwrap_or(now);

        let mut observations = Vec::with_capacity(aircraft_list.len());
        for raw in aircraft_list {
            if let Some(obs) = parse_raw_aircraft(raw, observed_at) {
                observations.push(obs);
            }
        }

        let count = observations.len();
        self.ingest_observations(&observations, now);
        Ok(count)
    }

    /// Evaluates the current GNSS integrity snapshot, optionally filtered by a bounding box.
    #[must_use]
    pub fn snapshot(&self, bbox_filter: Option<&BBox>) -> GnssSnapshot {
        let now = Timestamp::now();
        if let Ok(store) = self.store.read() {
            store.bin_cells(now, bbox_filter)
        } else {
            GnssSnapshot {
                as_of: now,
                window_minutes: default_gnss_provenance().window_minutes,
                cell_deg: records::GNSS_CELL_DEG,
                min_aircraft: records::GNSS_MIN_AIRCRAFT,
                total_cells: 0,
                anomaly_cells: 0,
                cells: Vec::new(),
            }
        }
    }

    /// Returns the static provenance metadata for the GNSS integrity classifier.
    #[must_use]
    pub fn provenance(&self) -> GnssProvenance {
        default_gnss_provenance()
    }
}

fn parse_raw_aircraft(
    raw: RawGnssAircraft,
    observed_at: Timestamp,
) -> Option<GnssAircraftObservation> {
    let hex = raw.hex?;
    if hex.is_empty() {
        return None;
    }

    // Filter qualifying transponder types: 'adsb_icao' or 'mlat'
    if let Some(ref t) = raw.type_code
        && t != "adsb_icao"
        && t != "mlat"
    {
        return None;
    }

    // Restrict to DO-260A/B version 1 or 2
    if let Some(v) = raw.version
        && v != 1
        && v != 2
    {
        return None;
    }

    // Must not be on ground
    if let Some(ref alt) = raw.alt_baro
        && alt.as_str() == Some("ground")
    {
        return None;
    }

    // Position age check
    if let Some(age) = raw.seen_pos
        && (!age.is_finite() || age < 0.0 || age > MAX_POSITION_AGE_S)
    {
        return None;
    }

    let raw_lat = raw.lat?;
    let raw_lon = raw.lon?;
    if !raw_lat.is_finite() || !raw_lon.is_finite() || raw_lat.abs() > 90.0 || raw_lon.abs() > 180.0
    {
        return None;
    }

    let is_derived = |field: &str| {
        raw.tisb.as_ref().is_some_and(|v| v.iter().any(|s| s == field))
            || raw.mlat.as_ref().is_some_and(|v| v.iter().any(|s| s == field))
    };

    let nic = if is_derived("nic") { None } else { raw.nic };
    let nac_p = if is_derived("nac_p") { None } else { raw.nac_p };

    let gps_lost = raw.gps_ok_before.is_some_and(|t| t.is_finite() && t > 0.0);
    if nic.is_none() && nac_p.is_none() && !gps_lost {
        return None;
    }

    // If GPS was lost and last good position is recorded, locate evidence at that last position
    let (lat, lon) = if gps_lost
        && let Some(ok_lat) = raw.gps_ok_lat
        && let Some(ok_lon) = raw.gps_ok_lon
        && ok_lat.is_finite()
        && ok_lon.is_finite()
        && ok_lat.abs() <= 90.0
        && ok_lon.abs() <= 180.0
    {
        (ok_lat, ok_lon)
    } else {
        (raw_lat, raw_lon)
    };

    let position = GeoPoint::new(lat, lon).ok()?;
    let degraded = is_degraded(nic, nac_p, gps_lost);

    Some(GnssAircraftObservation { hex, position, nic, nac_p, gps_lost, degraded, observed_at })
}
