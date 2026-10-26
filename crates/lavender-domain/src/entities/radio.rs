//! Radio Browser and tactical communications catalog domain entities.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::GeoPoint;

/// Operational or broadcast category for a georeferenced radio station.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum RadioCategory {
    /// Airband, air traffic control (`ATC`), and airport communications.
    Aviation,
    /// Maritime VHF, coast guard, and port operations.
    Marine,
    /// Fire, EMS, civil defense, and emergency management dispatch.
    Emergency,
    /// Amateur (`ham`) radio repeaters and public scanner feeds.
    Scanner,
    /// Weather radio (`NOAA`, `VOLMET`) and meteorological broadcasts.
    Weather,
    /// Domestic and international news broadcasters.
    News,
    /// Talk radio, interviews, and spoken-word programming.
    Talk,
    /// Highway, commuter, and transit traffic advisories.
    Traffic,
    /// General broadcast or music stations not matching a specialist tactical tag.
    General,
}

impl RadioCategory {
    /// Lowercase wire identifier for this category.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Aviation => "aviation",
            Self::Marine => "marine",
            Self::Emergency => "emergency",
            Self::Scanner => "scanner",
            Self::Weather => "weather",
            Self::News => "news",
            Self::Talk => "talk",
            Self::Traffic => "traffic",
            Self::General => "general",
        }
    }
}

/// A validated, georeferenced public radio station.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RadioStation {
    /// Stable station identifier (UUID or curated station key).
    pub id: String,
    /// Sanitized station display name.
    pub name: String,
    /// Validated public `HTTPS` audio stream URL.
    pub stream_url: String,
    /// Optional validated public `HTTPS` station homepage URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub homepage: Option<String>,
    /// Uppercase ISO 3166-1 alpha-2 country code (e.g. `"US"`, `"GB"`).
    pub country_code: String,
    /// Optional state, province, or administrative region.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub state: Option<String>,
    /// Primary broadcast language when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub language: Option<String>,
    /// Normalized lowercase station tags.
    pub tags: Vec<String>,
    /// Classified operational or broadcast category.
    pub category: RadioCategory,
    /// Uppercase browser-playable audio codec (`MP3`, `AAC`, `AAC+`, `OGG`, `OPUS`, `FLAC`).
    pub codec: String,
    /// Stream bitrate in kilobits per second.
    pub bitrate_kbps: u32,
    /// Community vote count from the directory.
    pub votes: u32,
    /// Recent listener click/tune count.
    pub click_count: u32,
    /// Validated transmitter or studio WGS84 coordinates.
    pub position: GeoPoint,
}

/// Point-in-time snapshot of the georeferenced radio station catalog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RadioCatalogSnapshot {
    /// Timestamp when this catalog generation was produced.
    pub as_of: Timestamp,
    /// Process-scoped catalog instance identifier scoping `generation`.
    pub instance: String,
    /// Monotonically increasing accepted catalog generation number.
    pub generation: u64,
    /// Whether the catalog is serving cached data past its fresh window.
    pub stale: bool,
    /// Total number of validated stations held in the catalog.
    pub total_stations: usize,
    /// Stations matching the caller's bounding box, category, and limit.
    pub stations: Vec<RadioStation>,
}
