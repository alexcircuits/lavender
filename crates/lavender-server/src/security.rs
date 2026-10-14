//! Request-level controls (security spec §2): host allowlist before routing, same-origin
//! enforcement for `/api`, per-client rate limiting and security headers.

use std::fmt::Write as _;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::uri::Authority;
use axum::http::{HeaderMap, HeaderValue, Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use lavender_domain::ErrorCode;

use crate::error::HttpError;

/// Rate-limit buckets kept at most (least recently and frequently used go first).
const MAX_TRACKED_CLIENTS: u64 = 100_000;

/// Strips the port from a `Host` / authority value (`[::1]:7410` → `[::1]`).
fn host_name(authority: &str) -> &str {
    // Userinfo (`user@host`) is not part of the host.
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    if authority.starts_with('[') {
        authority.split_inclusive(']').next().unwrap_or(authority)
    } else {
        authority.split(':').next().unwrap_or(authority)
    }
}

/// The authority a request is addressed to: the `Host` header (HTTP/1.1) or the request
/// target's authority (HTTP/2 `:authority`, absolute-form requests). `None` when neither
/// is present or the two disagree.
fn request_authority<'a>(headers: &'a HeaderMap, uri: &'a Uri) -> Option<&'a str> {
    let host = headers.get(header::HOST).map(|h| h.to_str().ok());
    let target = uri.authority().map(Authority::as_str);
    match (host, target) {
        (Some(Some(h)), Some(t)) => h.eq_ignore_ascii_case(t).then_some(h),
        (Some(Some(h)), None) => Some(h),
        (None, Some(t)) => Some(t),
        // A Host header that is not visible ASCII, or no host at all.
        (Some(None), _) | (None, None) => None,
    }
}

#[derive(Debug, Clone)]
pub struct HostPolicy {
    allowed: Arc<Vec<String>>,
}

impl HostPolicy {
    #[must_use]
    pub fn new(allowed: &[String]) -> Self {
        Self { allowed: Arc::new(allowed.iter().map(|h| h.to_ascii_lowercase()).collect()) }
    }

    fn allows(&self, headers: &HeaderMap, uri: &Uri) -> bool {
        request_authority(headers, uri)
            .is_some_and(|a| self.allowed.iter().any(|x| *x == host_name(a).to_ascii_lowercase()))
    }
}

/// Refuses requests whose host is not allowed (DNS-rebinding guard).
pub async fn require_allowed_host(
    State(policy): State<HostPolicy>,
    request: Request,
    next: Next,
) -> Response {
    if policy.allows(request.headers(), request.uri()) {
        next.run(request).await
    } else {
        HttpError::new(ErrorCode::Forbidden, "This host name is not served here.").into_response()
    }
}

/// The scheme the client used: the request target's scheme (HTTP/2), else
/// `X-Forwarded-Proto` from a TLS-terminating proxy, else plain HTTP (the server itself
/// does not terminate TLS). Browsers cannot set `X-Forwarded-Proto` on cross-site
/// requests, so trusting it here cannot widen what a foreign page may do.
fn request_scheme<'a>(headers: &'a HeaderMap, uri: &'a Uri) -> &'a str {
    uri.scheme_str()
        .or_else(|| {
            headers
                .get("x-forwarded-proto")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split(',').next())
                .map(str::trim)
        })
        .unwrap_or("http")
}

/// Whether a request comes from this site: `Origin`, when present, must name this host
/// with the scheme the request arrived over (an `https` page may call a server that sees
/// plain HTTP behind a TLS proxy, but never the reverse), and `Sec-Fetch-Site` must not
/// say `cross-site`.
#[must_use]
pub fn is_same_origin(headers: &HeaderMap, uri: &Uri) -> bool {
    let host = request_authority(headers, uri).unwrap_or_default();
    let scheme = request_scheme(headers, uri);
    let origin_ok =
        headers.get(header::ORIGIN).and_then(|o| o.to_str().ok()).is_none_or(|origin| {
            origin.split_once("://").is_some_and(|(origin_scheme, authority)| {
                let scheme_ok = match origin_scheme.to_ascii_lowercase().as_str() {
                    "https" => true,
                    "http" => !scheme.eq_ignore_ascii_case("https"),
                    _ => false,
                };
                scheme_ok && !host.is_empty() && authority.eq_ignore_ascii_case(host)
            })
        });
    let fetch_site_ok = headers
        .get("sec-fetch-site")
        .and_then(|v| v.to_str().ok())
        .is_none_or(|v| v != "cross-site");
    origin_ok && fetch_site_ok
}

/// Same-origin enforcement for `/api` (protects provider quotas from other web pages).
pub async fn require_same_origin(request: Request, next: Next) -> Response {
    if is_same_origin(request.headers(), request.uri()) {
        next.run(request).await
    } else {
        HttpError::new(ErrorCode::Forbidden, "Cross-site requests are not accepted.")
            .into_response()
    }
}

/// The key a client is tracked by: its IPv4 address, or its IPv6 /64 (one subscriber
/// line usually owns a whole /64, so per-address keys would let it rotate freely).
#[must_use]
pub fn client_key(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => ip,
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(Ipv6Addr::from(u128::from(v6) & (u128::MAX << 64))),
        },
    }
}

/// The client a request is attributed to (already reduced by [`client_key`]). The rate
/// limiter stores it in the request's extensions; handlers read it as an extractor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientIp(pub IpAddr);

impl ClientIp {
    fn of(parts: &Parts) -> Self {
        if let Some(client) = parts.extensions.get::<Self>() {
            return *client;
        }
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map_or(IpAddr::from([127, 0, 0, 1]), |c| c.0.ip());
        Self(client_key(peer))
    }
}

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(Ok(Self::of(parts)))
    }
}

#[derive(Debug)]
struct Bucket {
    tokens: f64,
    updated: Instant,
}

/// A token bucket per client (see [`client_key`]), in a bounded cache. A bucket idle long
/// enough to have refilled completely is equivalent to a new one, so it expires then.
#[derive(Clone)]
pub struct RateLimiter {
    buckets: moka::sync::Cache<IpAddr, Arc<Mutex<Bucket>>>,
    per_s: f64,
    burst: f64,
    trusted_proxies: Arc<Vec<IpAddr>>,
}

impl std::fmt::Debug for RateLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimiter")
            .field("clients", &self.buckets.entry_count())
            .field("per_s", &self.per_s)
            .field("burst", &self.burst)
            .field("trusted_proxies", &self.trusted_proxies)
            .finish()
    }
}

impl RateLimiter {
    #[must_use]
    pub fn new(per_s: u32, burst: u32, trusted_proxies: Vec<IpAddr>) -> Self {
        Self::with_capacity(per_s, burst, trusted_proxies, MAX_TRACKED_CLIENTS)
    }

    fn with_capacity(per_s: u32, burst: u32, trusted_proxies: Vec<IpAddr>, capacity: u64) -> Self {
        let per_s = f64::from(per_s.max(1));
        let burst = f64::from(burst.max(1));
        // Seconds for an empty bucket to refill, plus a margin.
        let refill = Duration::from_secs_f64((burst / per_s).ceil() + 1.0);
        Self {
            buckets: moka::sync::Cache::builder()
                .max_capacity(capacity)
                .time_to_idle(refill)
                .build(),
            per_s,
            burst,
            trusted_proxies: Arc::new(trusted_proxies.into_iter().map(canonical).collect()),
        }
    }

    fn is_trusted(&self, ip: IpAddr) -> bool {
        self.trusted_proxies.contains(&ip)
    }

    /// The client address for a request from `peer`. Only a trusted proxy's
    /// `X-Forwarded-For` is read, right to left: each trusted proxy appends the address it
    /// received from, so the first untrusted entry from the right is the client. Entries
    /// further left were written by the client itself and are ignored.
    fn client(&self, peer: IpAddr, headers: &HeaderMap) -> IpAddr {
        let peer = canonical(peer);
        if !self.is_trusted(peer) {
            return peer;
        }
        let hops: Vec<&str> = headers
            .get_all("x-forwarded-for")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .collect();
        let mut nearest = peer;
        for hop in hops.iter().rev() {
            match parse_hop(hop) {
                Some(ip) if self.is_trusted(ip) => nearest = ip,
                Some(ip) => return ip,
                // Garbage: the nearest trusted hop is the best address known.
                None => return nearest,
            }
        }
        nearest
    }

    /// Takes one token for `client`; returns the seconds to wait when empty.
    fn take(&self, client: IpAddr) -> Result<(), u64> {
        let now = Instant::now();
        let bucket = self.buckets.get_with(client_key(client), || {
            Arc::new(Mutex::new(Bucket { tokens: self.burst, updated: now }))
        });
        let mut bucket = bucket.lock().unwrap_or_else(PoisonError::into_inner);
        let elapsed = now.saturating_duration_since(bucket.updated).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.per_s).min(self.burst);
        bucket.updated = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            let wait = ((1.0 - bucket.tokens) / self.per_s).ceil().max(1.0);
            // Bounded by 1 / per_s seconds, far below u64::MAX.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Err(wait as u64)
        }
    }
}

/// IPv4-mapped IPv6 addresses (a dual-stack listener) compare as IPv4.
fn canonical(ip: IpAddr) -> IpAddr {
    ip.to_canonical()
}

/// One `X-Forwarded-For` entry: an address, optionally with a port or brackets.
fn parse_hop(hop: &str) -> Option<IpAddr> {
    let hop = hop.trim().trim_matches('"');
    hop.parse::<IpAddr>()
        .ok()
        .or_else(|| hop.parse::<SocketAddr>().ok().map(|s| s.ip()))
        .or_else(|| hop.strip_prefix('[')?.strip_suffix(']')?.parse().ok())
        .map(canonical)
}

/// Rate-limits `/api` per client and records the client (as [`ClientIp`]) for handlers.
pub async fn rate_limit(
    State(limiter): State<RateLimiter>,
    mut request: Request,
    next: Next,
) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map_or(IpAddr::from([127, 0, 0, 1]), |c| c.0.ip());
    let client = limiter.client(peer, request.headers());
    request.extensions_mut().insert(ClientIp(client_key(client)));
    match limiter.take(client) {
        Ok(()) => next.run(request).await,
        Err(wait) => {
            let mut response = HttpError::new(
                ErrorCode::RateLimited,
                "Too many requests. Slow down and retry shortly.",
            )
            .into_response();
            if let Ok(v) = HeaderValue::from_str(&wait.to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, v);
            }
            response
        }
    }
}

/// The browser Content-Security-Policy: no inline script, no `unsafe-eval` (Cesium needs
/// only `wasm-unsafe-eval`; its credit display needs inline styles), tiles only from the
/// catalog's tile hosts (ADR 0008).
#[must_use]
pub fn content_security_policy(tile_hosts: &[String]) -> String {
    let tiles = tile_hosts.iter().fold(String::new(), |mut acc, h| {
        let _ = write!(acc, " https://{h}");
        acc
    });
    [
        "default-src 'self'".to_owned(),
        "script-src 'self' 'wasm-unsafe-eval'".to_owned(),
        "style-src 'self' 'unsafe-inline'".to_owned(),
        format!("img-src 'self' data: blob:{tiles}"),
        format!("connect-src 'self'{tiles}"),
        "font-src 'self'".to_owned(),
        "worker-src 'self' blob:".to_owned(),
        "object-src 'none'".to_owned(),
        "base-uri 'self'".to_owned(),
        "form-action 'self'".to_owned(),
        "frame-ancestors 'none'".to_owned(),
    ]
    .join("; ")
}

#[cfg(test)]
mod tests;
