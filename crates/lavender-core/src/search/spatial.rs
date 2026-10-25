//! Spatial entity search: queries live feed items against a bounding box, radial circle,
//! polygon or buffered corridor.

use std::collections::HashSet;

use lavender_domain::{
    FeedId, FeedItems, GeoPoint, SpatialFilter, SpatialHit, SpatialSearchRequest,
    SpatialSearchResponse,
};
use lavender_geo::distance_m;
use lavender_geo::polygon::METRES_PER_DEGREE;

use crate::alerting::geometry::antimeridian_point_in_polygon;
use crate::error::CoreError;
use crate::hub::FeedHub;

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 500;
const MAX_CORRIDOR_WAYPOINTS: usize = 128;
const MAX_POLYGON_VERTICES: usize = 1024;
const MAX_RADIUS_M: f64 = 20_000_000.0;
const MAX_BUFFER_M: f64 = 5_000_000.0;

/// Default active feeds with geographic entity positions.
const SPATIAL_FEEDS: [FeedId; 9] = [
    FeedId::Aircraft,
    FeedId::Vessels,
    FeedId::Earthquakes,
    FeedId::Cyclones,
    FeedId::Fires,
    FeedId::Launches,
    FeedId::Bikeshare,
    FeedId::Transit,
    FeedId::Perimeters,
];

/// Computes the shortest distance in metres from point `p` to the line segment between `a` and `b`.
#[must_use]
pub fn point_to_segment_distance_m(p: &GeoPoint, a: &GeoPoint, b: &GeoPoint) -> f64 {
    let seg_len = distance_m(a, b);
    if seg_len < 1e-3 {
        return distance_m(p, a);
    }
    // Local equirectangular tangent projection around segment mean latitude.
    let mean_lat = (a.lat.midpoint(b.lat)).to_radians();
    let cos_lat = mean_lat.cos();

    let d_lon_b = (b.lon - a.lon + 540.0).rem_euclid(360.0) - 180.0;
    let d_lat_b = b.lat - a.lat;
    let bx = d_lon_b * cos_lat * METRES_PER_DEGREE;
    let by = d_lat_b * METRES_PER_DEGREE;

    let d_lon_p = (p.lon - a.lon + 540.0).rem_euclid(360.0) - 180.0;
    let d_lat_p = p.lat - a.lat;
    let px = d_lon_p * cos_lat * METRES_PER_DEGREE;
    let py = d_lat_p * METRES_PER_DEGREE;

    let dot = px * bx + py * by;
    let len_sq = bx * bx + by * by;
    let t = if len_sq > 0.0 { (dot / len_sq).clamp(0.0, 1.0) } else { 0.0 };

    let proj_x = t * bx;
    let proj_y = t * by;
    let dx = px - proj_x;
    let dy = py - proj_y;
    (dx * dx + dy * dy).sqrt()
}

/// Evaluates whether a geographic point matches the spatial filter.
/// Returns `Some(distance_m)` if matched, where `distance_m` is the reference distance
/// (from circle center or corridor centerline) if applicable, or `None` if unmatched.
#[must_use]
pub fn evaluate_spatial_filter(p: &GeoPoint, filter: &SpatialFilter) -> Option<Option<f64>> {
    match filter {
        SpatialFilter::BBox { bbox } => {
            if bbox.contains(p) {
                Some(None)
            } else {
                None
            }
        }
        SpatialFilter::Circle { center, radius_m } => {
            let dist = distance_m(center, p);
            if dist <= *radius_m { Some(Some(dist)) } else { None }
        }
        SpatialFilter::Polygon { points } => {
            if antimeridian_point_in_polygon(p.lon, p.lat, points) { Some(None) } else { None }
        }
        SpatialFilter::Corridor { waypoints, buffer_m } => {
            if waypoints.len() < 2 {
                return None;
            }
            let mut min_dist = f64::MAX;
            for pair in waypoints.windows(2) {
                let d = point_to_segment_distance_m(p, &pair[0], &pair[1]);
                if d < min_dist {
                    min_dist = d;
                }
            }
            if min_dist <= *buffer_m { Some(Some(min_dist)) } else { None }
        }
    }
}

/// Validates spatial search parameters.
fn validate_request(req: &SpatialSearchRequest) -> Result<(), CoreError> {
    match &req.filter {
        SpatialFilter::BBox { bbox } => {
            if bbox.south > bbox.north || !bbox.south.is_finite() || !bbox.north.is_finite() {
                return Err(CoreError::BadRequest("invalid bounding box coordinates".to_owned()));
            }
        }
        SpatialFilter::Circle { center, radius_m } => {
            if !center.lat.is_finite() || !center.lon.is_finite() {
                return Err(CoreError::BadRequest(
                    "center coordinates must be finite numbers".to_owned(),
                ));
            }
            if *radius_m <= 0.0 || *radius_m > MAX_RADIUS_M || !radius_m.is_finite() {
                return Err(CoreError::BadRequest(format!(
                    "search radius must be between 1m and {MAX_RADIUS_M:.0}m"
                )));
            }
        }
        SpatialFilter::Polygon { points } => {
            if points.len() < 3 {
                return Err(CoreError::BadRequest(
                    "polygon filter requires at least 3 vertices".to_owned(),
                ));
            }
            if points.len() > MAX_POLYGON_VERTICES {
                return Err(CoreError::BadRequest(format!(
                    "polygon cannot exceed {MAX_POLYGON_VERTICES} vertices"
                )));
            }
            for p in points {
                if !p.lat.is_finite() || !p.lon.is_finite() {
                    return Err(CoreError::BadRequest(
                        "polygon vertices must be finite coordinates".to_owned(),
                    ));
                }
            }
        }
        SpatialFilter::Corridor { waypoints, buffer_m } => {
            if waypoints.len() < 2 {
                return Err(CoreError::BadRequest(
                    "corridor filter requires at least 2 waypoints".to_owned(),
                ));
            }
            if waypoints.len() > MAX_CORRIDOR_WAYPOINTS {
                return Err(CoreError::BadRequest(format!(
                    "corridor cannot exceed {MAX_CORRIDOR_WAYPOINTS} waypoints"
                )));
            }
            if *buffer_m <= 0.0 || *buffer_m > MAX_BUFFER_M || !buffer_m.is_finite() {
                return Err(CoreError::BadRequest(format!(
                    "corridor buffer must be between 1m and {MAX_BUFFER_M:.0}m"
                )));
            }
            for p in waypoints {
                if !p.lat.is_finite() || !p.lon.is_finite() {
                    return Err(CoreError::BadRequest(
                        "corridor waypoints must be finite coordinates".to_owned(),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Executes a spatial entity query against all or filtered active feeds in the hub.
///
/// # Errors
/// Returns an error if the filter parameters or coordinates are invalid.
pub fn spatial_search(
    req: &SpatialSearchRequest,
    hub: &FeedHub,
) -> Result<SpatialSearchResponse, CoreError> {
    validate_request(req)?;

    let target_feeds: Vec<FeedId> = match &req.feeds {
        Some(feeds) if !feeds.is_empty() => feeds.clone(),
        _ => SPATIAL_FEEDS.to_vec(),
    };

    let mut holder = Vec::new();
    for &feed in &target_feeds {
        for arc_items in hub.current_items(feed) {
            holder.push((feed, arc_items));
        }
    }

    let borrowed: Vec<(FeedId, &FeedItems)> =
        holder.iter().map(|(f, arc)| (*f, arc.as_ref())).collect();

    search_feed_items(req, &borrowed)
}

/// Evaluates spatial queries directly against a slice of feed items.
///
/// # Errors
/// Returns an error if the filter parameters or coordinates are invalid.
pub fn search_feed_items(
    req: &SpatialSearchRequest,
    items_by_feed: &[(FeedId, &FeedItems)],
) -> Result<SpatialSearchResponse, CoreError> {
    validate_request(req)?;

    let limit = req.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let target_feeds: Option<HashSet<FeedId>> =
        req.feeds.as_ref().map(|f| f.iter().copied().collect());

    let mut all_hits = Vec::new();
    let mut seen_ids: HashSet<(FeedId, String)> = HashSet::new();

    for &(feed, items) in items_by_feed {
        if let Some(targets) = &target_feeds
            && !targets.contains(&feed)
        {
            continue;
        }
        collect_feed_hits(feed, items, &req.filter, &mut seen_ids, &mut all_hits);
    }

    // Sort results: by distance if radial/corridor query, otherwise by feed and label.
    match &req.filter {
        SpatialFilter::Circle { .. } | SpatialFilter::Corridor { .. } => {
            all_hits.sort_by(|a, b| {
                a.distance_m
                    .unwrap_or(f64::MAX)
                    .partial_cmp(&b.distance_m.unwrap_or(f64::MAX))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        SpatialFilter::BBox { .. } | SpatialFilter::Polygon { .. } => {
            all_hits.sort_by(|a, b| {
                a.feed.as_str().cmp(b.feed.as_str()).then_with(|| a.label.cmp(&b.label))
            });
        }
    }

    let total_matched = all_hits.len();
    let truncated = total_matched > limit;
    if truncated {
        all_hits.truncate(limit);
    }

    Ok(SpatialSearchResponse { hits: all_hits, total_matched, truncated, notices: Vec::new() })
}

#[allow(clippy::too_many_arguments)]
fn push_hit(
    feed: FeedId,
    id: String,
    label: String,
    detail: Option<String>,
    pos: GeoPoint,
    filter: &SpatialFilter,
    seen: &mut HashSet<(FeedId, String)>,
    out: &mut Vec<SpatialHit>,
) {
    if let Some(dist) = evaluate_spatial_filter(&pos, filter)
        && seen.insert((feed, id.clone()))
    {
        out.push(SpatialHit { feed, id, label, detail, position: pos, distance_m: dist });
    }
}

fn collect_transport_hits(
    feed: FeedId,
    items: &FeedItems,
    filter: &SpatialFilter,
    seen: &mut HashSet<(FeedId, String)>,
    out: &mut Vec<SpatialHit>,
) -> bool {
    match items {
        FeedItems::Aircraft(aircraft) => {
            for plane in aircraft {
                let label = plane.callsign.clone().unwrap_or_else(|| plane.hex.clone());
                let detail = plane.altitude_baro_ft.map(|ft| format!("{ft:.0} ft"));
                push_hit(feed, plane.hex.clone(), label, detail, plane.position, filter, seen, out);
            }
            true
        }
        FeedItems::Vessels(vessels) => {
            for vessel in vessels {
                let label = vessel.name.clone().unwrap_or_else(|| vessel.mmsi.clone());
                let detail = vessel.speed_knots.map(|s| format!("{s:.1} kt"));
                push_hit(
                    feed,
                    vessel.mmsi.clone(),
                    label,
                    detail,
                    vessel.position,
                    filter,
                    seen,
                    out,
                );
            }
            true
        }
        FeedItems::Transit(transit) => {
            for vehicle in transit {
                let label = vehicle.label.clone().unwrap_or_else(|| vehicle.id.clone());
                let detail = vehicle.route_id.clone().map(|r| format!("Route: {r}"));
                push_hit(
                    feed,
                    vehicle.id.clone(),
                    label,
                    detail,
                    vehicle.position,
                    filter,
                    seen,
                    out,
                );
            }
            true
        }
        FeedItems::Bikeshare(stations) => {
            for station in stations {
                let detail = Some(format!(
                    "{}/{} bikes",
                    station.bikes_available.unwrap_or(0),
                    station.capacity.unwrap_or(0)
                ));
                push_hit(
                    feed,
                    station.id.clone(),
                    station.name.clone(),
                    detail,
                    station.position,
                    filter,
                    seen,
                    out,
                );
            }
            true
        }
        _ => false,
    }
}

fn collect_feed_hits(
    feed: FeedId,
    items: &FeedItems,
    filter: &SpatialFilter,
    seen: &mut HashSet<(FeedId, String)>,
    out: &mut Vec<SpatialHit>,
) {
    if collect_transport_hits(feed, items, filter, seen, out) {
        return;
    }
    match items {
        FeedItems::Earthquakes(quakes) => {
            for quake in quakes {
                let label = format!(
                    "M{:.1} {}",
                    quake.magnitude,
                    quake.place.as_deref().unwrap_or("Seismic event")
                );
                let detail = Some(format!("Depth: {:.1} km", quake.depth_km));
                push_hit(feed, quake.id.clone(), label, detail, quake.position, filter, seen, out);
            }
        }
        FeedItems::Fires(fires) => {
            for fire in fires {
                if let Ok(pos) = GeoPoint::new(fire.lat, fire.lon) {
                    let id = format!("fire-{:.4}-{:.4}", fire.lat, fire.lon);
                    let label = format!("Hotspot {:.1} MW", fire.frp);
                    let detail = Some(format!("Confidence: {}", fire.confidence));
                    push_hit(feed, id, label, detail, pos, filter, seen, out);
                }
            }
        }
        FeedItems::Cyclones(cyclones) => {
            for cyclone in cyclones {
                let label = format!("{} ({})", cyclone.name, cyclone.classification);
                let detail = cyclone.intensity_kt.map(|kt| format!("{kt:.0} kt"));
                push_hit(
                    feed,
                    cyclone.id.clone(),
                    label,
                    detail,
                    cyclone.position,
                    filter,
                    seen,
                    out,
                );
            }
        }
        FeedItems::Launches(launches) => {
            for launch in launches {
                if let Some(pad) = &launch.pad
                    && let Ok(pos) = GeoPoint::new(pad.latitude, pad.longitude)
                {
                    push_hit(
                        feed,
                        launch.id.clone(),
                        launch.name.clone(),
                        launch.provider.clone(),
                        pos,
                        filter,
                        seen,
                        out,
                    );
                }
            }
        }
        FeedItems::Perimeters(perimeters) => {
            for perimeter in perimeters {
                let label = perimeter.name.clone().unwrap_or_else(|| perimeter.id.clone());
                let detail = perimeter.acres.map(|a| format!("{a:.0} acres"));
                push_hit(
                    feed,
                    perimeter.id.clone(),
                    label,
                    detail,
                    perimeter.anchor,
                    filter,
                    seen,
                    out,
                );
            }
        }
        FeedItems::Aircraft(_)
        | FeedItems::Vessels(_)
        | FeedItems::Transit(_)
        | FeedItems::Bikeshare(_)
        | FeedItems::Satellites(_) => {}
    }
}
