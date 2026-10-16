//! Provider behaviour against a local mock upstream: cadence, coalescing, cancellation.

use super::*;

#[tokio::test]
async fn concurrent_refreshes_share_one_upstream_request() {
    let (server, provider, fetcher, secrets) = provider_fixture().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_string(current_csv())
                .set_delay(Duration::from_millis(200)),
        )
        .expect(1)
        .mount(&server)
        .await;
    let ctx = context(&fetcher, &secrets);
    // Fifty callers arriving during one refresh all receive its result.
    let answers =
        futures::future::join_all((0..50).map(|_| provider.fetch(&ctx, Region::Global))).await;
    let first = answers[0].as_ref().unwrap();
    assert!(answers.iter().all(|a| a.as_ref().unwrap() == first));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[test]
fn cadence_and_retry_floor_never_go_below_ten_minutes() {
    let provider = FirmsProvider::new(FirmsConfig { cadence_s: 0, ..FirmsConfig::default() });
    assert_eq!(provider.cadence(), Duration::from_mins(10));
    assert_eq!(provider.retry_floor(), Duration::from_mins(10));
}

async fn provider_fixture()
-> (wiremock::MockServer, FirmsProvider, crate::fetch::Fetcher, crate::MemorySecretStore) {
    provider_with_sources(&["VIIRS_NOAA20_NRT"]).await
}

async fn provider_with_sources(
    sources: &[&str],
) -> (wiremock::MockServer, FirmsProvider, crate::fetch::Fetcher, crate::MemorySecretStore) {
    let server = wiremock::MockServer::start().await;
    let config = FirmsConfig {
        base_url: Url::parse(&format!("{}/firms/", server.uri())).unwrap(),
        sources: sources.iter().map(|s| (*s).to_owned()).collect(),
        cadence_s: 1800,
        ..FirmsConfig::default()
    };
    let fetch_config = crate::config::FetchConfig {
        allow_http_hosts: vec!["127.0.0.1".into()],
        retries: 2,
        ..crate::config::FetchConfig::default()
    };
    let fetcher = crate::fetch::Fetcher::new(fetch_config).unwrap();
    let secrets =
        crate::MemorySecretStore::with([(crate::SecretKey::FirmsMapKey, "synthetic-key")]);
    (server, FirmsProvider::new(config), fetcher, secrets)
}

fn context<'a>(
    fetcher: &'a crate::fetch::Fetcher,
    secrets: &'a crate::MemorySecretStore,
) -> FetchContext<'a> {
    FetchContext {
        fetcher,
        secrets,
        cancel: tokio_util::sync::CancellationToken::new(),
        cache_dir: None,
    }
}

fn current_csv() -> String {
    csv_at(10.0, 20.0)
}

/// One detection at (`lat`, `lon`), acquired now.
fn csv_at(lat: f64, lon: f64) -> String {
    let now = Timestamp::now();
    format!(
        "latitude,longitude,acq_date,acq_time,confidence,frp\n{lat},{lon},{},{},n,12\n",
        now.strftime("%Y-%m-%d"),
        now.strftime("%H%M")
    )
}

fn source_path(source: &str) -> String {
    format!("/firms/synthetic-key/{source}/world/2")
}

/// Answers `source` with `response` for its next `times` requests.
async fn respond(
    server: &wiremock::MockServer,
    source: &str,
    response: wiremock::ResponseTemplate,
    times: u64,
) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(source_path(source)))
        .respond_with(response)
        .up_to_n_times(times)
        .expect(times)
        .mount(server)
        .await;
}

/// Positions of the detections in an answer, sorted.
fn positions(answer: &Fetched) -> Vec<(f64, f64)> {
    let FeedItems::Fires(cells) = &answer.items else { panic!("unexpected feed") };
    let mut positions: Vec<(f64, f64)> = cells.iter().map(|c| (c.lat, c.lon)).collect();
    positions.sort_by(|a, b| a.0.total_cmp(&b.0));
    positions
}

/// Moves every remembered moment of the provider `by` into the past, as if that much
/// time had passed.
async fn rewind(provider: &FirmsProvider, by: Duration) {
    let back = |t: Instant| t.checked_sub(by).unwrap();
    let mut state = provider.state.lock().await;
    state.last_start = state.last_start.map(back);
    state.last_refresh = state.last_refresh.map(back);
    for data in state.sources.iter_mut().flatten() {
        data.started = back(data.started);
    }
}

async fn wait_for_requests(server: &wiremock::MockServer, n: usize) {
    while server.received_requests().await.unwrap().len() < n {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn a_failed_source_keeps_its_last_detections_until_it_recovers() {
    let (server, provider, fetcher, secrets) =
        provider_with_sources(&["SOURCE_A", "SOURCE_B"]).await;
    let ok = |lat, lon| wiremock::ResponseTemplate::new(200).set_body_string(csv_at(lat, lon));
    respond(&server, "SOURCE_A", ok(10.0, 20.0), 2).await;
    respond(&server, "SOURCE_B", ok(30.0, 40.0), 1).await;
    respond(&server, "SOURCE_B", wiremock::ResponseTemplate::new(503), 1).await;
    let ctx = context(&fetcher, &secrets);
    let first = provider.fetch(&ctx, Region::Global).await.unwrap();
    assert_eq!(positions(&first), vec![(10.0, 20.0), (30.0, 40.0)]);
    assert_eq!(first.degraded, None);

    rewind(&provider, Duration::from_secs(1801)).await;
    let second = provider.fetch(&ctx, Region::Global).await.unwrap();
    // B failed: its earlier detections stay on the map, and the answer says so and is
    // as old as the oldest data in it.
    assert_eq!(positions(&second), positions(&first));
    let reason = second.degraded.clone().unwrap();
    assert!(reason.contains("1 of 2 sources failed") && reason.contains("Earlier"), "{reason}");
    assert_eq!(second.as_of, first.as_of);
    assert_eq!(provider.fetch(&ctx, Region::Global).await.unwrap(), second, "cooldown answer");
    assert_eq!(server.received_requests().await.unwrap().len(), 4);
}

#[tokio::test]
async fn a_failed_source_stops_showing_detections_older_than_a_day() {
    let (server, provider, fetcher, secrets) =
        provider_with_sources(&["SOURCE_A", "SOURCE_B"]).await;
    let ok = |lat, lon| wiremock::ResponseTemplate::new(200).set_body_string(csv_at(lat, lon));
    respond(&server, "SOURCE_A", ok(10.0, 20.0), 2).await;
    respond(&server, "SOURCE_B", ok(30.0, 40.0), 1).await;
    respond(&server, "SOURCE_B", wiremock::ResponseTemplate::new(503), 1).await;
    let ctx = context(&fetcher, &secrets);
    provider.fetch(&ctx, Region::Global).await.unwrap();
    rewind(&provider, Duration::from_secs(1801)).await;
    // B's last good download is from 25 hours ago.
    if let Some(b) = provider.state.lock().await.sources[1].as_mut() {
        b.as_of = Timestamp::now() - jiff::SignedDuration::from_hours(25);
    }
    let answer = provider.fetch(&ctx, Region::Global).await.unwrap();
    assert_eq!(positions(&answer), vec![(10.0, 20.0)]);
    let reason = answer.degraded.unwrap();
    assert_eq!(reason, "NASA FIRMS: 1 of 2 sources failed to refresh.");
}

#[tokio::test]
async fn an_interrupted_first_download_is_restarted_instead_of_cooling_down() {
    let (server, provider, fetcher, secrets) = provider_fixture().await;
    let ok = wiremock::ResponseTemplate::new(200).set_body_string(current_csv());
    respond(&server, "VIIRS_NOAA20_NRT", ok.clone().set_delay(Duration::from_secs(5)), 2).await;
    respond(&server, "VIIRS_NOAA20_NRT", ok, 1).await;

    // Cancelled once the request reached the upstream ...
    let cancelled = context(&fetcher, &secrets);
    let cancel = cancelled.cancel.clone();
    let (result, ()) = tokio::join!(provider.fetch(&cancelled, Region::Global), async {
        wait_for_requests(&server, 1).await;
        cancel.cancel();
    });
    assert!(matches!(result, Err(ProviderError::Fetch(FetchError::Cancelled))));
    // ... and dropped once the restarted request reached it.
    rewind(&provider, Duration::from_secs(61)).await;
    let dropped = context(&fetcher, &secrets);
    let mut future = Box::pin(provider.fetch(&dropped, Region::Global));
    tokio::select! {
        _ = &mut future => panic!("the slow download finished early"),
        () = wait_for_requests(&server, 2) => {}
    }
    drop(future);

    // Neither counts as a refresh: the next caller downloads, it does not cool down.
    rewind(&provider, Duration::from_secs(61)).await;
    let answer = provider.fetch(&context(&fetcher, &secrets), Region::Global).await.unwrap();
    assert_eq!(positions(&answer), vec![(10.0, 20.0)]);
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn a_restart_waits_out_the_spacing_instead_of_failing() {
    let (server, provider, fetcher, secrets) = provider_fixture().await;
    let response = wiremock::ResponseTemplate::new(200).set_body_string(current_csv());
    respond(&server, "VIIRS_NOAA20_NRT", response.clone().set_delay(Duration::from_secs(5)), 1)
        .await;
    respond(&server, "VIIRS_NOAA20_NRT", response, 1).await;
    let cancelled = context(&fetcher, &secrets);
    let cancel = cancelled.cancel.clone();
    let (result, ()) = tokio::join!(provider.fetch(&cancelled, Region::Global), async {
        wait_for_requests(&server, 1).await;
        cancel.cancel();
    });
    assert!(result.is_err());
    // The interrupted request went out 59.6 s ago (as far as the provider knows).
    rewind(&provider, Duration::from_millis(59_600)).await;
    let started = Instant::now();
    let answer = provider.fetch(&context(&fetcher, &secrets), Region::Global).await.unwrap();
    assert!(started.elapsed() >= Duration::from_millis(300), "{:?}", started.elapsed());
    assert_eq!(positions(&answer), vec![(10.0, 20.0)]);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn an_interrupted_refresh_keeps_finished_sources_and_does_not_ask_them_again() {
    let (server, provider, fetcher, secrets) =
        provider_with_sources(&["SOURCE_A", "SOURCE_B"]).await;
    let ok = |lat, lon| wiremock::ResponseTemplate::new(200).set_body_string(csv_at(lat, lon));
    respond(&server, "SOURCE_A", ok(10.0, 20.0), 1).await;
    respond(&server, "SOURCE_B", ok(30.0, 40.0).set_delay(Duration::from_secs(5)), 1).await;
    respond(&server, "SOURCE_B", ok(30.0, 40.0), 1).await;
    let cancelled = context(&fetcher, &secrets);
    let cancel = cancelled.cancel.clone();
    let (result, ()) = tokio::join!(provider.fetch(&cancelled, Region::Global), async {
        wait_for_requests(&server, 2).await;
        cancel.cancel();
    });
    assert!(result.is_err());
    rewind(&provider, Duration::from_secs(61)).await;
    let answer = provider.fetch(&context(&fetcher, &secrets), Region::Global).await.unwrap();
    assert_eq!(positions(&answer), vec![(10.0, 20.0), (30.0, 40.0)]);
    assert_eq!(answer.degraded, None);
    // A finished during the interrupted refresh, so only B was asked again.
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn downloads_are_allowed_the_firms_timeout_not_the_general_one() {
    let server = wiremock::MockServer::start().await;
    let large = wiremock::ResponseTemplate::new(200)
        .set_body_string(current_csv())
        .set_delay(Duration::from_millis(1_500));
    respond(&server, "VIIRS_NOAA20_NRT", large, 1).await;
    let provider = FirmsProvider::new(FirmsConfig {
        base_url: Url::parse(&format!("{}/firms/", server.uri())).unwrap(),
        sources: vec!["VIIRS_NOAA20_NRT".into()],
        request_timeout_s: 5,
        ..FirmsConfig::default()
    });
    let fetcher = crate::fetch::Fetcher::new(crate::config::FetchConfig {
        allow_http_hosts: vec!["127.0.0.1".into()],
        request_timeout_s: 1,
        ..crate::config::FetchConfig::default()
    })
    .unwrap();
    let secrets =
        crate::MemorySecretStore::with([(crate::SecretKey::FirmsMapKey, "synthetic-key")]);
    let answer = provider.fetch(&context(&fetcher, &secrets), Region::Global).await.unwrap();
    assert_eq!(positions(&answer), vec![(10.0, 20.0)]);
}

#[tokio::test]
async fn a_caller_waiting_for_a_running_refresh_can_be_cancelled() {
    let (server, provider, fetcher, secrets) = provider_fixture().await;
    let slow = wiremock::ResponseTemplate::new(200)
        .set_body_string(current_csv())
        .set_delay(Duration::from_secs(2));
    respond(&server, "VIIRS_NOAA20_NRT", slow, 1).await;
    let running = context(&fetcher, &secrets);
    let mut refresh = Box::pin(provider.fetch(&running, Region::Global));
    tokio::select! {
        _ = &mut refresh => panic!("the slow download finished early"),
        () = wait_for_requests(&server, 1) => {}
    }
    let waiting = context(&fetcher, &secrets);
    let mut waiter = Box::pin(provider.fetch(&waiting, Region::Global));
    tokio::select! {
        _ = &mut waiter => panic!("the waiter got an answer while the refresh held the state"),
        () = tokio::time::sleep(Duration::from_millis(50)) => {}
    }
    waiting.cancel.cancel();
    // The waiter gives up at once, without waiting for the running refresh to finish.
    let result = tokio::time::timeout(Duration::from_millis(500), waiter).await.unwrap();
    assert!(matches!(result, Err(ProviderError::Fetch(FetchError::Cancelled))));
    assert!(refresh.await.is_ok());
}

#[tokio::test]
async fn failures_obey_configured_cadence_and_never_use_fetcher_retries() {
    let (server, provider, fetcher, secrets) = provider_fixture().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(wiremock::ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(provider.retry_floor(), Duration::from_mins(30));
    let ctx = context(&fetcher, &secrets);
    assert!(provider.fetch(&ctx, Region::Global).await.is_err());
    // Simulate elapsed 10-minute floor: configured 30-minute cooldown still applies.
    rewind(&provider, Duration::from_secs(601)).await;
    let err = provider.fetch(&ctx, Region::Global).await.unwrap_err();
    assert!(err.to_string().contains("cooling down"));
}

#[tokio::test]
async fn cache_keeps_original_age_and_degradation_across_cooldown() {
    let (server, provider, fetcher, secrets) = provider_fixture().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(current_csv()))
        .expect(1)
        .mount(&server)
        .await;
    let ctx = context(&fetcher, &secrets);
    let first = provider.fetch(&ctx, Region::Global).await.unwrap();
    let cached = provider.fetch(&ctx, Region::Global).await.unwrap();
    assert_eq!(first.as_of, cached.as_of);
    assert_eq!(first.items, cached.items);
    server.reset().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(wiremock::ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    rewind(&provider, Duration::from_secs(1801)).await;
    let failed = provider.fetch(&ctx, Region::Global).await.unwrap();
    assert_eq!(first.as_of, failed.as_of);
    assert!(failed.degraded.is_some());
    let cooldown = provider.fetch(&ctx, Region::Global).await.unwrap();
    assert_eq!(failed, cooldown);
}

#[tokio::test]
async fn missing_key_does_not_use_cadence_or_send_requests() {
    let (server, provider, fetcher, _) = provider_fixture().await;
    let empty = crate::MemorySecretStore::default();
    let err = provider.fetch(&context(&fetcher, &empty), Region::Global).await.unwrap_err();
    assert!(matches!(err, ProviderError::Unconfigured(_)));
    assert!(provider.state.lock().await.last_start.is_none());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn cancelled_and_dropped_refreshes_release_the_lock_and_never_restart_at_once() {
    let (server, provider, fetcher, secrets) = provider_fixture().await;
    let slow = wiremock::ResponseTemplate::new(200)
        .set_body_string(current_csv())
        .set_delay(Duration::from_secs(2));
    respond(&server, "VIIRS_NOAA20_NRT", slow, 2).await;
    // Cancelled before it starts: nothing is sent or remembered.
    let ctx = context(&fetcher, &secrets);
    ctx.cancel.cancel();
    assert!(provider.fetch(&ctx, Region::Global).await.is_err());
    assert!(provider.state.lock().await.last_start.is_none());

    // Cancelled once its request went out: the lock is free, and a caller arriving right
    // away waits for the restart spacing instead of sending another request.
    let active = context(&fetcher, &secrets);
    let cancel = active.cancel.clone();
    let (result, ()) = tokio::join!(provider.fetch(&active, Region::Global), async {
        wait_for_requests(&server, 1).await;
        cancel.cancel();
    });
    assert!(matches!(result, Err(ProviderError::Fetch(FetchError::Cancelled))));
    assert!(provider.state.try_lock().is_ok());
    let eager = context(&fetcher, &secrets);
    let waited =
        tokio::time::timeout(Duration::from_millis(300), provider.fetch(&eager, Region::Global));
    assert!(waited.await.is_err(), "the restart must wait");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);

    // Dropped once its request went out: the same.
    rewind(&provider, RESTART_SPACING).await;
    let next = context(&fetcher, &secrets);
    let mut future = Box::pin(provider.fetch(&next, Region::Global));
    tokio::select! {
        _ = &mut future => panic!("delayed refresh finished early"),
        () = wait_for_requests(&server, 2) => {}
    }
    drop(future);
    assert!(provider.state.try_lock().is_ok());
    let eager = context(&fetcher, &secrets);
    let waited =
        tokio::time::timeout(Duration::from_millis(300), provider.fetch(&eager, Region::Global));
    assert!(waited.await.is_err(), "the restart must wait");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}
