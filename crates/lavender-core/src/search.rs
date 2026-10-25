//! Search: coordinates (no network), live entities in active feeds, and places through a
//! configured geocoder. External lookup requires an explicit place-search request.

pub mod geocoder;
pub mod spatial;

pub use spatial::spatial_search;

use lavender_domain::{CoordinateHit, EntityHit, FeedId, FeedItems, SearchQuery, SearchResults};
use lavender_geo::{format_decimal, parse_coordinates};
use tokio_util::sync::CancellationToken;

use crate::error::CoreError;
use crate::hub::FeedHub;
use geocoder::{Geocoder, GeocoderError};

const MAX_QUERY_CHARS: usize = 128;

/// Runs a search against the parser and the hub's current data.
///
/// # Errors
/// Returns an error for empty or oversized queries.
pub fn search(query: &SearchQuery, hub: &FeedHub) -> Result<SearchResults, CoreError> {
    let mut results = search_local(query, hub)?;
    if query.places == Some(true) && results.coordinates.is_empty() {
        results.notices.push("Place search is not configured on this server.".to_owned());
    }
    Ok(results)
}

/// Adds places only on explicit submission. Coordinates and loaded-entity search
/// remain useful even while an upstream is unavailable or disabled.
///
/// # Errors
/// Returns an invalid query or cancellation; upstream failures become result notices.
pub async fn search_with_geocoder(
    query: &SearchQuery,
    hub: &FeedHub,
    geocoder: Option<&dyn Geocoder>,
    cancel: CancellationToken,
) -> Result<SearchResults, CoreError> {
    let mut results = search_local(query, hub)?;
    if query.places != Some(true) || !results.coordinates.is_empty() {
        return Ok(results);
    }
    let Some(geocoder) = geocoder else {
        results.notices.push("Place search is not configured on this server.".to_owned());
        return Ok(results);
    };
    match geocoder.geocode(query, cancel).await {
        Ok(places) => {
            results.places = places.places;
            results.notices.extend(places.notices);
            let source = geocoder.source_info();
            results.notices.push(format!("{} · {}", source.attribution, source.licence.url));
        }
        Err(GeocoderError::Cancelled) => {
            return Err(CoreError::Upstream(GeocoderError::Cancelled.to_string()));
        }
        Err(error) => results.notices.push(error.to_string()),
    }
    Ok(results)
}

fn search_local(query: &SearchQuery, hub: &FeedHub) -> Result<SearchResults, CoreError> {
    let q = query.q.trim();
    if q.is_empty() || q.chars().count() > MAX_QUERY_CHARS {
        return Err(CoreError::BadRequest(format!(
            "search text must be 1–{MAX_QUERY_CHARS} characters"
        )));
    }
    let limit = usize::try_from(query.limit.unwrap_or(8).clamp(1, 25)).unwrap_or(8);
    if let Some(near) = query.near {
        lavender_domain::GeoPoint::new(near.lat, near.lon).map_err(|_| {
            CoreError::BadRequest("search centre must be a valid position".to_owned())
        })?;
    }
    let mut results = SearchResults::default();

    if let Some(parsed) = parse_coordinates(q) {
        results.coordinates.push(CoordinateHit {
            position: parsed.point,
            format: parsed.format.as_str().to_owned(),
            label: format_decimal(&parsed.point),
        });
    }

    if q.chars().count() >= 2 {
        results.entities = entities(q, hub, limit);
    }
    Ok(results)
}

#[allow(clippy::too_many_lines)]
fn entities(q: &str, hub: &FeedHub, limit: usize) -> Vec<EntityHit> {
    let needle = q.to_lowercase();
    let mut hits = Vec::new();
    for feed in FeedId::ALL {
        for items in hub.current_items(feed) {
            match items.as_ref() {
                FeedItems::Aircraft(list) => {
                    hits.extend(list.iter().filter_map(|a| aircraft_hit(a, &needle, feed)));
                }
                FeedItems::Satellites(list) => hits.extend(list.iter().filter_map(|s| {
                    let matches =
                        s.norad_id.to_string() == needle || s.name.to_lowercase().contains(&needle);
                    matches.then(|| EntityHit {
                        feed,
                        id: s.norad_id.to_string(),
                        label: s.name.clone(),
                        detail: Some(format!("NORAD {} · {}", s.norad_id, s.group)),
                        position: None,
                    })
                })),
                FeedItems::Cyclones(list) => hits.extend(list.iter().filter_map(|c| {
                    let matches = c.id == needle || c.name.to_lowercase().contains(&needle);
                    matches.then(|| EntityHit {
                        feed,
                        id: c.id.clone(),
                        label: c.name.clone(),
                        detail: Some(format!(
                            "{} · {} · advisory {}",
                            c.basin, c.classification, c.advisory_number
                        )),
                        position: Some(c.position),
                    })
                })),
                FeedItems::Earthquakes(list) => hits.extend(list.iter().filter_map(|e| {
                    let place = e.place.as_deref().unwrap_or_default().to_lowercase();
                    let matches = e.id.to_lowercase() == needle
                        || (needle.len() >= 3 && place.contains(&needle));
                    matches.then(|| EntityHit {
                        feed,
                        id: e.id.clone(),
                        label: format!(
                            "M {:.1} — {}",
                            e.magnitude,
                            e.place.as_deref().unwrap_or("Unnamed location")
                        ),
                        detail: Some(e.time.to_string()),
                        position: Some(e.position),
                    })
                })),
                FeedItems::Fires(_) => {}
                FeedItems::Launches(list) => hits.extend(list.iter().filter_map(|l| {
                    let matches = l.id.to_lowercase() == needle
                        || l.name.to_lowercase().contains(&needle)
                        || l.mission.as_deref().is_some_and(|m| m.to_lowercase().contains(&needle));
                    matches.then(|| EntityHit {
                        feed,
                        id: l.id.clone(),
                        label: l.name.clone(),
                        detail: l.provider.clone(),
                        position: l.pad.as_ref().and_then(|p| {
                            lavender_domain::GeoPoint::new(p.latitude, p.longitude).ok()
                        }),
                    })
                })),
                FeedItems::Perimeters(list) => {
                    hits.extend(list.iter().filter_map(|p| perimeter_hit(p, &needle, feed)));
                }
                FeedItems::Vessels(list) => hits.extend(list.iter().filter_map(|v| {
                    let name = v.name.as_deref().unwrap_or_default().to_lowercase();
                    let matches = v.mmsi == needle
                        || v.mmsi.starts_with(&needle)
                        || (!name.is_empty() && name.contains(&needle));
                    matches.then(|| EntityHit {
                        feed,
                        id: v.mmsi.clone(),
                        label: v.name.clone().unwrap_or_else(|| format!("MMSI {}", v.mmsi)),
                        detail: v.vessel_type.clone(),
                        position: Some(v.position),
                    })
                })),
                FeedItems::Bikeshare(list) => hits.extend(list.iter().filter_map(|s| {
                    let matches = s.id == needle
                        || s.station_id == needle
                        || s.name.to_lowercase().contains(&needle);
                    matches.then(|| EntityHit {
                        feed,
                        id: s.id.clone(),
                        label: s.name.clone(),
                        detail: Some(format!(
                            "{} · {} bikes",
                            s.system_id,
                            s.bikes_available.unwrap_or(0)
                        )),
                        position: Some(s.position),
                    })
                })),
                FeedItems::Transit(list) => hits.extend(list.iter().filter_map(|v| {
                    let matches = v.id == needle
                        || v.vehicle_id == needle
                        || v.label.as_deref().is_some_and(|l| l.to_lowercase().contains(&needle))
                        || v.route_id
                            .as_deref()
                            .is_some_and(|r| r.to_lowercase().contains(&needle));
                    matches.then(|| EntityHit {
                        feed,
                        id: v.id.clone(),
                        label: v
                            .label
                            .clone()
                            .unwrap_or_else(|| format!("{}:{}", v.feed_id, v.vehicle_id)),
                        detail: Some(format!(
                            "{} · {} · route {}",
                            v.feed_id,
                            v.mode,
                            v.route_id.as_deref().unwrap_or("unknown")
                        )),
                        position: Some(v.position),
                    })
                })),
            }
            if hits.len() >= limit * 3 {
                break;
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    hits.retain(|h| seen.insert((h.feed, h.id.clone())));
    hits.truncate(limit);
    hits
}

/// Every detail level of a perimeter shares its facts; only the full-detail copy is searched.
fn perimeter_hit(
    perimeter: &lavender_domain::FirePerimeter,
    needle: &str,
    feed: FeedId,
) -> Option<EntityHit> {
    let name = perimeter.name.as_deref().unwrap_or_default().to_lowercase();
    let matches = perimeter.detail.level == 0
        && (perimeter.id.to_lowercase() == needle || (needle.len() >= 3 && name.contains(needle)));
    matches.then(|| EntityHit {
        feed,
        id: perimeter.id.clone(),
        label: perimeter.name.clone().unwrap_or_else(|| perimeter.id.clone()),
        detail: Some(
            [perimeter.state.clone(), perimeter.acres.map(|a| format!("{a:.0} acres"))]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · "),
        )
        .filter(|detail| !detail.is_empty()),
        position: Some(perimeter.anchor),
    })
}

fn aircraft_hit(
    aircraft: &lavender_domain::Aircraft,
    needle: &str,
    feed: FeedId,
) -> Option<EntityHit> {
    let callsign = aircraft.callsign.as_deref().unwrap_or_default().to_lowercase();
    let registration = aircraft.registration.as_deref().unwrap_or_default().to_lowercase();
    let matches = aircraft.hex == needle
        || (!callsign.is_empty() && callsign.starts_with(needle))
        || registration == needle;
    matches.then(|| EntityHit {
        feed,
        id: aircraft.hex.clone(),
        label: aircraft.callsign.clone().unwrap_or_else(|| aircraft.hex.to_uppercase()),
        detail: Some(
            [aircraft.type_code.clone(), aircraft.altitude_baro_ft.map(|ft| format!("{ft:.0} ft"))]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · "),
        )
        .filter(|detail| !detail.is_empty()),
        position: Some(aircraft.position),
    })
}
