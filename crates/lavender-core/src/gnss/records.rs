//! Classification rules and spatial grid mathematics for GNSS navigation integrity analysis.

use jiff::Timestamp;
use lavender_domain::{BBox, GeoPoint, GnssLevel, GnssProvenance};

/// Navigation Integrity Category threshold. NIC below 7 indicates containment radius >= 0.2 NM.
pub const GNSS_NIC_THRESHOLD: u8 = 7;
/// Navigation Accuracy Category for Position threshold. `NACp` below 8 indicates uncertainty >= 93 m.
pub const GNSS_NACP_THRESHOLD: u8 = 8;
/// Maximum age of an ADS-B position report in seconds to be considered current evidence.
pub const MAX_POSITION_AGE_S: f64 = 60.0;
/// Cell edge in degrees (0.5° ~ 55 km, comparable to an H3 resolution-4 cell).
pub const GNSS_CELL_DEG: f64 = 0.5;
/// Minimum distinct aircraft required in a cell before evaluating anomaly status.
pub const GNSS_MIN_AIRCRAFT: usize = 3;
/// Rolling evaluation window in minutes.
pub const GNSS_WINDOW_MINUTES: u32 = 30;
/// Band threshold for medium severity navigation integrity anomaly (percent degraded).
pub const GNSS_MEDIUM_PERCENT: f64 = 2.0;
/// Band threshold for high severity navigation integrity anomaly (percent degraded).
pub const GNSS_HIGH_PERCENT: f64 = 10.0;

/// Primary nominal cell state.
pub const GNSS_NOMINAL_STATE: &str = "nominal";
/// Primary anomaly cell state.
pub const GNSS_ANOMALY_STATE: &str = "navigation-integrity-anomaly";
/// Secondary unvalidated anomaly interpretation.
pub const GNSS_INTERPRETATION: &str = "suspected-gnss-interference";

/// An extracted aircraft observation relevant to GNSS navigation integrity.
#[derive(Debug, Clone, PartialEq)]
pub struct GnssAircraftObservation {
    /// 24-bit ICAO hexadecimal address.
    pub hex: String,
    /// Effective observation position (or last known good GNSS position if GPS was lost).
    pub position: GeoPoint,
    /// Navigation Integrity Category (0-11).
    pub nic: Option<u8>,
    /// Navigation Accuracy Category for Position (0-11).
    pub nac_p: Option<u8>,
    /// Whether the transponder reports loss of GPS lock (`gpsOkBefore > 0`).
    pub gps_lost: bool,
    /// Whether the aircraft reports degraded navigation integrity.
    pub degraded: bool,
    /// Observation timestamp.
    pub observed_at: Timestamp,
}

/// Evaluates whether an aircraft report is degraded per the 14 CFR 91.227-derived classifier rule.
#[must_use]
pub fn is_degraded(nic: Option<u8>, nac_p: Option<u8>, gps_lost: bool) -> bool {
    if gps_lost {
        return true;
    }
    if let Some(nac) = nac_p
        && nac < GNSS_NACP_THRESHOLD
    {
        return true;
    }
    if let Some(n) = nic
        && n < GNSS_NIC_THRESHOLD
    {
        // Exception: unknown NIC (0) with high position accuracy (NACp >= 10) is not classified degraded
        let is_unknown_with_high_accuracy = n == 0 && nac_p.is_some_and(|nac| nac >= 10);
        if !is_unknown_with_high_accuracy {
            return true;
        }
    }
    false
}

/// Computes the 0.5° grid row and column coordinates for a geographic point.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn gnss_cell_coords(lat: f64, lon: f64, cell_deg: f64) -> (u16, u16) {
    let clamped_lat = lat.clamp(-90.0, 89.999_999);
    let clamped_lon = lon.clamp(-180.0, 179.999_999);
    let row = ((clamped_lat + 90.0) / cell_deg).floor() as u16;
    let col = ((clamped_lon + 180.0) / cell_deg).floor() as u16;
    (row, col)
}

/// Formats the stable cell identifier string ("row:col").
#[must_use]
pub fn gnss_cell_id(row: u16, col: u16) -> String {
    format!("{row}:{col}")
}

/// Computes the bounding box for a 0.5° grid cell.
#[must_use]
pub fn gnss_cell_bbox(row: u16, col: u16, cell_deg: f64) -> BBox {
    let south = f64::from(row) * cell_deg - 90.0;
    let west = f64::from(col) * cell_deg - 180.0;
    let north = south + cell_deg;
    let east = west + cell_deg;
    BBox::new(west, south, east, north).unwrap_or(BBox::WORLD)
}

/// Computes the center coordinates for a 0.5° grid cell.
#[must_use]
pub fn gnss_cell_center(row: u16, col: u16, cell_deg: f64) -> GeoPoint {
    let south = f64::from(row) * cell_deg - 90.0;
    let west = f64::from(col) * cell_deg - 180.0;
    let lat = south + cell_deg / 2.0;
    let lon = west + cell_deg / 2.0;
    GeoPoint::new(lat, lon).unwrap_or(GeoPoint { lat: 0.0, lon: 0.0, alt_m: None })
}

/// Computes the anomaly level, state, and interpretation using the gpsjam.org 1-aircraft discount.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn evaluate_gnss_level(
    total: usize,
    degraded: usize,
) -> (f64, GnssLevel, String, Option<String>) {
    if total == 0 {
        return (0.0, GnssLevel::Low, GNSS_NOMINAL_STATE.to_owned(), None);
    }
    // Discount 1 degraded aircraft to limit false positives from single-airframe avionics faults
    let excess_degraded = degraded.saturating_sub(1);
    let percent = (100.0 * excess_degraded as f64) / total as f64;
    let rounded_percent = (percent * 10.0).round() / 10.0;

    let (level, state, interpretation) = if rounded_percent > GNSS_HIGH_PERCENT {
        (GnssLevel::High, GNSS_ANOMALY_STATE.to_owned(), Some(GNSS_INTERPRETATION.to_owned()))
    } else if rounded_percent >= GNSS_MEDIUM_PERCENT {
        (GnssLevel::Medium, GNSS_ANOMALY_STATE.to_owned(), Some(GNSS_INTERPRETATION.to_owned()))
    } else {
        (GnssLevel::Low, GNSS_NOMINAL_STATE.to_owned(), None)
    };

    (rounded_percent, level, state, interpretation)
}

/// Constructs the static machine-readable provenance metadata for GNSS integrity analysis.
#[must_use]
pub fn default_gnss_provenance() -> GnssProvenance {
    GnssProvenance {
        classifier_id: "gev-nic-nacp-v2".to_owned(),
        rule: format!(
            "gpsOkBefore set || NACp < {GNSS_NACP_THRESHOLD} || (NIC < {GNSS_NIC_THRESHOLD} && !(NIC == 0 && NACp >= 10))"
        ),
        basis: "14 CFR 91.227(c) ADS-B Out performance minima".to_owned(),
        aggregation_formula: "100 * max(0, degraded - 1) / total".to_owned(),
        medium_percent_threshold: GNSS_MEDIUM_PERCENT,
        high_percent_threshold: GNSS_HIGH_PERCENT,
        window_minutes: GNSS_WINDOW_MINUTES,
        cell_deg: GNSS_CELL_DEG,
        min_aircraft: GNSS_MIN_AIRCRAFT,
    }
}
