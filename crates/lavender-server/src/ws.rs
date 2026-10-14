//! `GET /api/v1/stream` — the multiplexed live stream (ADR 0005). Client frames subscribe,
//! update and unsubscribe; each subscription is a core `FeedSubscription` forwarded as
//! `snapshot` / `error` frames. Latest-value semantics hold end to end: a slow client
//! skips intermediate snapshots instead of queueing them.
//!
//! Limits (security spec §2): connections in total and per client; 64 KiB messages and
//! frames; an inbound message rate per connection; subscriptions per connection; a region
//! budget per client; an idle deadline (the server pings, and pongs count as activity);
//! and a send timeout, so a client that stops reading is closed instead of holding its
//! subscriptions forever. Every limit that ends a connection closes it with a code and a
//! reason.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code};
use axum::extract::{OriginalUri, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures::{Sink, SinkExt, StreamExt};
use lavender_core::{FeedSubscription, RegionBudget, SubscriptionEvent};
use lavender_domain::text::clean;
use lavender_domain::{ApiError, ClientFrame, ErrorCode, FeedQuery, ServerFrame};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::AppState;
use crate::clients::{ConnectionPermit, Refusal};
use crate::error::{HttpError, json_error};
use crate::security::{ClientIp, is_same_origin};

const MAX_MESSAGE_BYTES: usize = 64 * 1024;
const MAX_INVALID_FRAMES: u32 = 10;
/// Frames queued for one connection's writer.
const OUTBOUND_QUEUE: usize = 16;
/// `Retry-After` seconds when the server is full.
const FULL_RETRY_AFTER: &str = "10";

/// Stream limits taken from the server settings.
#[derive(Debug, Clone, Copy)]
pub struct StreamLimits {
    /// A connection that sends nothing for this long is closed.
    pub idle_timeout: Duration,
    /// A frame the client does not accept within this long closes the connection.
    pub send_timeout: Duration,
    pub messages_per_s: f64,
    pub message_burst: f64,
}

impl StreamLimits {
    /// Pings go out at 40 % of the idle timeout, so a live client answers well in time.
    fn ping_every(&self) -> Duration {
        (self.idle_timeout * 2 / 5).max(Duration::from_millis(100))
    }
}

pub async fn stream(
    State(state): State<AppState>,
    ClientIp(client): ClientIp,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    // Browsers always send Origin on WebSocket upgrades; a foreign one is refused.
    // Non-browser clients (CLI, tests) without Origin are allowed: same-origin rules
    // protect browsers from cross-site pages, which always send Origin.
    let from_browser =
        headers.contains_key(header::ORIGIN) || headers.contains_key("sec-fetch-site");
    if from_browser && !is_same_origin(&headers, &uri) {
        return HttpError::new(
            ErrorCode::Forbidden,
            "Cross-site WebSocket connections are not accepted.",
        )
        .into_response();
    }
    let ws = match ws {
        Ok(ws) => ws,
        Err(rejection) => {
            return json_error(
                rejection.status(),
                ErrorCode::InvalidRequest,
                "This endpoint accepts WebSocket connections only.",
            );
        }
    };
    let permit = match state.clients.admit(client) {
        Ok(permit) => permit,
        Err(Refusal::TooManyFromClient) => {
            return HttpError::new(
                ErrorCode::RateLimited,
                "Too many live connections from this client. Close another tab or window.",
            )
            .into_response();
        }
        Err(Refusal::ServerFull) => {
            let mut response = json_error(
                StatusCode::SERVICE_UNAVAILABLE,
                ErrorCode::RateLimited,
                "Lavender has too many live connections. Retry shortly.",
            );
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static(FULL_RETRY_AFTER));
            return response;
        }
    };
    let budget = state.clients.budget(client);
    ws.max_message_size(MAX_MESSAGE_BYTES)
        .max_frame_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| connection(socket, state, budget, permit))
}

struct Forwarder {
    /// Latest-value: an update replaces any update the forwarder has not applied yet.
    control: watch::Sender<FeedQuery>,
    task: JoinHandle<()>,
}

impl Drop for Forwarder {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn error_frame(id: Option<String>, error: ApiError) -> ServerFrame {
    ServerFrame::Error { id, error }
}

async fn forward(
    id: String,
    mut sub: FeedSubscription,
    mut control: watch::Receiver<FeedQuery>,
    out: mpsc::Sender<ServerFrame>,
) {
    loop {
        let frame = tokio::select! {
            changed = control.changed() => {
                if changed.is_err() {
                    return;
                }
                let query = control.borrow_and_update().clone();
                match sub.update(query) {
                    Ok(()) => continue,
                    Err(e) => error_frame(Some(id.clone()), ApiError::from(&e)),
                }
            }
            event = sub.next() => match event {
                Some(SubscriptionEvent::Snapshot(s)) => {
                    ServerFrame::Snapshot { id: id.clone(), snapshot: (*s).clone() }
                }
                Some(SubscriptionEvent::Error(e)) => error_frame(Some(id.clone()), e),
                None => return,
            },
        };
        if out.send(frame).await.is_err() {
            return;
        }
    }
}

/// Serialises outgoing frames, pings, and closes the connection. Every send is bounded
/// by `send_timeout`: a client that stops reading ends the writer (and the connection)
/// instead of stalling it. `close` carries the close code and reason; dropping it ends
/// the connection normally.
fn spawn_writer<S>(
    mut sink: S,
    mut out_rx: mpsc::Receiver<ServerFrame>,
    mut close: oneshot::Receiver<(u16, &'static str)>,
    ping_every: Duration,
    send_timeout: Duration,
) -> JoinHandle<()>
where
    S: Sink<Message> + Unpin + Send + 'static,
    S::Error: Send,
{
    tokio::spawn(async move {
        let mut ping = tokio::time::interval(ping_every);
        ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
        ping.tick().await;
        loop {
            let message = tokio::select! {
                biased;
                reason = &mut close => {
                    let frame = reason
                        .ok()
                        .map(|(code, reason)| CloseFrame { code, reason: reason.into() });
                    let _ = tokio::time::timeout(send_timeout, sink.send(Message::Close(frame)))
                        .await;
                    return;
                }
                frame = out_rx.recv() => match frame {
                    Some(frame) => match serde_json::to_string(&frame) {
                        Ok(text) => Message::Text(text.into()),
                        Err(_) => continue,
                    },
                    None => return,
                },
                _ = ping.tick() => Message::Ping(Bytes::new()),
            };
            if !matches!(tokio::time::timeout(send_timeout, sink.send(message)).await, Ok(Ok(()))) {
                tracing::debug!("WebSocket client stopped reading; closing the connection");
                return;
            }
        }
    })
}

/// Token bucket for one connection's inbound messages.
#[derive(Debug)]
struct MessageRate {
    tokens: f64,
    per_s: f64,
    burst: f64,
    updated: Instant,
}

impl MessageRate {
    fn new(per_s: f64, burst: f64) -> Self {
        Self { tokens: burst, per_s, burst, updated: Instant::now() }
    }

    fn allow(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.updated).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.per_s).min(self.burst);
        self.updated = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Per-connection subscription table.
struct Connection {
    state: AppState,
    out: mpsc::Sender<ServerFrame>,
    subs: HashMap<String, Forwarder>,
    budget: Arc<RegionBudget>,
}

impl Connection {
    /// Connection-level replies never wait: when the client is not reading, the queue is
    /// full and the reply is dropped (the send timeout closes such a client anyway).
    fn reply(&self, frame: ServerFrame) {
        let _ = self.out.try_send(frame);
    }

    fn reply_error(&self, id: Option<String>, code: ErrorCode, message: &str) {
        self.reply(error_frame(id, ApiError::new(code, message)));
    }

    fn subscribe(&mut self, id: String, query: FeedQuery) {
        if id.is_empty() || id.len() > 64 || self.subs.contains_key(&id) {
            return self.reply_error(
                Some(id),
                ErrorCode::InvalidRequest,
                "subscription ids must be unique and 1–64 characters",
            );
        }
        if self.subs.len() >= self.state.max_subscriptions {
            return self.reply_error(
                Some(id),
                ErrorCode::RateLimited,
                "too many subscriptions on this connection",
            );
        }
        match self.state.core.subscribe_with_budget(query.clone(), self.budget.clone()) {
            Ok(sub) => {
                let (control, control_rx) = watch::channel(query);
                let task = tokio::spawn(forward(id.clone(), sub, control_rx, self.out.clone()));
                self.subs.insert(id, Forwarder { control, task });
            }
            Err(e) => self.reply(error_frame(Some(id), ApiError::from(&e))),
        }
    }

    fn handle(&mut self, frame: ClientFrame) {
        match frame {
            ClientFrame::Subscribe { id, query } => self.subscribe(id, query),
            ClientFrame::Update { id, query } => match self.subs.get(&id) {
                Some(f) => {
                    f.control.send_replace(query);
                }
                None => self.reply_error(Some(id), ErrorCode::NotFound, "no such subscription"),
            },
            ClientFrame::Unsubscribe { id } => {
                self.subs.remove(&id);
            }
        }
    }
}

/// How the read loop ended.
enum Ended {
    /// The client closed or the socket failed.
    ClientGone,
    /// The writer gave up (the client stopped reading).
    WriterGone,
    /// A limit was hit: close with this code and reason.
    Close(u16, &'static str),
}

async fn connection(
    socket: WebSocket,
    state: AppState,
    budget: Arc<RegionBudget>,
    _permit: ConnectionPermit,
) {
    let limits = state.stream;
    let (sink, mut incoming) = socket.split();
    let (out_tx, out_rx) = mpsc::channel::<ServerFrame>(OUTBOUND_QUEUE);
    let (close_tx, close_rx) = oneshot::channel();
    let mut writer = spawn_writer(sink, out_rx, close_rx, limits.ping_every(), limits.send_timeout);
    let mut conn = Connection { state, out: out_tx, subs: HashMap::new(), budget };
    let mut rate = MessageRate::new(limits.messages_per_s, limits.message_burst);
    let mut invalid = 0u32;
    let ended = loop {
        let next = tokio::select! {
            _ = &mut writer => break Ended::WriterGone,
            next = tokio::time::timeout(limits.idle_timeout, incoming.next()) => next,
        };
        let message = match next {
            Err(_) => break Ended::Close(close_code::AWAY, "idle timeout"),
            Ok(None | Some(Err(_))) => break Ended::ClientGone,
            Ok(Some(Ok(message))) => message,
        };
        if !rate.allow() {
            break Ended::Close(close_code::POLICY, "too many messages");
        }
        let text = match message {
            Message::Text(t) => t,
            Message::Close(_) => break Ended::ClientGone,
            _ => continue,
        };
        match serde_json::from_str::<ClientFrame>(&text) {
            Ok(frame) => conn.handle(frame),
            Err(e) => {
                invalid += 1;
                let reason = format!("unreadable frame: {}", clean(&e.to_string(), 160));
                conn.reply_error(None, ErrorCode::InvalidRequest, &reason);
                if invalid >= MAX_INVALID_FRAMES {
                    break Ended::Close(close_code::POLICY, "too many unreadable messages");
                }
            }
        }
    };
    // Unsubscribe everything first, so the regions are released at once.
    drop(conn);
    match ended {
        Ended::WriterGone => {}
        Ended::ClientGone => {
            drop(close_tx);
            let _ = writer.await;
        }
        Ended::Close(code, reason) => {
            let _ = close_tx.send((code, reason));
            let _ = writer.await;
        }
    }
}

#[cfg(test)]
mod tests;
