#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lavender_core::OverpassClient;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_overpass_failover_and_cooldown() {
    let server1 = MockServer::start().await;
    let server2 = MockServer::start().await;

    // Server 1 responds with 503 error
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503).append_header("Retry-After", "60"))
        .mount(&server1)
        .await;

    // Server 2 responds with 200 OK
    let success_json = r#"{"elements":[{"type":"node","id":42}]}"#;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string(success_json))
        .mount(&server2)
        .await;

    let upstreams = format!("{}, {}", server1.uri(), server2.uri());
    let client = OverpassClient::new(&upstreams);

    let query = "data=[out:json];node(around:500,30.27,-97.74);out;";

    // First request: server 1 fails (503), fails over to server 2 (200)
    let payload = client.execute(query).await.unwrap();
    assert_eq!(payload.status, 200);
    assert!(payload.body.contains("\"id\":42"));

    // Second request: server 1 is cooled down, goes straight to server 2
    let payload2 = client.execute(query).await.unwrap();
    assert_eq!(payload2.status, 200);
    assert!(payload2.body.contains("\"id\":42"));
}
