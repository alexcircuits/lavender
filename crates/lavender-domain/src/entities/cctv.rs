//! Public camera (CCTV) domain entities.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A public traffic or weather camera.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CameraSource {
    pub id: String,
    pub name: String,
    pub city: String,
    pub city_id: String,
    pub provider: String,
    pub lat: f64,
    pub lon: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub heading_deg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub heading_confidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pitch_deg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fov_deg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub range_m: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mount_height_m: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ground_elevation_m: Option<f64>,
    pub feed_type: String,
    pub url: String,
    pub snapshot_url: String,
    pub license: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub credit: Option<String>,
    pub code: String,
    pub source_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pose_source: Option<String>,
}

/// Operational health and availability of a single camera.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CameraHealth {
    pub id: String,
    pub status: String,
    pub source_kind: String,
    pub label: String,
    pub message: String,
    pub updated_at: i64,
}

/// Information needed to stream or view frames from a camera.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CctvStreamInfo {
    pub id: String,
    pub feed_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub media_url: Option<String>,
    pub frame_url: String,
    pub provider: String,
    pub source_kind: String,
}

/// An automated computer vision target detection from a CCTV camera stream.
/// Emitted by the `lavender-vision` Mojo MLIR perception sidecar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CctvDetection {
    /// Bounding box in normalized coordinates: `[xmin, ymin, xmax, ymax]` in [0.0, 1.0].
    pub bbox: [f32; 4],
    /// Target classification identifier: 0 = Person, 1 = Car, 2 = Truck, 3 = Vessel, 4 = Aircraft.
    pub class_id: u16,
    /// Target class name (e.g. "vessel", "car", "aircraft").
    pub class_name: String,
    /// Model detection confidence score in range [0.0, 1.0].
    pub confidence: f32,
    /// Recognized ALPR vehicle license plate text if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub plate_text: Option<String>,
}

/// A collection of detections for a given camera frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CctvDetectionsResponse {
    pub camera_id: String,
    pub timestamp_ms: i64,
    pub frame_sequence: u64,
    pub engine: String,
    pub detections: Vec<CctvDetection>,
}
