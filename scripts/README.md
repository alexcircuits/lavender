# Scripts

| Script | Purpose |
|---|---|
| `check.sh` | The full local gate: Rust format, clippy, tests, deny, contracts, web typecheck, tests, lint and build |
| `check-contracts.sh` | Fails when the TypeScript contracts differ from `lavender-domain` |
| `perf.sh` | The performance harness (below) |

## Performance harness (`perf.sh`)

One command measures Lavender the way `docs/performance/specification.md` §3 prescribes and
God's Eye View (GEV) recorded its baseline (`legacy/docs/PERFORMANCE.md`):

```sh
scripts/perf.sh bench     # criterion micro-benchmarks, about 5 minutes
scripts/perf.sh load      # release server under load, about 5 minutes, ports 7428 + 7429
scripts/perf.sh browser   # production build in headless Chrome, about 5 minutes, port 7428
scripts/perf.sh all       # all three
```

Results land in `$LAVENDER_PERF_OUT` (default `${TMPDIR:-/tmp}/lavender-perf/results/<UTC time>/`):
`machine.txt` (hardware, OS, commit), `bench-*.txt`, `load.json` and `browser-*.json`. A capture
document (`docs/performance/YYYY-MM-DD-capture.md`) is written by hand from them and compares
the numbers with the specification's targets and GEV's baseline. Unmet targets are findings.

Prerequisites: the Rust toolchain, Node >= 22.12 and `pnpm` with the web dependencies
installed (the fixture generator runs with `node --experimental-strip-types`), and Google
Chrome for the browser part (`PLAYWRIGHT_CHANNEL=chrome` is the default here; set
`PLAYWRIGHT_CHANNEL=` to use Playwright's bundled Chromium). Set `CARGO_TARGET_DIR` to build
elsewhere. Close other servers on ports 7428 and 7429 first: the harness refuses to measure a
server it did not start.

### What each part measures

| Part | Where | Measures |
|---|---|---|
| `bench` | `crates/lavender-core/benches/{normalise,snapshot}.rs`, `crates/lavender-geo/benches/geo.rs` | USGS and adsb.lol normalisation (recorded 45-event feed, 10 MiB payloads); the hub's cached snapshot of 50 000 aircraft (`LavenderCore::snapshot`: de-duplication, bbox filter, cap) and its JSON encoding; `BBox::contains` over 50 000 points (linear scan and R-tree); region-key planning; SGP4 for 5 000 satellites |
| `load` | `crates/lavender-server/examples/perf_load.rs` | Fake upstreams: cold first request, requests/s and p50–p99 latency of snapshot, catalog and search routes; WebSocket fan-out to 100 subscribers of one (feed, region) key per update cycle, for a busy cell (1 000 contacts) and a quiet control cell (10); upstream calls per key; server RSS. Then fixture mode: the same measurements for worldwide snapshots of 20 000 aircraft, 5 000 satellites and 100 000 fire detections, and RSS with every fixture feed active |
| `browser` | `apps/web/perf/` (`playwright.perf.config.ts`, `browser.perf.ts`) | Shell first paint, globe engine first frame, first and last imagery tiles, initial settle, frames rendered at rest and in motion (5 s each), layer activation, keyed update cost, JS heap, interaction latency, WebGL renderer string |

### Conditions and choices

- **Fake upstreams for `load`, then fixture mode for dense snapshots.** The spec asks for
  wiremock upstreams: the real providers, fetcher, regional cells and hub run, and the fake
  upstream counts every call, which is how "one upstream call per (feed, region) per
  cadence" is checked. The load client is part of the tool (no `oha` install): it also
  drives the WebSocket fan-out. The second phase serves the browser scenarios' SYNTHETIC
  populations (`apps/web/perf/fixtures.ts`, 20 000 aircraft) in fixture mode, because the
  cost of a dense worldwide snapshot is the hub's composition and the JSON encoding, which
  fixture mode runs unchanged; only the upstream fetch is replaced by a file read.
  Rate and connection limits are raised because one client generates all traffic; the tool
  records which settings it changed.
- **Fixture mode for `browser`.** The browser needs dense, repeatable populations that no
  live source guarantees. `apps/web/perf/fixtures.ts` writes SYNTHETIC documents from a
  fixed seed (10 000 aircraft, 5 000 satellites, 100 000 fire detections) plus the recorded
  USGS feed, and the server says "Recorded fixture data, not live." on every snapshot.
  Update costs are measured by rewriting the aircraft file between the server's 30 s
  fixture refreshes.
- **One configuration per run.** Both server phases get only the settings the harness
  chooses: inherited `LAVENDER*` variables (configuration overrides, `LAVENDER_CONFIG`,
  provider keys) and the legacy `LL2_API_TOKEN` / `AISSTREAM_API_KEY` are removed. In fixture
  mode the catalog still offers a keyed layer only when its key is set, so the harness sets
  a placeholder `LAVENDER_FIRMS_MAP_KEY`; fixture mode reads `firms.csv` and contacts no
  upstream, so the placeholder is never sent anywhere.
- **Frame counting from outside the app.** Instead of a dev-only route, an init script
  counts WebGL draw calls per animation frame (= frames Cesium rendered), so the measured
  build is exactly the production build.
- **GEV's controls:** 1440×900 at DPR 1, medians of three cold startups with the cache
  disabled, 5 s motion and 5 s rest, object counts recorded with every sample, and the
  run stops when the WebGL renderer is software (`LAVENDER_PERF_ALLOW_SOFTWARE_GL=1` records
  anyway). Imagery tiles are answered locally with a synthetic 512×512 PNG unless
  `LAVENDER_PERF_LIVE_IMAGERY=1`, which loads real NASA GIBS tiles over the network.
- **Motion samples:** after the 5 s rest window, 2 s of unrecorded warm-up motion (its fps
  is reported as `warmUpFps`), then three 5 s motion samples and their median: one sample
  varied too much on a busy machine, and the first seconds in a new scene load tiles and
  shaders once.
- Headless Chrome by default (a locked macOS screen stops headed rendering);
  `LAVENDER_PERF_HEADED=1` shows the window. Record the machine's load average with the
  capture (`machine.txt` does): other work on the machine moves every number.

### Adding a scenario

Add a `test(...)` to `apps/web/perf/browser.perf.ts` that opens a fresh session, waits with a
time limit, and calls `record('<section>', …)` with the counts it measured
(`serverCount`). Keep fixtures deterministic and labelled synthetic in `fixtures.ts`.
