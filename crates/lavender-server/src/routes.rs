//! HTTP handlers. Each one parses and validates input into domain types, calls exactly one
//! `LavenderCore` method and serialises the result; there is no business logic here.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures::StreamExt;
use lavender_core::{
    CoreError, DivisionRecord, InfrastructureItem, LandingPoint, RegionRecord, SubmarineCable,
};
use lavender_domain::text::clean;
use lavender_domain::{
    AirQualitySnapshot, AircraftRouteEnrichment, AircraftTrack, AircraftTypeEnrichment, AlertEvent,
    AlprSnapshot, ApiError, BBox, BoundaryClass, BoundaryFeature, BoundaryResponse, CameraHealth,
    CameraSource, CanonicalEntity, Catalog, CctvStreamInfo, EntityIdentifier, ErrorCode,
    EvidencePack, FeedFilters, FeedId, FeedItems, FeedQuery, FeedSnapshot, GeoPoint, Geofence,
    GeofenceRule, GnssProvenance, GnssSnapshot, LocalReceiverBand, LocalReceiverSnapshot,
    MapCatalogSnapshot, QueryPackDefinition, QueryPackExecution, QueryPackRequest,
    RadioCatalogSnapshot, RadioCategory, SatellitePassRequest, SatellitePasses, SatelliteTrack,
    SatelliteTrackRequest, SavedSearch, SavedSearchMatch, SearchQuery, SearchResults,
    SpatialSearchRequest, SpatialSearchResponse, SystemStatus, Timestamp, VesselTrackResponse,
    WeatherOverlay, WindModel,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

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
    min_frp: Option<String>,
    system: Option<String>,
    feed: Option<String>,
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
    let min_frp = params
        .min_frp
        .as_deref()
        .map(|m| {
            m.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && *v >= 0.0)
                .ok_or_else(|| bad("minFrp must be a non-negative number"))
        })
        .transpose()?;
    let system = params.system.clone();
    let feed_filter = params.feed.clone();
    query.filters =
        FeedFilters { min_magnitude, military_only, groups, min_frp, system, feed: feed_filter };
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
    places: Option<bool>,
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
    let query = SearchQuery {
        q: params.q.unwrap_or_default(),
        near,
        limit: params.limit,
        places: params.places,
    };
    Ok(Json(state.core.search(&query).await?))
}

/// `POST /api/v1/search/spatial` — query live entities within a bounding box, radius circle, polygon or corridor.
///
/// # Errors
/// Returns an error if spatial search parameters are invalid or execution fails.
pub async fn spatial_search(
    State(state): State<AppState>,
    Json(payload): Json<SpatialSearchRequest>,
) -> Result<Json<SpatialSearchResponse>, HttpError> {
    let results = state.core.spatial_search(&payload)?;
    Ok(Json(results))
}

#[derive(Debug, Default, Deserialize)]
pub struct GnssCellsParams {
    pub bbox: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

/// `GET /api/v1/air-quality` — realtime Canadian AQHI station observations.
///
/// # Errors
/// Returns 502/503 when the upstream is unavailable or returns an error.
pub async fn air_quality(
    State(state): State<AppState>,
) -> Result<Json<AirQualitySnapshot>, HttpError> {
    let snapshot = state.core.air_quality_snapshot().await.map_err(HttpError::from)?;
    Ok(Json(snapshot))
}

/// `GET /api/v1/gnss/cells` — evaluates aircraft navigation accuracy anomaly cells.
///
/// # Errors
/// Returns 400 for invalid query coordinates or bounding box.
pub async fn gnss_cells(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<GnssCellsParams>,
) -> Result<Json<GnssSnapshot>, HttpError> {
    let bbox = if let Some(ref raw_bbox) = params.bbox {
        Some(BBox::parse(raw_bbox).map_err(|e| bad(e.to_string()))?)
    } else if let (Some(lat), Some(lon)) = (params.lat, params.lon) {
        Some(
            BBox::new(lon - 2.0, lat - 2.0, lon + 2.0, lat + 2.0)
                .map_err(|e| bad(e.to_string()))?,
        )
    } else {
        None
    };

    let snapshot = state.core.gnss_snapshot(bbox.as_ref())?;
    Ok(Json(snapshot))
}

/// `GET /api/v1/gnss/provenance` — returns machine-readable classifier specifications.
pub async fn gnss_provenance(State(state): State<AppState>) -> Json<GnssProvenance> {
    Json(state.core.gnss_provenance())
}

/// `POST /api/v1/gnss/ingest` — ingests raw readsb/adsb.lol JSON payload.
///
/// # Errors
/// Returns 400 for malformed payload.
pub async fn gnss_ingest(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<serde_json::Value>, HttpError> {
    let count = state.core.ingest_gnss_readsb(&body).map_err(|e| bad(e.to_string()))?;
    Ok(Json(serde_json::json!({ "ingested": count })))
}

/// `GET /api/gnss-integrity` — legacy parity endpoint for GNSS integrity queries.
///
/// # Errors
/// Returns 400 for invalid query parameters.
pub async fn legacy_gnss_integrity(
    state: State<AppState>,
    query: ApiQuery<GnssCellsParams>,
) -> Result<Json<GnssSnapshot>, HttpError> {
    gnss_cells(state, query).await
}

/// History already observed by this core; this endpoint never initiates a provider fetch.
///
/// # Errors
/// Invalid hex (400), missing retained history (404), or unavailable feed.
pub async fn aircraft_track(
    State(state): State<AppState>,
    ApiPath(hex): ApiPath<String>,
) -> Result<Json<AircraftTrack>, HttpError> {
    Ok(Json(state.core.aircraft_track(&hex)?))
}

/// `GET /api/v1/aircraft/{hex}/type` and `GET /api/adsbdb/type/{hex}` — lookup ICAO aircraft type,
/// model name, and registration via cached adsbdb service.
///
/// # Errors
/// Invalid hex (400) or licensing restriction (403).
pub async fn aircraft_type(
    State(state): State<AppState>,
    ApiPath(hex): ApiPath<String>,
) -> Result<Json<AircraftTypeEnrichment>, HttpError> {
    Ok(Json(state.core.aircraft_enrichment_type(&hex).await?))
}

/// `GET /api/v1/aircraft/route/{callsign}` and `GET /api/adsbdb/route/{callsign}` — lookup scheduled
/// route and origin/destination airports via cached adsbdb service.
///
/// # Errors
/// Invalid callsign (400) or licensing restriction (403).
pub async fn aircraft_route(
    State(state): State<AppState>,
    ApiPath(callsign): ApiPath<String>,
) -> Result<Json<AircraftRouteEnrichment>, HttpError> {
    Ok(Json(state.core.aircraft_enrichment_route(&callsign).await?))
}

#[derive(Debug, Deserialize)]
pub struct LegacyFlightsQuery {
    pub lamin: Option<f64>,
    pub lomin: Option<f64>,
    pub lamax: Option<f64>,
    pub lomax: Option<f64>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct LegacyFlightsResponse {
    pub time: i64,
    pub states: Vec<serde_json::Value>,
}

/// `GET /api/flights` — legacy OpenSky/adsb compatible route returning normalized state vectors.
///
/// # Errors
/// Core snapshot failure.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
pub async fn legacy_flights(
    State(state): State<AppState>,
    Query(params): Query<LegacyFlightsQuery>,
) -> Result<Json<LegacyFlightsResponse>, HttpError> {
    let bbox = if let (Some(s), Some(w), Some(n), Some(e)) =
        (params.lamin, params.lomin, params.lamax, params.lomax)
    {
        BBox::new(w, s, e, n).ok()
    } else if let (Some(lat), Some(lon)) = (params.lat, params.lon) {
        BBox::new(
            (lon - 4.0).max(-180.0),
            (lat - 4.0).max(-90.0),
            (lon + 4.0).min(180.0),
            (lat + 4.0).min(90.0),
        )
        .ok()
    } else {
        None
    };

    let mut feed_query = FeedQuery::new(FeedId::Aircraft);
    if let Some(b) = bbox {
        feed_query = feed_query.with_bbox(b);
    }

    let snapshot = state.core.snapshot(feed_query).await?;
    let now_sec = snapshot.generated_at.as_second();
    let mut states = Vec::new();

    if let FeedItems::Aircraft(aircraft) = &snapshot.items {
        for ac in aircraft {
            let alt_m = ac.position.alt_m;
            let baro_m = ac.altitude_baro_ft.map(|ft| ft * 0.3048);
            let geo_m = ac.altitude_geom_ft.map(|ft| ft * 0.3048);
            let speed_mps = ac.ground_speed_kt.map(|kt| kt * 0.514_444);
            let vert_mps = ac.vertical_rate_fpm.map(|fpm| fpm * 0.005_08);
            let pos_time = ((now_sec as f64) - ac.seen_pos_s).max(0.0) as i64;

            let row = serde_json::json!([
                ac.hex,
                ac.callsign,
                serde_json::Value::Null,
                pos_time,
                pos_time,
                ac.position.lon,
                ac.position.lat,
                baro_m.or(alt_m),
                ac.on_ground,
                speed_mps,
                ac.track_deg,
                vert_mps,
                serde_json::Value::Null,
                geo_m.or(alt_m),
                ac.squawk,
                false,
                0,
                ac.category
            ]);
            states.push(row);
        }
    }

    Ok(Json(LegacyFlightsResponse { time: now_sec, states }))
}

#[derive(Debug, Deserialize)]
pub struct LegacyTrackQuery {
    pub icao24: Option<String>,
    pub hex: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LegacyTrackResponse {
    pub path: Vec<serde_json::Value>,
}

/// `GET /api/flights/track` and `GET /api/military/track` — legacy track routes.
///
/// # Errors
/// Missing parameter (400) or missing history (404).
pub async fn legacy_flights_track(
    State(state): State<AppState>,
    Query(params): Query<LegacyTrackQuery>,
) -> Result<Json<LegacyTrackResponse>, HttpError> {
    let hex = params.icao24.or(params.hex).ok_or_else(|| {
        HttpError::from(CoreError::BadRequest("Missing icao24 or hex query parameter".to_owned()))
    })?;

    let track = state.core.aircraft_track(&hex)?;
    let mut path = Vec::new();
    for p in track.points {
        let alt_m = p.position.alt_m.unwrap_or(0.0);
        path.push(serde_json::json!([
            p.time.as_second(),
            p.position.lat,
            p.position.lon,
            alt_m,
            0,
            false
        ]));
    }
    Ok(Json(LegacyTrackResponse { path }))
}

/// `GET /api/v1/vessels/{mmsi}/track` — the positions this core has accumulated from the
/// live stream; never initiates an upstream request.
///
/// # Errors
/// An MMSI that is not nine digits (400), an unavailable feed, or no retained track (404).
pub async fn vessel_track(
    State(state): State<AppState>,
    ApiPath(mmsi): ApiPath<String>,
) -> Result<Json<VesselTrackResponse>, HttpError> {
    Ok(Json(state.core.vessel_track(&mmsi)?))
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PassParams {
    observer: String,
    alt_m: Option<f64>,
    from: Option<Timestamp>,
    hours: Option<u32>,
    min_elevation_deg: Option<f64>,
}

/// `GET /api/v1/satellites/{norad}/passes?observer=lat,lon&hours=24`
///
/// # Errors
/// Invalid parameters (400), missing elements (404), busy computation (429), or upstream failure.
pub async fn satellite_passes(
    State(state): State<AppState>,
    ApiPath(norad): ApiPath<u32>,
    ApiQuery(params): ApiQuery<PassParams>,
) -> Result<Json<SatellitePasses>, HttpError> {
    let (lat, lon) =
        params.observer.split_once(',').ok_or_else(|| bad("observer must be 'lat,lon'"))?;
    let latitude =
        lat.trim().parse::<f64>().map_err(|_| bad("observer latitude must be a number"))?;
    let longitude =
        lon.trim().parse::<f64>().map_err(|_| bad("observer longitude must be a number"))?;
    let observer = GeoPoint::new(latitude, longitude)
        .and_then(|point| point.with_altitude(params.alt_m))
        .map_err(|error| bad(error.to_string()))?;
    let request = SatellitePassRequest {
        norad_id: norad,
        observer,
        from: params.from,
        hours: params.hours,
        min_elevation_deg: params.min_elevation_deg,
    };
    Ok(Json(state.core.satellite_passes(request).await?))
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct WindParams {
    pub model: Option<String>,
    pub overlay: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct WindErrorEnvelope {
    error: &'static str,
    code: ErrorCode,
    message: &'static str,
    retryable: bool,
}

fn wind_error(status: StatusCode, code: ErrorCode, key: &'static str) -> Response {
    (status, Json(WindErrorEnvelope { error: key, code, message: key, retryable: false }))
        .into_response()
}

fn parse_wind_model(model: Option<&str>) -> Result<WindModel, &'static str> {
    match model.unwrap_or("gfs") {
        "gfs" => Ok(WindModel::Gfs),
        "ifs" => Ok(WindModel::Ifs),
        _ => Err("unknown_model"),
    }
}

fn parse_weather_overlay(overlay: Option<&str>) -> Result<WeatherOverlay, &'static str> {
    match overlay.unwrap_or("none") {
        "none" => Ok(WeatherOverlay::None),
        "temperature" => Ok(WeatherOverlay::Temperature),
        "pressure" => Ok(WeatherOverlay::Pressure),
        _ => Err("unknown_overlay"),
    }
}

/// `GET /api/v1/wind` and `GET /api/v1/wind/manifest`
///
/// # Errors
/// Returns a transport error when the core cannot serve this forecast variant.
pub async fn wind_manifest(
    State(state): State<AppState>,
    Query(params): Query<WindParams>,
) -> Result<Response, HttpError> {
    let model = match parse_wind_model(params.model.as_deref()) {
        Ok(m) => m,
        Err(key) => {
            return Ok(wind_error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest, key));
        }
    };
    let overlay = match parse_weather_overlay(params.overlay.as_deref()) {
        Ok(o) => o,
        Err(key) => {
            return Ok(wind_error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest, key));
        }
    };
    let manifest = state.core.wind_manifest(model, overlay).await?;
    let mut resp = Json(manifest).into_response();
    resp.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(resp)
}

/// `GET /api/v1/wind/status`
///
/// # Errors
/// Returns a transport error when the core cannot serve this forecast variant.
pub async fn wind_status(
    State(state): State<AppState>,
    Query(params): Query<WindParams>,
) -> Result<Response, HttpError> {
    let model = match parse_wind_model(params.model.as_deref()) {
        Ok(m) => m,
        Err(key) => {
            return Ok(wind_error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest, key));
        }
    };
    let overlay = match parse_weather_overlay(params.overlay.as_deref()) {
        Ok(o) => o,
        Err(key) => {
            return Ok(wind_error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest, key));
        }
    };
    let status = state.core.wind_status(model, overlay).await?;
    let mut resp = Json(status).into_response();
    resp.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(resp)
}

/// `GET /api/v1/wind/grid/{id}`: little-endian f32 blocks U, V and the optional scalar
/// (layout on `lavender_domain::WindGridDef`).
///
/// # Errors
/// Returns a transport error when the core cannot serve this forecast variant.
pub async fn wind_grid(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<WindParams>,
) -> Result<Response, HttpError> {
    let model = match parse_wind_model(params.model.as_deref()) {
        Ok(m) => m,
        Err(key) => {
            return Ok(wind_error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest, key));
        }
    };
    let overlay = match parse_weather_overlay(params.overlay.as_deref()) {
        Ok(o) => o,
        Err(key) => {
            return Ok(wind_error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest, key));
        }
    };
    let bytes = match state.core.wind_grid(model, overlay, &id).await {
        Ok(b) => b,
        Err(CoreError::NotFound(_)) => {
            return Ok(wind_error(StatusCode::NOT_FOUND, ErrorCode::NotFound, "unknown_grid"));
        }
        Err(e) => return Err(HttpError::from(e)),
    };
    let mut resp = bytes.into_response();
    resp.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=3600, immutable"));
    Ok(resp)
}

#[derive(Debug, Deserialize)]
pub struct RegionAtParams {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Default, Deserialize)]
pub struct RegionSearchParams {
    pub q: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InfrastructureParams {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
    pub zoom: Option<u8>,
    pub limit: Option<usize>,
}

/// `GET /api/v1/regions/at?lat&lon` — geographic region containing the given coordinate.
///
/// # Errors
/// Rejection for invalid coordinates.
pub async fn region_at(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<RegionAtParams>,
) -> Result<Json<Option<RegionRecord>>, HttpError> {
    if !params.lat.is_finite() || !(-90.0..=90.0).contains(&params.lat) {
        return Err(bad("lat must be a finite number between -90 and 90"));
    }
    if !params.lon.is_finite() || !(-180.0..=180.0).contains(&params.lon) {
        return Err(bad("lon must be a finite number between -180 and 180"));
    }
    Ok(Json(state.core.region_at(params.lon, params.lat)?))
}

/// `GET /api/v1/regions/search?q` — search geographic regions by name or query string.
///
/// # Errors
/// Bad request errors.
pub async fn search_regions(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<RegionSearchParams>,
) -> Result<Json<Vec<RegionRecord>>, HttpError> {
    let q = params.q.unwrap_or_default();
    Ok(Json(state.core.search_regions(&q)?))
}

/// `GET /api/v1/divisions/at?lat&lon` — admin-1 divisions containing the
/// coordinate, most specific first.
///
/// # Errors
/// Rejection for invalid coordinates.
pub async fn divisions_at(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<RegionAtParams>,
) -> Result<Json<Vec<DivisionRecord>>, HttpError> {
    if !params.lat.is_finite() || !(-90.0..=90.0).contains(&params.lat) {
        return Err(bad("lat must be a finite number between -90 and 90"));
    }
    if !params.lon.is_finite() || !(-180.0..=180.0).contains(&params.lon) {
        return Err(bad("lon must be a finite number between -180 and 180"));
    }
    Ok(Json(state.core.divisions_at(params.lon, params.lat)?))
}

/// `GET /api/v1/divisions/search?q` — admin-1 search by name, country or ISO.
///
/// # Errors
/// Bad request errors.
pub async fn divisions_search(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<RegionSearchParams>,
) -> Result<Json<Vec<DivisionRecord>>, HttpError> {
    let q = params.q.unwrap_or_default();
    Ok(Json(state.core.divisions_search(&q)?))
}

/// `GET /api/v1/infrastructure?west&south&east&north&zoom` — viewport query for static facilities.
///
/// # Errors
/// Rejection for invalid bounding box coordinates.
pub async fn infrastructure(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<InfrastructureParams>,
) -> Result<Json<Vec<InfrastructureItem>>, HttpError> {
    let bbox = BBox::new(params.west, params.south, params.east, params.north)
        .map_err(|e| bad(e.to_string()))?;
    let zoom = params.zoom.unwrap_or(10);
    let limit = params.limit.unwrap_or(500);
    Ok(Json(state.core.infrastructure_query(&bbox, zoom, limit)?))
}

/// `GET /api/v1/infrastructure/cables?west&south&east&north&zoom` — viewport query for submarine cables.
///
/// # Errors
/// Rejection for invalid bounding box coordinates.
pub async fn infrastructure_cables(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<InfrastructureParams>,
) -> Result<Json<Vec<SubmarineCable>>, HttpError> {
    let bbox = BBox::new(params.west, params.south, params.east, params.north)
        .map_err(|e| bad(e.to_string()))?;
    let zoom = params.zoom.unwrap_or(10);
    let limit = params.limit.unwrap_or(500);
    Ok(Json(state.core.cables_query(&bbox, zoom, limit)?))
}

/// `GET /api/v1/infrastructure/cables/{id}` — single submarine cable by ID.
///
/// # Errors
/// Not found error if the cable does not exist.
pub async fn infrastructure_cable_by_id(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<String>,
) -> Result<Json<SubmarineCable>, HttpError> {
    let cable = state.core.cable_by_id(&id)?.ok_or_else(|| {
        HttpError::new(ErrorCode::NotFound, format!("Submarine cable '{id}' not found."))
    })?;
    Ok(Json(cable))
}

/// `GET /api/v1/infrastructure/landings?west&south&east&north&zoom` — viewport query for cable landing points.
///
/// # Errors
/// Rejection for invalid bounding box coordinates.
pub async fn infrastructure_landings(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<InfrastructureParams>,
) -> Result<Json<Vec<LandingPoint>>, HttpError> {
    let bbox = BBox::new(params.west, params.south, params.east, params.north)
        .map_err(|e| bad(e.to_string()))?;
    let zoom = params.zoom.unwrap_or(10);
    let limit = params.limit.unwrap_or(500);
    Ok(Json(state.core.landings_query(&bbox, zoom, limit)?))
}

#[derive(Debug, Deserialize)]
pub struct BoundariesParams {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
    pub classes: Option<String>,
    pub zoom: Option<u8>,
    pub limit: Option<usize>,
}

/// `GET /api/v1/boundaries?west&south&east&north&classes&zoom&limit` — query boundary features in viewport.
///
/// # Errors
/// Rejection for invalid bounding box coordinates.
pub async fn boundaries(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<BoundariesParams>,
) -> Result<Json<BoundaryResponse>, HttpError> {
    let bbox = BBox::new(params.west, params.south, params.east, params.north)
        .map_err(|e| bad(e.to_string()))?;
    let classes: Option<Vec<BoundaryClass>> = params.classes.as_deref().map(|c| {
        c.split(',')
            .filter_map(|s| match s.trim() {
                "internationalLand" | "international_land" => {
                    Some(BoundaryClass::InternationalLand)
                }
                "disputedLine" | "disputed_line" => Some(BoundaryClass::DisputedLine),
                "breakawayArea" | "breakaway_area" => Some(BoundaryClass::BreakawayArea),
                "maritimeIndicative" | "maritime_indicative" => {
                    Some(BoundaryClass::MaritimeIndicative)
                }
                _ => None,
            })
            .collect()
    });

    let features =
        state.core.boundaries_query(&bbox, classes.as_deref(), params.zoom, params.limit)?;
    let total = features.len();

    Ok(Json(BoundaryResponse {
        features,
        total,
        attribution: lavender_core::BOUNDARIES_ATTRIBUTION.to_owned(),
    }))
}

/// `GET /api/v1/boundaries/{id}` — single boundary feature by ID.
///
/// # Errors
/// Returns 404 if the boundary feature does not exist.
pub async fn boundary_by_id(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<String>,
) -> Result<Json<BoundaryFeature>, HttpError> {
    let feature = state.core.boundary_by_id(&id)?.ok_or_else(|| {
        HttpError::new(ErrorCode::NotFound, format!("Boundary feature '{id}' not found."))
    })?;
    Ok(Json(feature))
}

/// `GET /api/v1/overpass/status` — whether an Overpass server is configured.
pub async fn overpass_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "configured": state.core.overpass_is_configured() }))
}

/// `POST /api/v1/overpass` — proxy a validated, bounded Overpass QL query.
///
/// # Errors
/// Returns 400 for queries that fail sanitization, 503 for upstream unavailability.
pub async fn overpass_query(
    State(state): State<AppState>,
    axum::extract::RawForm(body): axum::extract::RawForm,
) -> impl axum::response::IntoResponse {
    use axum::http::{StatusCode, header};
    let raw_body = match std::str::from_utf8(&body) {
        Ok(s) => s.to_owned(),
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                [(header::CONTENT_TYPE, "application/json")],
                r#"{"error":"Request body is not valid UTF-8"}"#.to_owned(),
            );
        }
    };
    match state.core.overpass_query(&raw_body).await {
        Ok(payload) => {
            let status =
                StatusCode::from_u16(payload.status).unwrap_or(StatusCode::SERVICE_UNAVAILABLE);
            (status, [(header::CONTENT_TYPE, "application/json")], payload.body)
        }
        Err(e) => {
            let msg = format!(r#"{{"error":"{e}"}}"#);
            (StatusCode::BAD_REQUEST, [(header::CONTENT_TYPE, "application/json")], msg)
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct MilitaryInstallationParams {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
    pub limit: Option<usize>,
}

/// `GET /api/v1/military-installations?west&south&east&north` — viewport query for military sites.
///
/// # Errors
/// Returns 400 for invalid bounding boxes, 503 when upstream is unavailable.
pub async fn military_installations(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<MilitaryInstallationParams>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let bbox = BBox::new(params.west, params.south, params.east, params.north)
        .map_err(|e| bad(e.to_string()))?;

    // Cap to 10 deg span per the legacy JS constraint
    if (bbox.north - bbox.south) > 10.0 || (bbox.east - bbox.west) > 10.0 {
        return Err(bad("Viewport too large: each dimension must be ≤ 10 degrees"));
    }

    let limit = params.limit.unwrap_or(lavender_core::InstallationStore::default_limit());

    let installations = state.core.military_installations_query(&bbox, limit).await?;

    if !state.core.overpass_is_configured() {
        return Ok(Json(serde_json::json!({
            "code": "OVERPASS_NOT_CONFIGURED",
            "retryable": false,
            "elements": [],
            "status": "unavailable"
        })));
    }

    Ok(Json(serde_json::json!({
        "elements": installations,
        "status": "ready",
        "elementCap": limit
    })))
}

#[derive(Debug, Default, Deserialize)]
pub struct WeatherManifestParams {
    pub product: Option<String>,
}

/// `GET /api/v1/weather/manifest?product` — manifests available observation frames for weather layers.
pub async fn weather_manifest(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<WeatherManifestParams>,
) -> Json<lavender_core::WeatherManifest> {
    let product = params.product.unwrap_or_else(|| "radar".to_string());
    Json(state.core.weather_manifest(&product).await)
}

#[derive(Debug, Deserialize)]
pub struct WeatherTilePath {
    pub product: String,
    pub time: String,
    pub z: u32,
    pub x: u32,
    pub y: String,
}

/// `GET /api/v1/tiles/weather/{product}/{time}/{z}/{x}/{y}` — relays and caches weather WMS tiles.
///
/// # Errors
/// Returns 400 for invalid tile bounds or product, 503 if upstream is unavailable.
pub async fn weather_tile(
    State(state): State<AppState>,
    ApiPath(params): ApiPath<WeatherTilePath>,
) -> Result<impl axum::response::IntoResponse, HttpError> {
    let y_str = params.y.strip_suffix(".png").unwrap_or(&params.y);
    let y = y_str.parse::<u32>().map_err(|_| bad("Invalid y tile coordinate"))?;

    let bytes =
        state.core.weather_tile(&params.product, &params.time, params.z, params.x, y, 256).await?;

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400, immutable"),
    );
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));

    Ok((StatusCode::OK, headers, bytes))
}

#[derive(Debug, Deserialize)]
pub struct ImageryRecentParams {
    pub bbox: String,
    pub days: Option<u32>,
}

/// `GET /api/v1/imagery/recent?bbox&days` — searches NASA CMR for recent HLS imagery and VIIRS overviews.
///
/// # Errors
/// Returns [`HttpError`] if the bounding box is invalid or query parameters cannot be parsed.
pub async fn imagery_recent(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<ImageryRecentParams>,
) -> Result<Json<lavender_core::RecentImageryResponse>, HttpError> {
    let bbox = BBox::parse(&params.bbox).map_err(|e| bad(e.to_string()))?;
    let days = params.days.unwrap_or(lavender_core::DEFAULT_CATALOG_DAYS);
    let response = state.core.imagery_search(&bbox, days).await?;
    Ok(Json(response))
}

#[derive(Debug, Deserialize)]
pub struct ImageryThumbnailProbeParams {
    pub product: String,
    pub day: String,
    pub bbox: String,
}

/// `GET /api/v1/imagery/thumbnail/probe?product&day&bbox` — probes Worldview Snapshots for pixel presence.
///
/// # Errors
/// Returns [`HttpError`] if parameters are invalid or upstream probe request fails.
pub async fn imagery_thumbnail_probe(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<ImageryThumbnailProbeParams>,
) -> Result<Json<lavender_core::ThumbnailProbeResponse>, HttpError> {
    let bbox = BBox::parse(&params.bbox).map_err(|e| bad(e.to_string()))?;
    let response = state.core.imagery_thumbnail_probe(&params.product, &params.day, &bbox).await?;
    Ok(Json(response))
}

#[derive(Debug, Deserialize)]
pub struct ImageryThumbnailParams {
    pub product: String,
    pub day: String,
    pub bbox: String,
    pub size: Option<u32>,
}

/// `GET /api/v1/imagery/thumbnail?product&day&bbox&size` — relays Worldview Snapshots thumbnail.
///
/// # Errors
/// Returns [`HttpError`] if parameters are invalid or image retrieval/validation fails.
pub async fn imagery_thumbnail(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<ImageryThumbnailParams>,
) -> Result<impl axum::response::IntoResponse, HttpError> {
    let bbox = BBox::parse(&params.bbox).map_err(|e| bad(e.to_string()))?;
    let size = params.size.unwrap_or(256);
    let (bytes, acq_time, data_present) =
        state.core.imagery_thumbnail(&params.product, &params.day, &bbox, size).await?;

    let mut headers = HeaderMap::new();
    let content_type = if params.product == "VIIRS" { "image/jpeg" } else { "image/png" };
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        header::HeaderName::from_static("x-data-present"),
        HeaderValue::from_static(if data_present { "true" } else { "false" }),
    );
    if let Some(t) = acq_time
        && let Ok(val) = HeaderValue::from_str(&t)
    {
        headers.insert(header::HeaderName::from_static("x-acquisition-time"), val);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400, immutable"),
    );
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));

    Ok((StatusCode::OK, headers, bytes))
}

#[derive(Debug, Serialize)]
pub struct MapillaryStatus {
    pub configured: bool,
}

/// `GET /api/v1/mapillary/status` — whether a Mapillary token is configured on the server.
pub async fn mapillary_status(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    (
        StatusCode::OK,
        headers,
        Json(MapillaryStatus { configured: state.core.mapillary_is_configured() }),
    )
}

#[derive(Debug, Deserialize)]
pub struct MapillaryTilePath {
    pub z: u32,
    pub x: u32,
    pub y: u32,
}

/// `GET /api/v1/tiles/mapillary/{z}/{x}/{y}` — cached and trimmed Mapillary coverage vector tile.
pub async fn mapillary_tile(
    State(state): State<AppState>,
    ApiPath(params): ApiPath<MapillaryTilePath>,
) -> Response {
    if !state.core.mapillary_is_configured() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "no_key",
                "keyRequired": true
            })),
        )
            .into_response();
    }

    match state.core.mapillary_provider().fetch_tile("coverage", params.z, params.x, params.y).await
    {
        Ok((bytes, source)) => {
            let mut headers = HeaderMap::new();
            headers
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/x-protobuf"));
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=3600"));
            headers.insert(
                header::HeaderName::from_static("x-gev-cache"),
                HeaderValue::from_static(source.as_header_str()),
            );

            if bytes.is_empty() {
                (StatusCode::NO_CONTENT, headers, bytes).into_response()
            } else {
                (StatusCode::OK, headers, bytes).into_response()
            }
        }
        Err(lavender_core::providers::mapillary::MapillaryError::InvalidAddress(msg)) => {
            (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": msg }))).into_response()
        }
        Err(lavender_core::providers::mapillary::MapillaryError::MissingToken) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "no_key",
                "keyRequired": true
            })),
        )
            .into_response(),
        Err(lavender_core::providers::mapillary::MapillaryError::KeyRejected) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Mapillary rejected the access token",
                "keyRejected": true
            })),
        )
            .into_response(),
        Err(lavender_core::providers::mapillary::MapillaryError::RateLimited {
            retry_after_secs,
        }) => {
            let mut headers = HeaderMap::new();
            if let Ok(val) = HeaderValue::from_str(&retry_after_secs.to_string()) {
                headers.insert(header::RETRY_AFTER, val);
            }
            (
                StatusCode::TOO_MANY_REQUESTS,
                headers,
                Json(serde_json::json!({
                    "error": "Mapillary is rate-limiting tile requests",
                    "retryAfter": retry_after_secs
                })),
            )
                .into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({ "error": e.to_string() })))
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct MapillarySearchParams {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
    pub limit: Option<usize>,
}

/// `GET /api/v1/mapillary/search?west&south&east&north&limit` — search street-level photos.
///
/// # Errors
/// Returns [`HttpError`] if parameters or bounding box are invalid, or upstream query fails.
pub async fn mapillary_search(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<MapillarySearchParams>,
) -> Result<Json<lavender_core::providers::mapillary::MapillaryImageSearchResponse>, HttpError> {
    let bbox = BBox::new(params.west, params.south, params.east, params.north)
        .map_err(|e| bad(e.to_string()))?;
    let limit = params.limit.unwrap_or(20);
    let res = state.core.mapillary_search_images(&bbox, limit).await?;
    Ok(Json(res))
}

#[derive(Debug, Deserialize, Default)]
pub struct CctvSourceParams {
    pub west: Option<f64>,
    pub south: Option<f64>,
    pub east: Option<f64>,
    pub north: Option<f64>,
    pub limit: Option<usize>,
}

/// `GET /api/v1/cctv/sources?west&south&east&north&limit` — list registered camera sources.
///
/// # Errors
/// Returns [`HttpError`] if bounding box coordinates are out of range.
pub async fn cctv_sources(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<CctvSourceParams>,
) -> Result<Json<Vec<CameraSource>>, HttpError> {
    let bbox = match (params.west, params.south, params.east, params.north) {
        (Some(w), Some(s), Some(e), Some(n)) => {
            Some(BBox::new(w, s, e, n).map_err(|err| bad(err.to_string()))?)
        }
        _ => None,
    };
    let cameras = state.core.cctv_sources(bbox.as_ref(), params.limit);
    Ok(Json(cameras))
}

/// `GET /api/v1/cctv/health` — per-camera health status report.
pub async fn cctv_health(State(state): State<AppState>) -> Json<Vec<CameraHealth>> {
    Json(state.core.cctv_health())
}

/// `GET /api/v1/cctv/stream/{id}` — stream descriptor for a camera.
///
/// # Errors
/// Returns [`HttpError`] with `ErrorCode::NotFound` if the camera does not exist in the catalog.
pub async fn cctv_stream(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<String>,
) -> Result<Json<CctvStreamInfo>, HttpError> {
    state
        .core
        .cctv_stream_info(&id)
        .map(Json)
        .ok_or_else(|| HttpError::new(ErrorCode::NotFound, "Camera not found"))
}

/// `GET /api/v1/cctv/{id}/detections` — computer vision detections from Mojo MLIR sidecar.
pub async fn cctv_detections(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<String>,
) -> Json<lavender_domain::entities::CctvDetectionsResponse> {
    Json(state.core.cctv_detections(&id).await)
}

/// `GET /api/v1/cctv/frame/{id}` — still frame with SSRF validation and synthetic SVG fallback.
pub async fn cctv_frame(State(state): State<AppState>, ApiPath(id): ApiPath<String>) -> Response {
    let (bytes, content_type, source_kind) = state.core.cctv_frame(&id).await;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-store, must-revalidate")
        .header("X-CCTV-Source", source_kind)
        .body(axum::body::Body::from(bytes))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// `GET /api/v1/cctv/media/{id}` — proxy live video/HLS stream from upstream with client disconnect cancellation.
pub async fn cctv_media(State(state): State<AppState>, ApiPath(id): ApiPath<String>) -> Response {
    let Some(cam) = state.core.cctv_catalog().get_camera(&id) else {
        return HttpError::new(ErrorCode::NotFound, "Camera not found").into_response();
    };

    let is_video = cam.feed_type == "video" || cam.feed_type == "hls" || cam.feed_type == "mjpeg";
    if !is_video {
        return HttpError::new(
            ErrorCode::InvalidRequest,
            "Camera does not publish a live video stream",
        )
        .into_response();
    }

    let Ok(permit) =
        lavender_core::providers::cctv::LIVE_STREAM_SEMAPHORE.clone().try_acquire_owned()
    else {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ApiError {
                code: ErrorCode::UpstreamUnavailable,
                message: "Maximum concurrent camera streams reached. Please try again shortly."
                    .to_owned(),
                retryable: true,
            }),
        )
            .into_response();
    };

    let Ok(client) = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(lavender_core::providers::cctv::CCTV_MEDIA_FETCH_TIMEOUT)
        .build()
    else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let upstream = match client
        .get(&cam.url)
        .header(header::USER_AGENT, lavender_core::providers::cctv::upstream_user_agent(&cam.url))
        .send()
        .await
    {
        Ok(res) => res,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(ApiError {
                    code: ErrorCode::UpstreamUnavailable,
                    message: format!("Upstream media connection failed: {e}"),
                    retryable: true,
                }),
            )
                .into_response();
        }
    };

    if !upstream.status().is_success() {
        return (
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                code: ErrorCode::UpstreamUnavailable,
                message: format!("Upstream camera returned status {}", upstream.status()),
                retryable: true,
            }),
        )
            .into_response();
    }

    let content_type = upstream
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_owned();

    if let Some(cl) = upstream.headers().get(header::CONTENT_LENGTH)
        && let Ok(len) = cl.to_str().unwrap_or("0").parse::<usize>()
        && len > lavender_core::providers::cctv::CCTV_MEDIA_MAX_BODY_BYTES
    {
        return (
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                code: ErrorCode::InvalidRequest,
                message: "Upstream media exceeds maximum size cap".to_owned(),
                retryable: false,
            }),
        )
            .into_response();
    }

    let stream = upstream.bytes_stream().map(move |chunk| {
        let _keep_permit = &permit;
        chunk
    });

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-store")
        .header("X-CCTV-Source", "upstream")
        .body(axum::body::Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[derive(Debug, Deserialize)]
pub struct RouteParams {
    pub profile: Option<String>,
    pub coords: Option<String>,
    pub steps: Option<String>,
}

pub async fn calculate_route(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<RouteParams>,
) -> Response {
    let profile = params.profile.as_deref().unwrap_or("foot");
    let Some(coords) = params.coords.as_deref() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: ErrorCode::InvalidRequest,
                message: "need 2-12 coordinates".to_owned(),
                retryable: false,
            }),
        )
            .into_response();
    };
    let with_steps = matches!(params.steps.as_deref(), Some("1" | "true" | "yes"));

    match state.core.calculate_route(profile, coords, with_steps).await {
        Ok(route) => (StatusCode::OK, Json((*route).clone())).into_response(),
        Err(e) => {
            let (status, code, message, retry_after) = match e {
                lavender_core::providers::routing::RouteError::InvalidProfile => (
                    StatusCode::BAD_REQUEST,
                    ErrorCode::InvalidRequest,
                    "invalid profile".to_owned(),
                    None,
                ),
                lavender_core::providers::routing::RouteError::InvalidCoordinateCount => (
                    StatusCode::BAD_REQUEST,
                    ErrorCode::InvalidRequest,
                    "need 2-12 coordinates".to_owned(),
                    None,
                ),
                lavender_core::providers::routing::RouteError::InvalidCoordinate => (
                    StatusCode::BAD_REQUEST,
                    ErrorCode::InvalidRequest,
                    "invalid coordinate".to_owned(),
                    None,
                ),
                lavender_core::providers::routing::RouteError::LegTooLong => (
                    StatusCode::BAD_REQUEST,
                    ErrorCode::InvalidRequest,
                    "route leg too long".to_owned(),
                    None,
                ),
                lavender_core::providers::routing::RouteError::RouteTooLong => (
                    StatusCode::BAD_REQUEST,
                    ErrorCode::InvalidRequest,
                    "route too long".to_owned(),
                    None,
                ),
                lavender_core::providers::routing::RouteError::NotFound => {
                    (StatusCode::NOT_FOUND, ErrorCode::NotFound, "no route found".to_owned(), None)
                }
                lavender_core::providers::routing::RouteError::Busy { retry_after_s } => (
                    StatusCode::TOO_MANY_REQUESTS,
                    ErrorCode::RateLimited,
                    "routing busy — too many routes at once".to_owned(),
                    Some(retry_after_s),
                ),
                lavender_core::providers::routing::RouteError::UpstreamRateLimited {
                    retry_after_s,
                } => (
                    StatusCode::TOO_MANY_REQUESTS,
                    ErrorCode::RateLimited,
                    "routing service is rate limited".to_owned(),
                    Some(retry_after_s),
                ),
                lavender_core::providers::routing::RouteError::ProxyError(msg) => {
                    (StatusCode::BAD_GATEWAY, ErrorCode::UpstreamUnavailable, msg, None)
                }
            };

            let mut response =
                (status, Json(ApiError { code, message, retryable: retry_after.is_some() }))
                    .into_response();

            if let Some(val) = retry_after.and_then(|s| HeaderValue::from_str(&s.to_string()).ok())
            {
                response.headers_mut().insert(header::RETRY_AFTER, val);
            }

            response
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct BikeshareStationParams {
    pub bbox: Option<String>,
}

pub async fn bikeshare_systems(State(state): State<AppState>) -> impl IntoResponse {
    let systems = state.core.bikeshare_systems();
    (StatusCode::OK, Json(systems))
}

pub async fn bikeshare_stations(
    State(state): State<AppState>,
    Path(system_id): Path<String>,
    ApiQuery(params): ApiQuery<BikeshareStationParams>,
) -> impl IntoResponse {
    let bbox = if let Some(ref raw) = params.bbox {
        match BBox::parse(raw) {
            Ok(b) => Some(b),
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ApiError {
                        code: ErrorCode::InvalidRequest,
                        message: format!("invalid bbox parameter: {e}"),
                        retryable: false,
                    }),
                )
                    .into_response();
            }
        }
    } else {
        None
    };

    match state.core.bikeshare_stations(&system_id, bbox.as_ref()).await {
        Ok(stations) => (StatusCode::OK, Json(stations)).into_response(),
        Err(lavender_core::providers::bikeshare::BikeshareError::NotFound(id)) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                code: ErrorCode::NotFound,
                message: format!("unknown bike-share system '{id}'"),
                retryable: false,
            }),
        )
            .into_response(),
        Err(lavender_core::providers::bikeshare::BikeshareError::UrlParameterNotAllowed) => (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: ErrorCode::InvalidRequest,
                message: "URLs are not accepted; use /api/v1/bikeshare/:system_id".to_string(),
                retryable: false,
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                code: ErrorCode::UpstreamUnavailable,
                message: format!("failed to fetch bike-share stations: {e}"),
                retryable: true,
            }),
        )
            .into_response(),
    }
}

pub async fn legacy_gbfs_reject() -> impl IntoResponse {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            code: ErrorCode::InvalidRequest,
            message: "URLs are not accepted; use /api/v1/bikeshare/:system_id".to_string(),
            retryable: false,
        }),
    )
}

#[derive(Debug, Deserialize, Default)]
pub struct TransitVehicleParams {
    pub bbox: Option<String>,
}

pub async fn transit_feeds(State(state): State<AppState>) -> impl IntoResponse {
    let feeds = state.core.transit_feeds();
    (StatusCode::OK, Json(serde_json::json!({ "feeds": feeds })))
}

pub async fn transit_vehicles(
    State(state): State<AppState>,
    Path(feed_id): Path<String>,
    ApiQuery(params): ApiQuery<TransitVehicleParams>,
) -> impl IntoResponse {
    let bbox = if let Some(ref raw) = params.bbox {
        match BBox::parse(raw) {
            Ok(b) => Some(b),
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ApiError {
                        code: ErrorCode::InvalidRequest,
                        message: format!("invalid bbox parameter: {e}"),
                        retryable: false,
                    }),
                )
                    .into_response();
            }
        }
    } else {
        None
    };

    match state.core.transit_vehicles(&feed_id, bbox.as_ref()).await {
        Ok(vehicles) => (StatusCode::OK, Json(vehicles)).into_response(),
        Err(lavender_core::providers::transit::TransitError::UnknownFeed(id)) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                code: ErrorCode::NotFound,
                message: format!("unknown transit feed '{id}'"),
                retryable: false,
            }),
        )
            .into_response(),
        Err(lavender_core::providers::transit::TransitError::RateLimited {
            feed_id,
            retry_after_s,
        }) => (
            StatusCode::TOO_MANY_REQUESTS,
            [(http::header::RETRY_AFTER, retry_after_s.to_string())],
            Json(ApiError {
                code: ErrorCode::RateLimited,
                message: format!(
                    "transit rate limit reached for '{feed_id}', retry in {retry_after_s}s"
                ),
                retryable: true,
            }),
        )
            .into_response(),
        Err(lavender_core::providers::transit::TransitError::Cooldown {
            feed_id,
            retry_after_s,
        }) => (
            StatusCode::SERVICE_UNAVAILABLE,
            [(http::header::RETRY_AFTER, retry_after_s.to_string())],
            Json(ApiError {
                code: ErrorCode::UpstreamUnavailable,
                message: format!(
                    "transit feed '{feed_id}' is in cooldown, retry in {retry_after_s}s"
                ),
                retryable: true,
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                code: ErrorCode::UpstreamUnavailable,
                message: format!("failed to fetch transit vehicles: {e}"),
                retryable: true,
            }),
        )
            .into_response(),
    }
}

pub async fn transit_trail(
    State(state): State<AppState>,
    Path((feed_id, vehicle_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.core.transit_trail(&feed_id, &vehicle_id) {
        Ok(trail) => (StatusCode::OK, Json(trail)).into_response(),
        Err(lavender_core::providers::transit::TransitError::InvalidIdentifier(id)) => (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: ErrorCode::InvalidRequest,
                message: format!("invalid transit vehicle identifier '{id}'"),
                retryable: false,
            }),
        )
            .into_response(),
        Err(lavender_core::providers::transit::TransitError::UnknownFeed(id)) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                code: ErrorCode::NotFound,
                message: format!("unknown transit feed '{id}'"),
                retryable: false,
            }),
        )
            .into_response(),
        Err(lavender_core::providers::transit::TransitError::TrailNotFound {
            feed_id,
            vehicle_id,
        }) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                code: ErrorCode::NotFound,
                message: format!("no trail found for vehicle '{vehicle_id}' in feed '{feed_id}'"),
                retryable: false,
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: ErrorCode::Internal,
                message: format!("failed to get transit trail: {e}"),
                retryable: false,
            }),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct TerrainQueryParams {
    pub points: Option<String>,
}

pub async fn terrain_heights(
    State(state): State<AppState>,
    Json(payload): Json<lavender_domain::TerrainHeightsRequest>,
) -> impl IntoResponse {
    match state.core.query_terrain_heights(&payload.points).await {
        Ok(resp) => (StatusCode::OK, Json(resp)).into_response(),
        Err(err) => map_terrain_error(err),
    }
}

pub async fn terrain_heights_get(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<TerrainQueryParams>,
) -> impl IntoResponse {
    let raw = match params.points {
        Some(s) if !s.trim().is_empty() => s,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    code: ErrorCode::InvalidRequest,
                    message: "missing or empty 'points' query parameter".to_string(),
                    retryable: false,
                }),
            )
                .into_response();
        }
    };

    let max_points = state.core.config().terrain.max_points;
    let points =
        match lavender_core::providers::terrain::parse_terrain_points_query(&raw, max_points) {
            Ok(pts) => pts,
            Err(err) => return map_terrain_error(err),
        };

    match state.core.query_terrain_heights(&points).await {
        Ok(resp) => (StatusCode::OK, Json(resp)).into_response(),
        Err(err) => map_terrain_error(err),
    }
}

fn map_terrain_error(
    err: lavender_core::providers::terrain::TerrainError,
) -> axum::response::Response {
    use lavender_core::providers::terrain::TerrainError;

    match err {
        TerrainError::EmptyPoints => (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: ErrorCode::InvalidRequest,
                message: "empty points array; at least 1 point required".to_string(),
                retryable: false,
            }),
        )
            .into_response(),
        TerrainError::TooManyPoints { count, max } => (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: ErrorCode::InvalidRequest,
                message: format!("too many points ({count}); max {max} per request"),
                retryable: false,
            }),
        )
            .into_response(),
        TerrainError::InvalidCoordinate { index, reason } => (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: ErrorCode::InvalidRequest,
                message: format!("invalid coordinate at index {index}: {reason}"),
                retryable: false,
            }),
        )
            .into_response(),
        TerrainError::MissingHeight { missing_count } => (
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                code: ErrorCode::UpstreamUnavailable,
                message: format!(
                    "terrain heights fetch failed and no cache available for every point ({missing_count} unresolved)"
                ),
                retryable: true,
            }),
        )
            .into_response(),
        TerrainError::Disabled => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError {
                code: ErrorCode::UpstreamUnavailable,
                message: "terrain heights provider is disabled".to_string(),
                retryable: true,
            }),
        )
            .into_response(),
        TerrainError::Upstream { status, message } => (
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                code: ErrorCode::UpstreamUnavailable,
                message: format!("upstream terrain error (HTTP {status}): {message}"),
                retryable: true,
            }),
        )
            .into_response(),
        TerrainError::Network(msg) => (
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                code: ErrorCode::UpstreamUnavailable,
                message: format!("terrain heights network error: {msg}"),
                retryable: true,
            }),
        )
            .into_response(),
    }
}

/// `GET /api/v1/traffic/status` and `GET /api/tomtom/status`
pub async fn traffic_status(State(state): State<AppState>) -> Json<lavender_domain::TrafficStatus> {
    Json(state.core.traffic_status())
}

/// `GET /api/v1/tiles/traffic/{z}/{x}/{y}`
pub async fn traffic_tile(
    State(state): State<AppState>,
    Path((z, x, y_str)): Path<(u32, u32, String)>,
) -> axum::response::Response {
    let clean_y = y_str.trim_end_matches(".json").trim_end_matches(".pbf");
    let Ok(y) = clean_y.parse::<u32>() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: ErrorCode::InvalidRequest,
                message: format!("invalid tile row: {y_str}"),
                retryable: false,
            }),
        )
            .into_response();
    };

    match state.core.query_traffic_tile(z, x, y).await {
        Ok(res) => Json(res).into_response(),
        Err(err) => map_traffic_error(err),
    }
}

/// `GET /api/tomtom/flow/{z}/{x}/{y}.pbf`
pub async fn traffic_flow_pbf(
    State(state): State<AppState>,
    Path((z, x, y_str)): Path<(u32, u32, String)>,
) -> axum::response::Response {
    let clean_y = y_str.trim_end_matches(".pbf");
    let Ok(y) = clean_y.parse::<u32>() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: ErrorCode::InvalidRequest,
                message: format!("invalid tile row: {y_str}"),
                retryable: false,
            }),
        )
            .into_response();
    };

    match state.core.query_flow_tile(z, x, y).await {
        Ok((bytes, cache_status)) => {
            let mut headers = HeaderMap::new();
            headers
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/x-protobuf"));
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            headers.insert(
                header::HeaderName::from_static("x-tomtom-cache"),
                HeaderValue::from_static(cache_status.as_header_str()),
            );
            (StatusCode::OK, headers, bytes).into_response()
        }
        Err(err) => map_traffic_error(err),
    }
}

fn map_traffic_error(
    err: lavender_core::providers::traffic::TrafficError,
) -> axum::response::Response {
    use lavender_core::providers::traffic::TrafficError;

    match err {
        TrafficError::NoKey => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError {
                code: ErrorCode::Unconfigured,
                message: "TomTom API key is not configured".to_string(),
                retryable: false,
            }),
        )
            .into_response(),
        TrafficError::BudgetExceeded(count) => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ApiError {
                code: ErrorCode::RateLimited,
                message: format!("daily request budget exceeded ({count} requests today)"),
                retryable: true,
            }),
        )
            .into_response(),
        TrafficError::InvalidTile { z, x, y } => (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: ErrorCode::InvalidRequest,
                message: format!("invalid tile coordinate: z={z}, x={x}, y={y}"),
                retryable: false,
            }),
        )
            .into_response(),
        TrafficError::Upstream(msg) | TrafficError::Decode(msg) | TrafficError::Internal(msg) => (
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                code: ErrorCode::UpstreamUnavailable,
                message: format!("traffic upstream error: {msg}"),
                retryable: true,
            }),
        )
            .into_response(),
    }
}

/// `GET /api/v1/secrets/status` — configuration status of upstream secrets.
pub async fn secret_status(
    State(state): State<AppState>,
) -> (StatusCode, axum::Json<serde_json::Value>) {
    let status = state.core.secret_status();
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "managedBy": "operator",
            "status": status,
        })),
    )
}

/// `POST /api/v1/secrets` — reject browser-submitted secrets in web mode.
pub async fn secrets_reject() -> (StatusCode, axum::Json<serde_json::Value>) {
    (
        StatusCode::FORBIDDEN,
        axum::Json(serde_json::json!({
            "error": "Forbidden",
            "message": "Secrets in web server deployment are managed by the operator via environment or vault, not through client HTTP requests.",
        })),
    )
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRuleRequest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub geofences: Vec<Geofence>,
    #[serde(default)]
    pub polygon: Option<Vec<GeoPoint>>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertInboxParams {
    pub limit: Option<usize>,
}

/// `POST /api/v1/alerts/rules` — create or replace a geofence alerting rule.
///
/// # Errors
/// Returns an error if the engine is disabled, rule capacity is exceeded, or parameters are invalid.
pub async fn create_alert_rule(
    State(state): State<AppState>,
    Json(payload): Json<CreateRuleRequest>,
) -> Result<Json<GeofenceRule>, HttpError> {
    let mut geofences = payload.geofences;
    if geofences.is_empty() {
        geofences.extend(payload.polygon.map(|polygon| Geofence {
            id: format!("{}-fence", payload.id),
            name: payload.name.clone(),
            polygon,
        }));
    }
    let rule =
        GeofenceRule { id: payload.id, name: payload.name, geofences, enabled: payload.enabled };
    let created = state.core.create_alert_rule(rule)?;
    Ok(Json(created))
}

/// `DELETE /api/v1/alerts/rules/{id}` — delete a geofence alerting rule.
///
/// # Errors
/// Returns an error if the engine is disabled or the rule was not found.
pub async fn delete_alert_rule(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<String>,
) -> Result<Json<serde_json::Value>, HttpError> {
    state.core.delete_alert_rule(&id)?;
    Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
}

/// `GET /api/v1/alerts/rules/{id}` — fetch a geofence alerting rule by ID.
///
/// # Errors
/// Returns an error if the engine is disabled or the rule was not found.
pub async fn get_alert_rule(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<String>,
) -> Result<Json<GeofenceRule>, HttpError> {
    let rule = state.core.get_alert_rule(&id)?;
    Ok(Json(rule))
}

/// `GET /api/v1/alerts/rules` — list all geofence alerting rules.
///
/// # Errors
/// Returns an error if alerting is disabled in server configuration.
pub async fn list_alert_rules(
    State(state): State<AppState>,
) -> Result<Json<Vec<GeofenceRule>>, HttpError> {
    let rules = state.core.list_alert_rules()?;
    Ok(Json(rules))
}

/// `GET /api/v1/alerts/inbox` — fetch recent alerting events from the ring buffer.
///
/// # Errors
/// Returns an error if alerting is disabled in server configuration.
pub async fn alerts_inbox(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<AlertInboxParams>,
) -> Result<Json<Vec<AlertEvent>>, HttpError> {
    let events = state.core.alert_inbox(params.limit)?;
    Ok(Json(events))
}

/// Query parameters for resolving a canonical entity by identifier scheme and value.
#[derive(Debug, Deserialize)]
pub struct ResolveParams {
    pub scheme: String,
    pub value: String,
}

/// `GET /api/v1/entities/resolve?scheme=...&value=...` — resolve a canonical entity by scheme and identifier.
///
/// # Errors
/// Returns 400 for empty scheme or value, or 404 if no matching canonical entity is found.
pub async fn resolve_entity(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<ResolveParams>,
) -> Result<Json<CanonicalEntity>, HttpError> {
    if params.scheme.trim().is_empty() {
        return Err(bad("scheme query parameter must not be empty"));
    }
    if params.value.trim().is_empty() {
        return Err(bad("value query parameter must not be empty"));
    }
    state.core.resolve_entity(&params.scheme, &params.value).map(Json).ok_or_else(|| {
        HttpError::new(
            ErrorCode::NotFound,
            format!(
                "No canonical entity found for scheme '{}' and value '{}'.",
                params.scheme, params.value
            ),
        )
    })
}

/// `GET /api/v1/entities/{id}` — fetch a canonical entity by its canonical ID.
///
/// # Errors
/// Returns 400 for empty id, or 404 if the canonical entity does not exist.
pub async fn get_canonical_entity(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<String>,
) -> Result<Json<CanonicalEntity>, HttpError> {
    if id.trim().is_empty() {
        return Err(bad("entity id must not be empty"));
    }
    state.core.get_canonical_entity(&id).map(Json).ok_or_else(|| {
        HttpError::new(ErrorCode::NotFound, format!("Canonical entity '{id}' not found."))
    })
}

/// `GET /api/v1/entities/{id}/aliases` — fetch all alias identifiers for a canonical entity.
///
/// # Errors
/// Returns 400 for empty id, or 404 if the canonical entity does not exist.
pub async fn get_entity_aliases(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<String>,
) -> Result<Json<Vec<EntityIdentifier>>, HttpError> {
    if id.trim().is_empty() {
        return Err(bad("entity id must not be empty"));
    }
    let aliases = state.core.get_entity_aliases(&id)?;
    Ok(Json(aliases))
}

/// Query parameters for fetching recent saved search matches.
#[derive(Debug, Deserialize)]
pub struct SavedSearchMatchesParams {
    /// Maximum number of matches to retrieve.
    pub limit: Option<usize>,
}

/// `POST /api/v1/searches/saved` — create or replace a persistent saved search.
///
/// # Errors
/// Returns [`HttpError`] with 400 Bad Request if search fields are malformed or invalid,
/// or 429 Too Many Requests if maximum saved search capacity is exceeded.
pub async fn create_saved_search(
    State(state): State<AppState>,
    Json(search): Json<SavedSearch>,
) -> Result<(StatusCode, Json<SavedSearch>), HttpError> {
    let created = state.core.create_saved_search(search)?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// `GET /api/v1/searches/saved` — list all persistent saved searches.
///
/// # Errors
/// Returns [`HttpError`] with 500 Internal Server Error if the core is shutting down.
pub async fn list_saved_searches(
    State(state): State<AppState>,
) -> Result<Json<Vec<SavedSearch>>, HttpError> {
    let list = state.core.list_saved_searches()?;
    Ok(Json(list))
}

/// `GET /api/v1/searches/saved/{id}` — fetch a saved search by ID.
///
/// # Errors
/// Returns [`HttpError`] with 404 Not Found if the saved search does not exist.
pub async fn get_saved_search(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<String>,
) -> Result<Json<SavedSearch>, HttpError> {
    let search = state.core.get_saved_search(&id)?;
    Ok(Json(search))
}

/// `DELETE /api/v1/searches/saved/{id}` — delete a persistent saved search.
///
/// # Errors
/// Returns [`HttpError`] with 404 Not Found if the saved search does not exist.
pub async fn delete_saved_search(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<String>,
) -> Result<StatusCode, HttpError> {
    state.core.delete_saved_search(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/searches/saved/{id}/feed` — get dynamic virtual feed snapshot for a saved search.
///
/// # Errors
/// Returns [`HttpError`] with 404 Not Found if the saved search does not exist.
pub async fn get_saved_search_feed(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<String>,
) -> Result<Json<FeedSnapshot>, HttpError> {
    let snapshot = state.core.get_saved_search_feed(&id)?;
    Ok(Json(snapshot))
}

/// `GET /api/v1/searches/saved/{id}/matches` — get recent matches for a saved search.
///
/// # Errors
/// Returns [`HttpError`] with 404 Not Found if the saved search does not exist.
pub async fn get_saved_search_matches(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<String>,
    ApiQuery(params): ApiQuery<SavedSearchMatchesParams>,
) -> Result<Json<Vec<SavedSearchMatch>>, HttpError> {
    let matches = state.core.get_saved_search_matches(&id, params.limit)?;
    Ok(Json(matches))
}

/// `GET /api/v1/local-receivers/aircraft` — returns the current local ADS-B (1090 MHz) and UAT (978 MHz) receiver snapshot.
pub async fn local_receivers_aircraft(
    State(state): State<AppState>,
) -> Json<LocalReceiverSnapshot> {
    Json(state.core.local_receivers_snapshot())
}

/// Query parameters for `POST /api/v1/local-receivers/ingest`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalReceiverIngestParams {
    pub feed_id: String,
    pub band: Option<String>,
}

/// `POST /api/v1/local-receivers/ingest` — ingests a `dump1090`/`readsb`/`dump978` `aircraft.json` payload.
///
/// # Errors
/// Returns 400 Bad Request if the band or JSON payload is malformed.
pub async fn local_receivers_ingest(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<LocalReceiverIngestParams>,
    body: Bytes,
) -> Result<Json<serde_json::Value>, HttpError> {
    let band_str = params.band.as_deref().unwrap_or("1090");
    let band =
        LocalReceiverBand::parse(band_str).ok_or_else(|| bad("band must be '1090' or '978'"))?;
    let count = state.core.ingest_local_receiver_payload(&params.feed_id, band, &body)?;
    Ok(Json(serde_json::json!({ "ingested": count })))
}

/// Query parameters for `GET /api/v1/radio/stations`.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RadioStationsParams {
    pub bbox: Option<String>,
    pub category: Option<String>,
    pub limit: Option<usize>,
}

/// `GET /api/v1/radio/stations` — queries georeferenced tactical & broadcast radio stations.
///
/// # Errors
/// Returns 400 Bad Request if `bbox` or `category` is malformed.
pub async fn radio_stations(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<RadioStationsParams>,
) -> Result<Json<RadioCatalogSnapshot>, HttpError> {
    let bbox =
        params.bbox.as_deref().map(BBox::parse).transpose().map_err(|e| bad(e.to_string()))?;
    let category = match params.category.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => None,
        Some(raw) => {
            let parsed: RadioCategory =
                serde_json::from_value(serde_json::Value::String(raw.to_ascii_lowercase()))
                    .map_err(|_| bad(format!("invalid radio category '{raw}'")))?;
            Some(parsed)
        }
    };
    let limit = params.limit.unwrap_or(500).clamp(1, 2000);
    Ok(Json(state.core.radio_stations(bbox.as_ref(), category, limit)))
}

/// Query parameters for `GET /api/v1/alpr`.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlprParams {
    pub bbox: Option<String>,
    pub limit: Option<usize>,
}

/// `GET /api/v1/alpr` — queries automated license plate reader (ALPR) surveillance cameras inside `bbox`.
///
/// # Errors
/// Returns 400 Bad Request if `bbox` is missing, malformed, or spans more than 3 degrees on a side.
pub async fn alpr_cameras(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<AlprParams>,
) -> Result<Json<AlprSnapshot>, HttpError> {
    let bbox = match params.bbox.as_deref() {
        Some(raw) => BBox::parse(raw).map_err(|e| bad(e.to_string()))?,
        None => BBox::new(-180.0, -90.0, 180.0, 90.0).map_err(|e| bad(e.to_string()))?,
    };
    let limit = params.limit.unwrap_or(500).clamp(1, 2000);
    Ok(Json(state.core.alpr_cameras(bbox, limit)))
}

/// `GET /api/v1/scenes/packs` — lists the curated scene evidence packs (`LB-118`).
pub async fn list_evidence_packs(State(state): State<AppState>) -> Json<Vec<EvidencePack>> {
    Json(state.core.evidence_packs())
}

/// `GET /api/v1/maps/catalog` — lists base map stacks with credential-aware availability (`LB-119`).
pub async fn map_catalog(State(state): State<AppState>) -> Json<MapCatalogSnapshot> {
    Json(state.core.map_catalog())
}

/// `GET /api/v1/analytics/query-packs` — lists all registered analyst query packs (`LB-121`).
pub async fn list_query_packs(State(state): State<AppState>) -> Json<Vec<QueryPackDefinition>> {
    Json(state.core.query_packs())
}

/// `POST /api/v1/analytics/query-packs/{id}/execute` — executes a named analyst query pack (`LB-121`).
///
/// # Errors
/// Returns 404 Not Found for an unknown query pack ID, or 400 Bad Request for invalid parameters.
pub async fn execute_query_pack(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<String>,
    Json(request): Json<QueryPackRequest>,
) -> Result<Json<QueryPackExecution>, HttpError> {
    let result = state.core.execute_query_pack(&id, &request)?;
    Ok(Json(result))
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
            min_frp: Some("25.5".into()),
            system: None,
            feed: None,
        };
        let q = parse_query("earthquakes", &p).unwrap();
        assert!(q.bbox.unwrap().crosses_antimeridian());
        assert_eq!(q.filters.min_magnitude, Some(4.5));
        assert_eq!(q.filters.groups, Some(vec!["stations".into(), "gps-ops".into()]));
        assert_eq!(q.filters.min_frp, Some(25.5));
        assert_eq!(
            parse_query("ufos", &FeedParams::default()).unwrap_err().0.code,
            ErrorCode::NotFound
        );
        let bad_mag = FeedParams { min_magnitude: Some("NaN".into()), ..FeedParams::default() };
        assert_eq!(
            parse_query("earthquakes", &bad_mag).unwrap_err().0.code,
            ErrorCode::InvalidRequest
        );
        let bad_frp = FeedParams { min_frp: Some("-5.0".into()), ..FeedParams::default() };
        assert_eq!(parse_query("fires", &bad_frp).unwrap_err().0.code, ErrorCode::InvalidRequest);
        let bad_box = FeedParams { bbox: Some("0,95,1,96".into()), ..FeedParams::default() };
        assert_eq!(
            parse_query("aircraft", &bad_box).unwrap_err().0.code,
            ErrorCode::InvalidRequest
        );
    }
}
