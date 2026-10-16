//! The single egress path for provider traffic (ADR 0008, security spec §2).
//!
//! Every request names the [`HostPolicy`] of the source making it. The fetcher enforces
//! HTTPS, follows redirects itself only to allowed hosts, bounds time and body size, limits
//! per-host concurrency, paces hosts with burst limits and retries idempotent failures with
//! jittered backoff. Errors name the host, never the full URL (some providers carry keys
//! in the path or query), and never quote more than a short, cleaned excerpt of a body.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::{Bytes, BytesMut};
use lavender_domain::text::clean;
use reqwest::header::{self, HeaderMap, HeaderValue};
use reqwest::{StatusCode, redirect};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::config::FetchConfig;

mod dns;

/// Hard ceiling on any response body. `fetch.max_body_bytes` is the cap for requests that
/// do not set their own; a provider's own cap (`FetchOptions::max_body_bytes`) is
/// authoritative up to this ceiling.
pub const MAX_BODY_CEILING: usize = 64 * 1024 * 1024;

/// Hosts a source may contact (exact names, no wildcards) and how fast it may contact them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPolicy {
    hosts: Vec<String>,
    min_interval: Duration,
}

impl HostPolicy {
    #[must_use]
    pub fn new(hosts: &[&str]) -> Self {
        Self {
            hosts: hosts.iter().map(|h| h.to_ascii_lowercase()).collect(),
            min_interval: Duration::ZERO,
        }
    }

    /// A policy for exactly the host of `base` (used when an operator overrides a
    /// provider's base URL, for example a mirror or a test server).
    #[must_use]
    pub fn for_url(base: &Url) -> Self {
        Self {
            hosts: base.host_str().map(str::to_ascii_lowercase).into_iter().collect(),
            min_interval: Duration::ZERO,
        }
    }

    /// Spaces requests to these hosts at least `interval` apart, across every caller in the
    /// process (for providers with burst limits or usage policies such as 1 request/second).
    #[must_use]
    pub fn with_min_interval(mut self, interval: Duration) -> Self {
        self.min_interval = interval;
        self
    }

    #[must_use]
    pub fn allows(&self, url: &Url) -> bool {
        url.host_str().is_some_and(|h| self.hosts.iter().any(|a| a.eq_ignore_ascii_case(h)))
    }

    #[must_use]
    pub fn hosts(&self) -> &[String] {
        &self.hosts
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FetchError {
    #[error("{host} resolves to an address refused by network policy")]
    NonPublicAddress { host: String },
    #[error("conditional request validators are invalid")]
    InvalidValidators,
    #[error("{host} is not an allowed host for this source")]
    HostNotAllowed { host: String },
    #[error("plain HTTP to {host} is refused; use HTTPS")]
    InsecureScheme { host: String },
    #[error("{host} must be reached over a secure WebSocket (wss://)")]
    InsecureStream { host: String },
    #[error("{host} did not respond in time")]
    Timeout { host: String },
    #[error("{host} answered HTTP {status}")]
    Status { host: String, status: u16 },
    #[error("{host} sent more than {limit} bytes")]
    BodyTooLarge { host: String, limit: usize },
    #[error("{host} redirected too many times")]
    TooManyRedirects { host: String },
    #[error("could not reach {host}: {reason}")]
    Network { host: String, reason: String },
    #[error("{host} sent data that could not be read: {reason}")]
    Decode { host: String, reason: String },
    #[error("the request was cancelled")]
    Cancelled,
}

/// Validators for one cached resource. Only typed, bounded conditional headers can be
/// emitted; arbitrary headers and credentials are deliberately not part of this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheValidators {
    etag: Option<String>,
    last_modified: Option<String>,
}

impl CacheValidators {
    /// Builds validators from an HTTP entity tag and/or IMF-fixdate Last-Modified value.
    ///
    /// # Errors
    /// Refuses malformed, oversized or control-bearing values without quoting them.
    pub fn new(etag: Option<&str>, last_modified: Option<&str>) -> Result<Self, FetchError> {
        if etag.is_none() && last_modified.is_none()
            || etag.is_some_and(|v| !valid_etag(v))
            || last_modified.is_some_and(|v| !valid_modified(v))
        {
            return Err(FetchError::InvalidValidators);
        }
        Ok(Self { etag: etag.map(str::to_owned), last_modified: last_modified.map(str::to_owned) })
    }

    #[must_use]
    pub fn etag(&self) -> Option<&str> {
        self.etag.as_deref()
    }

    #[must_use]
    pub fn last_modified(&self) -> Option<&str> {
        self.last_modified.as_deref()
    }

    fn from_headers(headers: &HeaderMap) -> Option<Self> {
        let etag = headers.get(header::ETAG).and_then(|v| v.to_str().ok());
        let modified = headers.get(header::LAST_MODIFIED).and_then(|v| v.to_str().ok());
        Self::new(etag, modified).ok()
    }
}

fn valid_etag(value: &str) -> bool {
    let body = value.strip_prefix("W/").unwrap_or(value);
    value.len() <= 512
        && HeaderValue::from_str(value).is_ok()
        && body.starts_with('"')
        && body.ends_with('"')
        && body.len() >= 2
        && body.as_bytes()[1..body.len() - 1]
            .iter()
            .all(|b| *b == b'!' || (0x23..=0x7e).contains(b))
}

fn valid_modified(value: &str) -> bool {
    value.len() == 29
        && value.is_ascii()
        && HeaderValue::from_str(value).is_ok()
        && jiff::fmt::strtime::parse("%a, %d %b %Y %H:%M:%S GMT", value)
            .and_then(|date| date.to_datetime())
            .is_ok()
}

impl FetchError {
    fn retryable(&self) -> bool {
        match self {
            Self::Timeout { .. } | Self::Network { .. } => true,
            // 420 is "Enhance Your Calm", a rate-limit status some APIs use instead of 429.
            Self::Status { status, .. } => matches!(*status, 420 | 429 | 500 | 502 | 503 | 504),
            _ => false,
        }
    }
}

/// Per-request options.
#[derive(Debug, Clone)]
pub struct FetchOptions {
    /// The body cap for this request, replacing the configured default (never above
    /// [`MAX_BODY_CEILING`]).
    pub max_body_bytes: Option<usize>,
    pub accept: &'static str,
    pub cancel: Option<CancellationToken>,
    /// Retries for this request, replacing the configured count (providers with strict
    /// usage policies retry on their own schedule instead).
    pub retries: Option<u8>,
    /// Redirects followed for this request, replacing the configured maximum. A provider
    /// that counts every HTTP request against a budget sets 0, so one reserved request is
    /// exactly one request upstream; a redirect then fails as `TooManyRedirects`.
    pub redirects: Option<u8>,
    /// Conditional GET for a cached copy of this exact URL. A redirect to another URL
    /// clears these headers and 304 acceptance, even when the host remains allowed.
    pub conditional: Option<CacheValidators>,
    /// Optional Authorization header value (e.g. `Token <secret>` or `Bearer <secret>`).
    pub auth_header: Option<String>,
    /// Optional HTTP byte-range request `(start, end)` inclusive, emitting `Range: bytes={start}-{end}`.
    pub range: Option<(u64, u64)>,
    /// Time allowed for this request, body included, replacing `fetch.request_timeout_s`
    /// (for example for large downloads).
    pub timeout: Option<Duration>,
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            max_body_bytes: None,
            accept: "application/json",
            cancel: None,
            retries: None,
            redirects: None,
            conditional: None,
            auth_header: None,
            range: None,
            timeout: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FetchResponse {
    pub status: u16,
    pub body: Bytes,
    pub elapsed: Duration,
    /// Validators are scoped to the original request URL. Responses from a different
    /// redirect target do not expose validators for reuse against that original resource.
    pub validators: Option<CacheValidators>,
}

#[derive(Debug)]
pub struct Fetcher {
    client: reqwest::Client,
    config: FetchConfig,
    semaphores: Mutex<HashMap<String, Arc<Semaphore>>>,
    /// When the last request to each paced host was sent. The async mutex queues waiters
    /// in arrival order.
    pacing: Mutex<HashMap<String, Arc<tokio::sync::Mutex<Option<Instant>>>>>,
}

fn host_of(url: &Url) -> String {
    url.host_str().unwrap_or("unknown host").to_owned()
}

/// The secure and the development-only plain scheme of one protocol.
#[derive(Clone, Copy)]
struct Scheme {
    secure: &'static str,
    plain: &'static str,
    insecure: fn(String) -> FetchError,
}

impl Scheme {
    const HTTP: Self = Self {
        secure: "https",
        plain: "http",
        insecure: |host| FetchError::InsecureScheme { host },
    };
    const WEBSOCKET: Self =
        Self { secure: "wss", plain: "ws", insecure: |host| FetchError::InsecureStream { host } };
}

impl Fetcher {
    /// Builds the HTTP client.
    ///
    /// # Errors
    /// Returns an error when the TLS backend cannot be initialised.
    pub fn new(config: FetchConfig) -> Result<Self, FetchError> {
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT_ENCODING, HeaderValue::from_static("gzip"));
        let client = reqwest::Client::builder()
            .user_agent(config.user_agent.clone())
            .default_headers(headers)
            .connect_timeout(config.connect_timeout())
            .redirect(redirect::Policy::none())
            .no_proxy()
            .dns_resolver(Arc::new(dns::PublicResolver::new(config.allow_http_hosts.clone())))
            .gzip(true)
            .build()
            .map_err(|e| FetchError::Network {
                host: "client".into(),
                reason: e.without_url().to_string(),
            })?;
        Ok(Self {
            client,
            config,
            semaphores: Mutex::new(HashMap::new()),
            pacing: Mutex::new(HashMap::new()),
        })
    }

    fn semaphore(&self, host: &str) -> Arc<Semaphore> {
        let mut map = self.semaphores.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        map.entry(host.to_owned())
            .or_insert_with(|| Arc::new(Semaphore::new(self.config.per_host_concurrency.max(1))))
            .clone()
    }

    /// Waits until a request to `host` may be sent and records the send. The send time is
    /// recorded only when the wait completes, so a request cancelled while it waits leaves
    /// nothing reserved and does not delay the requests queued behind it.
    async fn pace(&self, host: &str, interval: Duration) {
        if interval.is_zero() {
            return;
        }
        let gate = {
            let mut map = self.pacing.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            map.entry(host.to_owned()).or_default().clone()
        };
        let mut last_sent = gate.lock().await;
        if let Some(last) = *last_sent {
            tokio::time::sleep_until(tokio::time::Instant::from_std(last + interval)).await;
        }
        *last_sent = Some(Instant::now());
    }

    /// The body cap for a request: its own cap, else the configured default, never above
    /// [`MAX_BODY_CEILING`]. When a byte range is requested, caps to the range length.
    fn body_limit(&self, opts: &FetchOptions) -> usize {
        let configured =
            opts.max_body_bytes.unwrap_or(self.config.max_body_bytes).min(MAX_BODY_CEILING);
        if let Some((start, end)) = opts.range {
            let range_len =
                usize::try_from(end.saturating_sub(start).saturating_add(1)).unwrap_or(usize::MAX);
            configured.min(range_len)
        } else {
            configured
        }
    }

    fn check(&self, url: &Url, policy: &HostPolicy) -> Result<(), FetchError> {
        self.check_target(url, policy, Scheme::HTTP)
    }

    /// The egress rules every connection follows, whatever its protocol: an allowed host,
    /// the secure scheme (the plain one only for listed development hosts) and, unless
    /// the host is listed, a public literal address.
    fn check_target(
        &self,
        url: &Url,
        policy: &HostPolicy,
        scheme: Scheme,
    ) -> Result<(), FetchError> {
        let host = host_of(url);
        if !policy.allows(url) {
            return Err(FetchError::HostNotAllowed { host });
        }
        let plain_http_allowed =
            self.config.allow_http_hosts.iter().any(|h| h.eq_ignore_ascii_case(&host));
        if url.scheme() != scheme.secure && !(url.scheme() == scheme.plain && plain_http_allowed) {
            return Err((scheme.insecure)(host));
        }
        let address = match url.host() {
            Some(url::Host::Ipv4(ip)) => Some(std::net::IpAddr::V4(ip)),
            Some(url::Host::Ipv6(ip)) => Some(std::net::IpAddr::V6(ip)),
            _ => None,
        };
        if !plain_http_allowed && address.is_some_and(|ip| !dns::is_public(ip)) {
            return Err(FetchError::NonPublicAddress { host });
        }
        Ok(())
    }

    /// GETs `url` under `policy` and returns the body, retrying transient failures.
    ///
    /// # Errors
    /// Returns a [`FetchError`] describing the final failure.
    pub async fn get(
        &self,
        url: &Url,
        policy: &HostPolicy,
        opts: &FetchOptions,
    ) -> Result<FetchResponse, FetchError> {
        let retries = opts.retries.unwrap_or(self.config.retries);
        let mut attempt: u8 = 0;
        loop {
            match self.get_once(url, policy, opts).await {
                Ok(r) => return Ok(r),
                Err((err, retry_after)) => {
                    if attempt >= retries || !err.retryable() {
                        return Err(err);
                    }
                    let backoff = retry_after.unwrap_or_else(|| {
                        let base = 500u64 << attempt.min(6);
                        Duration::from_millis(base + fastrand::u64(0..=base / 2))
                    });
                    tracing::debug!(host = %host_of(url), attempt, ?backoff, error = %err, "retrying fetch");
                    let sleep = tokio::time::sleep(backoff.min(Duration::from_secs(30)));
                    if let Some(cancel) = &opts.cancel {
                        tokio::select! {
                            () = sleep => {}
                            () = cancel.cancelled() => return Err(FetchError::Cancelled),
                        }
                    } else {
                        sleep.await;
                    }
                    attempt += 1;
                }
            }
        }
    }

    async fn get_once(
        &self,
        url: &Url,
        policy: &HostPolicy,
        opts: &FetchOptions,
    ) -> Result<FetchResponse, (FetchError, Option<Duration>)> {
        let fut = self.follow(url, policy, opts);
        match &opts.cancel {
            Some(cancel) => tokio::select! {
                r = fut => r,
                () = cancel.cancelled() => Err((FetchError::Cancelled, None)),
            },
            None => fut.await,
        }
    }

    async fn follow(
        &self,
        url: &Url,
        policy: &HostPolicy,
        opts: &FetchOptions,
    ) -> Result<FetchResponse, (FetchError, Option<Duration>)> {
        let started = Instant::now();
        let limit = self.body_limit(opts);
        let mut current = url.clone();
        let mut conditional = opts.conditional.as_ref();
        let max_redirects = opts.redirects.unwrap_or(self.config.max_redirects);
        for _ in 0..=max_redirects {
            self.check(&current, policy).map_err(|e| (e, None))?;
            let host = host_of(&current);
            self.pace(&host, policy.min_interval).await;
            let permit = self.semaphore(&host).acquire_owned().await;
            let mut request = self
                .client
                .get(current.clone())
                .header(header::ACCEPT, opts.accept)
                .timeout(opts.timeout.unwrap_or_else(|| self.config.request_timeout()));
            if let Some(auth) = &opts.auth_header {
                request = request.header(header::AUTHORIZATION, auth);
            }
            if let Some((start, end)) = opts.range {
                request = request.header(header::RANGE, format!("bytes={start}-{end}"));
            }
            if let Some(validators) = conditional {
                if let Some(etag) = validators.etag() {
                    request = request.header(header::IF_NONE_MATCH, etag);
                }
                if let Some(modified) = validators.last_modified() {
                    request = request.header(header::IF_MODIFIED_SINCE, modified);
                }
            }
            let response = request.send().await.map_err(|e| (classify(&host, e), None))?;
            let status = response.status();
            let validators = if response.url() == url {
                CacheValidators::from_headers(response.headers())
            } else {
                None
            };
            if status == StatusCode::NOT_MODIFIED {
                if conditional.is_none() {
                    return Err((FetchError::Status { host, status: 304 }, None));
                }
                // 304 has no message body. Content-Length may describe the cached 200
                // representation, so neither body parsing nor its transfer-size test applies.
                return Ok(FetchResponse {
                    status: 304,
                    body: Bytes::new(),
                    elapsed: started.elapsed(),
                    validators,
                });
            }
            if status.is_redirection() {
                let next = response
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|loc| current.join(loc).ok())
                    .ok_or_else(|| {
                        (FetchError::Status { host: host.clone(), status: status.as_u16() }, None)
                    })?;
                drop(permit);
                if next != current {
                    conditional = None;
                }
                current = next;
                continue;
            }
            if !status.is_success() {
                let retry_after = retry_after(&response, status);
                return Err((FetchError::Status { host, status: status.as_u16() }, retry_after));
            }
            if let Some((start, _)) = opts.range
                && status != StatusCode::PARTIAL_CONTENT
                && !(status == StatusCode::OK && start == 0)
            {
                return Err((FetchError::Status { host, status: status.as_u16() }, None));
            }
            if let Some(len) = response.content_length()
                && usize::try_from(len).unwrap_or(usize::MAX) > limit
            {
                return Err((FetchError::BodyTooLarge { host, limit }, None));
            }
            let body = read_capped(response, &host, limit).await.map_err(|e| (e, None))?;
            drop(permit);
            return Ok(FetchResponse {
                status: status.as_u16(),
                body,
                elapsed: started.elapsed(),
                validators,
            });
        }
        Err((FetchError::TooManyRedirects { host: host_of(url) }, None))
    }

    /// GETs and decodes JSON.
    ///
    /// # Errors
    /// Returns a [`FetchError`] on transport failure or when the body is not the expected JSON.
    pub async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &Url,
        policy: &HostPolicy,
        opts: &FetchOptions,
    ) -> Result<(T, FetchResponse), FetchError> {
        let response = self.get(url, policy, opts).await?;
        let value = serde_json::from_slice(&response.body).map_err(|e| FetchError::Decode {
            host: host_of(url),
            reason: clean(&e.to_string(), 200),
        })?;
        Ok((value, response))
    }

    /// The `User-Agent` sent with every upstream request, for protocols that build their
    /// own requests (the WebSocket handshake).
    #[must_use]
    pub fn user_agent(&self) -> &str {
        &self.config.user_agent
    }

    /// Opens a TCP connection for a WebSocket under the same egress rules as [`Self::get`]:
    /// the host must be allowed by `policy`, the scheme must be `wss` (`ws` only for hosts
    /// in `fetch.allow_http_hosts`), and the name is resolved once with every address
    /// checked, so the socket can only reach the validated set. No proxy is used. The
    /// lookup and the connection attempts together take at most the configured connect
    /// timeout. The caller runs TLS and the WebSocket handshake on the returned stream.
    ///
    /// # Errors
    /// Returns a [`FetchError`] naming only the host when the policy refuses the target
    /// or no validated address accepts the connection in time.
    pub async fn connect_stream(
        &self,
        url: &Url,
        policy: &HostPolicy,
    ) -> Result<tokio::net::TcpStream, FetchError> {
        self.check_target(url, policy, Scheme::WEBSOCKET)?;
        let host = host_of(url);
        let network =
            |reason: &str| FetchError::Network { host: host.clone(), reason: reason.to_owned() };
        let timed_out = || FetchError::Timeout { host: host.clone() };
        let deadline = tokio::time::Instant::now() + self.config.connect_timeout();
        let port = url.port_or_known_default().ok_or_else(|| network("no port"))?;
        let addresses = match url.host() {
            Some(url::Host::Ipv4(ip)) => vec![std::net::SocketAddr::new(ip.into(), port)],
            Some(url::Host::Ipv6(ip)) => vec![std::net::SocketAddr::new(ip.into(), port)],
            Some(url::Host::Domain(name)) => {
                let resolver = dns::PublicResolver::new(self.config.allow_http_hosts.clone());
                tokio::time::timeout_at(deadline, resolver.lookup(name, port))
                    .await
                    .map_err(|_| timed_out())?
                    .map_err(|error| {
                        if error.downcast_ref::<dns::BlockedAddress>().is_some() {
                            FetchError::NonPublicAddress { host: host.clone() }
                        } else {
                            network("name lookup failed")
                        }
                    })?
            }
            None => return Err(network("no host")),
        };
        let mut last = network("no address accepted the connection");
        for (tried, address) in addresses.iter().enumerate() {
            // Like hyper's connector, the time left is shared by the addresses not yet
            // tried, so one unreachable address cannot use up the whole timeout.
            let untried = u32::try_from(addresses.len() - tried).unwrap_or(u32::MAX);
            let budget = deadline.saturating_duration_since(tokio::time::Instant::now()) / untried;
            match tokio::time::timeout(budget, tokio::net::TcpStream::connect(address)).await {
                Ok(Ok(stream)) => return Ok(stream),
                Ok(Err(error)) => last = network(&error.kind().to_string()),
                Err(_) => last = timed_out(),
            }
        }
        Err(last)
    }
}

fn classify(host: &str, err: reqwest::Error) -> FetchError {
    let mut source = std::error::Error::source(&err);
    while let Some(error) = source {
        if error.downcast_ref::<dns::BlockedAddress>().is_some() {
            return FetchError::NonPublicAddress { host: host.to_owned() };
        }
        source = error.source();
    }
    if err.is_timeout() {
        FetchError::Timeout { host: host.to_owned() }
    } else {
        FetchError::Network { host: host.to_owned(), reason: err.without_url().to_string() }
    }
}

fn retry_after(response: &reqwest::Response, status: StatusCode) -> Option<Duration> {
    let rate_limited = status == StatusCode::TOO_MANY_REQUESTS || status.as_u16() == 420;
    if !rate_limited && status != StatusCode::SERVICE_UNAVAILABLE {
        return None;
    }
    response
        .headers()
        .get(header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

async fn read_capped(
    mut response: reqwest::Response,
    host: &str,
    limit: usize,
) -> Result<Bytes, FetchError> {
    let mut buf = BytesMut::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| classify(host, e))? {
        if buf.len() + chunk.len() > limit {
            return Err(FetchError::BodyTooLarge { host: host.to_owned(), limit });
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf.freeze())
}

#[cfg(test)]
mod tests;
