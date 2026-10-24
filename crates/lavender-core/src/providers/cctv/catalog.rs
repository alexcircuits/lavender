//! CCTV Catalog: in-memory camera store, spatial filtering, health tracking, and stream descriptors.

#![allow(
    clippy::collapsible_if,
    clippy::if_not_else,
    clippy::assigning_clones,
    clippy::must_use_candidate,
    clippy::doc_markdown
)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use lavender_domain::{BBox, CameraHealth, CameraSource, CctvStreamInfo, GeoPoint, LicenceMode};

use super::cap::allocate_source_cap;
use super::constants::{CCTV_MAX_SOURCES_CEILING, DEFAULT_CCTV_MAX_SOURCES};
use super::loaders::{
    BUNDLED_TALLINN_JSON, BUNDLED_WARENDORF_JSON, parse_tallinn_catalog, parse_warendorf_catalog,
};

/// Thread-safe in-memory CCTV camera catalog and health state tracker.
#[derive(Debug, Clone)]
pub struct CctvCatalog {
    cameras: Arc<RwLock<Vec<CameraSource>>>,
    by_id: Arc<RwLock<HashMap<String, CameraSource>>>,
    health: Arc<Mutex<HashMap<String, CameraHealth>>>,
}

impl Default for CctvCatalog {
    fn default() -> Self {
        Self::new_with_bundled()
    }
}

impl CctvCatalog {
    /// Initialize an empty CCTV catalog with bounded health tracker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cameras: Arc::new(RwLock::new(Vec::new())),
            by_id: Arc::new(RwLock::new(HashMap::new())),
            health: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Initialize a CCTV catalog pre-loaded with bundled agency packs (Tallinn & Warendorf).
    #[must_use]
    pub fn new_with_bundled() -> Self {
        let catalog = Self::new();
        let tallinn = parse_tallinn_catalog(BUNDLED_TALLINN_JSON);
        let warendorf = parse_warendorf_catalog(BUNDLED_WARENDORF_JSON);

        let merged = allocate_source_cap(
            vec![("tallinn", tallinn), ("warendorf", warendorf)],
            DEFAULT_CCTV_MAX_SOURCES,
        );
        catalog.set_cameras(merged);
        catalog
    }

    /// Replace all cameras in the catalog.
    pub fn set_cameras(&self, new_cameras: Vec<CameraSource>) {
        let mut by_id = HashMap::with_capacity(new_cameras.len());
        for cam in &new_cameras {
            by_id.insert(cam.id.clone(), cam.clone());
        }

        let mut lock = self.cameras.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        *lock = new_cameras;

        let mut id_lock = self.by_id.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        *id_lock = by_id;
    }

    /// Retrieve a camera source by its unique ID.
    #[must_use]
    pub fn get_camera(&self, camera_id: &str) -> Option<CameraSource> {
        let lock = self.by_id.read().unwrap_or_else(std::sync::PoisonError::into_inner);
        lock.get(camera_id).cloned()
    }

    /// Query cameras, optionally filtered by geographic bounding box and license mode.
    #[must_use]
    pub fn query_cameras(
        &self,
        bbox: Option<&BBox>,
        limit: Option<usize>,
        licence_mode: LicenceMode,
    ) -> Vec<CameraSource> {
        let lock = self.cameras.read().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut results = Vec::new();

        for cam in lock.iter() {
            // ADR 0008 licence enforcement: In Commercial mode, restrict non-commercial sources
            if licence_mode == LicenceMode::Commercial
                && (cam.source_kind == "municipal-webcam" || cam.source_kind == "tallinn-ristmikud")
            {
                continue;
            }

            if let Some(b) = bbox {
                let point = GeoPoint { lat: cam.lat, lon: cam.lon, alt_m: None };
                if !b.contains(&point) {
                    continue;
                }
            }

            results.push(cam.clone());
            if limit.is_some_and(|max| results.len() >= max) {
                break;
            }
        }

        results
    }

    /// Total count of loaded cameras.
    #[must_use]
    pub fn count(&self) -> usize {
        let lock = self.cameras.read().unwrap_or_else(std::sync::PoisonError::into_inner);
        lock.len()
    }

    /// Update or record the health status of a camera, bounded to [`CCTV_MAX_SOURCES_CEILING`].
    pub fn record_health(&self, camera_id: &str, status: &str, message: &str) {
        let mut health_map = self.health.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

        let (source_kind, label) = if let Some(cam) = self.get_camera(camera_id) {
            (cam.source_kind, cam.name)
        } else {
            ("unknown".to_owned(), camera_id.to_owned())
        };

        if !health_map.contains_key(camera_id) && health_map.len() >= CCTV_MAX_SOURCES_CEILING {
            if let Some(first_key) = health_map.keys().next().cloned() {
                health_map.remove(&first_key);
            }
        }

        let now_s = jiff::Timestamp::now().as_second();
        health_map.insert(
            camera_id.to_owned(),
            CameraHealth {
                id: camera_id.to_owned(),
                status: status.to_owned(),
                source_kind,
                label,
                message: message.to_owned(),
                updated_at: now_s,
            },
        );
    }

    /// List all tracked camera health entries.
    #[must_use]
    pub fn list_health(&self) -> Vec<CameraHealth> {
        let health_map = self.health.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        health_map.values().cloned().collect()
    }

    /// Build stream descriptor payload for a given camera ID.
    #[must_use]
    pub fn get_stream_info(&self, camera_id: &str) -> Option<CctvStreamInfo> {
        let cam = self.get_camera(camera_id)?;
        let is_video =
            cam.feed_type == "video" || cam.feed_type == "hls" || cam.feed_type == "mjpeg";

        let enc_id: String = url::form_urlencoded::byte_serialize(camera_id.as_bytes()).collect();
        let media_url = if is_video { Some(format!("/api/v1/cctv/media/{enc_id}")) } else { None };

        let frame_url = format!("/api/v1/cctv/frame/{enc_id}");

        Some(CctvStreamInfo {
            id: camera_id.to_owned(),
            feed_type: cam.feed_type,
            media_url,
            frame_url,
            provider: cam.provider,
            source_kind: cam.source_kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_bundled_and_query() {
        let catalog = CctvCatalog::new_with_bundled();
        assert!(catalog.count() >= 200);

        let warendorf = catalog.get_camera("warendorf-marktplatz-rathaus");
        assert!(warendorf.is_some());

        // In Evaluation mode, Warendorf is returned
        let eval_cams = catalog.query_cameras(None, None, LicenceMode::Evaluation);
        assert!(eval_cams.iter().any(|c| c.id == "warendorf-marktplatz-rathaus"));

        // In Commercial mode, non-commercial sources are gated
        let comm_cams = catalog.query_cameras(None, None, LicenceMode::Commercial);
        assert!(!comm_cams.iter().any(|c| c.id == "warendorf-marktplatz-rathaus"));
    }

    #[test]
    fn test_stream_info_generation() {
        let catalog = CctvCatalog::new_with_bundled();
        let stream_info = catalog.get_stream_info("warendorf-marktplatz-rathaus");
        assert!(stream_info.is_some());
        let info = stream_info.unwrap();
        assert_eq!(info.id, "warendorf-marktplatz-rathaus");
        assert_eq!(info.feed_type, "image");
        assert!(info.media_url.is_none());
        assert!(info.frame_url.contains("/api/v1/cctv/frame/"));
    }

    #[test]
    fn test_health_tracking() {
        let catalog = CctvCatalog::new();
        catalog.record_health("test-cam-1", "online", "Frame rendered");
        let list = catalog.list_health();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "test-cam-1");
        assert_eq!(list[0].status, "online");
    }
}
