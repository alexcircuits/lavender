//! Regression tests for the parent module.

use super::*;

const SAMPLE: &str = r#"[
  {"OBJECT_NAME":"TEST STATION","OBJECT_ID":"1998-067A","EPOCH":"2026-10-10T12:00:00.000000","MEAN_MOTION":15.5,"ECCENTRICITY":0.0007,"INCLINATION":51.64,"RA_OF_ASC_NODE":247.46,"ARG_OF_PERICENTER":130.53,"MEAN_ANOMALY":325.03,"EPHEMERIS_TYPE":0,"CLASSIFICATION_TYPE":"U","NORAD_CAT_ID":25544,"ELEMENT_SET_NO":999,"REV_AT_EPOCH":44393,"BSTAR":0.0003,"MEAN_MOTION_DOT":0.00016,"MEAN_MOTION_DDOT":0},
  {"OBJECT_NAME":"BROKEN","EPOCH":"2026-10-10T12:00:00","MEAN_MOTION":-1,"ECCENTRICITY":0.1,"INCLINATION":10,"RA_OF_ASC_NODE":0,"ARG_OF_PERICENTER":0,"MEAN_ANOMALY":0,"NORAD_CAT_ID":2,"BSTAR":0,"MEAN_MOTION_DOT":0,"MEAN_MOTION_DDOT":0}
]"#;

#[test]
fn parses_omm_and_drops_invalid_sets() {
    let (sats, dropped) = parse(SAMPLE.as_bytes(), "stations").unwrap();
    assert_eq!((sats.len(), dropped), (1, 1));
    let s = &sats[0];
    assert_eq!(s.norad_id, 25544);
    assert_eq!(s.group, "stations");
    assert_eq!(s.epoch.to_string(), "2026-10-10T12:00:00Z");
    lavender_geo::Propagator::new(s).unwrap();
}

#[test]
fn refuses_plain_text_answers() {
    assert!(parse(b"Invalid query: GROUP", "x").is_err());
}

#[test]
fn a_null_field_drops_only_its_element_set() {
    // Review finding 10: `"BSTAR": null` used to reject the whole group.
    let body = SAMPLE.replacen(
        r"[
  {",
        r#"[
  {"OBJECT_NAME":"NO DRAG","EPOCH":"2026-10-10T12:00:00","MEAN_MOTION":15.0,"ECCENTRICITY":0.001,"INCLINATION":50,"RA_OF_ASC_NODE":0,"ARG_OF_PERICENTER":0,"MEAN_ANOMALY":0,"NORAD_CAT_ID":3,"BSTAR":null,"MEAN_MOTION_DOT":0,"MEAN_MOTION_DDOT":0},
  "garbage",
  {"#,
        1,
    );
    let (sats, dropped) = parse(body.as_bytes(), "stations").unwrap();
    assert_eq!(sats.iter().map(|s| s.norad_id).collect::<Vec<_>>(), vec![25544]);
    assert_eq!(dropped, 3);
}

#[test]
fn builds_group_urls() {
    let p = CelestrakProvider::new(CelestrakConfig::default());
    assert_eq!(
        p.group_url("gps-ops").unwrap().as_str(),
        "https://celestrak.org/NORAD/elements/gp.php?GROUP=gps-ops&FORMAT=json"
    );
}

#[test]
fn failed_groups_wait_a_twelfth_of_the_cadence_and_downloads_a_full_cadence() {
    let p = CelestrakProvider::new(CelestrakConfig::default());
    assert_eq!(p.retry_floor(), Duration::from_secs(600));
    assert!(p.throttled_for("stations").is_none());
    p.record_attempt("stations", false);
    let wait = p.throttled_for("stations").unwrap();
    assert!(wait > Duration::from_secs(590) && wait <= Duration::from_secs(600), "{wait:?}");
    p.record_attempt("stations", true);
    assert!(p.throttled_for("stations").unwrap() > Duration::from_secs(7_000));
}

#[test]
fn readable_ages() {
    assert_eq!(format_duration(Duration::from_secs(45)), "45 s");
    assert_eq!(format_duration(Duration::from_secs(600)), "10 min");
    assert_eq!(format_duration(Duration::from_hours(5)), "5 h");
    assert_eq!(format_duration(Duration::from_hours(72)), "3 days");
}

#[tokio::test]
async fn memory_revalidation_reuses_omm_and_restarts_the_success_floor() {
    use crate::config::FetchConfig;
    use crate::fetch::Fetcher;
    use crate::secrets::MemorySecretStore;
    use jiff::SignedDuration;
    use tokio_util::sync::CancellationToken;
    use wiremock::matchers::{header, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(path("/gp.php"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(SAMPLE)
                .insert_header("etag", "\"memory-v1\""),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/gp.php"))
        .and(header("if-none-match", "\"memory-v1\""))
        .respond_with(ResponseTemplate::new(304))
        .expect(1)
        .mount(&server)
        .await;
    let provider = CelestrakProvider::new(CelestrakConfig {
        base_url: Url::parse(&format!("{}/", server.uri())).unwrap(),
        groups: vec!["stations".into()],
        ..CelestrakConfig::default()
    });
    let fetcher = Fetcher::new(FetchConfig {
        allow_http_hosts: vec!["127.0.0.1".into()],
        ..FetchConfig::default()
    })
    .unwrap();
    let secrets = MemorySecretStore::default();
    let ctx = FetchContext {
        fetcher: &fetcher,
        secrets: &secrets,
        cancel: CancellationToken::new(),
        cache_dir: None,
    };
    let first = provider.fetch(&ctx, Region::Global).await.unwrap();
    let old = Timestamp::now().checked_sub(SignedDuration::from_hours(3)).unwrap();
    {
        let mut memory = provider.memory.lock().unwrap();
        let data = memory.get_mut("stations").unwrap();
        data.downloaded_at = old;
        data.validated_at = old;
    }
    provider.attempts.lock().unwrap().get_mut("stations").unwrap().at =
        Instant::now().checked_sub(Duration::from_hours(3)).unwrap();
    let before = Timestamp::now();
    let second = provider.fetch(&ctx, Region::Global).await.unwrap();
    assert_eq!(first.items, second.items);
    assert_eq!(first.notices, second.notices, "dropped record accounting survives 304");
    assert!(second.as_of.unwrap() >= before);
    assert_eq!(provider.memory.lock().unwrap()["stations"].downloaded_at, old);
    assert!(provider.throttled_for("stations").unwrap() > Duration::from_secs(7_000));
    assert_eq!(provider.fetch(&ctx, Region::Global).await.unwrap().as_of, second.as_of);
}

#[test]
fn a_304_without_a_cached_representation_is_an_explicit_error() {
    let response = FetchResponse {
        status: 304,
        body: Bytes::new(),
        elapsed: Duration::ZERO,
        validators: None,
        headers: reqwest::header::HeaderMap::new(),
    };
    let error = refreshed_group(response, None, "stations").unwrap_err();
    assert!(error.to_string().contains("without a cached representation"));
}
