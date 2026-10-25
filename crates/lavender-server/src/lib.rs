//! Lavender's web backend: an Axum adapter around `lavender-core` (ADR 0002). Routing,
//! WebSocket framing, static hosting and request-level security live here; everything
//! else is a call into `LavenderCore`.

pub mod clients;
pub mod config;
pub mod error;
pub mod routes;
pub mod security;
pub mod ws;

use std::any::Any;
use std::path::Path;
use std::time::Duration;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{any, delete, get, post};
use lavender_core::LavenderCore;
use lavender_domain::ErrorCode;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use crate::clients::{ClientLimits, Clients};
use crate::config::ServerSettings;
use crate::error::{code_for_status, json_error};
use crate::security::{HostPolicy, RateLimiter, content_security_policy};
use crate::ws::StreamLimits;

/// Shared request state.
#[derive(Debug, Clone)]
pub struct AppState {
    pub core: LavenderCore,
    pub max_subscriptions: usize,
    pub clients: Clients,
    pub stream: StreamLimits,
}

/// Cache policy for static files: content-hashed bundles forever, everything else
/// revalidated (so a deploy is picked up on the next load).
async fn static_cache_headers(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_owned();
    let mut response = next.run(request).await;
    let value = if path.starts_with("/assets/") {
        "public, max-age=31536000, immutable"
    } else if path.starts_with("/cesium/") || path.starts_with("/fonts/") {
        "public, max-age=86400"
    } else {
        "no-cache"
    };
    if response.status().is_success() {
        response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static(value));
    }
    response
}

/// Bounds every request's handling time; a timeout answers with the JSON envelope.
async fn request_timeout(State(limit): State<Duration>, request: Request, next: Next) -> Response {
    match tokio::time::timeout(limit, next.run(request)).await {
        Ok(response) => response,
        Err(_) => json_error(
            StatusCode::GATEWAY_TIMEOUT,
            ErrorCode::UpstreamUnavailable,
            "The request took too long. Try again shortly.",
        ),
    }
}

/// Every `/api` error leaves as Lavender's JSON `ApiError` envelope, whatever produced
/// it (method mismatches, body limits, caught panics). Handler errors already are.
async fn api_error_envelope(request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let is_api = path == "/api" || path.starts_with("/api/");
    let response = next.run(request).await;
    let status = response.status();
    let is_error = status.is_client_error() || status.is_server_error();
    let is_json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes().starts_with(b"application/json"));
    if !is_api || !is_error || is_json {
        return response;
    }
    let message = format!(
        "The request could not be served ({}).",
        status.canonical_reason().unwrap_or("error")
    );
    let mut envelope = json_error(status, code_for_status(status), message);
    for name in [header::ALLOW, header::RETRY_AFTER] {
        if let Some(value) = response.headers().get(&name) {
            envelope.headers_mut().insert(name, value.clone());
        }
    }
    envelope
}

/// A panic becomes a JSON 500 that does not repeat the panic message.
// `CatchPanicLayer` hands the payload over by value.
#[allow(clippy::needless_pass_by_value)]
fn panic_response(_err: Box<dyn Any + Send + 'static>) -> Response {
    // Panic payloads may contain credentials or provider documents. The event is
    // useful; repeating its arbitrary payload in structured diagnostics is not.
    tracing::error!("a request handler panicked");
    json_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        ErrorCode::Internal,
        "An unexpected error occurred.",
    )
}

/// Builds the application router.
#[allow(clippy::too_many_lines)]
pub fn app(core: LavenderCore, settings: &ServerSettings) -> Router {
    let tile_hosts: Vec<String> =
        core.catalog().imagery.iter().flat_map(|i| i.tile_hosts.clone()).collect();
    let csp = HeaderValue::from_str(&content_security_policy(&tile_hosts))
        .unwrap_or(HeaderValue::from_static("default-src 'self'"));
    let limiter = RateLimiter::new(
        settings.rate_limit_per_s,
        settings.rate_limit_burst,
        settings.trusted_proxies.clone(),
    );
    let state = AppState {
        core,
        max_subscriptions: settings.max_subscriptions_per_connection,
        clients: Clients::new(ClientLimits {
            regions: settings.max_regions_per_client,
            connections: settings.max_connections,
            connections_per_client: settings.max_connections_per_client,
        }),
        stream: StreamLimits {
            idle_timeout: Duration::from_secs(settings.ws_idle_timeout_s),
            send_timeout: Duration::from_secs(settings.ws_send_timeout_s),
            messages_per_s: f64::from(settings.ws_messages_per_s),
            message_burst: f64::from(settings.ws_message_burst),
        },
    };

    let api = Router::new()
        .route("/status", get(routes::status))
        .route("/catalog", get(routes::catalog))
        .route("/feeds/{feed}", get(routes::feed))
        .route("/search", get(routes::search))
        .route("/search/spatial", post(routes::spatial_search))
        .route("/aircraft/{hex}/track", get(routes::aircraft_track))
        .route("/aircraft/{hex}/type", get(routes::aircraft_type))
        .route("/aircraft/route/{callsign}", get(routes::aircraft_route))
        .route("/vessels/{mmsi}/track", get(routes::vessel_track))
        .route("/satellites/{norad}/track", get(routes::satellite_track))
        .route("/satellites/{norad}/passes", get(routes::satellite_passes))
        .route("/wind", get(routes::wind_manifest))
        .route("/wind/manifest", get(routes::wind_manifest))
        .route("/wind/status", get(routes::wind_status))
        .route("/wind/grid/{id}", get(routes::wind_grid))
        .route("/regions/at", get(routes::region_at))
        .route("/divisions/at", get(routes::divisions_at))
        .route("/regions/search", get(routes::search_regions))
        .route("/divisions/search", get(routes::divisions_search))
        .route("/infrastructure", get(routes::infrastructure))
        .route("/infrastructure/cables", get(routes::infrastructure_cables))
        .route("/infrastructure/cables/{id}", get(routes::infrastructure_cable_by_id))
        .route("/infrastructure/landings", get(routes::infrastructure_landings))
        .route("/boundaries", get(routes::boundaries))
        .route("/boundaries/{id}", get(routes::boundary_by_id))
        .route("/overpass", post(routes::overpass_query))
        .route("/overpass/status", get(routes::overpass_status))
        .route("/military-installations", get(routes::military_installations))
        .route("/weather/manifest", get(routes::weather_manifest))
        .route("/tiles/weather/{product}/{time}/{z}/{x}/{y}", get(routes::weather_tile))
        .route("/imagery/recent", get(routes::imagery_recent))
        .route("/imagery/thumbnail/probe", get(routes::imagery_thumbnail_probe))
        .route("/imagery/thumbnail", get(routes::imagery_thumbnail))
        .route("/mapillary/status", get(routes::mapillary_status))
        .route("/tiles/mapillary/{z}/{x}/{y}", get(routes::mapillary_tile))
        .route("/mapillary/search", get(routes::mapillary_search))
        .route("/cctv/sources", get(routes::cctv_sources))
        .route("/cctv/health", get(routes::cctv_health))
        .route("/cctv/stream/{id}", get(routes::cctv_stream))
        .route("/cctv/{id}/detections", get(routes::cctv_detections))
        .route("/cctv/frame/{id}", get(routes::cctv_frame))
        .route("/cctv/media/{id}", get(routes::cctv_media))
        .route("/route", get(routes::calculate_route))
        .route("/bikeshare/systems", get(routes::bikeshare_systems))
        .route("/bikeshare/{system_id}", get(routes::bikeshare_stations))
        .route("/transit/feeds", get(routes::transit_feeds))
        .route("/transit/{feed_id}/vehicles", get(routes::transit_vehicles))
        .route("/transit/{feed_id}/{vehicle_id}/trail", get(routes::transit_trail))
        .route("/transit/trail/{feed_id}/{vehicle_id}", get(routes::transit_trail))
        .route("/terrain/heights", post(routes::terrain_heights).get(routes::terrain_heights_get))
        .route("/traffic/status", get(routes::traffic_status))
        .route("/tiles/traffic/{z}/{x}/{y}", get(routes::traffic_tile))
        .route("/traffic/flow/{z}/{x}/{y}", get(routes::traffic_flow_pbf))
        .route("/secrets/status", get(routes::secret_status))
        .route("/secrets", post(routes::secrets_reject))
        .route("/alerts/rules", post(routes::create_alert_rule).get(routes::list_alert_rules))
        .route("/alerts/rules/{id}", delete(routes::delete_alert_rule).get(routes::get_alert_rule))
        .route("/alerts/inbox", get(routes::alerts_inbox))
        .route("/gnss/cells", get(routes::gnss_cells))
        .route("/gnss/provenance", get(routes::gnss_provenance))
        .route("/gnss/ingest", post(routes::gnss_ingest))
        .route("/air-quality", get(routes::air_quality))
        .route("/stream", get(ws::stream))
        .fallback(routes::api_not_found)
        .layer(middleware::from_fn(security::require_same_origin))
        .layer(middleware::from_fn_with_state(limiter.clone(), security::rate_limit));

    let static_files = static_service(&settings.static_dir);

    // Layers run outside-in in the reverse of the order they are added: the security
    // headers are added last so that they wrap every response, including host refusals,
    // timeouts and caught panics.
    Router::new()
        .route("/healthz", get(routes::healthz))
        .nest("/api/v1", api.clone())
        .route("/api/overpass", post(routes::overpass_query))
        .route("/api/overpass/status", get(routes::overpass_status))
        .route("/api/boundaries", get(routes::boundaries))
        .route("/api/boundaries/{id}", get(routes::boundary_by_id))
        .route("/api/military-installations", get(routes::military_installations))
        .route("/api/weather/manifest", get(routes::weather_manifest))
        .route("/api/imagery/recent", get(routes::imagery_recent))
        .route("/api/mapillary/status", get(routes::mapillary_status))
        .route("/api/mapillary/tiles/coverage/{z}/{x}/{y}", get(routes::mapillary_tile))
        .route("/api/mapillary/search", get(routes::mapillary_search))
        .route("/api/cctv/sources", get(routes::cctv_sources))
        .route("/api/cctv/health", get(routes::cctv_health))
        .route("/api/cctv/stream/{id}", get(routes::cctv_stream))
        .route("/api/cctv/{id}/detections", get(routes::cctv_detections))
        .route("/api/cctv/frame/{id}", get(routes::cctv_frame))
        .route("/api/cctv/media/{id}", get(routes::cctv_media))
        .route("/api/adsbdb/type/{hex}", get(routes::aircraft_type))
        .route("/api/adsbdb/route/{callsign}", get(routes::aircraft_route))
        .route("/api/flights", get(routes::legacy_flights))
        .route("/api/flights/track", get(routes::legacy_flights_track))
        .route("/api/military/track", get(routes::legacy_flights_track))
        .route("/api/route", get(routes::calculate_route))
        .nest(
            "/api/wind",
            Router::new()
                .route("/", get(routes::wind_manifest))
                .route("/manifest", get(routes::wind_manifest))
                .route("/status", get(routes::wind_status))
                .route("/grid/{id}", get(routes::wind_grid)),
        )
        .route("/api/gbfs", any(routes::legacy_gbfs_reject))
        .route("/api/gbfs/{*path}", any(routes::legacy_gbfs_reject))
        .route("/api/transit/feeds", get(routes::transit_feeds))
        .route("/api/transit/vehicles/{feed_id}", get(routes::transit_vehicles))
        .route("/api/transit/trail/{feed_id}/{vehicle_id}", get(routes::transit_trail))
        .route(
            "/api/terrain/heights",
            post(routes::terrain_heights).get(routes::terrain_heights_get),
        )
        .route("/api/traffic/status", get(routes::traffic_status))
        .route("/api/alerts/rules", post(routes::create_alert_rule).get(routes::list_alert_rules))
        .route(
            "/api/alerts/rules/{id}",
            delete(routes::delete_alert_rule).get(routes::get_alert_rule),
        )
        .route("/api/alerts/inbox", get(routes::alerts_inbox))
        .route("/api/search/spatial", post(routes::spatial_search))
        .route("/api/gnss-integrity", get(routes::legacy_gnss_integrity))
        .route("/api/gnss/cells", get(routes::gnss_cells))
        .route("/api/gnss/provenance", get(routes::gnss_provenance))
        .route("/api/air-quality", get(routes::air_quality))
        .nest(
            "/api/tomtom",
            Router::new()
                .route("/status", get(routes::traffic_status))
                .route("/flow/{z}/{x}/{y}", get(routes::traffic_flow_pbf))
                .layer(middleware::from_fn(security::require_same_origin))
                .layer(middleware::from_fn_with_state(limiter, security::rate_limit)),
        )
        .route("/api", any(routes::api_not_found))
        .route("/api/{*rest}", any(routes::api_not_found))
        .fallback_service(static_files)
        .layer(middleware::from_fn_with_state(
            HostPolicy::new(&settings.allowed_hosts),
            security::require_allowed_host,
        ))
        .layer(middleware::from_fn_with_state(
            Duration::from_secs(settings.request_timeout_s),
            request_timeout,
        ))
        .layer(RequestBodyLimitLayer::new(64 * 1024))
        .layer(CatchPanicLayer::custom(panic_response))
        .layer(middleware::from_fn(api_error_envelope))
        .layer(SetResponseHeaderLayer::overriding(header::CONTENT_SECURITY_POLICY, csp))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
        ))
        .layer(TraceLayer::new_for_http().make_span_with(|request: &Request| {
            // The default span includes the full URI. Provider credentials and
            // private search text can be in its query, so keep method/path only.
            tracing::debug_span!(
                "http_request",
                method = %request.method(),
                path = request.uri().path(),
            )
        }))
        .with_state(state)
}

/// Built assets are served as files (a missing one is a real 404, never HTML served as
/// JavaScript); every other path falls back to `index.html` for client-side routing.
fn static_service(dir: &Path) -> Router {
    let index = dir.join("index.html");
    let spa =
        ServeDir::new(dir).append_index_html_on_directories(true).fallback(ServeFile::new(index));
    Router::new()
        .nest_service("/assets", ServeDir::new(dir.join("assets")))
        .nest_service("/cesium", ServeDir::new(dir.join("cesium")))
        .nest_service("/fonts", ServeDir::new(dir.join("fonts")))
        .fallback_service(spa)
        .layer(middleware::from_fn(static_cache_headers))
}

#[cfg(test)]
mod security_regression_tests;
