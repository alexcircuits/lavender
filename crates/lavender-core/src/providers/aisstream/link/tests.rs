//! Link unit tests: the subscription message, close and retry classification, and the
//! production `wss://` path (egress-checked socket, rustls with SNI and hostname checks,
//! upgrade, subscription, frames) against a local TLS server. Its certificate is issued by
//! a throwaway CA generated in memory for each test, so no key is stored in the repository
//! and nothing but these tests ever trusts the CA.

use rcgen::{
    BasicConstraints, CertificateParams, CertifiedIssuer, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair, KeyUsagePurpose,
};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::rustls::{ClientConfig, RootCertStore, ServerConfig};

use super::*;
use crate::config::FetchConfig;
use crate::vessel_store::VesselLimits;
const KEY: &str = "test-key-not-real";
const CONFIRMATION: &[u8] =
    br#"{"MessageType":"SubscriptionConfirmation","Message":{"CompressionEnabled":false}}"#;
const POSITION: &[u8] = br#"{"MessageType":"PositionReport","MetaData":{"MMSI":230990021},"Message":{"PositionReport":{"UserID":230990021,"Valid":true,"Latitude":60.1,"Longitude":24.9,"Sog":10.0,"Cog":90.0,"TrueHeading":90}}}"#;

#[test]
fn the_subscription_names_the_key_boxes_and_message_types() {
    let message = subscription(
        &SecretString::from("test-key".to_owned()),
        &[[[-90.0, -180.0], [90.0, 180.0]]],
    );
    let value: serde_json::Value = serde_json::from_str(&message).unwrap();
    assert_eq!(value["APIKey"], "test-key");
    assert_eq!(value["BoundingBoxes"], serde_json::json!([[[-90.0, -180.0], [90.0, 180.0]]]));
    assert_eq!(
        value["FilterMessageTypes"],
        serde_json::json!(["PositionReport", "StandardClassBPositionReport", "ShipStaticData"])
    );
}

#[test]
fn close_reasons_and_retry_after_are_classified() {
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
    let close = |reason: &str| CloseFrame { code: CloseCode::Policy, reason: reason.into() };
    assert_eq!(
        classify_close(Some(&close("Api Key Is Not Valid"))),
        Failure::Rejected { status: None }
    );
    assert!(matches!(
        classify_close(Some(&close("too many connections"))),
        Failure::RateLimited { .. }
    ));
    assert_eq!(
        classify_close(Some(&close("bye"))),
        Failure::Transport("AISStream closed the connection (code 1008)".to_owned())
    );
    let mut headers = HeaderMap::new();
    headers.insert(header::RETRY_AFTER, HeaderValue::from_static("120"));
    assert_eq!(retry_after(&headers), Some(Duration::from_secs(120)));
    headers.insert(header::RETRY_AFTER, HeaderValue::from_static("Wed, 21 Oct 2026 07:28:00 GMT"));
    assert_eq!(retry_after(&headers), None);
}

/// A test CA and the `localhost` server certificate it issued, made fresh for each test.
struct TestPki {
    ca: CertificateDer<'static>,
    server_cert: CertificateDer<'static>,
    server_key: PrivateKeyDer<'static>,
}

fn test_pki() -> TestPki {
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    ca_params.distinguished_name.push(DnType::CommonName, "Lavender test CA (in memory)");
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let ca = CertifiedIssuer::self_signed(ca_params, KeyPair::generate().unwrap()).unwrap();

    let mut server_params = CertificateParams::new(vec!["localhost".to_owned()]).unwrap();
    server_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let server_key = KeyPair::generate().unwrap();
    let server_cert = server_params.signed_by(&server_key, &ca).unwrap();
    TestPki {
        ca: ca.der().clone(),
        server_cert: server_cert.der().clone(),
        server_key: PrivatePkcs8KeyDer::from(server_key.serialize_der()).into(),
    }
}

/// A `wss://` AISStream stand-in on 127.0.0.1 presenting the `localhost` test certificate.
/// Each connection reports the SNI name and subscription it received, then sends a
/// confirmation and one position and closes.
struct TlsServer {
    port: u16,
    subscriptions: mpsc::UnboundedReceiver<(Option<String>, serde_json::Value)>,
}

async fn tls_server(pki: &TestPki) -> TlsServer {
    let certs = vec![pki.server_cert.clone()];
    let key = pki.server_key.clone_key();
    let config =
        ServerConfig::builder().with_no_client_auth().with_single_cert(certs, key).unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, subscriptions) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let (acceptor, tx) = (acceptor.clone(), tx.clone());
            tokio::spawn(async move {
                let Ok(tls) = acceptor.accept(tcp).await else { return };
                let sni = tls.get_ref().1.server_name().map(str::to_owned);
                let Ok(mut ws) = tokio_tungstenite::accept_async(tls).await else { return };
                if let Some(Ok(Message::Text(text))) = ws.next().await {
                    let _ = tx.send((sni, serde_json::from_str(text.as_str()).unwrap_or_default()));
                }
                let _ = ws.send(Message::binary(CONFIRMATION.to_vec())).await;
                let _ = ws.send(Message::binary(POSITION.to_vec())).await;
                let _ = ws.close(None).await;
            });
        }
    });
    TlsServer { port, subscriptions }
}

/// A link to `url`; `localhost` and 127.0.0.1 are development hosts here, so the
/// egress policy lets them resolve to loopback (`wss` is required all the same).
fn link_to(url: &str) -> Link {
    let url = Url::parse(url).unwrap();
    let fetch = FetchConfig {
        allow_http_hosts: vec!["localhost".into(), "127.0.0.1".into()],
        ..FetchConfig::default()
    };
    let settings = LinkSettings {
        policy: HostPolicy::for_url(&url),
        url,
        boxes: vec![[[-90.0, -180.0], [90.0, 180.0]]],
        read_timeout: Duration::from_secs(10),
        steady_reports: true,
        idle_disconnect: None,
        reconnect_min: Duration::from_millis(50),
        reconnect_max: Duration::from_secs(1),
    };
    let store = VesselStore::new(VesselLimits {
        max_vessels: 10,
        max_track_points: 4,
        max_total_track_points: 40,
        retention: Duration::from_mins(30),
    });
    Link::new(settings, Arc::new(Fetcher::new(fetch).unwrap()), Arc::new(store))
}

fn trusting(pki: &TestPki) -> Connector {
    let mut roots = RootCertStore::empty();
    roots.add(pki.ca.clone()).unwrap();
    Connector::Rustls(Arc::new(
        ClientConfig::builder().with_root_certificates(roots).with_no_client_auth(),
    ))
}

async fn run(link: &Link, tls: Option<Connector>) -> Ended {
    let key = SecretString::from(KEY.to_owned());
    tokio::time::timeout(Duration::from_secs(10), session(&link.shared, &key, tls))
        .await
        .expect("the session ends when the server closes")
}

fn tls_failure(ended: &Ended) -> &str {
    match ended {
        Ended::Failed { failure: Failure::Transport(reason), .. } => reason,
        other => panic!("expected a transport failure, got {other:?}"),
    }
}

#[tokio::test]
async fn the_production_wss_path_streams_from_a_trusted_tls_server() {
    let pki = test_pki();
    let mut server = tls_server(&pki).await;
    let link = link_to(&format!("wss://localhost:{}/v0/stream", server.port));
    let ended = run(&link, Some(trusting(&pki))).await;
    assert_eq!(tls_failure(&ended), "AISStream closed the connection");
    let (sni, subscription) = server.subscriptions.recv().await.unwrap();
    assert_eq!(sni.as_deref(), Some("localhost"), "SNI names the configured host");
    assert_eq!(subscription["APIKey"], KEY);
    assert_eq!(subscription["FilterMessageTypes"][0], "PositionReport");
    assert_eq!(link.status().sessions, 1);
    assert_eq!(link.shared.store.stats().positioned, 1);
}

#[tokio::test]
async fn the_bundled_roots_refuse_an_unknown_authority_before_the_key_is_sent() {
    let mut server = tls_server(&test_pki()).await;
    let link = link_to(&format!("wss://localhost:{}/v0/stream", server.port));
    // `None` is exactly what the supervisor passes in production.
    let ended = run(&link, None).await;
    let reason = tls_failure(&ended);
    assert!(reason.starts_with("the TLS handshake failed ("), "{reason}");
    assert!(reason.contains("UnknownIssuer"), "{reason}");
    assert!(!reason.contains(KEY));
    assert!(server.subscriptions.try_recv().is_err(), "the key never reaches the server");
    assert_eq!(link.shared.store.stats().vessels, 0);
}

#[tokio::test]
async fn a_certificate_for_another_name_is_refused() {
    let pki = test_pki();
    let mut server = tls_server(&pki).await;
    let link = link_to(&format!("wss://127.0.0.1:{}/v0/stream", server.port));
    let ended = run(&link, Some(trusting(&pki))).await;
    let reason = tls_failure(&ended);
    assert!(reason.starts_with("the TLS handshake failed ("), "{reason}");
    assert!(reason.contains("not valid for name"), "{reason}");
    assert!(server.subscriptions.try_recv().is_err(), "the key never reaches the server");
}
