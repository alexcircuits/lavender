//! Validated vessel records and track types for maritime AIS data.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::GeoPoint;

/// Speed over ground in knots, or None when the report carries AIS's "not available"
/// code (ITU-R M.1371 sentinel 102.3).
#[must_use]
pub fn normalize_speed(speed: Option<f64>) -> Option<f64> {
    speed.filter(|&s| (0.0..102.3).contains(&s))
}

/// Course over ground in degrees, or None when unavailable (360.0 sentinel in 0.1 deg units).
#[must_use]
pub fn normalize_course(course: Option<f64>) -> Option<f64> {
    course.filter(|&c| (0.0..360.0).contains(&c))
}

/// True heading in degrees, or None when unavailable (511 sentinel, valid 0..=360).
#[must_use]
pub fn normalize_heading(heading: Option<f64>) -> Option<f64> {
    heading.filter(|&h| (0.0..=360.0).contains(&h))
}

/// A maritime vessel contact from AIS tracking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Vessel {
    // Maritime Mobile Service Identity (9-digit string identifier).
    pub mmsi: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub imo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub vessel_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub destination: Option<String>,
    pub position: GeoPoint,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub speed_knots: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub course_deg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub heading_deg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_position_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub last_position_epoch: Option<i64>,
}

/// Voyage and status details of one ship that only its inspector needs. They travel with
/// the ship's track (`VesselTrackResponse`), fetched for the selected ship, rather than in
/// every feed snapshot: carried by all 12 000 ships of a busy view they nearly doubled each
/// full snapshot (ENG-980 lane measurement).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct VesselVoyage {
    /// Radio call sign from static data.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub call_sign: Option<String>,
    /// The navigational status of the latest Class A position report, in words (ITU-R
    /// M.1371); absent for Class B reports and the "not defined" code.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub navigational_status: Option<String>,
    /// Estimated time of arrival as the crew entered it (UTC, no year).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub eta: Option<VesselEta>,
    /// Maximum present static draught in metres (AIS reports 0.1 m steps up to 25.5).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub draught_m: Option<f64>,
    /// Overall length and beam from the AIS reference-point offsets.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dimensions: Option<VesselDimensions>,
}

impl VesselVoyage {
    /// Whether nothing is known.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// An AIS estimated time of arrival: month, day and optionally hour and minute in UTC. AIS
/// carries no year, so none is invented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct VesselEta {
    pub month: u8,
    pub day: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hour: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub minute: Option<u8>,
}

/// Overall ship dimensions in metres: length is bow plus stern offset (A + B), beam is
/// port plus starboard offset (C + D). Either is absent when AIS reports it unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct VesselDimensions {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub length_m: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub beam_m: Option<u16>,
}

impl Vessel {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mmsi: String,
        name: Option<String>,
        imo: Option<String>,
        vessel_type: Option<String>,
        destination: Option<String>,
        position: GeoPoint,
        speed_knots: Option<f64>,
        course_deg: Option<f64>,
        heading_deg: Option<f64>,
        last_position_utc: Option<String>,
        last_position_epoch: Option<i64>,
    ) -> Self {
        Self {
            mmsi,
            name,
            imo,
            vessel_type,
            destination,
            position,
            speed_knots: normalize_speed(speed_knots),
            course_deg: normalize_course(course_deg),
            heading_deg: normalize_heading(heading_deg),
            last_position_utc,
            last_position_epoch,
        }
    }
}

/// A thinned recent position fix in a vessel's track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct VesselTrackSample {
    pub lat: f64,
    pub lon: f64,
    #[ts(type = "number")]
    pub epoch_sec: i64,
}

/// Recent trajectory response for a vessel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct VesselTrackResponse {
    pub mmsi: String,
    pub samples: Vec<VesselTrackSample>,
    pub source: String,
    #[ts(type = "number")]
    pub retained_sec: u64,
    /// What the server knows of the ship's voyage and status now, when anything.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub voyage: Option<VesselVoyage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_sentinels() {
        assert_eq!(normalize_speed(Some(102.3)), None);
        assert_eq!(normalize_speed(Some(102.2)), Some(102.2));
        assert_eq!(normalize_speed(Some(-1.0)), None);
        assert_eq!(normalize_speed(Some(f64::NAN)), None);

        assert_eq!(normalize_course(Some(360.0)), None);
        assert_eq!(normalize_course(Some(359.9)), Some(359.9));
        assert_eq!(normalize_course(Some(0.0)), Some(0.0));
        assert_eq!(normalize_course(Some(-0.1)), None);

        assert_eq!(normalize_heading(Some(511.0)), None);
        assert_eq!(normalize_heading(Some(360.0)), Some(360.0));
        assert_eq!(normalize_heading(Some(0.0)), Some(0.0));
        assert_eq!(normalize_heading(Some(-1.0)), None);
    }
}
