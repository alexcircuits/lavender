//! Regression checks for request and panic diagnostics: useful fields without secrets.
// Test writers outside #[test] functions need the same allowances as API fixtures.
#![allow(clippy::unwrap_used)]
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::{LavenderCore, config::CoreConfig};
use tower::ServiceExt;
use tracing::instrument::WithSubscriber;

use crate::{app, config::ServerSettings, panic_response};

#[derive(Clone, Default)]
struct LogBuffer(Arc<Mutex<Vec<u8>>>);

impl Write for LogBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn subscriber(logs: &LogBuffer) -> impl tracing::Subscriber + Send + Sync + 'static {
    let writer = logs.clone();
    tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(move || writer.clone())
        .finish()
}

#[tokio::test]
async fn request_logs_and_error_envelopes_exclude_query_and_authorization_secrets() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(vec![]).build().unwrap();
    let router = app(core.clone(), &ServerSettings::default());
    let secrets = [
        "query-key-sentinel",
        "query-token-sentinel",
        "api-key-sentinel",
        "authorization-sentinel",
    ];
    let request = Request::get("/api/v1/search?q=x&limit=invalid&key=query-key-sentinel&token=query-token-sentinel&api_key=api-key-sentinel")
        .header(header::HOST, "localhost:7410")
        .header(header::AUTHORIZATION, "Bearer authorization-sentinel")
        .body(Body::empty()).unwrap();
    let logs = LogBuffer::default();
    let response = router.oneshot(request).with_subscriber(subscriber(&logs)).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body.to_vec()).unwrap();
    let logs = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    assert!(
        logs.contains("/api/v1/search") && logs.contains("GET"),
        "request diagnostics must retain method/path"
    );
    for secret in secrets {
        assert!(!logs.contains(secret), "request log includes a credential sentinel");
        assert!(!body.contains(secret), "error envelope includes a credential sentinel");
    }
    core.shutdown();
}

#[tokio::test]
async fn caught_panic_response_and_logs_exclude_panic_payload() {
    let logs = LogBuffer::default();
    let response = tracing::subscriber::with_default(subscriber(&logs), || {
        panic_response(Box::new(String::from("panic-secret-sentinel")))
    });
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body.to_vec()).unwrap();
    let logs = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    assert!(logs.contains("request handler panicked"), "failure must still be logged");
    assert!(!logs.contains("panic-secret-sentinel"), "log includes panic payload");
    assert!(!body.contains("panic-secret-sentinel"), "response includes panic payload");
    let body: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["code"], "internal");
}
