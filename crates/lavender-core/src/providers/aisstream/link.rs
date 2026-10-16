//! The persistent AISStream connection: connect → subscribe → stream, supervised by the
//! watchdog policy. One link (one socket) per provider, started by the first demand
//! and kept while vessels are wanted.
//!
//! The socket is opened through [`Fetcher::connect_stream`], so it obeys the same egress
//! policy as HTTP fetches (allowed host, `wss`, public addresses only). The API key is
//! sent only inside the subscription message; it never appears in URLs, logs, health or
//! errors.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures::{Sink, SinkExt, Stream, StreamExt};
use jiff::Timestamp;
use lavender_domain::text::clean;
use secrecy::{ExposeSecret, SecretString};
use tokio::sync::Notify;
use tokio::time::{Interval, MissedTickBehavior};
use tokio_tungstenite::Connector;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::client::Request;
use tokio_tungstenite::tungstenite::http::{HeaderMap, HeaderValue, header};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::protocol::frame::CloseFrame;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_util::sync::CancellationToken;
use url::Url;

use super::parsing::{Frame, MESSAGE_TYPES, parse_frame};
use super::watchdog::{Backoff, Failure, STABLE_AFTER, classify_status, classify_upstream_text};
use crate::fetch::{FetchError, Fetcher, HostPolicy};
use crate::sources;
use crate::vessel_store::VesselStore;

/// AIS envelopes are a few kilobytes; anything near this is not a feed message.
const MAX_FRAME_BYTES: usize = 1 << 20;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest wait to send the subscription or a ping. AISStream closes connections whose
/// subscription does not arrive within 3 s.
const SEND_TIMEOUT: Duration = Duration::from_secs(3);
/// Longest interval between checks that vessels are still wanted.
const DEMAND_CHECK: Duration = Duration::from_secs(5);
/// Messages in a row that Lavender cannot use (malformed, or of a type it did not ask
/// for) that end a session: the stream is not in the format Lavender reads.
const MAX_UNUSABLE_IN_ROW: u32 = 50;
/// Characters of a TLS failure reason kept for health and logs.
const MAX_TLS_REASON_CHARS: usize = 100;

/// How the link connects and when it gives up on a silent or unwanted connection.
#[derive(Debug, Clone)]
pub struct LinkSettings {
    pub url: Url,
    pub policy: HostPolicy,
    /// AISStream `BoundingBoxes`: `[[south, west], [north, east]]` corner pairs.
    pub boxes: Vec<[[f64; 2]; 2]>,
    /// A connection that sends nothing for this long, not even the answer to one of the
    /// pings sent every quarter of it, is dead. Until a session has proved itself (and
    /// always with `steady_reports`) it must deliver AIS messages within this time too.
    pub read_timeout: Duration,
    /// The subscription covers the whole world, where reports never pause for minutes: a
    /// session that delivers none for `read_timeout` is replaced even while its
    /// connection answers pings. Narrower subscriptions may be legitimately quiet.
    pub steady_reports: bool,
    /// Disconnect after this long without demand; `None` keeps the link up.
    pub idle_disconnect: Option<Duration>,
    pub reconnect_min: Duration,
    pub reconnect_max: Duration,
}

/// What the link is doing.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkState {
    /// Not running: vessels have not been requested (or demand lapsed), or no key is set.
    Idle,
    Connecting {
        attempt: u32,
    },
    /// Subscribed and receiving frames.
    Streaming {
        since: Timestamp,
    },
    /// Waiting after a failure until `retry_at`.
    Waiting {
        failure: Failure,
        retry_at: Timestamp,
    },
    /// Shut down for good.
    Stopped,
}

/// The link's state and counters, for the provider, health and tests.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkStatus {
    pub state: LinkState,
    /// The latest failure; cleared once the link streams again.
    pub last_failure: Option<Failure>,
    /// Connection attempts made.
    pub attempts: u64,
    /// Sessions that reached streaming.
    pub sessions: u64,
    pub frames: u64,
    pub malformed_frames: u64,
    /// Malformed frames since the current connection attempt began.
    pub malformed_this_session: u64,
    pub unavailable_positions: u64,
    pub last_frame_at: Option<Timestamp>,
}

impl Default for LinkStatus {
    fn default() -> Self {
        Self {
            state: LinkState::Idle,
            last_failure: None,
            attempts: 0,
            sessions: 0,
            frames: 0,
            malformed_frames: 0,
            malformed_this_session: 0,
            unavailable_positions: 0,
            last_frame_at: None,
        }
    }
}

#[derive(Debug)]
struct Control {
    key: Option<SecretString>,
    last_demand: Instant,
    running: bool,
}

#[derive(Debug)]
struct Shared {
    settings: LinkSettings,
    fetcher: Arc<Fetcher>,
    store: Arc<VesselStore>,
    control: Mutex<Control>,
    status: Mutex<LinkStatus>,
    key_changed: Notify,
    stop: CancellationToken,
}

/// Handle to the connection supervisor.
#[derive(Debug)]
pub struct Link {
    shared: Arc<Shared>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Link {
    #[must_use]
    pub fn new(settings: LinkSettings, fetcher: Arc<Fetcher>, store: Arc<VesselStore>) -> Self {
        Self {
            shared: Arc::new(Shared {
                settings,
                fetcher,
                store,
                control: Mutex::new(Control {
                    key: None,
                    last_demand: Instant::now(),
                    running: false,
                }),
                status: Mutex::new(LinkStatus::default()),
                key_changed: Notify::new(),
                stop: CancellationToken::new(),
            }),
        }
    }

    /// Vessels are wanted with this key: start the link if it is not running. A new
    /// key wakes a link that is waiting after a failure. Needs a Tokio runtime.
    pub fn demand(&self, key: SecretString) {
        if self.shared.stop.is_cancelled() {
            return;
        }
        let (start, changed) = {
            let mut control = lock(&self.shared.control);
            let changed =
                control.key.as_ref().map(ExposeSecret::expose_secret) != Some(key.expose_secret());
            control.key = Some(key);
            control.last_demand = Instant::now();
            (!std::mem::replace(&mut control.running, true), changed)
        };
        if changed {
            self.shared.key_changed.notify_waiters();
        }
        if start {
            match tokio::runtime::Handle::try_current() {
                Ok(runtime) => drop(runtime.spawn(supervise(self.shared.clone()))),
                Err(_) => lock(&self.shared.control).running = false,
            }
        }
    }

    /// The key was removed: the link stops at its next check and does not reconnect.
    pub fn forget_key(&self) {
        lock(&self.shared.control).key = None;
    }

    #[must_use]
    pub fn status(&self) -> LinkStatus {
        lock(&self.shared.status).clone()
    }

    /// Closes the connection and never reconnects.
    pub fn stop(&self) {
        self.shared.stop.cancel();
    }
}

impl Shared {
    /// The key to connect with, or `None` when vessels are no longer wanted (the
    /// supervisor then exits). Checked and cleared under one lock, so a concurrent demand
    /// either sees the supervisor running or starts a new one.
    fn wanted_key(&self) -> Option<SecretString> {
        let mut control = lock(&self.control);
        if self.wanted(&control) {
            control.key.clone()
        } else {
            control.running = false;
            None
        }
    }

    fn still_wanted(&self) -> bool {
        self.wanted(&lock(&self.control))
    }

    fn wanted(&self, control: &Control) -> bool {
        let lapsed =
            self.settings.idle_disconnect.is_some_and(|idle| control.last_demand.elapsed() >= idle);
        control.key.is_some() && !lapsed && !self.stop.is_cancelled()
    }

    fn update(&self, change: impl FnOnce(&mut LinkStatus)) {
        change(&mut lock(&self.status));
    }

    fn finish(&self, state: LinkState) {
        lock(&self.control).running = false;
        self.update(|s| s.state = state);
    }
}

/// Resets the running flag if the supervisor panics, so the next demand restarts it.
struct PanicGuard<'a>(&'a Shared);

impl Drop for PanicGuard<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.finish(LinkState::Idle);
        }
    }
}

/// How a session ended.
#[derive(Debug)]
enum Ended {
    Stopped,
    NotWanted,
    Failed { failure: Failure, streamed_for: Duration },
}

impl Ended {
    fn failed(failure: Failure, streaming_since: Option<Instant>) -> Self {
        Self::Failed {
            failure,
            streamed_for: streaming_since.map_or(Duration::ZERO, |s| s.elapsed()),
        }
    }
}

async fn supervise(shared: Arc<Shared>) {
    let _guard = PanicGuard(&shared);
    let settings = &shared.settings;
    let mut backoff = Backoff::new(settings.reconnect_min, settings.reconnect_max);
    let mut attempt: u32 = 0;
    loop {
        let Some(key) = shared.wanted_key() else {
            let state =
                if shared.stop.is_cancelled() { LinkState::Stopped } else { LinkState::Idle };
            shared.update(|s| s.state = state);
            return;
        };
        attempt = attempt.saturating_add(1);
        shared.update(|s| {
            s.state = LinkState::Connecting { attempt };
            s.attempts += 1;
            s.malformed_this_session = 0;
        });
        // `None`: TLS trusts the bundled webpki roots.
        let outcome = tokio::select! {
            biased;
            () = shared.stop.cancelled() => Ended::Stopped,
            ended = session(&shared, &key, None) => ended,
        };
        drop(key);
        let (failure, streamed_for) = match outcome {
            Ended::Stopped => return shared.finish(LinkState::Stopped),
            Ended::NotWanted => continue,
            Ended::Failed { failure, streamed_for } => (failure, streamed_for),
        };
        if streamed_for >= STABLE_AFTER {
            backoff.reset();
            attempt = 0;
        }
        let delay = backoff.delay(&failure);
        tracing::warn!(
            source = sources::AISSTREAM,
            error = %failure.describe(),
            retry_in_s = delay.as_secs(),
            "vessel stream failed; waiting before reconnecting"
        );
        let retry_at = Timestamp::now().checked_add(delay).unwrap_or(Timestamp::MAX);
        shared.update(|s| {
            s.state = LinkState::Waiting { failure: failure.clone(), retry_at };
            s.last_failure = Some(failure);
        });
        tokio::select! {
            biased;
            () = shared.stop.cancelled() => return shared.finish(LinkState::Stopped),
            () = shared.key_changed.notified() => backoff.reset(),
            () = tokio::time::sleep(delay) => {}
        }
    }
}

/// One connection: open the socket under the egress policy, run TLS (with `tls`, or
/// the bundled webpki roots when `None`) and the upgrade, subscribe, then read until the
/// connection fails, goes silent, or is no longer wanted.
async fn session(shared: &Shared, key: &SecretString, tls: Option<Connector>) -> Ended {
    let settings = &shared.settings;
    let tcp = match shared.fetcher.connect_stream(&settings.url, &settings.policy).await {
        Ok(tcp) => tcp,
        Err(error) => return Ended::failed(classify_fetch_error(&error), None),
    };
    let request = match handshake_request(&settings.url, shared.fetcher.user_agent()) {
        Ok(request) => request,
        Err(failure) => return Ended::failed(failure, None),
    };
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_FRAME_BYTES))
        .max_frame_size(Some(MAX_FRAME_BYTES));
    let handshake =
        tokio_tungstenite::client_async_tls_with_config(request, tcp, Some(config), tls);
    let mut ws = match tokio::time::timeout(HANDSHAKE_TIMEOUT, handshake).await {
        Ok(Ok((ws, _response))) => ws,
        Ok(Err(error)) => return Ended::failed(classify_handshake_error(&error), None),
        Err(_) => return Ended::failed(transport("the WebSocket handshake timed out"), None),
    };
    let subscription = Message::text(subscription(key, &settings.boxes));
    match tokio::time::timeout(SEND_TIMEOUT, ws.send(subscription)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Ended::failed(classify_ws_error(&error), None),
        Err(_) => return Ended::failed(transport("sending the subscription timed out"), None),
    }
    read_frames(shared, &mut ws).await
}

/// Reads until the connection fails, goes silent, sends only messages Lavender cannot
/// use, or vessels are no longer wanted. A ping goes out every quarter of the read
/// timeout, so a healthy connection is never silent even where no ship reports.
async fn read_frames<S>(shared: &Shared, ws: &mut S) -> Ended
where
    S: Stream<Item = Result<Message, WsError>> + Sink<Message, Error = WsError> + Unpin,
{
    let settings = &shared.settings;
    let read_timeout = settings.read_timeout;
    let mut demand_check =
        ticker(settings.idle_disconnect.map_or(DEMAND_CHECK, |idle| {
            (idle / 2).clamp(Duration::from_millis(50), DEMAND_CHECK)
        }));
    let mut ping = ticker((read_timeout / 4).max(Duration::from_millis(100)));
    // Any frame (pongs included) shows the connection is alive; a usable message shows
    // the stream works.
    let mut last_frame = Instant::now();
    let mut last_message = last_frame;
    let mut unusable_in_row = 0_u32;
    let mut streaming_since: Option<Instant> = None;
    loop {
        let (silent_since, silence) = if streaming_since.is_none() || settings.steady_reports {
            (last_message, "no AIS messages")
        } else {
            (last_frame, "no response")
        };
        let silent_until = tokio::time::Instant::from_std(silent_since + read_timeout);
        let next = tokio::select! {
            () = tokio::time::sleep_until(silent_until) => {
                let reason = format!("{silence} for {} s", read_timeout.as_secs());
                return Ended::failed(Failure::Transport(reason), streaming_since);
            }
            _ = demand_check.tick() => {
                if !shared.still_wanted() {
                    return Ended::NotWanted;
                }
                continue;
            }
            _ = ping.tick() => {
                if let Err(failure) = send_ping(ws).await {
                    return Ended::failed(failure, streaming_since);
                }
                continue;
            }
            next = ws.next() => next,
        };
        let message = match next {
            Some(Ok(message)) => message,
            Some(Err(error)) => return Ended::failed(classify_ws_error(&error), streaming_since),
            None => {
                return Ended::failed(
                    transport("AISStream closed the connection"),
                    streaming_since,
                );
            }
        };
        last_frame = Instant::now();
        let frame = match &message {
            Message::Text(text) => parse_frame(text.as_str().as_bytes()),
            Message::Binary(bytes) => parse_frame(bytes),
            Message::Close(close) => {
                return Ended::failed(classify_close(close.as_ref()), streaming_since);
            }
            Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
        };
        let now = Timestamp::now();
        let usable = match record(shared, frame, now) {
            Ok(usable) => usable,
            Err(failure) => return Ended::failed(failure, streaming_since),
        };
        if usable {
            last_message = last_frame;
            unusable_in_row = 0;
        } else {
            unusable_in_row += 1;
            if unusable_in_row >= MAX_UNUSABLE_IN_ROW {
                let reason = format!(
                    "AISStream sent {MAX_UNUSABLE_IN_ROW} messages in a row that Lavender could not read"
                );
                return Ended::failed(Failure::Transport(reason), streaming_since);
            }
        }
        let first_proof = usable && streaming_since.is_none();
        if first_proof {
            streaming_since = Some(Instant::now());
        }
        shared.update(|s| {
            s.frames += 1;
            s.last_frame_at = Some(now);
            if first_proof {
                s.state = LinkState::Streaming { since: now };
                s.last_failure = None;
                s.sessions += 1;
            }
        });
    }
}

/// An interval whose first tick is one `period` from now; missed ticks are skipped.
fn ticker(period: Duration) -> Interval {
    let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    interval
}

async fn send_ping<S>(ws: &mut S) -> Result<(), Failure>
where
    S: Sink<Message, Error = WsError> + Unpin,
{
    match tokio::time::timeout(SEND_TIMEOUT, ws.send(Message::Ping(Bytes::new()))).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(classify_ws_error(&error)),
        Err(_) => Err(transport("sending a ping timed out")),
    }
}

/// Stores one frame. `Ok(true)` when it is usable: a recognised AIS message or the
/// subscription confirmation, which also prove the stream works. Malformed frames and
/// types Lavender did not subscribe to are not. `Err` when AISStream reported an error.
fn record(shared: &Shared, frame: Frame, now: Timestamp) -> Result<bool, Failure> {
    match frame {
        Frame::Position(report) => {
            shared.store.record_position(report, now);
        }
        Frame::Details(report) => {
            shared.store.record_details(report, now);
        }
        Frame::PositionUnavailable(_) => shared.update(|s| s.unavailable_positions += 1),
        Frame::Confirmation => {}
        Frame::Ignored => return Ok(false),
        Frame::Malformed => {
            shared.update(|s| {
                s.malformed_frames += 1;
                s.malformed_this_session += 1;
            });
            return Ok(false);
        }
        // The text may echo subscription content, so only its class is kept.
        Frame::UpstreamError(text) => return Err(classify_upstream_text(&text)),
    }
    Ok(true)
}

fn transport(reason: &str) -> Failure {
    Failure::Transport(reason.to_owned())
}

/// The AISStream subscription message. This string holds the key: it is sent and
/// dropped, never logged.
fn subscription(key: &SecretString, boxes: &[[[f64; 2]; 2]]) -> String {
    serde_json::json!({
        "APIKey": key.expose_secret(),
        "BoundingBoxes": boxes,
        "FilterMessageTypes": MESSAGE_TYPES,
    })
    .to_string()
}

fn handshake_request(url: &Url, user_agent: &str) -> Result<Request, Failure> {
    let mut request = url.as_str().into_client_request().map_err(|_| {
        Failure::Refused("the configured URL is not a WebSocket address".to_owned())
    })?;
    if let Ok(value) = HeaderValue::from_str(user_agent) {
        request.headers_mut().insert(header::USER_AGENT, value);
    }
    Ok(request)
}

fn classify_fetch_error(error: &FetchError) -> Failure {
    match error {
        FetchError::HostNotAllowed { .. }
        | FetchError::InsecureScheme { .. }
        | FetchError::InsecureStream { .. }
        | FetchError::NonPublicAddress { .. } => Failure::Refused(error.to_string()),
        other => Failure::Transport(other.to_string()),
    }
}

/// Classifies a failed TLS or WebSocket handshake. Rustls reports TLS failures (an
/// untrusted certificate, a name mismatch, a fatal alert) as `InvalidData` I/O errors;
/// their text is rustls's own description, never upstream content.
fn classify_handshake_error(error: &WsError) -> Failure {
    match error {
        WsError::Io(io) if io.kind() == std::io::ErrorKind::InvalidData => {
            match io.get_ref().map(|inner| clean(&inner.to_string(), MAX_TLS_REASON_CHARS)) {
                Some(reason) if !reason.is_empty() => {
                    Failure::Transport(format!("the TLS handshake failed ({reason})"))
                }
                _ => transport("the TLS handshake failed"),
            }
        }
        other => classify_ws_error(other),
    }
}

fn classify_ws_error(error: &WsError) -> Failure {
    match error {
        WsError::Http(response) => {
            classify_status(response.status().as_u16(), retry_after(response.headers()))
        }
        WsError::Capacity(_) => transport("AISStream sent a frame larger than 1 MiB"),
        WsError::Tls(_) => transport("the TLS handshake failed"),
        WsError::Io(io) => Failure::Transport(format!("network error ({})", io.kind())),
        WsError::ConnectionClosed | WsError::AlreadyClosed => {
            transport("AISStream closed the connection")
        }
        _ => transport("the WebSocket protocol was violated"),
    }
}

fn classify_close(frame: Option<&CloseFrame>) -> Failure {
    let Some(frame) = frame else { return transport("AISStream closed the connection") };
    match classify_upstream_text(frame.reason.as_str()) {
        Failure::Transport(_) => Failure::Transport(format!(
            "AISStream closed the connection (code {})",
            u16::from(frame.code)
        )),
        failure => failure,
    }
}

/// `Retry-After` in delta-seconds; the HTTP-date form falls back to the watchdog floor.
fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(|s| Duration::from_secs(s.min(86_400)))
}

#[cfg(test)]
mod tests;
