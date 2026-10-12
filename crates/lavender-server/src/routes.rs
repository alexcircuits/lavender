//! HTTP handlers. Each one parses and validates input into domain types, calls exactly one
//! `LavenderCore` method and serialises the result; there is no business logic here.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use axum::Json;
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use lavender_domain::text::clean;
use lavender_domain::{
    BBox, Catalog, ErrorCode, FeedFilters, FeedId, FeedQuery, GeoPoint, SatelliteTrack,
    SatelliteTrackRequest, SearchQuery, SearchResults, SystemStatus, Timestamp,
};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::AppState;
use crate::error::HttpError;
use crate::security::ClientIp;

/// `Query` whose rejection is Lavender's JSON `ApiError` (400), not axum's plain text.
#[derive(Debug)]
pub struct ApiQuery<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequestParts<S> for ApiQuery<T> {
    type Rejection = HttpError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Query::<T>::from_request_parts(parts, state).await {
            Ok(Query(value)) => Ok(Self(value)),
            Err(e) => Err(bad(clean(&e.body_text(), 200))),
        }
    }
}

/// `Path` whose rejection is Lavender's JSON `ApiError`.
#[derive(Debug)]
pub struct ApiPath<T>(pub T);

impl<T: DeserializeOwned + Send, S: Send + Sync> FromRequestParts<S> for ApiPath<T> {
    type Rejection = HttpError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Path::<T>::from_request_parts(parts, state).await {
            Ok(Path(value)) => Ok(Self(value)),
            Err(e) => Err(bad(clean(&e.body_text(), 200))),
        }
    }
}

pub async fn healthz() -> &'static str {
    "ok"
}

pub async fn status(State(state): State<AppState>) -> Json<SystemStatus> {
    Json(state.core.status())
}

pub async fn catalog(State(state): State<AppState>) -> Json<Catalog> {
    Json(state.core.catalog())
}

pub async fn api_not_found() -> HttpError {
    HttpError::new(ErrorCode::NotFound, "There is no such API route.")
}

fn bad(message: impl Into<String>) -> HttpError {
    HttpError::new(ErrorCode::InvalidRequest, message)
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedParams {
    bbox: Option<String>,
    min_magnitude: Option<String>,
    military_only: Option<String>,
    groups: Option<String>,
}

fn parse_query(feed: &str, params: &FeedParams) -> Result<FeedQuery, HttpError> {
    let feed: FeedId = feed
        .parse()
        .map_err(|_| HttpError::new(ErrorCode::NotFound, format!("There is no '{feed}' feed.")))?;
    let mut query = FeedQuery::new(feed);
    if let Some(b) = &params.bbox {
        query.bbox = Some(BBox::parse(b).map_err(|e| bad(e.to_string()))?);
    }
    let min_magnitude = params
        .min_magnitude
        .as_deref()
        .map(|m| {
            m.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| bad("minMagnitude must be a number"))
        })
        .transpose()?;
    let military_only = params
        .military_only
        .as_deref()
        .map(|m| m.parse::<bool>().map_err(|_| bad("militaryOnly must be true or false")))
        .transpose()?;
    let groups = params.groups.as_deref().map(|g| {
        g.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect::<Vec<_>>()
    });
    if groups.as_ref().is_some_and(|g| g.len() > 20) {
        return Err(bad("at most 20 groups"));
    }
    query.filters = FeedFilters { min_magnitude, military_only, groups };
    Ok(query)
}

/// `GET /api/v1/feeds/{feed}` — one snapshot, with a weak `ETag` over the data (not the
/// generation time) so unchanged data revalidates as 304.
///
/// # Errors
/// Invalid parameters (400), unknown feeds (404) and core failures, as `ApiError`.
pub async fn feed(
    State(state): State<AppState>,
    ClientIp(client): ClientIp,
    ApiPath(feed): ApiPath<String>,
    ApiQuery(params): ApiQuery<FeedParams>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let query = parse_query(&feed, &params)?;
    // The regions this snapshot needs count against the client's budget while it waits.
    let snapshot = state.core.snapshot_with_budget(query, state.clients.budget(client)).await?;
    let mut hasher = DefaultHasher::new();
    serde_json::to_string(&(
        &snapshot.items,
        &snapshot.fetched_at,
        &snapshot.freshness,
        &snapshot.coverage,
    ))
    .unwrap_or_default()
    .hash(&mut hasher);
    let etag = format!("W/\"{:016x}\"", hasher.finish());
    let not_modified = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|t| t.trim() == etag));
    let mut response = if not_modified {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        Json(snapshot).into_response()
    };
    let h = response.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&etag) {
        h.insert(header::ETAG, v);
    }
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    Ok(response)
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    q: Option<String>,
    limit: Option<u32>,
    near: Option<String>,
}

/// `GET /api/v1/search?q=…&near=lat,lon&limit=n`
///
/// # Errors
/// Invalid parameters (400), as `ApiError`.
pub async fn search(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<SearchParams>,
) -> Result<Json<SearchResults>, HttpError> {
    let near = params
        .near
        .as_deref()
        .map(|n| {
            let (lat, lon) = n.split_once(',').ok_or_else(|| bad("near must be 'lat,lon'"))?;
            let lat =
                lat.trim().parse::<f64>().map_err(|_| bad("near latitude must be a number"))?;
            let lon =
                lon.trim().parse::<f64>().map_err(|_| bad("near longitude must be a number"))?;
            GeoPoint::new(lat, lon).map_err(|e| bad(e.to_string()))
        })
        .transpose()?;
    let query = SearchQuery { q: params.q.unwrap_or_default(), near, limit: params.limit };
    Ok(Json(state.core.search(&query)?))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackParams {
    minutes: Option<u32>,
    step_s: Option<u32>,
    from: Option<String>,
}

/// `GET /api/v1/satellites/{norad}/track?minutes=&stepS=&from=`
///
/// # Errors
/// Invalid parameters (400), unknown satellites (404) and core failures.
pub async fn satellite_track(
    State(state): State<AppState>,
    ApiPath(norad): ApiPath<String>,
    ApiQuery(params): ApiQuery<TrackParams>,
) -> Result<Json<SatelliteTrack>, HttpError> {
    let norad_id = norad.parse::<u32>().map_err(|_| bad("NORAD id must be a positive integer"))?;
    let from = params
        .from
        .as_deref()
        .map(|f| f.parse::<Timestamp>().map_err(|_| bad("from must be an RFC 3339 timestamp")))
        .transpose()?;
    let req =
        SatelliteTrackRequest { norad_id, from, minutes: params.minutes, step_s: params.step_s };
    Ok(Json(state.core.satellite_track(req).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_feed_parameters() {
        let p = FeedParams {
            bbox: Some("170,-10,-170,10".into()),
            min_magnitude: Some("4.5".into()),
            military_only: None,
            groups: Some("stations, gps-ops,".into()),
        };
        let q = parse_query("earthquakes", &p).unwrap();
        assert!(q.bbox.unwrap().crosses_antimeridian());
        assert_eq!(q.filters.min_magnitude, Some(4.5));
        assert_eq!(q.filters.groups, Some(vec!["stations".into(), "gps-ops".into()]));
        assert_eq!(
            parse_query("ufos", &FeedParams::default()).unwrap_err().0.code,
            ErrorCode::NotFound
        );
        let bad_mag = FeedParams { min_magnitude: Some("NaN".into()), ..FeedParams::default() };
        assert_eq!(
            parse_query("earthquakes", &bad_mag).unwrap_err().0.code,
            ErrorCode::InvalidRequest
        );
        let bad_box = FeedParams { bbox: Some("0,95,1,96".into()), ..FeedParams::default() };
        assert_eq!(
            parse_query("aircraft", &bad_box).unwrap_err().0.code,
            ErrorCode::InvalidRequest
        );
    }
}
