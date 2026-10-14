//! Search: coordinates (no network), live entities in active feeds, and places through a
//! configured geocoder. External lookup requires an explicit place-search request.

pub mod geocoder;

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

fn entities(q: &str, hub: &FeedHub, limit: usize) -> Vec<EntityHit> {
    let needle = q.to_lowercase();
    let mut hits = Vec::new();
    for feed in FeedId::ALL {
        for items in hub.current_items(feed) {
            match items.as_ref() {
                FeedItems::Aircraft(list) => hits.extend(list.iter().filter_map(|a| {
                    let callsign = a.callsign.as_deref().unwrap_or_default().to_lowercase();
                    let reg = a.registration.as_deref().unwrap_or_default().to_lowercase();
                    let matches = a.hex == needle
                        || (!callsign.is_empty() && callsign.starts_with(&needle))
                        || reg == needle;
                    matches.then(|| EntityHit {
                        feed,
                        id: a.hex.clone(),
                        label: a.callsign.clone().unwrap_or_else(|| a.hex.to_uppercase()),
                        detail: Some(
                            [
                                a.type_code.clone(),
                                a.altitude_baro_ft.map(|ft| format!("{ft:.0} ft")),
                            ]
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>()
                            .join(" · "),
                        )
                        .filter(|d| !d.is_empty()),
                        position: Some(a.position),
                    })
                })),
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
