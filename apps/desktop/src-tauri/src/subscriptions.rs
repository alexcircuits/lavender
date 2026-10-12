//! Live subscriptions per window. Each one forwards a core `FeedSubscription` to a Tauri
//! `Channel` as the same `ServerFrame` values the web transport sends over WebSocket.

use std::collections::HashMap;
use std::sync::Mutex;

use lavender_core::{FeedSubscription, SubscriptionEvent};
use lavender_domain::{ApiError, ErrorCode, FeedQuery, ServerFrame};
use tauri::async_runtime::{self, JoinHandle};
use tauri::ipc::Channel;
use tokio::sync::mpsc;

const MAX_PER_WINDOW: usize = 32;

struct Forwarder {
    control: mpsc::Sender<FeedQuery>,
    task: JoinHandle<()>,
}

impl Drop for Forwarder {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Subscriptions keyed by (window label, client-chosen id).
#[derive(Default)]
pub struct Subscriptions {
    inner: Mutex<HashMap<(String, String), Forwarder>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

async fn forward(
    id: String,
    mut sub: FeedSubscription,
    mut control: mpsc::Receiver<FeedQuery>,
    channel: Channel<ServerFrame>,
) {
    loop {
        tokio::select! {
            q = control.recv() => match q {
                Some(query) => {
                    if let Err(e) = sub.update(query) {
                        let _ = channel.send(ServerFrame::Error { id: Some(id.clone()), error: ApiError::from(&e) });
                    }
                }
                None => return,
            },
            event = sub.next() => {
                let frame = match event {
                    Some(SubscriptionEvent::Snapshot(s)) => ServerFrame::Snapshot { id: id.clone(), snapshot: (*s).clone() },
                    Some(SubscriptionEvent::Error(error)) => ServerFrame::Error { id: Some(id.clone()), error },
                    None => return,
                };
                if channel.send(frame).is_err() {
                    return;
                }
            }
        }
    }
}

impl Subscriptions {
    /// Registers and starts forwarding a subscription.
    pub fn open(
        &self,
        window: &str,
        id: String,
        sub: FeedSubscription,
        channel: Channel<ServerFrame>,
    ) -> Result<(), ApiError> {
        if id.is_empty() || id.len() > 64 {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "subscription ids must be 1–64 characters",
            ));
        }
        let mut map = lock(&self.inner);
        let key = (window.to_owned(), id.clone());
        if map.contains_key(&key) {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "subscription ids must be unique",
            ));
        }
        if map.keys().filter(|(w, _)| w == window).count() >= MAX_PER_WINDOW {
            return Err(ApiError::new(
                ErrorCode::RateLimited,
                "too many subscriptions in this window",
            ));
        }
        let (control, control_rx) = mpsc::channel(4);
        let task = async_runtime::spawn(forward(id, sub, control_rx, channel));
        map.insert(key, Forwarder { control, task });
        Ok(())
    }

    pub fn update(&self, window: &str, id: &str, query: FeedQuery) -> Result<(), ApiError> {
        let map = lock(&self.inner);
        let f = map
            .get(&(window.to_owned(), id.to_owned()))
            .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "no such subscription"))?;
        f.control
            .try_send(query)
            .map_err(|_| ApiError::new(ErrorCode::RateLimited, "updates are arriving too fast"))
    }

    pub fn close(&self, window: &str, id: &str) {
        lock(&self.inner).remove(&(window.to_owned(), id.to_owned()));
    }

    /// Drops every subscription of a closed window (stopping its refresh work).
    pub fn close_window(&self, window: &str) {
        lock(&self.inner).retain(|(w, _), _| w != window);
    }
}
