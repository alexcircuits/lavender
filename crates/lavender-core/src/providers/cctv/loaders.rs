//! The 14 agency camera loaders for public CCTV feeds.

#![allow(
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::similar_names,
    clippy::unnecessary_wraps,
    clippy::collapsible_if,
    clippy::if_not_else,
    clippy::assigning_clones,
    clippy::must_use_candidate
)]

use lavender_domain::CameraSource;
use serde_json::Value;
use std::collections::HashMap;

use super::constants::{
    CALGARY_IMAGE_ORIGIN, FINTRAFFIC_IMAGE_ORIGIN, NSW_IMAGE_ORIGIN, ONTARIO_511_IMAGE_ORIGIN,
    TALLINN_IMAGE_ORIGIN, TARKTEE_IMAGE_ORIGIN, TFL_IMAGE_ORIGIN, TXDOT_ORIGIN,
    TXDOT_SNAPSHOT_PATH, VEGVESEN_IMAGE_ORIGIN, WARENDORF_IMAGE_ORIGINS,
};
use super::normalize::{
    base64_url_encode, camera_display_code, direction_to_heading, fallback_heading_from_id,
    is_likely_austin, is_likely_bc, is_likely_calgary, is_likely_finland, is_likely_norway,
    is_likely_nsw, is_likely_texas, is_plausible_lat_lon,
};

pub const BUNDLED_TALLINN_JSON: &str =
    include_str!("../../../../../legacy/config/cctv_sources.tallinn.json");
pub const BUNDLED_WARENDORF_JSON: &str =
    include_str!("../../../../../legacy/config/cctv_sources.warendorf.json");

// 1. Austin Open Data
pub fn parse_austin_rows(json_str: &str) -> Vec<CameraSource> {
    let Ok(v) = serde_json::from_str::<Value>(json_str) else {
        return Vec::new();
    };

    let mut cameras = Vec::new();
    let rows = if let Some(arr) = v.get("data").and_then(Value::as_array) {
        arr
    } else if let Some(arr) = v.as_array() {
        arr
    } else {
        return Vec::new();
    };

    // Columns detection if Socrata rows.json
    let mut id_col = None;
    let mut name_col = None;
    let mut lat_col = None;
    let mut lon_col = None;
    let mut heading_col = None;

    if let Some(cols) = v
        .get("meta")
        .and_then(|m| m.get("view"))
        .and_then(|vw| vw.get("columns"))
        .and_then(Value::as_array)
    {
        for (i, col) in cols.iter().enumerate() {
            if let Some(field) = col.get("fieldName").and_then(Value::as_str) {
                let lower = field.to_lowercase();
                if lower == "camera_id" || lower == "device_id" {
                    id_col = Some(i);
                } else if lower == "camera_name" || lower == "location_name" {
                    name_col = Some(i);
                } else if lower == "latitude" || lower == "camera_latitude" {
                    lat_col = Some(i);
                } else if lower == "longitude" || lower == "camera_longitude" {
                    lon_col = Some(i);
                } else if lower == "heading" || lower == "direction" {
                    heading_col = Some(i);
                }
            }
        }
    }

    for row in rows {
        if let Some(cells) = row.as_array() {
            let id = id_col
                .and_then(|c| cells.get(c))
                .and_then(|v| {
                    v.as_str().map(str::to_owned).or_else(|| v.as_i64().map(|n| n.to_string()))
                })
                .unwrap_or_default();
            if id.is_empty() {
                continue;
            }

            let name = name_col
                .and_then(|c| cells.get(c))
                .and_then(Value::as_str)
                .unwrap_or("Austin Camera");
            let lat = lat_col
                .and_then(|c| cells.get(c))
                .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
                .unwrap_or(f64::NAN);
            let lon = lon_col
                .and_then(|c| cells.get(c))
                .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
                .unwrap_or(f64::NAN);

            if !is_likely_austin(lat, lon) {
                continue;
            }

            let heading_deg = heading_col
                .and_then(|c| cells.get(c))
                .and_then(|v| {
                    v.as_f64().or_else(|| v.as_str().and_then(|s| direction_to_heading(s, true)))
                })
                .or_else(|| direction_to_heading(name, false))
                .or_else(|| Some(fallback_heading_from_id(&id)));

            let camera_id = format!("austin-{id}");
            let code = camera_display_code(name);
            let url = format!("https://atd.knack.com/arm/#cctv-cameras/cctv-camera-details/{id}/");

            cameras.push(CameraSource {
                id: camera_id,
                name: name.to_owned(),
                city: "Austin".to_owned(),
                city_id: "austin".to_owned(),
                provider: "City of Austin".to_owned(),
                lat,
                lon,
                heading_deg,
                heading_confidence: Some("medium".to_owned()),
                pitch_deg: Some(-24.0),
                fov_deg: Some(60.0),
                range_m: Some(150.0),
                mount_height_m: Some(9.0),
                ground_elevation_m: Some(149.0),
                feed_type: "image".to_owned(),
                url: url.clone(),
                snapshot_url: url,
                license: "Public city traffic camera frame".to_owned(),
                credit: None,
                code,
                source_kind: "austin-open-data".to_owned(),
                pose_source: None,
            });
        }
    }

    cameras
}

// 2. Caltrans
pub fn parse_caltrans_district_cameras(json_str: &str, district: u8) -> Vec<CameraSource> {
    let Ok(v) = serde_json::from_str::<Value>(json_str) else {
        return Vec::new();
    };

    let mut cameras = Vec::new();
    let Some(arr) = v.as_array().or_else(|| v.get("data").and_then(Value::as_array)) else {
        return Vec::new();
    };

    for item in arr {
        let Some(cctv) = item.get("cctv").or(Some(item)) else { continue };
        let in_service = cctv
            .get("cctvStatus")
            .and_then(|s| s.get("inService"))
            .and_then(Value::as_str)
            .unwrap_or("false");
        if in_service != "true" {
            continue;
        }

        let id = cctv
            .get("index")
            .and_then(|v| {
                v.as_str().map(str::to_owned).or_else(|| v.as_i64().map(|n| n.to_string()))
            })
            .unwrap_or_default();
        if id.is_empty() {
            continue;
        }

        let loc = cctv.get("location");
        let lat = loc
            .and_then(|l| l.get("latitude"))
            .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
            .unwrap_or(f64::NAN);
        let lon = loc
            .and_then(|l| l.get("longitude"))
            .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
            .unwrap_or(f64::NAN);

        if !is_plausible_lat_lon(lat, lon) {
            continue;
        }

        let name = loc
            .and_then(|l| l.get("locationName"))
            .and_then(Value::as_str)
            .unwrap_or("Caltrans Camera");
        let image_data = cctv.get("imageData");
        let still_url = image_data
            .and_then(|i| i.get("static"))
            .and_then(|s| s.get("currentImageURL"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let video_url = image_data
            .and_then(|i| i.get("streamingVideoURL"))
            .and_then(Value::as_str)
            .unwrap_or("");

        if still_url.is_empty() && video_url.is_empty() {
            continue;
        }

        let (feed_type, active_url) = if !video_url.is_empty() {
            ("hls".to_owned(), video_url.to_owned())
        } else {
            ("image".to_owned(), still_url.to_owned())
        };

        let direction_str =
            loc.and_then(|l| l.get("direction")).and_then(Value::as_str).unwrap_or("");
        let heading_deg = direction_to_heading(direction_str, true)
            .or_else(|| direction_to_heading(name, false))
            .or_else(|| Some(fallback_heading_from_id(&id)));

        cameras.push(CameraSource {
            id: format!("caltrans-d{district}-{id}"),
            name: name.to_owned(),
            city: "California".to_owned(),
            city_id: "california".to_owned(),
            provider: "Caltrans".to_owned(),
            lat,
            lon,
            heading_deg,
            heading_confidence: Some("medium".to_owned()),
            pitch_deg: Some(-20.0),
            fov_deg: Some(60.0),
            range_m: Some(180.0),
            mount_height_m: Some(10.0),
            ground_elevation_m: Some(50.0),
            feed_type,
            url: active_url,
            snapshot_url: still_url.to_owned(),
            license: "Public Caltrans highway camera frame".to_owned(),
            credit: None,
            code: camera_display_code(name),
            source_kind: "caltrans-cctv".to_owned(),
            pose_source: None,
        });
    }

    cameras
}

// 3. TfL JamCams
pub fn parse_tfl_jamcams(json_str: &str) -> Vec<CameraSource> {
    let Ok(v) = serde_json::from_str::<Value>(json_str) else {
        return Vec::new();
    };

    let Some(arr) = v.as_array() else { return Vec::new() };
    let mut cameras = Vec::new();

    for item in arr {
        let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        let name = item.get("commonName").and_then(Value::as_str).unwrap_or("TfL JamCam");
        let lat = item.get("lat").and_then(Value::as_f64).unwrap_or(f64::NAN);
        let lon = item.get("lon").and_then(Value::as_f64).unwrap_or(f64::NAN);

        if !is_plausible_lat_lon(lat, lon) {
            continue;
        }

        let mut image_url = String::new();
        let mut video_url = String::new();
        let mut view_desc = String::new();

        if let Some(props) = item.get("additionalProperties").and_then(Value::as_array) {
            for prop in props {
                let key = prop.get("key").and_then(Value::as_str).unwrap_or("");
                let val = prop.get("value").and_then(Value::as_str).unwrap_or("");
                if key == "imageUrl" {
                    image_url = val.to_owned();
                } else if key == "videoUrl" {
                    video_url = val.to_owned();
                } else if key == "view" {
                    view_desc = val.to_owned();
                }
            }
        }

        if !image_url.starts_with(TFL_IMAGE_ORIGIN) {
            continue;
        }

        let heading_deg = direction_to_heading(&view_desc, true)
            .or_else(|| direction_to_heading(name, false))
            .or_else(|| Some(fallback_heading_from_id(id)));

        let (feed_type, active_url) = if !video_url.is_empty() {
            ("video".to_owned(), video_url)
        } else {
            ("image".to_owned(), image_url.clone())
        };

        cameras.push(CameraSource {
            id: format!("tfl-{id}"),
            name: name.to_owned(),
            city: "London".to_owned(),
            city_id: "london".to_owned(),
            provider: "Transport for London".to_owned(),
            lat,
            lon,
            heading_deg,
            heading_confidence: Some("medium".to_owned()),
            pitch_deg: Some(-20.0),
            fov_deg: Some(60.0),
            range_m: Some(150.0),
            mount_height_m: Some(9.0),
            ground_elevation_m: Some(15.0),
            feed_type,
            url: active_url,
            snapshot_url: image_url,
            license: "Powered by TfL Open Data".to_owned(),
            credit: None,
            code: camera_display_code(name),
            source_kind: "tfl-jamcam".to_owned(),
            pose_source: None,
        });
    }

    cameras
}

// 4. Ontario 511
pub fn parse_ontario_511(json_str: &str) -> Vec<CameraSource> {
    let Ok(v) = serde_json::from_str::<Value>(json_str) else {
        return Vec::new();
    };

    let Some(arr) = v.as_array() else { return Vec::new() };
    let mut cameras = Vec::new();

    for item in arr {
        let id = item
            .get("Id")
            .and_then(|v| {
                v.as_str().map(str::to_owned).or_else(|| v.as_i64().map(|n| n.to_string()))
            })
            .unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        let lat = item.get("Latitude").and_then(Value::as_f64).unwrap_or(f64::NAN);
        let lon = item.get("Longitude").and_then(Value::as_f64).unwrap_or(f64::NAN);
        if !is_plausible_lat_lon(lat, lon) {
            continue;
        }

        let url = item.get("Url").and_then(Value::as_str).unwrap_or("");
        if !url.starts_with(ONTARIO_511_IMAGE_ORIGIN) {
            continue;
        }

        let name = item.get("RoadwayName").and_then(Value::as_str).unwrap_or("Ontario 511");
        let dir = item.get("DirectionOfTravel").and_then(Value::as_str).unwrap_or("");
        let heading_deg = direction_to_heading(dir, true)
            .or_else(|| direction_to_heading(name, false))
            .or_else(|| Some(fallback_heading_from_id(&id)));

        cameras.push(CameraSource {
            id: format!("on511-{id}"),
            name: name.to_owned(),
            city: "Ontario".to_owned(),
            city_id: "ontario".to_owned(),
            provider: "Ontario 511".to_owned(),
            lat,
            lon,
            heading_deg,
            heading_confidence: Some("medium".to_owned()),
            pitch_deg: Some(-20.0),
            fov_deg: Some(60.0),
            range_m: Some(180.0),
            mount_height_m: Some(10.0),
            ground_elevation_m: Some(100.0),
            feed_type: "image".to_owned(),
            url: url.to_owned(),
            snapshot_url: url.to_owned(),
            license: "Open Government Licence - Ontario".to_owned(),
            credit: None,
            code: camera_display_code(name),
            source_kind: "ontario-511".to_owned(),
            pose_source: None,
        });
    }

    cameras
}

// 5. Fintraffic (Digitraffic Finland)
pub fn parse_fintraffic_stations(json_str: &str) -> Vec<CameraSource> {
    let Ok(v) = serde_json::from_str::<Value>(json_str) else {
        return Vec::new();
    };

    let Some(features) = v.get("features").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut cameras = Vec::new();

    for feat in features {
        let props = feat.get("properties");
        let station_id = props.and_then(|p| p.get("id")).and_then(Value::as_str).unwrap_or("");
        let station_name = props.and_then(|p| p.get("name")).and_then(Value::as_str).unwrap_or("");

        let geom = feat.get("geometry");
        let coords = geom.and_then(|g| g.get("coordinates")).and_then(Value::as_array);
        let Some(coords) = coords else { continue };
        if coords.len() < 2 {
            continue;
        }

        let lon = coords[0].as_f64().unwrap_or(f64::NAN);
        let lat = coords[1].as_f64().unwrap_or(f64::NAN);
        let alt = if coords.len() >= 3 {
            coords[2].as_f64().filter(|&a| a > 0.0).unwrap_or(90.0)
        } else {
            90.0
        };

        if !is_likely_finland(lat, lon) {
            continue;
        }

        let presets = props.and_then(|p| p.get("presets")).and_then(Value::as_array);
        let Some(presets) = presets else { continue };

        for preset in presets {
            let preset_id = preset.get("id").and_then(Value::as_str).unwrap_or("");
            if preset_id.is_empty() {
                continue;
            }

            let view_suffix = preset_id.strip_prefix(station_id).unwrap_or("");
            let base_name = station_name.replace('_', " ").trim().to_owned();
            let name = if view_suffix.is_empty() {
                base_name
            } else {
                format!("{base_name} (view {view_suffix})")
            };

            let camera_id = format!("fi-fintraffic-{preset_id}");
            let url = format!("{FINTRAFFIC_IMAGE_ORIGIN}{preset_id}.jpg");
            let heading_deg = direction_to_heading(&name, false)
                .or_else(|| Some(fallback_heading_from_id(&camera_id)));

            cameras.push(CameraSource {
                id: camera_id,
                name: name.clone(),
                city: "Finland".to_owned(),
                city_id: "finland".to_owned(),
                provider: "Fintraffic".to_owned(),
                lat,
                lon,
                heading_deg,
                heading_confidence: Some("low".to_owned()),
                pitch_deg: Some(-20.0),
                fov_deg: Some(60.0),
                range_m: Some(150.0),
                mount_height_m: Some(8.0),
                ground_elevation_m: Some(alt),
                feed_type: "image".to_owned(),
                url: url.clone(),
                snapshot_url: url,
                license: "Fintraffic / digitraffic.fi (CC BY 4.0)".to_owned(),
                credit: None,
                code: camera_display_code(&name),
                source_kind: "fintraffic-weathercam".to_owned(),
                pose_source: None,
            });
        }
    }

    cameras
}

// 6. DriveBC
pub fn parse_drivebc_webcams(json_str: &str) -> Vec<CameraSource> {
    let Ok(v) = serde_json::from_str::<Value>(json_str) else {
        return Vec::new();
    };

    let Some(arr) = v.as_array() else { return Vec::new() };
    let mut cameras = Vec::new();

    for item in arr {
        let is_on = item.get("is_on").and_then(Value::as_bool).unwrap_or(true);
        let should_appear = item.get("should_appear").and_then(Value::as_bool).unwrap_or(true);
        if !is_on || !should_appear {
            continue;
        }

        let id = item
            .get("id")
            .and_then(|v| {
                v.as_i64().map(|n| n.to_string()).or_else(|| v.as_str().map(str::to_owned))
            })
            .unwrap_or_default();
        if id.is_empty() {
            continue;
        }

        let loc = item.get("location");
        let coords = loc.and_then(|l| l.get("coordinates")).and_then(Value::as_array);
        let Some(coords) = coords else { continue };
        if coords.len() < 2 {
            continue;
        }

        let lon = coords[0].as_f64().unwrap_or(f64::NAN);
        let lat = coords[1].as_f64().unwrap_or(f64::NAN);
        if !is_likely_bc(lat, lon) {
            continue;
        }

        let name = item.get("name").and_then(Value::as_str).unwrap_or("DriveBC Camera");
        let orientation = item.get("orientation").and_then(Value::as_str).unwrap_or("");
        let elevation = item.get("elevation").and_then(Value::as_f64).unwrap_or(50.0);

        let heading_deg = direction_to_heading(orientation, true)
            .or_else(|| direction_to_heading(name, false))
            .or_else(|| Some(fallback_heading_from_id(&id)));

        let url = format!("https://www.drivebc.ca/images/{id}.jpg");

        cameras.push(CameraSource {
            id: format!("drivebc-{id}"),
            name: name.to_owned(),
            city: "British Columbia".to_owned(),
            city_id: "british-columbia".to_owned(),
            provider: "DriveBC".to_owned(),
            lat,
            lon,
            heading_deg,
            heading_confidence: Some("medium".to_owned()),
            pitch_deg: Some(-20.0),
            fov_deg: Some(60.0),
            range_m: Some(180.0),
            mount_height_m: Some(9.0),
            ground_elevation_m: Some(elevation),
            feed_type: "image".to_owned(),
            url: url.clone(),
            snapshot_url: url,
            license: "DriveBC, Open Government Licence – British Columbia".to_owned(),
            credit: None,
            code: camera_display_code(name),
            source_kind: "drivebc-webcam".to_owned(),
            pose_source: None,
        });
    }

    cameras
}

// 7. TxDOT
pub fn normalize_txdot_district_payload(json_str: &str, district: &str) -> Vec<CameraSource> {
    let Ok(v) = serde_json::from_str::<Value>(json_str) else {
        return Vec::new();
    };

    let Some(statuses) = v.get("roadwayCctvStatuses").and_then(Value::as_object) else {
        return Vec::new();
    };

    let mut cameras = Vec::new();
    let district_upper = district.to_uppercase();
    let district_lower = district.to_lowercase();

    for rows in statuses.values() {
        let Some(row_arr) = rows.as_array() else { continue };
        for row in row_arr {
            let status = row.get("statusDescription").and_then(Value::as_str).unwrap_or("");
            if status != "Device Online" {
                continue;
            }

            let has_snapshot = row.get("hasSnapshot").and_then(Value::as_bool).unwrap_or(false);
            if !has_snapshot {
                continue;
            }

            let icd_id = row.get("icd_Id").and_then(Value::as_str).unwrap_or("");
            if icd_id.is_empty() {
                continue;
            }

            let lat = row.get("latitude").and_then(Value::as_f64).unwrap_or(f64::NAN);
            let lon = row.get("longitude").and_then(Value::as_f64).unwrap_or(f64::NAN);
            if !is_likely_texas(lat, lon) {
                continue;
            }

            let name = row.get("name").and_then(Value::as_str).unwrap_or(icd_id);
            let dir = row.get("dirDescription").and_then(Value::as_str).unwrap_or("");
            let heading_deg = direction_to_heading(dir, true)
                .or_else(|| direction_to_heading(name, false))
                .or_else(|| Some(fallback_heading_from_id(icd_id)));

            let id_token = base64_url_encode(icd_id.as_bytes());
            let camera_id = format!("txdot-{district_lower}-{id_token}");
            let snapshot_url = format!(
                "{TXDOT_ORIGIN}{TXDOT_SNAPSHOT_PATH}?icdId={icd_id}&districtCode={district_upper}"
            );

            cameras.push(CameraSource {
                id: camera_id,
                name: name.to_owned(),
                city: "Texas".to_owned(),
                city_id: "texas".to_owned(),
                provider: "TxDOT".to_owned(),
                lat,
                lon,
                heading_deg,
                heading_confidence: Some("medium".to_owned()),
                pitch_deg: Some(-20.0),
                fov_deg: Some(60.0),
                range_m: Some(180.0),
                mount_height_m: Some(10.0),
                ground_elevation_m: Some(150.0),
                feed_type: "image".to_owned(),
                url: snapshot_url.clone(),
                snapshot_url,
                license: "Public TxDOT traffic camera data".to_owned(),
                credit: None,
                code: camera_display_code(name),
                source_kind: "txdot-its".to_owned(),
                pose_source: None,
            });
        }
    }

    cameras
}

// 8. Tallinn (curated catalog)
pub fn parse_tallinn_catalog(json_str: &str) -> Vec<CameraSource> {
    let Ok(v) = serde_json::from_str::<Value>(json_str) else {
        return Vec::new();
    };

    let Some(arr) = v.as_array() else { return Vec::new() };
    let mut cameras = Vec::new();

    for item in arr {
        let id = item.get("id").and_then(Value::as_str).unwrap_or("");
        if !id.starts_with("tln-") {
            continue;
        }

        let url = item.get("url").and_then(Value::as_str).unwrap_or("");
        if !url.starts_with(TALLINN_IMAGE_ORIGIN) {
            continue;
        }

        let lat = item.get("lat").and_then(Value::as_f64).unwrap_or(f64::NAN);
        let lon = item.get("lon").and_then(Value::as_f64).unwrap_or(f64::NAN);
        if !is_plausible_lat_lon(lat, lon) {
            continue;
        }

        let name = item.get("name").and_then(Value::as_str).unwrap_or(id);
        let heading_deg = item
            .get("headingDeg")
            .and_then(Value::as_f64)
            .or_else(|| direction_to_heading(name, false))
            .or_else(|| Some(fallback_heading_from_id(id)));

        cameras.push(CameraSource {
            id: id.to_owned(),
            name: name.to_owned(),
            city: "Tallinn".to_owned(),
            city_id: "tallinn".to_owned(),
            provider: "City of Tallinn".to_owned(),
            lat,
            lon,
            heading_deg,
            heading_confidence: Some("curated".to_owned()),
            pitch_deg: item.get("pitchDeg").and_then(Value::as_f64).or(Some(-20.0)),
            fov_deg: item.get("fovDeg").and_then(Value::as_f64).or(Some(60.0)),
            range_m: item.get("rangeM").and_then(Value::as_f64).or(Some(150.0)),
            mount_height_m: item.get("mountHeightM").and_then(Value::as_f64).or(Some(9.0)),
            ground_elevation_m: item.get("groundElevationM").and_then(Value::as_f64).or(Some(25.0)),
            feed_type: "image".to_owned(),
            url: url.to_owned(),
            snapshot_url: url.to_owned(),
            license: "Public City of Tallinn traffic camera data".to_owned(),
            credit: None,
            code: camera_display_code(name),
            source_kind: "tallinn-ristmikud".to_owned(),
            pose_source: Some("curated".to_owned()),
        });
    }

    cameras
}

// 9. Tarktee DATEX2
pub fn parse_tarktee_datex(locations_xml: &str, images_xml: &str) -> Vec<CameraSource> {
    // 1. Locations: extract predefinedLocation id, name, lat, lon
    let mut locations = HashMap::new();
    for block in locations_xml.split("<predefinedLocation ") {
        let Some(id_part) = block.split("id=\"").nth(1) else { continue };
        let Some(id) = id_part.split('"').next() else { continue };

        let name = block
            .split("<predefinedLocationName>")
            .nth(1)
            .and_then(|s| s.split("<value").nth(1))
            .and_then(|s| s.split('>').nth(1))
            .and_then(|s| s.split("</value>").next())
            .unwrap_or("Tarktee Camera")
            .trim();

        let lat = block
            .split("<latitude>")
            .nth(1)
            .and_then(|s| s.split("</latitude>").next())
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(f64::NAN);

        let lon = block
            .split("<longitude>")
            .nth(1)
            .and_then(|s| s.split("</longitude>").next())
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(f64::NAN);

        if is_plausible_lat_lon(lat, lon) {
            locations.insert(id.to_owned(), (name.to_owned(), lat, lon));
        }
    }

    // 2. Images: extract linearPredefinedLocationReference id, urlLinkAddress
    let mut images = HashMap::new();
    for block in images_xml.split("<linearTrafficView ") {
        let Some(loc_ref_part) = block.split("linearPredefinedLocationReference").nth(1) else {
            continue;
        };
        let Some(loc_id_part) = loc_ref_part.split("id=\"").nth(1) else { continue };
        let Some(loc_id) = loc_id_part.split('"').next() else { continue };

        let Some(url_part) = block.split("<urlLinkAddress>").nth(1) else { continue };
        let Some(url) = url_part.split("</urlLinkAddress>").next() else { continue };
        let url_clean = url.trim();
        if url_clean.starts_with(TARKTEE_IMAGE_ORIGIN) {
            images.insert(loc_id.to_owned(), url_clean.to_owned());
        }
    }

    // 3. Join
    let mut cameras = Vec::new();
    for (loc_id, (name, lat, lon)) in locations {
        if let Some(image_url) = images.get(&loc_id) {
            let camera_id = format!("ee-tarktee-{loc_id}");
            let heading_deg = direction_to_heading(&name, false)
                .or_else(|| Some(fallback_heading_from_id(&camera_id)));

            cameras.push(CameraSource {
                id: camera_id,
                name: name.clone(),
                city: "Estonia".to_owned(),
                city_id: "estonia".to_owned(),
                provider: "Transpordiamet".to_owned(),
                lat,
                lon,
                heading_deg,
                heading_confidence: Some("medium".to_owned()),
                pitch_deg: Some(-20.0),
                fov_deg: Some(60.0),
                range_m: Some(150.0),
                mount_height_m: Some(8.0),
                ground_elevation_m: Some(30.0),
                feed_type: "image".to_owned(),
                url: image_url.clone(),
                snapshot_url: image_url.clone(),
                license: "Public Transpordiamet / Tarktee road weather camera data".to_owned(),
                credit: None,
                code: camera_display_code(&name),
                source_kind: "tarktee-datex".to_owned(),
                pose_source: None,
            });
        }
    }

    cameras
}

// 10. Warendorf
pub fn parse_warendorf_catalog(json_str: &str) -> Vec<CameraSource> {
    let Ok(v) = serde_json::from_str::<Value>(json_str) else {
        return Vec::new();
    };

    let Some(arr) = v.as_array() else { return Vec::new() };
    let mut cameras = Vec::new();

    for item in arr {
        let id = item.get("id").and_then(Value::as_str).unwrap_or("");
        if id.is_empty() {
            continue;
        }

        let url = item.get("url").and_then(Value::as_str).unwrap_or("");
        let is_allowed_origin = WARENDORF_IMAGE_ORIGINS.iter().any(|&o| url.starts_with(o));
        if !is_allowed_origin {
            continue;
        }

        let lat = item.get("lat").and_then(Value::as_f64).unwrap_or(f64::NAN);
        let lon = item.get("lon").and_then(Value::as_f64).unwrap_or(f64::NAN);
        if !is_plausible_lat_lon(lat, lon) {
            continue;
        }

        let name = item.get("name").and_then(Value::as_str).unwrap_or("Marktplatz Warendorf");
        let heading_deg = item
            .get("headingDeg")
            .and_then(Value::as_f64)
            .or_else(|| Some(fallback_heading_from_id(id)));

        cameras.push(CameraSource {
            id: id.to_owned(),
            name: name.to_owned(),
            city: "Warendorf".to_owned(),
            city_id: "warendorf".to_owned(),
            provider: "Stadt Warendorf".to_owned(),
            lat,
            lon,
            heading_deg,
            heading_confidence: Some("curated".to_owned()),
            pitch_deg: item.get("pitchDeg").and_then(Value::as_f64).or(Some(-20.0)),
            fov_deg: item.get("fovDeg").and_then(Value::as_f64).or(Some(60.0)),
            range_m: item.get("rangeM").and_then(Value::as_f64).or(Some(120.0)),
            mount_height_m: item.get("mountHeightM").and_then(Value::as_f64).or(Some(8.0)),
            ground_elevation_m: item.get("groundElevationM").and_then(Value::as_f64).or(Some(60.0)),
            feed_type: "image".to_owned(),
            url: url.to_owned(),
            snapshot_url: url.to_owned(),
            license: "Public municipal webcam data (Stadt Warendorf)".to_owned(),
            credit: None,
            code: camera_display_code(name),
            source_kind: "municipal-webcam".to_owned(),
            pose_source: Some("curated".to_owned()),
        });
    }

    cameras
}

// 11. NSW (Live Traffic NSW)
pub fn parse_nsw_cameras(json_str: &str) -> Vec<CameraSource> {
    let Ok(v) = serde_json::from_str::<Value>(json_str) else {
        return Vec::new();
    };

    let Some(features) = v.get("features").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut cameras = Vec::new();

    for feat in features {
        let id = feat.get("id").and_then(Value::as_str).unwrap_or("");
        if id.is_empty() {
            continue;
        }

        let geom = feat.get("geometry");
        let coords = geom.and_then(|g| g.get("coordinates")).and_then(Value::as_array);
        let Some(coords) = coords else { continue };
        if coords.len() < 2 {
            continue;
        }

        let lon = coords[0].as_f64().unwrap_or(f64::NAN);
        let lat = coords[1].as_f64().unwrap_or(f64::NAN);
        if !is_likely_nsw(lat, lon) {
            continue;
        }

        let props = feat.get("properties");
        let href = props.and_then(|p| p.get("href")).and_then(Value::as_str).unwrap_or("");
        if !href.starts_with(NSW_IMAGE_ORIGIN) {
            continue;
        }

        let title = props.and_then(|p| p.get("title")).and_then(Value::as_str).unwrap_or(id);
        let view = props.and_then(|p| p.get("view")).and_then(Value::as_str).unwrap_or("");
        let label = if !view.is_empty() && view.len() <= 140 && !view.contains('\n') {
            view
        } else {
            title
        };

        let dir = props.and_then(|p| p.get("direction")).and_then(Value::as_str).unwrap_or("");
        let heading_deg = direction_to_heading(dir, true)
            .or_else(|| direction_to_heading(label, false))
            .or_else(|| Some(fallback_heading_from_id(id)));

        let region = props
            .and_then(|p| p.get("region"))
            .and_then(Value::as_str)
            .unwrap_or("SYDNEY")
            .replace('_', " ");

        cameras.push(CameraSource {
            id: format!("nsw-{id}"),
            name: label.to_owned(),
            city: region,
            city_id: "sydney".to_owned(),
            provider: "Live Traffic NSW".to_owned(),
            lat,
            lon,
            heading_deg,
            heading_confidence: Some("high".to_owned()),
            pitch_deg: Some(-20.0),
            fov_deg: Some(60.0),
            range_m: Some(150.0),
            mount_height_m: Some(9.0),
            ground_elevation_m: Some(25.0),
            feed_type: "image".to_owned(),
            url: href.to_owned(),
            snapshot_url: href.to_owned(),
            license: "Live Traffic NSW — Transport for NSW, CC BY 4.0".to_owned(),
            credit: None,
            code: camera_display_code(title),
            source_kind: "nsw-livetraffic".to_owned(),
            pose_source: None,
        });
    }

    cameras
}

// 12. Calgary
pub fn parse_calgary_rows(json_str: &str) -> Vec<CameraSource> {
    let Ok(v) = serde_json::from_str::<Value>(json_str) else {
        return Vec::new();
    };

    let Some(arr) = v.as_array() else { return Vec::new() };
    let mut cameras = Vec::new();

    for row in arr {
        let cam_url_obj = row.get("camera_url");
        let raw_url = cam_url_obj.and_then(|o| o.get("url")).and_then(Value::as_str).unwrap_or("");
        let https_url = if raw_url.starts_with("http://") {
            raw_url.replacen("http://", "https://", 1)
        } else {
            raw_url.to_owned()
        };

        if !https_url.starts_with(CALGARY_IMAGE_ORIGIN) {
            continue;
        }

        let point = row.get("point");
        let coords = point.and_then(|p| p.get("coordinates")).and_then(Value::as_array);
        let Some(coords) = coords else { continue };
        if coords.len() < 2 {
            continue;
        }

        let lon = coords[0].as_f64().unwrap_or(f64::NAN);
        let lat = coords[1].as_f64().unwrap_or(f64::NAN);
        if !is_likely_calgary(lat, lon) {
            continue;
        }

        let camera_id = https_url
            .split('/')
            .next_back()
            .and_then(|f| f.strip_suffix(".jpg"))
            .unwrap_or("")
            .trim_start_matches("loc");
        if camera_id.is_empty() {
            continue;
        }

        let location =
            row.get("camera_location").and_then(Value::as_str).unwrap_or("Calgary Camera");
        let heading_deg = direction_to_heading(location, false)
            .or_else(|| Some(fallback_heading_from_id(camera_id)));

        cameras.push(CameraSource {
            id: format!("calgary-{camera_id}"),
            name: location.to_owned(),
            city: "Calgary".to_owned(),
            city_id: "calgary".to_owned(),
            provider: "The City of Calgary".to_owned(),
            lat,
            lon,
            heading_deg,
            heading_confidence: Some("medium".to_owned()),
            pitch_deg: Some(-20.0),
            fov_deg: Some(60.0),
            range_m: Some(150.0),
            mount_height_m: Some(9.0),
            ground_elevation_m: Some(1045.0),
            feed_type: "image".to_owned(),
            url: https_url.clone(),
            snapshot_url: https_url,
            license:
                "Contains information licensed under the Open Government Licence – City of Calgary"
                    .to_owned(),
            credit: None,
            code: camera_display_code(location),
            source_kind: "calgary-open-data".to_owned(),
            pose_source: None,
        });
    }

    cameras
}

// 13. DelDOT
pub fn parse_deldot_cameras(json_str: &str) -> Vec<CameraSource> {
    let Ok(v) = serde_json::from_str::<Value>(json_str) else {
        return Vec::new();
    };

    let Some(arr) = v.as_array() else { return Vec::new() };
    let mut cameras = Vec::new();

    for item in arr {
        let id = item
            .get("id")
            .and_then(|v| {
                v.as_str().map(str::to_owned).or_else(|| v.as_i64().map(|n| n.to_string()))
            })
            .unwrap_or_default();
        if id.is_empty() {
            continue;
        }

        let lat = item
            .get("lat")
            .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
            .unwrap_or(f64::NAN);
        let lon = item
            .get("lon")
            .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
            .unwrap_or(f64::NAN);
        if !is_plausible_lat_lon(lat, lon) {
            continue;
        }

        let title = item.get("title").and_then(Value::as_str).unwrap_or("DelDOT Camera");
        let image_url = item
            .get("image_url")
            .or_else(|| item.get("imageUrl"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let video_url = item
            .get("video_url")
            .or_else(|| item.get("videoUrl"))
            .and_then(Value::as_str)
            .unwrap_or("");

        let (feed_type, active_url) = if !video_url.is_empty() {
            ("video".to_owned(), video_url.to_owned())
        } else {
            ("image".to_owned(), image_url.to_owned())
        };

        if active_url.is_empty() {
            continue;
        }

        let heading_deg =
            direction_to_heading(title, false).or_else(|| Some(fallback_heading_from_id(&id)));

        cameras.push(CameraSource {
            id: format!("deldot-{id}"),
            name: title.to_owned(),
            city: "Delaware".to_owned(),
            city_id: "delaware".to_owned(),
            provider: "DelDOT".to_owned(),
            lat,
            lon,
            heading_deg,
            heading_confidence: Some("medium".to_owned()),
            pitch_deg: Some(-20.0),
            fov_deg: Some(60.0),
            range_m: Some(150.0),
            mount_height_m: Some(9.0),
            ground_elevation_m: Some(15.0),
            feed_type,
            url: active_url,
            snapshot_url: image_url.to_owned(),
            license: "Public DelDOT traffic camera".to_owned(),
            credit: None,
            code: camera_display_code(title),
            source_kind: "deldot-cctv".to_owned(),
            pose_source: None,
        });
    }

    cameras
}

// 14. Vegvesen (Norway)
pub fn parse_vegvesen_features(json_str: &str) -> Vec<CameraSource> {
    let Ok(v) = serde_json::from_str::<Value>(json_str) else {
        return Vec::new();
    };

    let Some(features) = v.get("features").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut cameras = Vec::new();

    for feat in features {
        let props = feat.get("properties");
        let camera_id = props.and_then(|p| p.get("cameraId")).and_then(Value::as_str).unwrap_or("");
        if camera_id.is_empty() {
            continue;
        }

        let still_url =
            props.and_then(|p| p.get("stillImageUrl")).and_then(Value::as_str).unwrap_or("");
        if !still_url.starts_with(VEGVESEN_IMAGE_ORIGIN) {
            continue;
        }

        let geom = feat.get("geometry");
        let coords = geom.and_then(|g| g.get("coordinates")).and_then(Value::as_array);
        let Some(coords) = coords else { continue };
        if coords.len() < 2 {
            continue;
        }

        let lon = coords[0].as_f64().unwrap_or(f64::NAN);
        let lat = coords[1].as_f64().unwrap_or(f64::NAN);
        if !is_likely_norway(lat, lon) {
            continue;
        }

        let road = props.and_then(|p| p.get("roadNumber")).and_then(Value::as_str).unwrap_or("");
        let desc = props.and_then(|p| p.get("description")).and_then(Value::as_str).unwrap_or("");
        let orient = props
            .and_then(|p| p.get("orientationDescription"))
            .and_then(Value::as_str)
            .unwrap_or("");

        let name = if !road.is_empty() && !orient.is_empty() {
            format!("{road} {desc} → {orient}")
        } else if !desc.is_empty() {
            desc.to_owned()
        } else {
            format!("Vegvesen {camera_id}")
        };

        let raw_video = props.and_then(|p| p.get("videoUrl")).and_then(Value::as_str).unwrap_or("");
        let expected_video = format!("https://kamera.vegvesen.no/public/{camera_id}/manifest.m3u8");
        let (feed_type, active_url) = if raw_video == expected_video {
            ("hls".to_owned(), raw_video.to_owned())
        } else {
            ("image".to_owned(), still_url.to_owned())
        };

        let heading_deg = direction_to_heading(&name, false)
            .or_else(|| Some(fallback_heading_from_id(camera_id)));

        cameras.push(CameraSource {
            id: format!("no-vegvesen-{camera_id}"),
            name: name.clone(),
            city: "Norway".to_owned(),
            city_id: "norway".to_owned(),
            provider: "Statens vegvesen".to_owned(),
            lat,
            lon,
            heading_deg,
            heading_confidence: Some("low".to_owned()),
            pitch_deg: Some(-20.0),
            fov_deg: Some(60.0),
            range_m: Some(150.0),
            mount_height_m: Some(8.0),
            ground_elevation_m: Some(150.0),
            feed_type,
            url: active_url,
            snapshot_url: still_url.to_owned(),
            license: "Contains data under the Norwegian licence for Open Government data (NLOD) distributed by Statens vegvesen".to_owned(),
            credit: None,
            code: camera_display_code(if desc.is_empty() { &name } else { desc }),
            source_kind: "vegvesen-datex".to_owned(),
            pose_source: None,
        });
    }

    cameras
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tallinn_bundled_pack_loads() {
        let cameras = parse_tallinn_catalog(BUNDLED_TALLINN_JSON);
        assert!(cameras.len() >= 200);
        let first = &cameras[0];
        assert!(first.id.starts_with("tln-"));
        assert_eq!(first.city_id, "tallinn");
        assert!(first.url.starts_with(TALLINN_IMAGE_ORIGIN));
    }

    #[test]
    fn test_warendorf_bundled_pack_loads() {
        let cameras = parse_warendorf_catalog(BUNDLED_WARENDORF_JSON);
        assert_eq!(cameras.len(), 1);
        let cam = &cameras[0];
        assert_eq!(cam.id, "warendorf-marktplatz-rathaus");
        assert_eq!(cam.city_id, "warendorf");
        assert_eq!(cam.pose_source.as_deref(), Some("curated"));
    }

    #[test]
    fn test_txdot_normalization() {
        let payload = r#"{
            "roadwayCctvStatuses": {
                "FM-734": [
                    {
                        "icd_Id": "test_cam",
                        "name": "FM-734 Online",
                        "latitude": 30.34,
                        "longitude": -97.58,
                        "statusDescription": "Device Online",
                        "hasSnapshot": true,
                        "dirDescription": "North"
                    },
                    {
                        "icd_Id": "offline_cam",
                        "name": "Offline",
                        "latitude": 30.34,
                        "longitude": -97.58,
                        "statusDescription": "Device Offline",
                        "hasSnapshot": true
                    }
                ]
            }
        }"#;
        let cameras = normalize_txdot_district_payload(payload, "AUS");
        assert_eq!(cameras.len(), 1);
        assert_eq!(cameras[0].name, "FM-734 Online");
        assert_eq!(cameras[0].heading_deg, Some(0.0));
    }

    #[test]
    fn test_vegvesen_parsing_and_hls_validation() {
        let payload = r#"{
            "type": "FeatureCollection",
            "features": [
                {
                    "type": "Feature",
                    "geometry": { "type": "Point", "coordinates": [5.459, 61.832] },
                    "properties": {
                        "cameraId": "3000063_1",
                        "roadNumber": "F614",
                        "description": "Langesi",
                        "orientationDescription": "Svelgen",
                        "stillImageUrl": "https://kamera.atlas.vegvesen.no/api/images/3000063_1",
                        "videoUrl": "https://kamera.vegvesen.no/public/3000063_1/manifest.m3u8"
                    }
                },
                {
                    "type": "Feature",
                    "geometry": { "type": "Point", "coordinates": [5.459, 61.832] },
                    "properties": {
                        "cameraId": "bad_cam",
                        "stillImageUrl": "https://evil.test/bad.jpg"
                    }
                }
            ]
        }"#;
        let cameras = parse_vegvesen_features(payload);
        assert_eq!(cameras.len(), 1);
        let cam = &cameras[0];
        assert_eq!(cam.id, "no-vegvesen-3000063_1");
        assert_eq!(cam.feed_type, "hls");
    }

    #[test]
    fn test_nsw_parsing_and_label() {
        let payload = r#"{
            "type": "FeatureCollection",
            "features": [
                {
                    "type": "Feature",
                    "id": "miranda",
                    "geometry": { "type": "Point", "coordinates": [151.1036, -34.0337] },
                    "properties": {
                        "title": "5 Ways (Miranda)",
                        "view": "Looking west",
                        "direction": "W",
                        "href": "https://webcams.transport.nsw.gov.au/cameras/miranda.jpeg",
                        "region": "SYD_SOUTH"
                    }
                }
            ]
        }"#;
        let cameras = parse_nsw_cameras(payload);
        assert_eq!(cameras.len(), 1);
        assert_eq!(cameras[0].name, "Looking west");
        assert_eq!(cameras[0].heading_deg, Some(270.0));
    }

    #[test]
    fn test_calgary_parsing() {
        let payload = r#"[
            {
                "camera_url": { "url": "http://trafficcam.calgary.ca/loc142.jpg" },
                "camera_location": "Bow Trail / 37 Street SW",
                "point": { "type": "Point", "coordinates": [-114.141, 51.045] }
            }
        ]"#;
        let cameras = parse_calgary_rows(payload);
        assert_eq!(cameras.len(), 1);
        assert_eq!(cameras[0].id, "calgary-142");
        assert!(cameras[0].url.starts_with("https://trafficcam.calgary.ca/"));
    }
}
