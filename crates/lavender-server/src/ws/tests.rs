//! Regression tests for the parent module.

use std::pin::Pin;
use std::task::{Context, Poll};

use super::*;

/// A client that never reads: its sink never becomes ready.
struct NeverReads;

impl Sink<Message> for NeverReads {
    type Error = std::convert::Infallible;
    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Pending
    }
    fn start_send(self: Pin<&mut Self>, _: Message) -> Result<(), Self::Error> {
        Ok(())
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Pending
    }
    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Pending
    }
}

#[tokio::test]
async fn a_client_that_stops_reading_ends_the_writer() {
    // Review finding 7: `sink.send` had no timeout, so a client that never read
    // stalled the writer and kept its subscriptions forever.
    let (out_tx, out_rx) = mpsc::channel(4);
    let (_close_tx, close_rx) = oneshot::channel();
    let writer = spawn_writer(
        NeverReads,
        out_rx,
        close_rx,
        Duration::from_secs(60),
        Duration::from_millis(100),
    );
    out_tx.send(error_frame(None, ApiError::new(ErrorCode::Internal, "x"))).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), writer)
        .await
        .expect("the writer gives up after the send timeout")
        .unwrap();
}

#[test]
fn message_rate_allows_the_burst_then_refuses() {
    let mut rate = MessageRate::new(1.0, 3.0);
    assert!((0..3).all(|_| rate.allow()));
    assert!(!rate.allow());
}
