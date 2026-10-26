//! Local SDR and Mode-S/UAT ADS-B receiver domain entities.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::GeoPoint;

/// Radio frequency band decoded by a local ADS-B receiver feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum LocalReceiverBand {
    /// 1090 MHz Mode S Extended Squitter (`dump1090`, `readsb`, `tar1090`).
    #[serde(rename = "1090")]
    Band1090,
    /// 978 MHz Universal Access Transceiver (`dump978-fa`, `skyaware978`).
    #[serde(rename = "978")]
    Band978,
}

impl LocalReceiverBand {
    /// Wire and configuration token for this band (`"1090"` or `"978"`).
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Band1090 => "1090",
            Self::Band978 => "978",
        }
    }

    /// Human-readable display label for this band.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Band1090 => "1090 MHz",
            Self::Band978 => "978 MHz UAT",
        }
    }

    /// Parses a band identifier (`"1090"` or `"978"`).
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "1090" => Some(Self::Band1090),
            "978" => Some(Self::Band978),
            _ => None,
        }
    }
}

/// Public operational status of a configured local receiver feed.
///
/// Never exposes the underlying receiver URL or IP address to browser clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LocalReceiverFeedStatus {
    /// Deterministic feed identifier (e.g. `"feed-1"`).
    pub id: String,
    /// Frequency band decoded by this feed when valid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub band: Option<LocalReceiverBand>,
    /// Display label naming the band and optional ordinal (e.g. `"1090 MHz #1"`).
    pub label: String,
    /// Whether the feed entry passed validation and is eligible for polling.
    pub configured: bool,
    /// Whether the feed data is stale or unreachable.
    pub stale: bool,
    /// Validation error or operational status reason when degraded or rejected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
    /// Number of active aircraft currently reported by this feed.
    pub aircraft_count: usize,
}

/// A normalized aircraft contact from a local SDR / `dump1090` / `dump978` receiver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LocalAdsbAircraft {
    /// 24-bit ICAO transponder address in 6-character lowercase hex.
    pub hex: String,
    /// Receiver frequency band that decoded this aircraft.
    pub band: LocalReceiverBand,
    /// Identifier of the local receiver feed that reported this contact.
    pub feed_id: String,
    /// Cleaned uppercase callsign / flight identifier when broadcast.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub callsign: Option<String>,
    /// Decoded WGS84 position when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub position: Option<GeoPoint>,
    /// Barometric pressure altitude in feet (`0` when `alt_baro` is `"ground"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub altitude_baro_ft: Option<i32>,
    /// Geometric (GNSS/INS) altitude in feet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub altitude_geom_ft: Option<i32>,
    /// Ground speed in knots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ground_speed_kt: Option<f64>,
    /// True track over ground in degrees `[0, 360)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub track_deg: Option<f64>,
    /// Vertical climb/descent rate in feet per minute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub vertical_rate_fpm: Option<i32>,
    /// 4-digit octal Mode-A squawk code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub squawk: Option<String>,
    /// Emergency/priority status when not `"none"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub emergency: Option<String>,
    /// ADS-B emitter category (e.g. `"A1"`, `"A3"`, `"A7"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub category: Option<String>,
    /// Navigation Integrity Category (`NIC`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub nic: Option<u8>,
    /// Navigation Accuracy Category for Position (`NACp`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub nac_p: Option<u8>,
    /// Received signal strength in `dBFS`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rssi_dbfs: Option<f64>,
    /// Total CRC-valid Mode-S/UAT messages decoded for this aircraft.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub messages: Option<u64>,
    /// Seconds elapsed since the last message of any type was received.
    pub seen_s: f64,
    /// Seconds elapsed since the last position message was decoded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub seen_pos_s: Option<f64>,
    /// Whether the aircraft reported surface status (`alt_baro: "ground"`).
    pub on_ground: bool,
    /// Whether position or velocity fields were derived via multilateration (`MLAT`).
    pub mlat: bool,
    /// Whether the contact originates from Traffic Information Service – Broadcast (`TIS-B`).
    pub tisb: bool,
}

/// Aggregated snapshot across all configured local ADS-B receiver feeds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LocalReceiverSnapshot {
    /// Timestamp when this snapshot was generated.
    pub as_of: Timestamp,
    /// Whether at least one local receiver feed is configured.
    pub configured: bool,
    /// Whether all configured feeds are currently stale or unpopulated.
    pub stale: bool,
    /// Per-feed status summaries (never containing raw receiver URLs).
    pub feeds: Vec<LocalReceiverFeedStatus>,
    /// Merged active aircraft across all local receiver feeds.
    pub aircraft: Vec<LocalAdsbAircraft>,
}
