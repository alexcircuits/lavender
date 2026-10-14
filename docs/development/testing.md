# Testing and Debugging

Run commands from Lavender's root. Tests use mock or recorded provider documents;
normal verification should not consume live provider quotas.

## Which tests exist

| Test layer | Location | Command |
|---|---|---|
| Domain serialization, validation and TS exports | `lavender-domain/src` and `tests/roundtrip.rs` | `cargo test -p lavender-domain` |
| Geodesy, coordinates, region bounds and orbit calculations | `lavender-geo/src` | `cargo test -p lavender-geo` |
| Core units and real-provider contracts | `lavender-core/src`, `tests/providers.rs` | `cargo test -p lavender-core` |
| Hub sharing, capacity, cancellation and lifecycle | `lavender-core/tests/hub.rs` | `cargo test -p lavender-core --test hub` |
| HTTP, errors, security, ETags and WebSockets | `lavender-server/src`, `tests/api.rs` | `cargo test -p lavender-server` |
| Frontend units and both platform contracts | `apps/web/src/**/*.test.*` | `pnpm --dir apps/web run test` |
| Browser journeys and rendering/CSP smoke | `apps/web/e2e` | `PLAYWRIGHT_CHANNEL=chrome pnpm --dir apps/web run e2e` |

`./scripts/check.sh` verifies backend and frontend with one command. It excludes
native desktop compilation by default; set `LAVENDER_CHECK_DESKTOP=1` on a prepared
machine to include the desktop crate. End-to-end tests are a separate browser check.
A current verification record is in [Verification](verification.md).

## Test one area while changing it

```bash
cargo test -p lavender-core providers::adsb_lol
cargo test -p lavender-core --test providers
cargo test -p lavender-core --test hub
cargo test -p lavender-server --test api
pnpm --dir apps/web exec vitest run src/platform/contract.test.ts
pnpm --dir apps/web exec vitest run src/feeds/FeedController.test.ts
```

Wiremock contract tests use the actual provider/fetcher/core. They exercise body
caps, host policy, parsing, cache age, cancellation and error categories. A
normalization unit test alone does not establish correct API integration.

For browser tests, either install managed Chromium:

```bash
pnpm --dir apps/web exec playwright install chromium
pnpm --dir apps/web run e2e
```

or use an installed Chrome with the command in the table. Playwright builds the
frontend and starts Rust on port 7412 with fixture providers. See
[`playwright.config.ts`](../../apps/web/playwright.config.ts). `reuseExistingServer`
is enabled locally: ensure anything already on that port is the intended fixture
server. Use `CI=1 PLAYWRIGHT_CHANNEL=chrome pnpm --dir apps/web run e2e` for a fresh
server and CI behavior. Browser traces are retained on failure.

## Debug the Rust backend

Start recorded feeds with useful logs:

```bash
RUST_LOG=info,lavender_core=debug,tower_http=debug LAVENDER__SERVER__FIXTURES_DIR=apps/web/e2e/fixtures cargo run --locked -p lavender-server
```

`RUST_LOG` controls tracing filters. For machine-readable lines, set
`LAVENDER__SERVER__LOG_FORMAT=json`. Source and region fields identify the upstream
operation; a fetch failure differs from an unreadable provider document, missing
configuration or an internal panic. Avoid logging raw request URLs or response
bodies: providers can put keys in URL paths and query parameters.

Set debugger breakpoints at `routes::feed`, `LavenderCore::snapshot_inner`,
`AdsbLolProvider::fetch`, or the pure normalization function. For stream behavior,
start at `ws::Connection::subscribe` and `hub/subscription.rs`. `RUST_BACKTRACE=1`
helps diagnose a panic; ordinary provider failures are `Result` values and do not
produce a panic backtrace.

## Diagnose an API or layer failure

With the backend running at its default address:

```bash
curl -fsS http://127.0.0.1:7410/healthz
curl -fsS http://127.0.0.1:7410/api/v1/catalog
curl -fsS http://127.0.0.1:7410/api/v1/status
curl -i 'http://127.0.0.1:7410/api/v1/feeds/aircraft?bbox=20,58,30,62'
curl -i 'http://127.0.0.1:7410/api/v1/search?q=60.17,24.94'
```

1. **Connection refused:** verify process, listen address and Vite proxy target.
2. **403:** check host/origin and license availability. Do not weaken origin or
   licence checks to make a browser request work.
3. **429:** inspect `Retry-After`, client connection/region budget and upstream
   pacing. Zoom in or close unused views.
4. **502/504 or an unavailable result:** inspect provider/health logs and whether
   any previous records exist. Check the recorded provider file before blaming
   transport code in fixture mode.
5. **200 with missing data:** read `freshness`, `coverage`, `pendingRegions`,
   `notices`, and source count. Partial military-only aircraft coverage is expected
   for very large views. Empty earthquakes can be a valid result.

In browser DevTools, inspect `/api/v1/catalog`, the stream frames and console/CSP
errors. Confirm the feed ID, bbox ordering (**west,south,east,north**) and filters.
Transport status distinguishes live stream, polling and offline state. A broken
imagery host can affect the globe even when feed requests succeed.

## Verify migration parity

Use the same recorded inputs in legacy and Rust tests. Compare stable IDs,
coordinates/units, normalized fields, timestamps, duplicates, empty results,
invalid rows and the fields the inspector uses. Test error behavior deliberately;
legacy quirks are not automatically requirements. Record intentional differences
in the task and [parity matrix](../audit/feature-parity-matrix.md).

Do not infer a full migration from a passing parser or one screenshot. Exercise
layer on/off, filters, pan/zoom, selection, reconnection, stale data and credit
visibility. Update fixtures with provenance and reuse rights, without secrets.
Legacy test commands are recorded in its package; install/run them only when
needed for a specific parity check. No legacy-wide test run is claimed by the
current readability pass.

Desktop rendering uses the native self-check rather than browser Playwright;
see [Web and Desktop](web-and-desktop.md). Performance targets need the planned
measurement harness (ENG-1000), even when functional tests pass.
