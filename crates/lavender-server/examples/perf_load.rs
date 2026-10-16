//! Load test for the release `lavender-server` binary (ENG-1000,
//! `docs/performance/specification.md` §2 backend targets and §3 "Server latency"), in two
//! phases on 127.0.0.1:7428:
//!
//! 1. **Fake upstreams.** wiremock on 127.0.0.1:7429 serves the recorded USGS `2.5_day` feed
//!    and an adsb.lol point endpoint that answers every cell with synthetic contacts (moved a
//!    little on every request, so each refresh is a real update); every request is logged.
//!    The server uses them through `LAVENDER__*` overrides; providers without a fake are
//!    disabled. Measured: the cold first request, warm latency percentiles on one connection,
//!    and requests/s with percentiles under `--connections` clients, for snapshot, catalog and
//!    search routes; WebSocket fan-out to `--subscribers` clients of one (feed, region) key per
//!    update cycle, for a busy cell (1 000 contacts) and a quiet control cell (10 contacts)
//!    that separates the per-subscriber cost from the payload size; upstream calls per key;
//!    server RSS per phase.
//! 2. **Dense snapshots** (with `--fixtures <dir>` from `apps/web/perf/fixtures.ts`): the same
//!    binary in fixture mode answers worldwide queries over the SYNTHETIC populations the
//!    browser scenarios use (20 000 aircraft, 5 000 satellites, 100 000 fire detections).
//!    Fixture mode replaces only the upstream fetch and parse; the hub's composition,
//!    filtering and the JSON encoding are the production path. Measured as above, plus RSS
//!    with every fixture feed active.
//!
//! Client and server share this machine, so latencies include client scheduling and the
//! loopback hop; they are an upper bound of server time.
//!
//! ```text
//! cargo build --release --locked -p lavender-server --bin lavender-server --example perf_load
//! target/release/examples/perf_load --server target/release/lavender-server --out load.json \
//!     [--fixtures <dir>]
//! ```
//! Options: `--seconds 10` (per route), `--connections 32`, `--subscribers 100`, `--cycles 6`.

use std::collections::BTreeMap;
use std::error::Error;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::{SinkExt, StreamExt};
use serde_json::{Map, Value, json};
use tokio_tungstenite::tungstenite::Message;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

type Fallible<T> = Result<T, Box<dyn Error + Send + Sync>>;

const SERVER: &str = "127.0.0.1:7428";
const UPSTREAM: &str = "127.0.0.1:7429";
/// The shortest adsb.lol cadence the configuration accepts.
const AIRCRAFT_CADENCE_S: u64 = 5;
/// Contacts in an adsb.lol answer for a cell that is neither of the two below.
const AIRCRAFT_PER_CELL: u32 = 1_000;
/// Seconds of single-connection requests per route.
const UNLOADED_SECONDS: u64 = 5;
/// Secret variables the server reads without the `LAVENDER` prefix (legacy names).
const LEGACY_SECRET_VARS: [&str; 2] = ["LL2_API_TOKEN", "AISSTREAM_API_KEY"];
const USGS_RECORDED: &[u8] =
    include_bytes!("../../lavender-core/tests/fixtures/usgs-2.5_day.geojson");

/// One 5° adsb.lol cell: the query box inside it and the contacts the fake answers with.
#[derive(Debug)]
struct Cell {
    name: &'static str,
    bbox: [f64; 4],
    contacts: u32,
}

/// A busy 5° cell over Europe (20–25° E, 55–60° N).
const BUSY_CELL: Cell = Cell { name: "busy", bbox: [20.5, 55.5, 24.5, 59.5], contacts: 1_000 };
/// The control: the cell south of it (20–25° E, 50–55° N) with a hundredth of the payload.
const QUIET_CELL: Cell = Cell { name: "quiet", bbox: [20.5, 50.5, 24.5, 54.5], contacts: 10 };

impl Cell {
    /// The upstream path prefix of this cell (`v2/point/{lat}/{lon}/` at the cell centre).
    fn upstream_path(&self) -> String {
        let [west, south, east, north] = self.bbox;
        format!(
            "/adsb/v2/point/{:.4}/{:.4}/",
            f64::midpoint(south, north),
            f64::midpoint(west, east)
        )
    }

    fn query(&self) -> String {
        let [west, south, east, north] = self.bbox;
        format!("bbox={west},{south},{east},{north}")
    }
}

#[derive(Debug)]
struct Options {
    server: PathBuf,
    out: Option<PathBuf>,
    fixtures: Option<PathBuf>,
    seconds: u64,
    connections: usize,
    subscribers: usize,
    cycles: u64,
}

fn options() -> Fallible<Options> {
    let mut o = Options {
        server: PathBuf::new(),
        out: None,
        fixtures: None,
        seconds: 10,
        connections: 32,
        subscribers: 100,
        cycles: 6,
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--server" => o.server = PathBuf::from(value),
            "--out" => o.out = Some(PathBuf::from(value)),
            "--fixtures" => o.fixtures = Some(PathBuf::from(value)),
            "--seconds" => o.seconds = value.parse()?,
            "--connections" => o.connections = value.parse()?,
            "--subscribers" => o.subscribers = value.parse()?,
            "--cycles" => o.cycles = value.parse()?,
            other => return Err(format!("unknown option {other}").into()),
        }
    }
    if !o.server.is_file() {
        return Err("--server must name the release lavender-server binary".into());
    }
    if o.fixtures.as_ref().is_some_and(|dir| !dir.join("adsb-lol.json").is_file()) {
        return Err("--fixtures must name a directory written by apps/web/perf/fixtures.ts".into());
    }
    Ok(o)
}

/// Every upstream request the fake servers answered: (path, when).
type RequestLog = Arc<Mutex<Vec<(String, Instant)>>>;

fn record(log: &RequestLog, request: &Request) {
    if let Ok(mut entries) = log.lock() {
        entries.push((request.url.path().to_owned(), Instant::now()));
    }
}

/// When the upstream answered requests whose path starts with `prefix`.
fn upstream_requests(log: &RequestLog, prefix: &str) -> Vec<Instant> {
    log.lock().map_or_else(
        |_| Vec::new(),
        |entries| {
            entries.iter().filter(|(p, _)| p.starts_with(prefix)).map(|(_, at)| *at).collect()
        },
    )
}

/// The recorded USGS document, as recorded.
struct UsgsFeed(RequestLog);

impl Respond for UsgsFeed {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        record(&self.0, request);
        ResponseTemplate::new(200).set_body_raw(USGS_RECORDED, "application/json")
    }
}

/// Synthetic adsb.lol `v2/point/{lat}/{lon}/{radius}` answers around the requested centre.
struct AdsbCell {
    log: RequestLog,
    calls: AtomicU32,
}

impl Respond for AdsbCell {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        record(&self.log, request);
        let call = self.calls.fetch_add(1, Ordering::Relaxed);
        let path = request.url.path();
        let parts: Vec<f64> = path.split('/').filter_map(|part| part.parse().ok()).collect();
        let (lat, lon) = match parts[..] {
            [lat, lon, _radius] => (lat, lon),
            _ => (0.0, 0.0),
        };
        let count = [&BUSY_CELL, &QUIET_CELL]
            .into_iter()
            .find(|cell| path.starts_with(&cell.upstream_path()))
            .map_or(AIRCRAFT_PER_CELL, |cell| cell.contacts);
        let now_ms = jiff::Timestamp::now().as_millisecond();
        let drift = f64::from(call % 100) * 0.001;
        let contacts: Vec<String> = (0..count)
            .map(|i| {
                let (row, col) = (f64::from(i / 40), f64::from(i % 40));
                format!(
                    r#"{{"hex":"{hex:06x}","flight":"SYN{i:05} ","r":"SY-{i:05}","t":"A320","category":"A3","lat":{lat:.5},"lon":{lon:.5},"alt_baro":{alt},"alt_geom":{alt},"gs":430.5,"track":{track:.1},"baro_rate":0,"squawk":"2000","emergency":"none","nic":8,"rc":186,"version":2,"nac_p":9,"sil":3,"messages":{msgs},"seen_pos":0.3,"seen":0.1,"rssi":-20.1}}"#,
                    hex = 0x00c0_0000 + i,
                    lat = lat - 1.9 + row * 0.15 + drift,
                    lon = lon - 1.9 + col * 0.095 + drift,
                    alt = 5_000 + (i * 31) % 35_000,
                    track = f64::from((i * 7) % 360),
                    msgs = 1_000 + call,
                )
            })
            .collect();
        let body = format!(r#"{{"ac":[{}],"msg":"No error","now":{now_ms}}}"#, contacts.join(","));
        ResponseTemplate::new(200).set_body_raw(body.into_bytes(), "application/json")
    }
}

async fn fake_upstreams(log: &RequestLog) -> Fallible<MockServer> {
    let listener = TcpListener::bind(UPSTREAM)?;
    let server = MockServer::builder().listener(listener).start().await;
    Mock::given(method("GET"))
        .and(path("/usgs/2.5_day.geojson"))
        .respond_with(UsgsFeed(log.clone()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/adsb/v2/(point/.+|mil)$"))
        .respond_with(AdsbCell { log: log.clone(), calls: AtomicU32::new(0) })
        .mount(&server)
        .await;
    Ok(server)
}

/// The server child process; killed when dropped, whatever happens to the run.
#[derive(Debug)]
struct ServerProcess(Child);

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

type Settings = Vec<(&'static str, String)>;

fn quoted(value: &str) -> String {
    format!("\"{value}\"")
}

/// Settings both phases share: address, scratch directories, limits for a single client.
fn base_settings(scratch: &Path) -> Settings {
    let dir = |name: &str| quoted(&scratch.join(name).display().to_string());
    vec![
        ("LAVENDER__SERVER__ADDR", quoted(SERVER)),
        ("LAVENDER__SERVER__STATIC_DIR", dir("no-frontend")),
        ("LAVENDER__SERVER__DATA_DIR", dir("data")),
        ("LAVENDER__SERVER__CACHE_DIR", dir("cache")),
        // One client generates all load: the per-client limits would measure the limiter.
        ("LAVENDER__SERVER__RATE_LIMIT_PER_S", "1000000".to_owned()),
        ("LAVENDER__SERVER__RATE_LIMIT_BURST", "1000000".to_owned()),
        ("LAVENDER__SERVER__MAX_CONNECTIONS", "1024".to_owned()),
        ("LAVENDER__SERVER__MAX_CONNECTIONS_PER_CLIENT", "1024".to_owned()),
        ("RUST_LOG", "warn".to_owned()),
    ]
}

/// Phase 1: real providers against the fake upstreams; the others disabled.
fn upstream_settings(scratch: &Path) -> Settings {
    let upstream = format!("http://{UPSTREAM}");
    let mut settings = base_settings(scratch);
    settings.extend([
        ("LAVENDER__CORE__FETCH__ALLOW_HTTP_HOSTS", "[\"127.0.0.1\"]".to_owned()),
        ("LAVENDER__CORE__PROVIDERS__USGS__BASE_URL", quoted(&format!("{upstream}/usgs/"))),
        ("LAVENDER__CORE__PROVIDERS__ADSB_LOL__BASE_URL", quoted(&format!("{upstream}/adsb/"))),
        ("LAVENDER__CORE__PROVIDERS__ADSB_LOL__CADENCE_S", AIRCRAFT_CADENCE_S.to_string()),
        ("LAVENDER__CORE__PROVIDERS__CELESTRAK__ENABLED", "false".to_owned()),
        ("LAVENDER__CORE__PROVIDERS__CYCLONES__ENABLED", "false".to_owned()),
        ("LAVENDER__CORE__PROVIDERS__FIRMS__ENABLED", "false".to_owned()),
        ("LAVENDER__CORE__PROVIDERS__LAUNCHES__ENABLED", "false".to_owned()),
        ("LAVENDER__CORE__PROVIDERS__WIND__ENABLED", "false".to_owned()),
    ]);
    settings
}

/// Phase 2: fixture mode over the dense synthetic files.
fn fixture_settings(scratch: &Path, fixtures: &Path) -> Settings {
    let mut settings = base_settings(scratch);
    settings.extend([
        ("LAVENDER__SERVER__FIXTURES_DIR", quoted(&fixtures.display().to_string())),
        // The catalog offers a keyed source only when its key is set, also in fixture mode.
        // Fixture mode reads files instead of upstreams, so this placeholder goes nowhere.
        ("LAVENDER_FIRMS_MAP_KEY", "perf-fixture-placeholder".to_owned()),
    ]);
    settings
}

/// Starts the server with exactly `settings`: no inherited configuration, overrides or keys.
async fn start_server(
    client: &reqwest::Client,
    binary: &Path,
    settings: &Settings,
    scratch: &Path,
    log_name: &str,
) -> Fallible<ServerProcess> {
    let mut command = Command::new(binary);
    for (name, _) in std::env::vars().filter(|(name, _)| {
        name.starts_with("LAVENDER") || LEGACY_SECRET_VARS.contains(&name.as_str())
    }) {
        command.env_remove(name);
    }
    let log = std::fs::File::create(scratch.join(log_name))?;
    command.envs(settings.iter().cloned()).stdout(Stdio::null()).stderr(log);
    let mut server = ServerProcess(command.spawn()?);
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if let Some(status) = server.0.try_wait()? {
            return Err(format!("lavender-server exited during startup ({status})").into());
        }
        if client.get(format!("http://{SERVER}/healthz")).send().await.is_ok() {
            return Ok(server);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("lavender-server did not become healthy within 30 s".into())
}

/// Resident set size of a process in MiB (`ps`, available on macOS and Linux).
fn rss_mib(pid: u32) -> Option<f64> {
    let out = Command::new("ps").args(["-o", "rss=", "-p", &pid.to_string()]).output().ok()?;
    let kib: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(kib / 1024.0)
}

/// Nearest-rank percentile of sorted milliseconds.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    // Ranks are small positive integers.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
    let rank = ((p / 100.0) * sorted.len() as f64).ceil().max(1.0) as usize;
    sorted[rank.min(sorted.len()) - 1]
}

fn millis(d: Duration) -> f64 {
    d.as_secs_f64() * 1_000.0
}

/// One GET, reading the whole body. Returns the body size.
async fn fetch(client: &reqwest::Client, url: &str) -> Fallible<usize> {
    let response = client.get(url).send().await?;
    let status = response.status();
    let body = response.bytes().await?;
    if !status.is_success() {
        return Err(format!("HTTP {status}").into());
    }
    Ok(body.len())
}

/// `connections` concurrent keep-alive clients requesting `url` for `seconds`.
async fn load(client: &reqwest::Client, url: &str, connections: usize, seconds: u64) -> Value {
    // One second of the same traffic first, so pools and caches are warm.
    run_workers(client, url, connections, Duration::from_secs(1)).await;
    let duration = Duration::from_secs(seconds);
    let started = Instant::now();
    let results = run_workers(client, url, connections, duration).await;
    let elapsed = started.elapsed().as_secs_f64();
    let mut latencies: Vec<f64> = results.iter().flat_map(|r| r.0.iter().copied()).collect();
    latencies.sort_by(f64::total_cmp);
    let errors: usize = results.iter().map(|r| r.1).sum();
    let bytes = results.iter().map(|r| r.2).max().unwrap_or(0);
    // Request counts are far below 2^52.
    #[allow(clippy::cast_precision_loss)]
    let rps = latencies.len() as f64 / elapsed;
    json!({
        "requests": latencies.len(), "errors": errors, "seconds": elapsed,
        "requests_per_s": rps, "response_bytes": bytes,
        "p50_ms": percentile(&latencies, 50.0), "p90_ms": percentile(&latencies, 90.0),
        "p95_ms": percentile(&latencies, 95.0), "p99_ms": percentile(&latencies, 99.0),
        "max_ms": latencies.last().copied().unwrap_or(f64::NAN),
    })
}

async fn run_workers(
    client: &reqwest::Client,
    url: &str,
    connections: usize,
    duration: Duration,
) -> Vec<(Vec<f64>, usize, usize)> {
    let deadline = Instant::now() + duration;
    let workers = (0..connections).map(|_| {
        let (client, url) = (client.clone(), url.to_owned());
        tokio::spawn(async move {
            let (mut latencies, mut errors, mut bytes) = (Vec::new(), 0, 0);
            while Instant::now() < deadline {
                let started = Instant::now();
                match fetch(&client, &url).await {
                    Ok(size) => {
                        latencies.push(millis(started.elapsed()));
                        bytes = size;
                    }
                    Err(_) => errors += 1,
                }
            }
            (latencies, errors, bytes)
        })
    });
    futures::future::join_all(workers).await.into_iter().filter_map(Result::ok).collect()
}

/// A named route of the phase.
type Route = (&'static str, String);

/// The first request per route, in order: each one makes its slot fetch (or read) its data.
/// A route that does not answer would make its numbers meaningless, so it stops the run.
async fn cold_requests(client: &reqwest::Client, routes: &[Route]) -> Fallible<Value> {
    let mut cold = Map::new();
    for (name, route) in routes {
        let started = Instant::now();
        let bytes = fetch(client, &format!("http://{SERVER}{route}"))
            .await
            .map_err(|e| format!("GET {route} failed on its first request: {e}"))?;
        let ms = millis(started.elapsed());
        cold.insert((*name).to_owned(), json!({"route": route, "ms": ms, "bytes": bytes}));
    }
    Ok(Value::Object(cold))
}

/// One-connection latency, then `connections` concurrent clients, route by route.
async fn warm_requests(client: &reqwest::Client, routes: &[Route], options: &Options) -> Value {
    let mut unloaded = Map::new();
    for (name, route) in routes {
        let url = format!("http://{SERVER}{route}");
        unloaded.insert((*name).to_owned(), load(client, &url, 1, UNLOADED_SECONDS).await);
    }
    let mut warm = Map::new();
    for (name, route) in routes {
        eprintln!(
            "perf_load: {name} for {} s with {} connections",
            options.seconds, options.connections
        );
        let url = format!("http://{SERVER}{route}");
        warm.insert(
            (*name).to_owned(),
            load(client, &url, options.connections, options.seconds).await,
        );
    }
    json!({"warm_one_connection": unloaded, "warm": warm})
}

/// Cold requests, then the server's RSS with those feeds active and idle, then warm requests.
async fn measure_routes(
    client: &reqwest::Client,
    routes: &[Route],
    options: &Options,
    pid: u32,
) -> Fallible<Value> {
    let cold = cold_requests(client, routes).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let rss_feeds_active_idle = rss_mib(pid);
    let warm = warm_requests(client, routes, options).await;
    Ok(json!({
        "cold_first_request": cold, "rss_mib_feeds_active_idle": rss_feeds_active_idle,
        "warm_one_connection": warm["warm_one_connection"], "warm": warm["warm"],
        "rss_mib_after_http_load": rss_mib(pid),
    }))
}

/// The `fetchedAt` of a snapshot frame, without parsing the whole frame.
fn fetched_at(frame: &str) -> Option<String> {
    const KEY: &str = r#""fetchedAt":""#;
    let start = frame.find(KEY)? + KEY.len();
    let end = frame[start..].find('"')? + start;
    Some(frame[start..end].to_owned())
}

/// One subscriber of `cell`'s aircraft; returns every snapshot frame's (fetchedAt, arrival).
async fn subscriber(
    bbox: [f64; 4],
    window_end: Instant,
    ready: tokio::sync::mpsc::Sender<()>,
) -> Fallible<Vec<(String, Instant)>> {
    let (mut ws, _) =
        tokio_tungstenite::connect_async(format!("ws://{SERVER}/api/v1/stream")).await?;
    let [west, south, east, north] = bbox;
    let subscribe = json!({"op": "subscribe", "id": "perf",
        "query": {"feed": "aircraft", "bbox": {"west": west, "south": south, "east": east, "north": north}}});
    ws.send(Message::Text(subscribe.to_string().into())).await?;
    let mut frames = Vec::new();
    let mut announced = false;
    while let Ok(Some(message)) = tokio::time::timeout_at(window_end.into(), ws.next()).await {
        let arrived = Instant::now();
        if let Message::Text(text) = message?
            && let Some(stamp) = fetched_at(&text)
        {
            frames.push((stamp, arrived));
            if !announced {
                announced = true;
                let _ = ready.send(()).await;
            }
        }
    }
    let _ = ws.close(None).await;
    Ok(frames)
}

/// WebSocket fan-out: `subscribers` clients on `cell`'s key for `cycles` refresh cycles.
async fn fan_out(cell: &Cell, options: &Options, log: &RequestLog, pid: u32) -> Value {
    let subscribers = options.subscribers;
    let settle = Duration::from_secs(10);
    let window = Duration::from_secs(AIRCRAFT_CADENCE_S * options.cycles);
    let window_end = Instant::now() + settle + window;
    let (ready_tx, mut ready_rx) = tokio::sync::mpsc::channel(subscribers);
    let tasks: Vec<_> = (0..subscribers)
        .map(|_| tokio::spawn(subscriber(cell.bbox, window_end, ready_tx.clone())))
        .collect();
    drop(ready_tx);
    let mut connected = 0;
    let _ = tokio::time::timeout(settle, async {
        while connected < subscribers && ready_rx.recv().await.is_some() {
            connected += 1;
        }
    })
    .await;
    // Cycles count from here: every subscriber has its first snapshot.
    let window_start = Instant::now();
    let measured = |at: &Instant| *at > window_start && *at <= window_start + window;
    tokio::time::sleep_until((window_start + window).into()).await;
    let rss_during = rss_mib(pid);
    let mut by_cycle: BTreeMap<String, Vec<Instant>> = BTreeMap::new();
    let mut failed = 0;
    for task in tasks {
        match task.await {
            Ok(Ok(frames)) => {
                for (stamp, arrived) in frames.into_iter().filter(|f| measured(&f.1)) {
                    by_cycle.entry(stamp).or_default().push(arrived);
                }
            }
            _ => failed += 1,
        }
    }
    let upstream = upstream_requests(log, &cell.upstream_path());
    let cycles: Vec<Value> =
        by_cycle.values_mut().map(|arrivals| cycle_report(arrivals, &upstream)).collect();
    json!({
        "cell": cell.name, "aircraft_per_snapshot": cell.contacts,
        "subscribers": subscribers, "connected": connected, "failed": failed,
        "window_s": window.as_secs(), "cadence_s": AIRCRAFT_CADENCE_S,
        "upstream_requests_in_window": upstream.iter().filter(|at| measured(at)).count(),
        "cycles": cycles, "rss_mib_during": rss_during,
    })
}

/// One update cycle: how far apart the first and last subscriber received it.
fn cycle_report(arrivals: &mut [Instant], upstream: &[Instant]) -> Value {
    arrivals.sort();
    let (Some(&first), Some(&last)) = (arrivals.first(), arrivals.last()) else {
        return json!({"receivers": 0});
    };
    let requested = upstream.iter().filter(|at| **at <= first).max().copied();
    json!({
        "receivers": arrivals.len(),
        "spread_ms": millis(last - first),
        "upstream_request_to_last_ms": requested.map(|at| millis(last - at)),
    })
}

/// Phase 1: fake upstreams, HTTP routes, then fan-out on the busy and the quiet cell.
async fn upstream_phase(
    client: &reqwest::Client,
    options: &Options,
    scratch: &Path,
) -> Fallible<Value> {
    let log: RequestLog = Arc::default();
    let _upstreams = fake_upstreams(&log).await?;
    let settings = upstream_settings(scratch);
    let server =
        start_server(client, &options.server, &settings, scratch, "server-upstreams.log").await?;
    let pid = server.0.id();
    let started = Instant::now();
    let rss_idle_no_feeds = rss_mib(pid);
    let routes = [
        ("snapshot_earthquakes", "/api/v1/feeds/earthquakes".to_owned()),
        ("snapshot_aircraft_cell", format!("/api/v1/feeds/aircraft?{}", BUSY_CELL.query())),
        ("catalog", "/api/v1/catalog".to_owned()),
        ("search", "/api/v1/search?q=SYN00&limit=10".to_owned()),
    ];
    let http = measure_routes(client, &routes, options, pid).await?;
    let mut fan_outs = Map::new();
    for cell in [&BUSY_CELL, &QUIET_CELL] {
        eprintln!(
            "perf_load: WebSocket fan-out, {} subscribers, {} cell",
            options.subscribers, cell.name
        );
        fan_outs.insert(cell.name.to_owned(), fan_out(cell, options, &log, pid).await);
    }
    drop(server);
    Ok(json!({
        "server_settings": settings.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>(),
        "seconds": started.elapsed().as_secs_f64(),
        "upstream_aircraft_requests_total": upstream_requests(&log, "/adsb/").len(),
        "rss_mib_idle_no_feeds": rss_idle_no_feeds, "http": http, "websocket_fan_out": fan_outs,
        "notes": "Fake upstreams: recorded USGS 2.5_day (45 events); synthetic adsb.lol cell answers.",
    }))
}

/// Phase 2: fixture mode over the dense synthetic files, worldwide queries.
async fn dense_phase(
    client: &reqwest::Client,
    options: &Options,
    fixtures: &Path,
    scratch: &Path,
) -> Fallible<Value> {
    let settings = fixture_settings(scratch, fixtures);
    let server =
        start_server(client, &options.server, &settings, scratch, "server-fixtures.log").await?;
    let pid = server.0.id();
    let started = Instant::now();
    let rss_idle_no_feeds = rss_mib(pid);
    let routes = [
        ("snapshot_aircraft_world", "/api/v1/feeds/aircraft".to_owned()),
        ("snapshot_aircraft_europe", "/api/v1/feeds/aircraft?bbox=-12,35,32,62".to_owned()),
        ("snapshot_satellites_world", "/api/v1/feeds/satellites".to_owned()),
        ("snapshot_fires_world", "/api/v1/feeds/fires".to_owned()),
        ("snapshot_earthquakes", "/api/v1/feeds/earthquakes".to_owned()),
        ("snapshot_cyclones", "/api/v1/feeds/cyclones".to_owned()),
        ("search", "/api/v1/search?q=SYN01&limit=10".to_owned()),
    ];
    let http = measure_routes(client, &routes, options, pid).await?;
    drop(server);
    Ok(json!({
        "fixtures": fixtures.display().to_string(),
        "server_settings": settings.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>(),
        "seconds": started.elapsed().as_secs_f64(),
        "rss_mib_idle_no_feeds": rss_idle_no_feeds, "http": http,
        "notes": "SYNTHETIC fixtures (apps/web/perf/fixtures.ts) in fixture mode; every snapshot says \
                  'Recorded fixture data, not live.'. Cold requests read and parse the fixture file.",
    }))
}

#[tokio::main]
async fn main() -> Fallible<()> {
    let options = options()?;
    let scratch = std::env::temp_dir().join(format!("lavender-perf-load-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .pool_max_idle_per_host(options.connections)
        .timeout(Duration::from_secs(30))
        .build()?;
    let run_started = Instant::now();
    let upstreams = upstream_phase(&client, &options, &scratch).await?;
    let dense = match &options.fixtures {
        Some(fixtures) => dense_phase(&client, &options, fixtures, &scratch).await?,
        None => json!({"measured": false, "reason": "no --fixtures directory given"}),
    };
    let report = json!({
        "server": SERVER, "upstream": UPSTREAM, "connections": options.connections,
        "run_seconds": run_started.elapsed().as_secs_f64(),
        "fake_upstreams": upstreams, "dense_fixtures": dense,
        "notes": "Rate and connection limits raised for a single load client; client and server share the machine.",
    });
    let text = serde_json::to_string_pretty(&report)?;
    if let Some(out) = &options.out {
        std::fs::write(out, &text)?;
    }
    println!("{text}");
    let _ = std::fs::remove_dir_all(&scratch);
    Ok(())
}
