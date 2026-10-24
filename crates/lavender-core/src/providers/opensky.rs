//! Aircraft from The OpenSky Network (evaluation / non-commercial license).
//!
//! Provides regional bounding-box and global state-vector queries with `OAuth2`
//! token management and daily credit budget governor.

use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use jiff::{SignedDuration, Timestamp};
use lavender_domain::text::clean_opt;
use lavender_domain::{Aircraft, FeedId, FeedItems, GeoPoint, ProviderScope};
use secrecy::ExposeSecret;
use serde::Deserialize;
use url::Url;

use super::{FeedProvider, FetchContext, Fetched, ProviderError, Region};
use crate::config::OpenSkyConfig;
use crate::fetch::{FetchOptions, HostPolicy};
use crate::secrets::SecretKey;
use crate::sources;

const MAX_BODY: usize = 16 * 1024 * 1024;
const FEET_PER_METRE: f64 = 3.280_84;
const KNOTS_PER_MPS: f64 = 1.943_844;
const FPM_PER_MPS: f64 = 196.8504;

#[derive(Debug, Clone)]
struct CachedToken {
    token: String,
    expires_at: Timestamp,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<i64>,
}

#[derive(Debug)]
pub struct OpenSkyProvider {
    config: OpenSkyConfig,
    policy: HostPolicy,
    token: Mutex<Option<CachedToken>>,
    cooldown_until: Mutex<Option<Timestamp>>,
    remaining_credits: Mutex<Option<u32>>,
}

impl OpenSkyProvider {
    #[must_use]
    pub fn new(config: OpenSkyConfig) -> Self {
        let policy = HostPolicy::for_url(&config.base_url)
            .with_min_interval(Duration::from_millis(config.min_request_interval_ms));
        Self {
            config,
            policy,
            token: Mutex::new(None),
            cooldown_until: Mutex::new(None),
            remaining_credits: Mutex::new(None),
        }
    }

    /// Returns the remaining credit estimate if received from upstream headers.
    pub fn remaining_credits(&self) -> Option<u32> {
        *self.remaining_credits.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether an upstream 429 rate limit cooldown is currently active.
    pub fn is_cooling_down(&self) -> bool {
        let now = Timestamp::now();
        self.cooldown_until
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some_and(|cd| cd > now)
    }

    async fn ensure_token(&self, ctx: &FetchContext<'_>) -> Result<Option<String>, ProviderError> {
        let client_id = ctx.secrets.get(SecretKey::OpenSkyClientId);
        let client_secret = ctx.secrets.get(SecretKey::OpenSkyClientSecret);
        let (Some(id), Some(secret)) = (client_id, client_secret) else {
            return Ok(None);
        };
        let now = Timestamp::now();
        {
            let guard = self.token.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(cached) = &*guard
                && cached.expires_at > now
            {
                return Ok(Some(cached.token.clone()));
            }
        }

        let body = format!(
            "grant_type=client_credentials&client_id={}&client_secret={}",
            id.expose_secret(),
            secret.expose_secret()
        );

        let resp = ctx
            .fetcher
            .client()
            .post(self.config.auth_url.as_str())
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|e| {
                ProviderError::Unconfigured(format!("OpenSky OAuth token request failed: {e}"))
            })?;

        let token_resp: TokenResponse =
            resp.json().await.map_err(|e| ProviderError::parse("OpenSky OAuth", &e))?;

        // 60-second safety margin
        let margin_secs = (token_resp.expires_in.unwrap_or(1800) - 60).max(60);
        let expires_at = now.checked_add(SignedDuration::from_secs(margin_secs)).unwrap_or(now);

        let mut guard = self.token.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = Some(CachedToken { token: token_resp.access_token.clone(), expires_at });
        Ok(Some(token_resp.access_token))
    }

    fn url_for(&self, region: Region) -> Result<Url, ProviderError> {
        let path = match region {
            Region::Global => "states/all?extended=1".to_owned(),
            Region::Cell(cell) => {
                let bbox = cell.bbox();
                format!(
                    "states/all?lamin={:.4}&lomin={:.4}&lamax={:.4}&lomax={:.4}&extended=1",
                    bbox.south, bbox.west, bbox.north, bbox.east
                )
            }
        };
        self.config
            .base_url
            .join(&path)
            .map_err(|e| ProviderError::Unconfigured(format!("OpenSky URL: {e}")))
    }
}

/// Category code mapping according to OpenSky extended state vectors.
fn category_code(cat: Option<i64>) -> Option<String> {
    match cat? {
        2 => Some("A1".to_owned()),
        3 => Some("A2".to_owned()),
        4 => Some("A3".to_owned()),
        5 => Some("A4".to_owned()),
        6 => Some("A5".to_owned()),
        7 => Some("A6".to_owned()),
        8 => Some("A7".to_owned()),
        9 => Some("B1".to_owned()),
        10 => Some("B2".to_owned()),
        11 => Some("B3".to_owned()),
        12 => Some("B4".to_owned()),
        14 => Some("B6".to_owned()),
        15 => Some("B7".to_owned()),
        16 => Some("C1".to_owned()),
        17 => Some("C2".to_owned()),
        18 => Some("C3".to_owned()),
        19 => Some("C4".to_owned()),
        20 => Some("C5".to_owned()),
        _ => None,
    }
}

fn valid_hex(hex: &str) -> bool {
    let body = hex.strip_prefix('~').unwrap_or(hex);
    body.len() == 6 && body.chars().all(|c| c.is_ascii_hexdigit())
}

fn finite(v: Option<f64>) -> Option<f64> {
    v.filter(|x| x.is_finite())
}

#[derive(Debug, Deserialize)]
struct OpenSkyResponse {
    #[serde(default)]
    time: Option<serde_json::Number>,
    #[serde(default)]
    states: Option<Vec<Vec<serde_json::Value>>>,
}

/// Parses an OpenSky state vectors response into normalized `Aircraft` records.
///
/// Returns valid aircraft, dropped row count, and the document timestamp.
///
/// # Errors
/// Returns `ProviderError` if the response body is not valid JSON.
pub fn parse_opensky_states(
    body: &[u8],
) -> Result<(Vec<Aircraft>, usize, Option<Timestamp>), ProviderError> {
    let response: OpenSkyResponse =
        serde_json::from_slice(body).map_err(|e| ProviderError::parse("OpenSky", &e))?;

    let doc_time_sec = response
        .time
        .as_ref()
        .and_then(serde_json::Number::as_f64)
        .filter(|n| n.is_finite() && *n > 0.0);

    let doc_timestamp = doc_time_sec.and_then(|secs| {
        let duration = SignedDuration::try_from_secs_f64(secs).ok()?;
        Timestamp::UNIX_EPOCH.checked_add(duration).ok()
    });

    let rows = response.states.unwrap_or_default();
    let total = rows.len();
    let mut aircraft = Vec::with_capacity(total);

    for row in rows {
        if let Some(ac) = normalize_opensky_row(&row, doc_time_sec) {
            aircraft.push(ac);
        }
    }

    let dropped = total - aircraft.len();
    Ok((aircraft, dropped, doc_timestamp))
}

fn normalize_opensky_row(row: &[serde_json::Value], doc_time_sec: Option<f64>) -> Option<Aircraft> {
    if row.len() < 7 {
        return None;
    }

    let hex_raw = row[0].as_str()?.trim().to_ascii_lowercase();
    if !valid_hex(&hex_raw) {
        return None;
    }

    let lon = row[5].as_f64()?;
    let lat = row[6].as_f64()?;
    let position = GeoPoint::new(lat, lon).ok()?;

    let callsign = row.get(1).and_then(|v| v.as_str()).and_then(|s| clean_opt(Some(s), 12));
    let time_position = row.get(3).and_then(serde_json::Value::as_f64);
    let last_contact = row.get(4).and_then(serde_json::Value::as_f64);

    let baro_alt_m = row.get(7).and_then(serde_json::Value::as_f64).filter(|v| v.is_finite());
    let on_ground = row.get(8).and_then(serde_json::Value::as_bool).unwrap_or(false);
    let velocity_mps = row.get(9).and_then(serde_json::Value::as_f64).filter(|v| v.is_finite());
    let track = row.get(10).and_then(serde_json::Value::as_f64).filter(|v| v.is_finite());
    let vertical_rate_mps =
        row.get(11).and_then(serde_json::Value::as_f64).filter(|v| v.is_finite());
    let geo_alt_m = row.get(13).and_then(serde_json::Value::as_f64).filter(|v| v.is_finite());
    let squawk = row
        .get(14)
        .and_then(|v| v.as_str())
        .and_then(|s| clean_opt(Some(s), 4).filter(|sq| sq.chars().all(|c| c.is_ascii_digit())));
    let category_int = row.get(17).and_then(serde_json::Value::as_i64);

    let altitude_baro_ft = if on_ground { None } else { baro_alt_m.map(|m| m * FEET_PER_METRE) };
    let altitude_geom_ft = geo_alt_m.map(|m| m * FEET_PER_METRE);

    let position = position.with_altitude(baro_alt_m.or(geo_alt_m)).ok()?;

    let seen_pos_s = match (doc_time_sec, time_position.or(last_contact)) {
        (Some(now_sec), Some(pos_sec)) => (now_sec - pos_sec).max(0.0),
        _ => 0.0,
    };

    Some(Aircraft {
        hex: hex_raw,
        callsign,
        registration: None,
        type_code: None,
        category: category_code(category_int),
        position,
        altitude_baro_ft,
        altitude_geom_ft,
        on_ground,
        ground_speed_kt: finite(velocity_mps.map(|v| v * KNOTS_PER_MPS))
            .filter(|v| (0.0..2_500.0).contains(v)),
        track_deg: finite(track).filter(|v| (0.0..=360.0).contains(v)),
        vertical_rate_fpm: finite(vertical_rate_mps.map(|v| v * FPM_PER_MPS)),
        squawk,
        emergency: None,
        is_military: false,
        seen_pos_s,
        source: sources::OPENSKY.to_owned(),
    })
}

#[async_trait]
impl FeedProvider for OpenSkyProvider {
    fn feed(&self) -> FeedId {
        FeedId::Aircraft
    }

    fn source_id(&self) -> &'static str {
        sources::OPENSKY
    }

    fn cadence(&self) -> Duration {
        Duration::from_secs(self.config.cadence_s)
    }

    fn scope(&self) -> ProviderScope {
        ProviderScope::Regional { cell_deg: self.config.cell_deg, max_cells: self.config.max_cells }
    }

    fn enabled(&self) -> bool {
        self.config.enabled
    }

    fn global_fallback_notice(&self) -> Option<&'static str> {
        Some("Showing OpenSky worldwide aircraft snapshot. Zoom in for regional tracking.")
    }

    async fn fetch(
        &self,
        ctx: &FetchContext<'_>,
        region: Region,
    ) -> Result<Fetched, ProviderError> {
        if self.is_cooling_down() {
            return Err(ProviderError::Unavailable(
                "OpenSky upstream rate limit cooldown is active".to_owned(),
            ));
        }

        let url = self.url_for(region)?;
        let auth_header =
            self.ensure_token(ctx).await.ok().flatten().map(|t| format!("Bearer {t}"));

        let opts = FetchOptions {
            max_body_bytes: Some(MAX_BODY),
            auth_header,
            cancel: Some(ctx.cancel.clone()),
            ..FetchOptions::default()
        };

        let response = match ctx.fetcher.get(&url, &self.policy, &opts).await {
            Ok(resp) => resp,
            Err(e) => {
                return Err(ProviderError::Fetch(e));
            }
        };

        // Inspect rate limit headers
        if let Some(remaining) = response
            .headers
            .get("x-rate-limit-remaining")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u32>().ok())
        {
            *self.remaining_credits.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
                Some(remaining);
        }

        let (aircraft, dropped, as_of) = parse_opensky_states(&response.body)?;
        if dropped > 0 {
            tracing::debug!(dropped, "OpenSky records skipped during validation");
        }

        let mut fetched =
            Fetched::fresh(FeedItems::Aircraft(aircraft)).with_dropped("opensky", dropped);
        fetched.as_of = as_of;
        if as_of.is_none() {
            fetched.degraded = Some("OpenSky document timestamp is missing or invalid".to_owned());
        }
        Ok(fetched)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_opensky_extended_state_vectors() {
        let json_data = br#"{
            "time": 1760000000,
            "states": [
                [
                    "a1b2c3",
                    "UAL123 ",
                    "United States",
                    1759999990,
                    1759999992,
                    -97.7431,
                    30.2672,
                    10668.0,
                    false,
                    231.5,
                    90.5,
                    5.2,
                    null,
                    10700.0,
                    "1200",
                    false,
                    0,
                    4
                ],
                [
                    "4ca1b2",
                    "RYR456",
                    "Ireland",
                    1759999995,
                    1759999995,
                    -6.2603,
                    53.3498,
                    null,
                    true,
                    10.0,
                    180.0,
                    0.0,
                    null,
                    null,
                    "7000",
                    false,
                    0,
                    2
                ],
                [
                    "invalid",
                    "BAD",
                    "Nowhere",
                    1759999990,
                    1759999990,
                    200.0,
                    100.0
                ]
            ]
        }"#;

        let (aircraft, dropped, doc_time) = parse_opensky_states(json_data).unwrap();
        assert_eq!(aircraft.len(), 2);
        assert_eq!(dropped, 1);
        assert!(doc_time.is_some());

        let ac1 = &aircraft[0];
        assert_eq!(ac1.hex, "a1b2c3");
        assert_eq!(ac1.callsign.as_deref(), Some("UAL123"));
        assert_eq!(ac1.category.as_deref(), Some("A3"));
        assert!(!ac1.on_ground);
        assert!((ac1.position.lat - 30.2672).abs() < 1e-4);
        assert!((ac1.position.lon - (-97.7431)).abs() < 1e-4);
        assert!((ac1.altitude_baro_ft.unwrap() - 10668.0 * FEET_PER_METRE).abs() < 1.0);
        assert!((ac1.altitude_geom_ft.unwrap() - 10700.0 * FEET_PER_METRE).abs() < 1.0);
        assert!((ac1.ground_speed_kt.unwrap() - 231.5 * KNOTS_PER_MPS).abs() < 1.0);
        assert_eq!(ac1.track_deg, Some(90.5));
        assert!((ac1.vertical_rate_fpm.unwrap() - 5.2 * FPM_PER_MPS).abs() < 1.0);
        assert_eq!(ac1.squawk.as_deref(), Some("1200"));
        assert_eq!(ac1.seen_pos_s, 10.0);
        assert_eq!(ac1.source, sources::OPENSKY);

        let ac2 = &aircraft[1];
        assert_eq!(ac2.hex, "4ca1b2");
        assert_eq!(ac2.callsign.as_deref(), Some("RYR456"));
        assert_eq!(ac2.category.as_deref(), Some("A1"));
        assert!(ac2.on_ground);
        assert!(ac2.altitude_baro_ft.is_none());
        assert_eq!(ac2.squawk.as_deref(), Some("7000"));
        assert_eq!(ac2.seen_pos_s, 5.0);
    }

    #[test]
    fn category_code_mappings() {
        assert_eq!(category_code(Some(2)).as_deref(), Some("A1"));
        assert_eq!(category_code(Some(6)).as_deref(), Some("A5"));
        assert_eq!(category_code(Some(8)).as_deref(), Some("A7"));
        assert_eq!(category_code(Some(14)).as_deref(), Some("B6"));
        assert_eq!(category_code(Some(16)).as_deref(), Some("C1"));
        assert_eq!(category_code(Some(0)), None);
        assert_eq!(category_code(None), None);
    }

    #[test]
    fn bbox_url_generation() {
        let p = OpenSkyProvider::new(OpenSkyConfig::default());
        let cell = lavender_geo::plan_cells(
            &lavender_domain::BBox::new(24.0, 60.0, 25.0, 60.5).unwrap(),
            5.0,
            1,
        )
        .cells[0];
        let url = p.url_for(Region::Cell(cell)).unwrap();
        assert!(url.as_str().contains("states/all?lamin="));
        assert!(url.as_str().contains("extended=1"));
        assert_eq!(
            p.url_for(Region::Global).unwrap().as_str(),
            "https://opensky-network.org/api/states/all?extended=1"
        );
    }
}
