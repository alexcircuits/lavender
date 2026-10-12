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
    #[error("{host} is not an allowed host for this source")]
    HostNotAllowed { host: String },
    #[error("plain HTTP to {host} is refused; use HTTPS")]
    InsecureScheme { host: String },
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
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self { max_body_bytes: None, accept: "application/json", cancel: None, retries: None }
    }
}

#[derive(Debug, Clone)]
pub struct FetchResponse {
    pub status: u16,
    pub body: Bytes,
    pub elapsed: Duration,
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
    /// [`MAX_BODY_CEILING`].
    fn body_limit(&self, opts: &FetchOptions) -> usize {
        opts.max_body_bytes.unwrap_or(self.config.max_body_bytes).min(MAX_BODY_CEILING)
    }

    fn check(&self, url: &Url, policy: &HostPolicy) -> Result<(), FetchError> {
        let host = host_of(url);
        if !policy.allows(url) {
            return Err(FetchError::HostNotAllowed { host });
        }
        let plain_http_allowed =
            self.config.allow_http_hosts.iter().any(|h| h.eq_ignore_ascii_case(&host));
        match url.scheme() {
            "https" => Ok(()),
            "http" if plain_http_allowed => Ok(()),
            _ => Err(FetchError::InsecureScheme { host }),
        }
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
        for _ in 0..=self.config.max_redirects {
            self.check(&current, policy).map_err(|e| (e, None))?;
            let host = host_of(&current);
            self.pace(&host, policy.min_interval).await;
            let permit = self.semaphore(&host).acquire_owned().await;
            let response = self
                .client
                .get(current.clone())
                .header(header::ACCEPT, opts.accept)
                .timeout(self.config.request_timeout())
                .send()
                .await
                .map_err(|e| (classify(&host, e), None))?;
            let status = response.status();
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
                current = next;
                continue;
            }
            if !status.is_success() {
                let retry_after = retry_after(&response, status);
                return Err((FetchError::Status { host, status: status.as_u16() }, retry_after));
            }
            if let Some(len) = response.content_length()
                && usize::try_from(len).unwrap_or(usize::MAX) > limit
            {
                return Err((FetchError::BodyTooLarge { host, limit }, None));
            }
            let body = read_capped(response, &host, limit).await.map_err(|e| (e, None))?;
            drop(permit);
            return Ok(FetchResponse { status: status.as_u16(), body, elapsed: started.elapsed() });
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
}

fn classify(host: &str, err: reqwest::Error) -> FetchError {
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
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn config() -> FetchConfig {
        FetchConfig {
            allow_http_hosts: vec!["127.0.0.1".into()],
            retries: 2,
            request_timeout_s: 2,
            ..FetchConfig::default()
        }
    }

    fn policy(server: &MockServer) -> HostPolicy {
        HostPolicy::for_url(&Url::parse(&server.uri()).unwrap())
    }

    #[tokio::test]
    async fn fetches_json_from_an_allowed_host() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/ok"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"a":1}"#))
            .mount(&server)
            .await;
        let f = Fetcher::new(config()).unwrap();
        let url = Url::parse(&format!("{}/ok", server.uri())).unwrap();
        let (v, _): (serde_json::Value, _) =
            f.get_json(&url, &policy(&server), &FetchOptions::default()).await.unwrap();
        assert_eq!(v["a"], 1);
    }

    #[tokio::test]
    async fn refuses_hosts_outside_the_policy_and_plain_http() {
        let f = Fetcher::new(FetchConfig::default()).unwrap();
        let url = Url::parse("https://evil.example/x").unwrap();
        let err = f
            .get(&url, &HostPolicy::new(&["good.example"]), &FetchOptions::default())
            .await
            .unwrap_err();
        assert_eq!(err, FetchError::HostNotAllowed { host: "evil.example".into() });
        let url = Url::parse("http://good.example/x").unwrap();
        let err = f
            .get(&url, &HostPolicy::new(&["good.example"]), &FetchOptions::default())
            .await
            .unwrap_err();
        assert_eq!(err, FetchError::InsecureScheme { host: "good.example".into() });
    }

    #[tokio::test]
    async fn refuses_redirects_to_other_hosts() {
        let server = MockServer::start().await;
        Mock::given(path("/r"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", "https://elsewhere.example/steal"),
            )
            .mount(&server)
            .await;
        let f = Fetcher::new(config()).unwrap();
        let url = Url::parse(&format!("{}/r", server.uri())).unwrap();
        let err = f.get(&url, &policy(&server), &FetchOptions::default()).await.unwrap_err();
        assert_eq!(err, FetchError::HostNotAllowed { host: "elsewhere.example".into() });
    }

    #[tokio::test]
    async fn follows_same_host_redirects() {
        let server = MockServer::start().await;
        Mock::given(path("/old"))
            .respond_with(ResponseTemplate::new(301).insert_header("location", "/new"))
            .mount(&server)
            .await;
        Mock::given(path("/new"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;
        let f = Fetcher::new(config()).unwrap();
        let url = Url::parse(&format!("{}/old", server.uri())).unwrap();
        let r = f.get(&url, &policy(&server), &FetchOptions::default()).await.unwrap();
        assert_eq!(&r.body[..], b"ok");
    }

    #[tokio::test]
    async fn caps_body_size() {
        let server = MockServer::start().await;
        Mock::given(path("/big"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'x'; 4096]))
            .mount(&server)
            .await;
        let f = Fetcher::new(config()).unwrap();
        let url = Url::parse(&format!("{}/big", server.uri())).unwrap();
        let opts = FetchOptions { max_body_bytes: Some(1024), ..FetchOptions::default() };
        let err = f.get(&url, &policy(&server), &opts).await.unwrap_err();
        assert!(matches!(err, FetchError::BodyTooLarge { limit: 1024, .. }), "{err:?}");
    }

    #[tokio::test]
    async fn retries_429_then_succeeds() {
        let server = MockServer::start().await;
        Mock::given(path("/flaky"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(path("/flaky"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;
        let f = Fetcher::new(config()).unwrap();
        let url = Url::parse(&format!("{}/flaky", server.uri())).unwrap();
        let r = f.get(&url, &policy(&server), &FetchOptions::default()).await.unwrap();
        assert_eq!(&r.body[..], b"ok");
    }

    #[tokio::test]
    async fn does_not_retry_404_and_reports_host_only() {
        let server = MockServer::start().await;
        Mock::given(path("/missing"))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;
        let f = Fetcher::new(config()).unwrap();
        let url = Url::parse(&format!("{}/missing?key=SECRET", server.uri())).unwrap();
        let err = f.get(&url, &policy(&server), &FetchOptions::default()).await.unwrap_err();
        assert!(matches!(err, FetchError::Status { status: 404, .. }));
        assert!(!err.to_string().contains("SECRET"), "{err}");
    }

    #[tokio::test]
    async fn paces_requests_to_a_rate_limited_host() {
        let server = MockServer::start().await;
        Mock::given(path("/p"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;
        let f = Arc::new(Fetcher::new(config()).unwrap());
        let policy = policy(&server).with_min_interval(Duration::from_millis(150));
        let url = Url::parse(&format!("{}/p", server.uri())).unwrap();
        let started = Instant::now();
        let calls = (0..4).map(|_| {
            let (f, policy, url) = (f.clone(), policy.clone(), url.clone());
            tokio::spawn(
                async move { f.get(&url, &policy, &FetchOptions::default()).await.unwrap() },
            )
        });
        for c in calls.collect::<Vec<_>>() {
            c.await.unwrap();
        }
        assert!(
            started.elapsed() >= Duration::from_millis(450),
            "4 requests at ≥150 ms spacing took {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn cancelled_waiters_do_not_delay_later_requests() {
        // Review finding 5b: each cancelled waiter used to keep its pacing reservation, so
        // five cancellations pushed the next real request out by five intervals.
        let server = MockServer::start().await;
        Mock::given(path("/p"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;
        let f = Arc::new(Fetcher::new(config()).unwrap());
        let policy = policy(&server).with_min_interval(Duration::from_millis(300));
        let url = Url::parse(&format!("{}/p", server.uri())).unwrap();
        let started = Instant::now();
        f.get(&url, &policy, &FetchOptions::default()).await.unwrap();
        for _ in 0..5 {
            let cancel = CancellationToken::new();
            let opts = FetchOptions { cancel: Some(cancel.clone()), ..FetchOptions::default() };
            let (f, policy, url) = (f.clone(), policy.clone(), url.clone());
            let waiter = tokio::spawn(async move { f.get(&url, &policy, &opts).await });
            tokio::time::sleep(Duration::from_millis(10)).await;
            cancel.cancel();
            assert_eq!(waiter.await.unwrap().unwrap_err(), FetchError::Cancelled);
        }
        f.get(&url, &policy, &FetchOptions::default()).await.unwrap();
        let elapsed = started.elapsed();
        assert!(elapsed >= Duration::from_millis(290), "still paced: {elapsed:?}");
        assert!(elapsed < Duration::from_millis(900), "cancelled waiters delayed it: {elapsed:?}");
    }

    #[tokio::test]
    async fn per_request_body_caps_are_authoritative_within_the_ceiling() {
        let server = MockServer::start().await;
        Mock::given(path("/mid"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'x'; 4096]))
            .mount(&server)
            .await;
        let f = Fetcher::new(FetchConfig { max_body_bytes: 1024, ..config() }).unwrap();
        let url = Url::parse(&format!("{}/mid", server.uri())).unwrap();
        assert!(
            f.get(&url, &policy(&server), &FetchOptions::default()).await.is_err(),
            "the configured default applies without a provider cap"
        );
        let opts = FetchOptions { max_body_bytes: Some(8192), ..FetchOptions::default() };
        assert_eq!(f.get(&url, &policy(&server), &opts).await.unwrap().body.len(), 4096);
        let huge = FetchOptions { max_body_bytes: Some(usize::MAX), ..FetchOptions::default() };
        assert_eq!(f.body_limit(&huge), MAX_BODY_CEILING);
    }

    #[tokio::test]
    async fn per_request_retry_count_overrides_the_default() {
        let server = MockServer::start().await;
        Mock::given(path("/down"))
            .respond_with(ResponseTemplate::new(503))
            .expect(1)
            .mount(&server)
            .await;
        let f = Fetcher::new(config()).unwrap();
        let url = Url::parse(&format!("{}/down", server.uri())).unwrap();
        let opts = FetchOptions { retries: Some(0), ..FetchOptions::default() };
        let err = f.get(&url, &policy(&server), &opts).await.unwrap_err();
        assert!(matches!(err, FetchError::Status { status: 503, .. }), "{err:?}");
    }

    #[tokio::test]
    async fn treats_420_as_a_retryable_rate_limit() {
        let server = MockServer::start().await;
        Mock::given(path("/calm"))
            .respond_with(ResponseTemplate::new(420))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(path("/calm"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;
        let f = Fetcher::new(config()).unwrap();
        let url = Url::parse(&format!("{}/calm", server.uri())).unwrap();
        assert_eq!(
            &f.get(&url, &policy(&server), &FetchOptions::default()).await.unwrap().body[..],
            b"ok"
        );
    }

    #[tokio::test]
    async fn times_out() {
        let server = MockServer::start().await;
        Mock::given(path("/slow"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
            .mount(&server)
            .await;
        let f = Fetcher::new(FetchConfig { retries: 0, request_timeout_s: 1, ..config() }).unwrap();
        let url = Url::parse(&format!("{}/slow", server.uri())).unwrap();
        let err = f.get(&url, &policy(&server), &FetchOptions::default()).await.unwrap_err();
        assert!(matches!(err, FetchError::Timeout { .. }), "{err:?}");
    }

    #[tokio::test]
    async fn cancellation_stops_a_request() {
        let server = MockServer::start().await;
        Mock::given(path("/slow"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
            .mount(&server)
            .await;
        let f = Fetcher::new(config()).unwrap();
        let url = Url::parse(&format!("{}/slow", server.uri())).unwrap();
        let cancel = CancellationToken::new();
        let opts = FetchOptions { cancel: Some(cancel.clone()), ..FetchOptions::default() };
        let canceller = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            cancel.cancel();
        });
        let err = f.get(&url, &policy(&server), &opts).await.unwrap_err();
        canceller.await.unwrap();
        assert_eq!(err, FetchError::Cancelled);
    }
}
