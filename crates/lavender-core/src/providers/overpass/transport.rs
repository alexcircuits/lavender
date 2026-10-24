//! Overpass transport, upstreams resolution, and failover client.
//!
//! Ported directly from `legacy/server/providers/overpass/transport.js` and `constants.js`.

#![allow(clippy::expect_used)] // LazyLock regex statics: patterns are verified at construction

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER, USER_AGENT};
use tokio::sync::Mutex;
use url::Url;

use super::constants::OVERPASS_USER_AGENT;
use super::sanitiser::{OverpassError, sanitize_overpass_body};

static RE_BOUNDARY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)is_in\s*\(|\bpivot\b").expect("valid regex"));

static RE_RUNTIME_ERROR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""remark"\s*:\s*"runtime error"#).expect("valid regex"));

/// Parse only operator-supplied HTTP(S) endpoints; private instances are allowed.
#[must_use]
pub fn parse_overpass_upstreams(raw: &str) -> Vec<String> {
    let mut endpoints = Vec::new();
    for token in raw.split(',') {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(url) = Url::parse(trimmed) {
            let scheme = url.scheme();
            if (scheme == "http" || scheme == "https") && url.has_host() && url.fragment().is_none()
            {
                let href = url.to_string();
                if !endpoints.contains(&href) {
                    endpoints.push(href);
                }
            }
        }
    }
    endpoints.truncate(8);
    endpoints
}

/// Detects whether an Overpass query is a boundary-class query (`is_in` / pivot)
/// eligible for long-lived caching.
#[must_use]
pub fn is_overpass_boundary_query(query: &str) -> bool {
    RE_BOUNDARY.is_match(query)
}

/// Determines whether an HTTP payload is valid Overpass data.
#[must_use]
pub fn overpass_payload_is_data(status: u16, body: &str) -> bool {
    if !(200..=299).contains(&status) {
        return false;
    }
    if RE_RUNTIME_ERROR.is_match(body) {
        return false;
    }
    if body.contains("rate_limited") {
        return false;
    }
    true
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OverpassPayload {
    pub status: u16,
    pub body: String,
    pub endpoint: Option<String>,
    pub retry_after_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct OverpassClient {
    endpoints: Vec<String>,
    cooldowns: Arc<Mutex<HashMap<String, Instant>>>,
    client: reqwest::Client,
}

impl OverpassClient {
    #[must_use]
    pub fn new(raw_upstreams: &str) -> Self {
        let endpoints = parse_overpass_upstreams(raw_upstreams);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(
                crate::providers::overpass::constants::OVERPASS_TIMEOUT_MS,
            ))
            .build()
            .unwrap_or_default();

        Self { endpoints, cooldowns: Arc::new(Mutex::new(HashMap::new())), client }
    }

    #[must_use]
    pub fn is_configured(&self) -> bool {
        !self.endpoints.is_empty()
    }

    /// Queries the Overpass upstream endpoints with sanitisation and automatic failover.
    ///
    /// # Errors
    /// Returns error if the query fails sanitisation or all upstreams fail.
    pub async fn execute(&self, raw_body: &str) -> Result<OverpassPayload, OverpassError> {
        let sanitized = sanitize_overpass_body(raw_body)?;

        if !self.is_configured() {
            return Ok(OverpassPayload {
                status: 503,
                body: r#"{"error":"Detailed OpenStreetMap queries are not configured","code":"OVERPASS_NOT_CONFIGURED","retryable":false}"#.to_string(),
                endpoint: None,
                retry_after_ms: None,
            });
        }

        let now = Instant::now();
        let mut cooldowns = self.cooldowns.lock().await;

        for endpoint_str in &self.endpoints {
            if cooldowns.get(endpoint_str).is_some_and(|&expiry| now < expiry) {
                continue; // Skip cooled-down upstream
            }

            let Ok(mut target_url) = Url::parse(endpoint_str) else {
                continue;
            };

            let mut headers = HeaderMap::new();
            headers.insert(USER_AGENT, HeaderValue::from_static(OVERPASS_USER_AGENT));

            // Extract and strip credentials from URL if present
            if !target_url.username().is_empty() {
                let _ = target_url.set_username("");
                let _ = target_url.set_password(None);
            }

            let resp = self
                .client
                .post(target_url.as_str())
                .headers(headers)
                .body(sanitized.body.clone())
                .send()
                .await;

            match resp {
                Ok(response) => {
                    let status = response.status().as_u16();
                    let retry_after = response
                        .headers()
                        .get(RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .map(|s| s * 1000);

                    if status == 429 || status == 406 || status == 503 || status == 504 {
                        let cooldown_ms = retry_after.unwrap_or(60_000);
                        cooldowns
                            .insert(endpoint_str.clone(), now + Duration::from_millis(cooldown_ms));
                    }

                    if let Ok(body) = response.text().await
                        && overpass_payload_is_data(status, &body)
                    {
                        return Ok(OverpassPayload {
                            status,
                            body,
                            endpoint: Some("configured".to_string()),
                            retry_after_ms: retry_after,
                        });
                    }
                }
                Err(_) => {
                    // Upstream connection error: cooldown 30s
                    cooldowns.insert(endpoint_str.clone(), now + Duration::from_secs(30));
                }
            }
        }

        Ok(OverpassPayload {
            status: 503,
            body: r#"{"error":"All Overpass upstreams are temporarily unavailable","code":"OVERPASS_UPSTREAMS_UNAVAILABLE","retryable":true}"#.to_string(),
            endpoint: None,
            retry_after_ms: Some(30_000),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_upstreams_cleanly() {
        let raw = "http://localhost:1234/api, https://paid.example/query?key=secret, http://localhost:1234/api, ftp://invalid, javascript:void(0)";
        let parsed = parse_overpass_upstreams(raw);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], "http://localhost:1234/api");
        assert_eq!(parsed[1], "https://paid.example/query?key=secret");
    }

    #[test]
    fn detects_boundary_queries() {
        assert!(is_overpass_boundary_query(
            "[out:json][timeout:25];is_in(37.5,14.2)->.a;area.a[\"boundary\"];out tags;"
        ));
        assert!(is_overpass_boundary_query(
            "[out:json][timeout:25];area(3600039152)->.x;rel(pivot.x);out geom;"
        ));
        assert!(!is_overpass_boundary_query("[out:json][timeout:25];node(around:500,30,-97);out;"));
    }

    #[test]
    fn validates_payload_is_data() {
        assert!(overpass_payload_is_data(200, r#"{"elements":[]}"#));
        assert!(!overpass_payload_is_data(400, r#"{"elements":[]}"#));
        assert!(!overpass_payload_is_data(429, r#"{"elements":[]}"#));
        assert!(!overpass_payload_is_data(
            200,
            r#"{"remark":"runtime error: timeout","elements":[]}"#
        ));
        assert!(!overpass_payload_is_data(200, "rate_limited"));
    }
}
