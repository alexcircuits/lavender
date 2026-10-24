//! OSRM routing client with pacing, coalescing, and caching.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use bytes::BytesMut;
use futures::StreamExt;
use lavender_domain::{GeoPoint, Route};
use moka::future::Cache;
use reqwest::header::{self, HeaderValue};
use tokio::sync::watch;

use super::pacer::{RouteError, RoutePacer};
use super::steps::{RawOsrmResponse, normalize_osrm_steps};

pub const DEFAULT_CAR_ENDPOINT: &str = "https://routing.openstreetmap.de/routed-car";
pub const DEFAULT_BIKE_ENDPOINT: &str = "https://routing.openstreetmap.de/routed-bike";
pub const DEFAULT_FOOT_ENDPOINT: &str = "https://routing.openstreetmap.de/routed-foot";

pub const ROUTE_CACHE_MAX_ENTRIES: u64 = 200;
pub const ROUTE_CACHE_TTL: Duration = Duration::from_secs(600); // 10 minutes
pub const ROUTE_MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024; // 8 MB
pub const ROUTE_REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
pub const ROUTE_MAX_LEG_KM: f64 = 600.0;
pub const ROUTE_MAX_TOTAL_KM: f64 = 2500.0;

/// Travel mode profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RouteProfile {
    Car,
    Bike,
    Foot,
}

impl RouteProfile {
    /// Parse travel profile from user input.
    ///
    /// # Errors
    /// Returns [`RouteError::InvalidProfile`] if the profile name is unrecognised.
    pub fn parse(s: &str) -> Result<Self, RouteError> {
        match s.trim().to_lowercase().as_str() {
            "car" | "driving" => Ok(Self::Car),
            "bike" | "cycling" | "bicycle" => Ok(Self::Bike),
            "foot" | "walking" | "walk" => Ok(Self::Foot),
            _ => Err(RouteError::InvalidProfile),
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Car => "car",
            Self::Bike => "bike",
            Self::Foot => "foot",
        }
    }

    #[must_use]
    pub const fn osrm_profile(self) -> &'static str {
        match self {
            Self::Car => "driving",
            Self::Bike => "bike",
            Self::Foot => "foot",
        }
    }
}

/// Parse and validate coordinates string: `lon,lat;lon,lat[;...]`.
///
/// # Errors
/// Returns [`RouteError`] on invalid coordinate counts, out-of-range coords,
/// or legs/total distance exceeding safety caps.
pub fn parse_and_validate_coords(
    coords_str: &str,
    max_leg_km: f64,
    max_total_km: f64,
) -> Result<(String, Vec<[f64; 2]>), RouteError> {
    let pairs: Vec<&str> = coords_str.split(';').map(str::trim).filter(|s| !s.is_empty()).collect();

    if pairs.len() < 2 || pairs.len() > 12 {
        return Err(RouteError::InvalidCoordinateCount);
    }

    let mut clean_pairs = Vec::with_capacity(pairs.len());
    let mut points: Vec<[f64; 2]> = Vec::with_capacity(pairs.len());

    for pr in pairs {
        let parts: Vec<&str> = pr.split(',').map(str::trim).collect();
        if parts.len() != 2 {
            return Err(RouteError::InvalidCoordinate);
        }

        let lon: f64 = parts[0].parse().map_err(|_| RouteError::InvalidCoordinate)?;
        let lat: f64 = parts[1].parse().map_err(|_| RouteError::InvalidCoordinate)?;

        if !lon.is_finite()
            || !lat.is_finite()
            || !(-90.0..=90.0).contains(&lat)
            || !(-180.0..=180.0).contains(&lon)
        {
            return Err(RouteError::InvalidCoordinate);
        }

        clean_pairs.push(format!("{lon},{lat}"));
        points.push([lon, lat]);
    }

    // Reject abusive spans using geodesic distance
    let mut total_km = 0.0;
    for i in 1..points.len() {
        let p1 = GeoPoint { lat: points[i - 1][1], lon: points[i - 1][0], alt_m: None };
        let p2 = GeoPoint { lat: points[i][1], lon: points[i][0], alt_m: None };
        let leg_km = lavender_geo::distance_m(&p1, &p2) / 1000.0;
        if leg_km > max_leg_km {
            return Err(RouteError::LegTooLong);
        }
        total_km += leg_km;
    }

    if total_km > max_total_km {
        return Err(RouteError::RouteTooLong);
    }

    Ok((clean_pairs.join(";"), points))
}

type InflightReceiver = watch::Receiver<Option<Result<Arc<Route>, RouteError>>>;
type InflightSender = watch::Sender<Option<Result<Arc<Route>, RouteError>>>;

/// Core routing engine.
#[derive(Debug)]
pub struct RouteEngine {
    client: reqwest::Client,
    pacer: Arc<RoutePacer>,
    cache: Cache<String, Arc<Route>>,
    inflight: Arc<tokio::sync::Mutex<HashMap<String, InflightReceiver>>>,
    car_endpoint: String,
    bike_endpoint: String,
    foot_endpoint: String,
    max_leg_km: f64,
    max_total_km: f64,
    max_response_bytes: usize,
    request_timeout: Duration,
}

impl RouteEngine {
    #[must_use]
    pub fn new(
        client: reqwest::Client,
        pacer: Arc<RoutePacer>,
        car_endpoint: Option<String>,
        bike_endpoint: Option<String>,
        foot_endpoint: Option<String>,
    ) -> Self {
        Self {
            client,
            pacer,
            cache: Cache::builder()
                .max_capacity(ROUTE_CACHE_MAX_ENTRIES)
                .time_to_live(ROUTE_CACHE_TTL)
                .build(),
            inflight: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            car_endpoint: car_endpoint.unwrap_or_else(|| DEFAULT_CAR_ENDPOINT.to_owned()),
            bike_endpoint: bike_endpoint.unwrap_or_else(|| DEFAULT_BIKE_ENDPOINT.to_owned()),
            foot_endpoint: foot_endpoint.unwrap_or_else(|| DEFAULT_FOOT_ENDPOINT.to_owned()),
            max_leg_km: ROUTE_MAX_LEG_KM,
            max_total_km: ROUTE_MAX_TOTAL_KM,
            max_response_bytes: ROUTE_MAX_RESPONSE_BYTES,
            request_timeout: ROUTE_REQUEST_TIMEOUT,
        }
    }

    #[must_use]
    pub fn pacer(&self) -> &Arc<RoutePacer> {
        &self.pacer
    }

    /// Calculate a route between coordinates.
    ///
    /// # Errors
    /// Returns [`RouteError`] on validation errors, rate limits, or upstream failures.
    pub async fn calculate_route(
        &self,
        profile_raw: &str,
        coords_raw: &str,
        with_steps: bool,
    ) -> Result<Arc<Route>, RouteError> {
        let profile = RouteProfile::parse(profile_raw)?;
        let (coords_clean, _pts) =
            parse_and_validate_coords(coords_raw, self.max_leg_km, self.max_total_km)?;

        let cache_key = format!("{}|{}", profile.as_str(), coords_clean);

        // 1. Check Cache
        if let Some(cached) = self.cache.get(&cache_key).await {
            if !with_steps {
                if cached.steps.is_none() {
                    return Ok(cached);
                }
                let mut stripped = (*cached).clone();
                stripped.steps = None;
                stripped.steps_truncated = None;
                return Ok(Arc::new(stripped));
            } else if cached.steps.is_some() {
                return Ok(cached);
            }
        }

        let base = match profile {
            RouteProfile::Car => &self.car_endpoint,
            RouteProfile::Bike => &self.bike_endpoint,
            RouteProfile::Foot => &self.foot_endpoint,
        };

        let inflight_key_self =
            format!("{base}|{cache_key}|{}", if with_steps { "s" } else { "n" });
        let inflight_key_stepful = format!("{base}|{cache_key}|s");

        // 2. Check In-Flight Deduplication
        let maybe_sub = {
            let map = self.inflight.lock().await;
            if let Some(sub) = map.get(&inflight_key_self) {
                Some(sub.clone())
            } else if !with_steps {
                map.get(&inflight_key_stepful).cloned()
            } else {
                None
            }
        };

        if let Some(mut sub) = maybe_sub {
            loop {
                if let Some(res) = sub.borrow().clone() {
                    return match res {
                        Ok(route) => {
                            if !with_steps && route.steps.is_some() {
                                let mut stripped = (*route).clone();
                                stripped.steps = None;
                                stripped.steps_truncated = None;
                                Ok(Arc::new(stripped))
                            } else {
                                Ok(route)
                            }
                        }
                        Err(e) => Err(e),
                    };
                }
                if sub.changed().await.is_err() {
                    break;
                }
            }
        }

        // 3. Leader execution
        let (tx, rx) = watch::channel(None);
        {
            let mut map = self.inflight.lock().await;
            map.insert(inflight_key_self.clone(), rx);
        }

        let mut guard =
            InflightGuard { inflight: self.inflight.clone(), key: inflight_key_self, tx: Some(tx) };

        let result = self.fetch_upstream(profile, &coords_clean, with_steps, base).await;

        if let Ok(ref route) = result {
            self.cache.insert(cache_key, route.clone()).await;
        }

        if let Some(tx) = guard.tx.take() {
            let _ = tx.send(Some(result.clone()));
        }

        result
    }

    async fn fetch_upstream(
        &self,
        profile: RouteProfile,
        coords: &str,
        with_steps: bool,
        base: &str,
    ) -> Result<Arc<Route>, RouteError> {
        let _permit = self.pacer.acquire().await?;

        let url = format!(
            "{}/route/v1/{}/{}?overview=full&geometries=geojson&alternatives=false&steps={}",
            base.trim_end_matches('/'),
            profile.osrm_profile(),
            coords,
            if with_steps { "true" } else { "false" }
        );

        let req = self
            .client
            .get(&url)
            .header(header::USER_AGENT, HeaderValue::from_static("Lavender/0.1.0 (routing)"))
            .timeout(self.request_timeout);

        let upstream_res = req.send().await.map_err(|e| RouteError::ProxyError(e.to_string()))?;

        let status = upstream_res.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = upstream_res
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(5);
            return Err(RouteError::UpstreamRateLimited { retry_after_s: retry_after });
        }

        if !status.is_success() {
            return Err(RouteError::NotFound);
        }

        let ctype = upstream_res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");

        if !ctype.contains("json") {
            return Err(RouteError::NotFound);
        }

        // Stream response body with hard limit
        let mut stream = upstream_res.bytes_stream();
        let mut buf = BytesMut::new();

        while let Some(chunk_res) = stream.next().await {
            let chunk = chunk_res.map_err(|e| RouteError::ProxyError(e.to_string()))?;
            if buf.len() + chunk.len() > self.max_response_bytes {
                return Err(RouteError::ProxyError("OSRM response exceeded size cap".to_owned()));
            }
            buf.extend_from_slice(&chunk);
        }

        let parsed: RawOsrmResponse =
            serde_json::from_slice(&buf).map_err(|_| RouteError::NotFound)?;

        if parsed.code.as_deref() != Some("Ok") || parsed.routes.is_empty() {
            return Err(RouteError::NotFound);
        }

        let osrm_route = &parsed.routes[0];
        if osrm_route.geometry.coordinates.is_empty() {
            return Err(RouteError::NotFound);
        }

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let distance_m = osrm_route.distance.max(0.0).round() as u64;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let duration_s = osrm_route.duration.max(0.0).round() as u64;

        let (steps, steps_truncated) = if with_steps {
            let (s, truncated) = normalize_osrm_steps(&osrm_route.legs);
            (Some(s), if truncated { Some(true) } else { None })
        } else {
            (None, None)
        };

        let route = Route {
            ok: true,
            profile: profile.as_str().to_owned(),
            distance_m,
            duration_s,
            geometry: osrm_route.geometry.coordinates.clone(),
            steps,
            steps_truncated,
        };

        Ok(Arc::new(route))
    }
}

struct InflightGuard {
    inflight: Arc<tokio::sync::Mutex<HashMap<String, InflightReceiver>>>,
    key: String,
    tx: Option<InflightSender>,
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx
                .send(Some(Err(RouteError::ProxyError("in-flight request cancelled".to_owned()))));
        }
        let inflight = self.inflight.clone();
        let key = std::mem::take(&mut self.key);
        tokio::spawn(async move {
            let mut map = inflight.lock().await;
            map.remove(&key);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_profiles() {
        assert_eq!(RouteProfile::parse("car").unwrap(), RouteProfile::Car);
        assert_eq!(RouteProfile::parse("DRIVING").unwrap(), RouteProfile::Car);
        assert_eq!(RouteProfile::parse("bike").unwrap(), RouteProfile::Bike);
        assert_eq!(RouteProfile::parse("cycling").unwrap(), RouteProfile::Bike);
        assert_eq!(RouteProfile::parse("bicycle").unwrap(), RouteProfile::Bike);
        assert_eq!(RouteProfile::parse("foot").unwrap(), RouteProfile::Foot);
        assert_eq!(RouteProfile::parse("walking").unwrap(), RouteProfile::Foot);
        assert_eq!(RouteProfile::parse("walk").unwrap(), RouteProfile::Foot);
        assert!(matches!(RouteProfile::parse("submarine"), Err(RouteError::InvalidProfile)));
    }

    #[test]
    fn coordinate_validation_rules() {
        // Need at least 2
        assert_eq!(
            parse_and_validate_coords("13.4,52.5", 600.0, 2500.0).unwrap_err(),
            RouteError::InvalidCoordinateCount
        );

        // Max 12 coordinates
        let thirteen = (0..13).map(|i| format!("13.{i},52.{i}")).collect::<Vec<_>>().join(";");
        assert_eq!(
            parse_and_validate_coords(&thirteen, 600.0, 2500.0).unwrap_err(),
            RouteError::InvalidCoordinateCount
        );

        // Bad coordinate formatting
        assert_eq!(
            parse_and_validate_coords("abc,def;13.4,52.5", 600.0, 2500.0).unwrap_err(),
            RouteError::InvalidCoordinate
        );

        // Out of lat/lon bounds
        assert_eq!(
            parse_and_validate_coords("190.0,52.5;13.4,52.5", 600.0, 2500.0).unwrap_err(),
            RouteError::InvalidCoordinate
        );

        // Leg distance too long (> 600 km)
        // London (0.12, 51.5) to Rome (12.49, 41.9) is ~1400 km
        assert_eq!(
            parse_and_validate_coords("0.12,51.5;12.49,41.9", 600.0, 2500.0).unwrap_err(),
            RouteError::LegTooLong
        );

        // Valid pairs
        let (clean, pts) =
            parse_and_validate_coords("13.388860, 52.517037 ; 13.397634, 52.529407", 600.0, 2500.0)
                .unwrap();
        assert_eq!(pts.len(), 2);
        assert_eq!(clean, "13.38886,52.517037;13.397634,52.529407");
    }
}
