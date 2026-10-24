//! Distance prioritization and fair round-robin catalog capacity allocation.

#![allow(
    clippy::collapsible_if,
    clippy::if_not_else,
    clippy::assigning_clones,
    clippy::must_use_candidate,
    clippy::doc_markdown,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]

use lavender_domain::{CameraSource, GeoPoint};
use std::collections::{HashMap, HashSet};

/// Haversine distance in kilometres between two geographic coordinates.
#[must_use]
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;
    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let a = (d_lat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    EARTH_RADIUS_KM * c
}

/// Distance-prioritizes cameras to a cap: keeps the cameras closest to ANY of the given
/// anchor points (min distance over anchors), tie-broken by original array order.
#[must_use]
pub fn prioritize_sources(
    mut list: Vec<CameraSource>,
    max_count: usize,
    anchors: &[GeoPoint],
) -> Vec<CameraSource> {
    if anchors.is_empty() {
        if max_count > 0 && max_count < list.len() {
            list.truncate(max_count);
        }
        return list;
    }

    let mut scored: Vec<(usize, f64, CameraSource)> = list
        .into_iter()
        .enumerate()
        .map(|(idx, cam)| {
            let mut min_dist = f64::INFINITY;
            for anchor in anchors {
                let dist = haversine_km(cam.lat, cam.lon, anchor.lat, anchor.lon);
                if dist < min_dist {
                    min_dist = dist;
                }
            }
            (idx, min_dist, cam)
        })
        .collect();

    scored.sort_by(|a, b| {
        a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.0.cmp(&b.0))
    });

    let cap = if max_count > 0 { max_count.min(scored.len()) } else { scored.len() };
    scored.into_iter().take(cap).map(|(_, _, cam)| cam).collect()
}

struct PackLane {
    queue: Vec<CameraSource>,
    cursor: usize,
}

/// Merge source packs under one cap without starving any of them.
///
/// Every pack arrives already ordered by its own priority (nearest-to-anchor first)
/// and already trimmed to its own per-pack cap.
/// When the packs together exceed the catalog cap, the cap is filled ROUND-ROBIN —
/// one camera from each pack per turn, in each pack's own order.
/// Duplicate IDs resolve last-pack-wins.
#[must_use]
pub fn allocate_source_cap(
    packs: Vec<(&'static str, Vec<CameraSource>)>,
    max_total: usize,
) -> Vec<CameraSource> {
    // 1. Determine which pack wins each camera ID (last occurrence wins).
    let mut last_winner: HashMap<String, &'static str> = HashMap::new();
    for (pack_name, sources) in &packs {
        for source in sources {
            if !source.id.is_empty() {
                last_winner.insert(source.id.clone(), pack_name);
            }
        }
    }

    // 2. Build per-pack queues containing only the sources that won their ID.
    let mut lanes: Vec<PackLane> = Vec::with_capacity(packs.len());
    let mut total_available = 0;
    for (pack_name, sources) in packs {
        let mut queue = Vec::new();
        let mut seen_in_pack = HashSet::new();
        for source in sources {
            if !source.id.is_empty()
                && last_winner.get(&source.id) == Some(&pack_name)
                && seen_in_pack.insert(source.id.clone())
            {
                queue.push(source);
            }
        }
        total_available += queue.len();
        lanes.push(PackLane { queue, cursor: 0 });
    }

    let target_limit = max_total.min(total_available);
    let mut result = Vec::with_capacity(target_limit);

    // 3. Round-robin draw across lanes
    while result.len() < target_limit {
        let mut added_in_round = false;
        for lane in &mut lanes {
            if lane.cursor < lane.queue.len() {
                result.push(lane.queue[lane.cursor].clone());
                lane.cursor += 1;
                added_in_round = true;
                if result.len() >= target_limit {
                    break;
                }
            }
        }
        if !added_in_round {
            break;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_camera(id: &str, lat: f64, lon: f64) -> CameraSource {
        CameraSource {
            id: id.to_owned(),
            name: format!("Camera {id}"),
            city: "Test".to_owned(),
            city_id: "test".to_owned(),
            provider: "Test Provider".to_owned(),
            lat,
            lon,
            heading_deg: None,
            heading_confidence: None,
            pitch_deg: None,
            fov_deg: None,
            range_m: None,
            mount_height_m: None,
            ground_elevation_m: None,
            feed_type: "image".to_owned(),
            url: format!("https://example.com/{id}.jpg"),
            snapshot_url: format!("https://example.com/{id}.jpg"),
            license: "Public".to_owned(),
            credit: None,
            code: id.to_owned(),
            source_kind: "test".to_owned(),
            pose_source: None,
        }
    }

    #[test]
    fn test_prioritize_sources() {
        let anchor = GeoPoint { lat: 30.0, lon: -97.0, alt_m: None };
        let c1 = dummy_camera("c1", 30.01, -97.01); // closest
        let c2 = dummy_camera("c2", 30.50, -97.50); // far
        let c3 = dummy_camera("c3", 30.10, -97.10); // middle

        let prioritized = prioritize_sources(vec![c2, c1, c3], 2, &[anchor]);
        assert_eq!(prioritized.len(), 2);
        assert_eq!(prioritized[0].id, "c1");
        assert_eq!(prioritized[1].id, "c3");
    }

    #[test]
    fn test_allocate_source_cap_round_robin() {
        let pack_a = vec![dummy_camera("a1", 1.0, 1.0), dummy_camera("a2", 1.0, 1.0)];
        let pack_b = vec![dummy_camera("b1", 2.0, 2.0), dummy_camera("b2", 2.0, 2.0)];

        let allocated = allocate_source_cap(vec![("a", pack_a), ("b", pack_b)], 3);
        assert_eq!(allocated.len(), 3);
        assert_eq!(allocated[0].id, "a1");
        assert_eq!(allocated[1].id, "b1");
        assert_eq!(allocated[2].id, "a2");
    }

    #[test]
    fn test_allocate_source_cap_duplicate_resolution() {
        let pack_a = vec![dummy_camera("dup", 1.0, 1.0)];
        let pack_b = vec![dummy_camera("dup", 2.0, 2.0)]; // wins

        let allocated = allocate_source_cap(vec![("a", pack_a), ("b", pack_b)], 10);
        assert_eq!(allocated.len(), 1);
        assert_eq!(allocated[0].lat, 2.0);
    }
}
