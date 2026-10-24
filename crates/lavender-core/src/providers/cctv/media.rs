//! Media proxying, SSRF validation, same-origin redirect enforcement, and synthetic SVG fallback.

#![allow(
    clippy::collapsible_if,
    clippy::if_not_else,
    clippy::assigning_clones,
    clippy::must_use_candidate,
    clippy::doc_markdown
)]

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, USER_AGENT};
use tokio::sync::Semaphore;
use url::Url;

use super::constants::{
    CCTV_FRAME_FETCH_TIMEOUT, DEFAULT_PROXY_USER_AGENT, MAX_CONCURRENT_LIVE_STREAMS,
    MAX_SAME_HOST_REDIRECTS, NSW_IMAGE_ORIGIN, NSW_IMAGE_USER_AGENT, TXDOT_ORIGIN,
    TXDOT_SNAPSHOT_PATH,
};
use super::normalize::{base64_decode, escape_xml, hash_seed};

#[derive(Debug, thiserror::Error)]
pub enum CctvMediaError {
    #[error("Invalid or disallowed media URL: {0}")]
    InvalidUrl(String),
    #[error("Cross-origin or excessive redirects refused: {0}")]
    RedirectRefused(String),
    #[error("Upstream media fetch failed: {0}")]
    FetchFailed(String),
    #[error("Media payload exceeds maximum size cap")]
    ExceedsSizeCap,
    #[error("Upstream returned non-image content type: {0}")]
    NonImageContentType(String),
    #[error("TxDOT payload missing or invalid base64 snippet")]
    InvalidTxdotSnippet,
    #[error("Concurrent live stream capacity exceeded")]
    CapacityExceeded,
    #[error("Stream timed out")]
    Timeout,
}

/// Global concurrency limiter for live camera streams.
pub static LIVE_STREAM_SEMAPHORE: std::sync::LazyLock<Arc<Semaphore>> =
    std::sync::LazyLock::new(|| Arc::new(Semaphore::new(MAX_CONCURRENT_LIVE_STREAMS)));

/// Determine the appropriate User-Agent string for an upstream frame request.
#[must_use]
pub fn upstream_user_agent(url_str: &str) -> &'static str {
    if let Ok(parsed) = Url::parse(url_str) {
        if let Some(host) = parsed.host_str() {
            if let Ok(nsw_url) = Url::parse(NSW_IMAGE_ORIGIN) {
                if let Some(nsw_host) = nsw_url.host_str() {
                    if host.eq_ignore_ascii_case(nsw_host) {
                        return NSW_IMAGE_USER_AGENT;
                    }
                }
            }
        }
    }
    DEFAULT_PROXY_USER_AGENT
}

/// Generate a synthetic SVG billboard image for a CCTV camera placeholder.
///
/// Produces a 960x540 SVG with a deterministic gradient (hue derived from camera ID hash),
/// scanline overlay, HUD-style grid, and text labels showing camera name, city, ID, and status.
#[must_use]
pub fn build_synthetic_cctv_svg(camera_id: &str, label: &str, city: &str, status: &str) -> String {
    let seed = hash_seed(&format!("{camera_id}:{label}:{city}"));
    let hue = seed % 360;
    let hue2 = (hue + 46) % 360;
    let safe_label = escape_xml(label);
    let safe_city = escape_xml(if city.is_empty() { "GLOBAL GRID" } else { city });
    let safe_id = escape_xml(camera_id);
    let safe_status = escape_xml(if status.is_empty() { "SYNTHETIC" } else { status });

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="960" height="540" viewBox="0 0 960 540">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="hsl({hue}, 35%, 10%)" />
      <stop offset="60%" stop-color="hsl({hue2}, 42%, 6%)" />
      <stop offset="100%" stop-color="#020509" />
    </linearGradient>
    <radialGradient id="flare" cx="0.22" cy="0.24" r="0.78">
      <stop offset="0%" stop-color="hsla({hue2}, 100%, 65%, 0.35)" />
      <stop offset="100%" stop-color="hsla({hue2}, 100%, 40%, 0)" />
    </radialGradient>
    <pattern id="scan" width="8" height="8" patternUnits="userSpaceOnUse">
      <rect width="8" height="8" fill="transparent" />
      <rect y="0" width="8" height="1" fill="rgba(255,255,255,0.08)" />
      <rect y="4" width="8" height="1" fill="rgba(255,255,255,0.05)" />
    </pattern>
  </defs>
  <rect width="960" height="540" fill="url(#bg)" />
  <rect width="960" height="540" fill="url(#flare)" />
  <rect width="960" height="540" fill="url(#scan)" />
  <g stroke="rgba(123,233,255,0.25)" stroke-width="1" fill="none">
    <path d="M60 460 Q300 300 520 420 T900 320" />
    <path d="M100 160 Q340 40 620 130 T920 90" />
    <path d="M20 280 Q220 230 390 270 T760 250" />
  </g>
  <g fill="none" stroke="rgba(180,248,255,0.2)" stroke-width="1">
    <rect x="70" y="80" width="820" height="380" rx="8" />
    <line x1="70" y1="270" x2="890" y2="270" />
    <line x1="480" y1="80" x2="480" y2="460" />
  </g>
  <g fill="#9cefff" font-family="JetBrains Mono, monospace" text-transform="uppercase">
    <text x="74" y="54" font-size="16" letter-spacing="2">CCTV FEED PLACEHOLDER</text>
    <text x="74" y="512" font-size="14" letter-spacing="1.5">{safe_label} · {safe_city}</text>
    <text x="646" y="512" font-size="13" letter-spacing="1.2">{safe_id}</text>
    <text x="74" y="486" font-size="13" letter-spacing="1.3">{safe_status}</text>
  </g>
</svg>"##
    )
}

#[derive(serde::Deserialize)]
struct TxdotEnvelope {
    snippet: Option<String>,
}

/// Fetch a frame following redirects ONLY within the original origin (scheme, host and port).
/// Refuses any cross-origin redirects (SSRF boundary) and caps redirects at [`MAX_SAME_HOST_REDIRECTS`].
///
/// # Errors
/// Returns [`CctvMediaError`] on invalid URLs, off-host redirects, size cap overruns, or upstream failures.
pub async fn fetch_within_host(
    client: &reqwest::Client,
    initial_url_str: &str,
    max_bytes: usize,
    timeout: Duration,
) -> Result<(Bytes, String), CctvMediaError> {
    let mut current_url = Url::parse(initial_url_str)
        .map_err(|e| CctvMediaError::InvalidUrl(format!("Malformed URL: {e}")))?;

    if current_url.scheme() != "http" && current_url.scheme() != "https" {
        return Err(CctvMediaError::InvalidUrl("Only http/https allowed".to_owned()));
    }

    let origin = current_url.origin();

    for hop in 0..=MAX_SAME_HOST_REDIRECTS {
        let ua = upstream_user_agent(current_url.as_str());
        let response = client
            .get(current_url.clone())
            .header(USER_AGENT, ua)
            .timeout(timeout)
            .send()
            .await
            .map_err(|e| CctvMediaError::FetchFailed(e.to_string()))?;

        let status = response.status();
        if status.is_redirection() {
            if hop == MAX_SAME_HOST_REDIRECTS {
                return Err(CctvMediaError::RedirectRefused(
                    "Exceeded maximum same-origin redirect hops".to_owned(),
                ));
            }

            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .ok_or_else(|| {
                    CctvMediaError::RedirectRefused("Redirect missing Location header".to_owned())
                })?
                .to_str()
                .map_err(|_| {
                    CctvMediaError::RedirectRefused("Invalid Location header encoding".to_owned())
                })?;

            let next_url = current_url.join(location).map_err(|e| {
                CctvMediaError::RedirectRefused(format!("Invalid redirect target: {e}"))
            })?;

            if next_url.origin() != origin {
                return Err(CctvMediaError::RedirectRefused(format!(
                    "Refusing cross-origin redirect from {} to {}",
                    current_url.origin().ascii_serialization(),
                    next_url.origin().ascii_serialization()
                )));
            }

            current_url = next_url;
            continue;
        }

        if !status.is_success() {
            return Err(CctvMediaError::FetchFailed(format!("HTTP status {status}")));
        }

        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_owned();

        if !content_type.starts_with("image/") {
            return Err(CctvMediaError::NonImageContentType(content_type));
        }

        if let Some(len_header) = response.headers().get(CONTENT_LENGTH) {
            if let Ok(declared_len) = len_header.to_str().unwrap_or("0").parse::<usize>() {
                if declared_len > max_bytes {
                    return Err(CctvMediaError::ExceedsSizeCap);
                }
            }
        }

        let bytes =
            response.bytes().await.map_err(|e| CctvMediaError::FetchFailed(e.to_string()))?;

        if bytes.len() > max_bytes {
            return Err(CctvMediaError::ExceedsSizeCap);
        }

        return Ok((bytes, content_type));
    }

    Err(CctvMediaError::RedirectRefused("Too many redirects".to_owned()))
}

/// Fetch and decode a `TxDOT` ITS snapshot from JSON base64 snippet into a JPEG image.
///
/// # Errors
/// Returns [`CctvMediaError`] on invalid snapshot URLs, upstream HTTP failure, or corrupted base64 payloads.
pub async fn fetch_txdot_snapshot(
    client: &reqwest::Client,
    url_str: &str,
    max_bytes: usize,
) -> Result<(Bytes, String), CctvMediaError> {
    let parsed = Url::parse(url_str)
        .map_err(|e| CctvMediaError::InvalidUrl(format!("Invalid TxDOT snapshot URL: {e}")))?;

    if parsed.origin().ascii_serialization() != TXDOT_ORIGIN || parsed.path() != TXDOT_SNAPSHOT_PATH
    {
        return Err(CctvMediaError::InvalidUrl(
            "TxDOT snapshot URL does not match pinned origin/path".to_owned(),
        ));
    }

    let response = client
        .get(parsed)
        .header(USER_AGENT, "gods-eye-view-cctv-proxy/1.0")
        .header(reqwest::header::ACCEPT, "application/json")
        .timeout(CCTV_FRAME_FETCH_TIMEOUT)
        .send()
        .await
        .map_err(|e| CctvMediaError::FetchFailed(e.to_string()))?;

    if !response.status().is_success() {
        return Err(CctvMediaError::FetchFailed(format!(
            "TxDOT returned status {}",
            response.status()
        )));
    }

    let envelope: TxdotEnvelope = response
        .json()
        .await
        .map_err(|e| CctvMediaError::FetchFailed(format!("Invalid TxDOT JSON: {e}")))?;

    let snippet_raw = envelope.snippet.ok_or(CctvMediaError::InvalidTxdotSnippet)?;
    let snippet_clean = snippet_raw
        .trim()
        .trim_start_matches("data:image/jpeg;base64,")
        .trim_start_matches("data:image/jpg;base64,");

    let decoded = base64_decode(snippet_clean).ok_or(CctvMediaError::InvalidTxdotSnippet)?;

    if decoded.len() < 4 || decoded.len() > max_bytes {
        return Err(CctvMediaError::ExceedsSizeCap);
    }

    // JPEG SOI check: 0xFF, 0xD8, 0xFF
    if decoded[0] != 0xff || decoded[1] != 0xd8 || decoded[2] != 0xff {
        return Err(CctvMediaError::NonImageContentType("Missing JPEG magic bytes".to_owned()));
    }

    Ok((Bytes::from(decoded), "image/jpeg".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synthetic_svg_contains_labels() {
        let svg = build_synthetic_cctv_svg("CAM-123", "Main St & 5th Ave", "Austin", "OFFLINE");
        assert!(svg.contains("Main St &amp; 5th Ave"));
        assert!(svg.contains("Austin"));
        assert!(svg.contains("CAM-123"));
        assert!(svg.contains("OFFLINE"));
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn test_upstream_user_agent_detection() {
        let nsw = "https://webcams.transport.nsw.gov.au/livetraffic-webcams/cam1.jpeg";
        assert_eq!(upstream_user_agent(nsw), NSW_IMAGE_USER_AGENT);

        let general = "https://example.com/camera.jpg";
        assert_eq!(upstream_user_agent(general), DEFAULT_PROXY_USER_AGENT);
    }
}
