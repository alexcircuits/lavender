# How to Add a Data Provider

Use the existing USGS provider as the small working example. This tutorial follows
**adding FIRMS active fires** (ENG-950), a planned fourth feed, to show where a new
record type must go. FIRMS files/types mentioned below are **files to create**, not
implemented APIs. Read the current task, terms and source audit first.

## 1. Locate the behavior and data contract

Read [`usgs.rs`](../../crates/lavender-core/src/providers/usgs.rs),
[`providers/mod.rs`](../../crates/lavender-core/src/providers/mod.rs) and
[`tests/providers.rs`](../../crates/lavender-core/tests/providers.rs). For fires,
inspect [`legacy/server/providers/firms.js`](../../legacy/server/providers/firms.js)
and the [backend audit](../audit/upstream-backend.md). Note the source's required
key, CSV fields, cadence, quotas, attribution and malformed-row behavior.

Keep this feature inside `lavender-core`. A separate crate or generic manager is
not needed for another polled source. Do not register two providers with the same
`FeedId`: the hub currently keeps one provider per feed and would replace one.
For multiple sources serving one feed, implement explicit merging inside its
provider and test provenance/duplicates.

## 2. Add typed records and configuration

Create the fire record in `lavender-domain/src/entities/` and extend
[`feed.rs`](../../crates/lavender-domain/src/feed.rs): `FeedId`, `ALL`, string name,
`FeedItems`, empty results and related exhaustive matches. Add only required
filters. Keep serialized naming consistent with existing camelCase fields.

Add provider configuration/defaults/validation to
[`core/config.rs`](../../crates/lavender-core/src/config.rs). FIRMS already has
`SecretKey::FirmsMapKey` (`LAVENDER_FIRMS_MAP_KEY`) in
[`secrets.rs`](../../crates/lavender-core/src/secrets.rs); use it rather than making
up `FIRMS_MAP_KEY` as a second environment convention. Prefer existing dependencies;
add a CSV dependency to the crate only if its parser actually needs one.

## 3. Fetch through the supplied context

Create `crates/lavender-core/src/providers/firms.rs` and register the module in
`providers/mod.rs`. Follow `UsgsProvider`'s concrete config/client pattern and
implement `FeedProvider` methods: `feed`, `source_id`, `cadence`, `scope`, `enabled`
and `fetch`. Read keys through `context.secrets`, not directly from environment.

`fetch` receives `&FetchContext` and `Region`, returning
`Result<Fetched, ProviderError>`. Build URLs from trusted provider configuration and
validated region inputs. Use `Fetcher::get`, `HostPolicy`, and `FetchOptions` with
a byte cap and `Some(context.cancel.clone())`. Provider keys in URL paths must
never appear in logs or error messages. The parser receives the response bytes.

## 4. Parse and normalize separately

A pure `parse` function should return valid records and a dropped-row count, like
`usgs::parse`. Normalize each row into a validated record with `GeoPoint::new`,
finite/range checks, cleaned text and meaningful UTC timestamps. Distinguish a
valid empty feed from an unreadable document. A malformed optional field should
not discard every otherwise valid row.

For a new JSON source, the existing `lenient` and `parse_rows` helpers can support
this pattern. A CSV source needs its own row parser; do not force the JSON helpers
into a universal parsing framework. Finish with `Fetched::fresh(items)` and
`with_dropped` where that fits the source.

## 5. Handle quotas, caching and failures

The hub supplies shared refresh scheduling. Set source cadence and any
`retry_floor`; use `HostPolicy::with_min_interval` for hosts that forbid bursts.
Do not add a refresh loop per browser. Configure the source's own no-retry policy
when the provider's cadence must determine all retry timing, as CelesTrak does.

Use hub memory caching for simple polled data. Add disk caching only for a real
provider requirement. Cache writes are atomic, bounded appropriately for the
source and rooted below `context.cache_dir`; cached answers carry their original
`as_of`. Return a `degraded` reason if a refresh failed but usable old data remains.
Use `ProviderError::parse` or `public_message` for safe diagnostics; never forward
raw provider bodies or secrets.

Fire aggregation is a separate pure calculation: test that cell counts and newest
timestamps are preserved. Expose the approved per-zoom representation rather than
shipping an unbounded worldwide CSV to the browser.

## 6. Register availability and attribution

Add the actual source entry to [`sources.rs`](../../crates/lavender-core/src/sources.rs),
the feed's source/availability metadata to
[`registry.rs`](../../crates/lavender-core/src/registry.rs), and the provider instance
to [`LavenderCoreBuilder::build`](../../crates/lavender-core/src/core.rs).

Declare required keys, verbatim attribution, terms link, commercial status and
privacy. Unknown terms stay unknown and subject to the existing commercial gate.
Do not label a source commercially allowed merely because its endpoint is public.
Update [source licensing inventory](../audit/data-sources-and-licensing.md),
[notices](../../THIRD_PARTY_NOTICES.md), and fixture provenance. Consult the owner
for any agreement required by the existing licensing task.

## 7. Expose the feed and connect the frontend

Generic `/api/v1/feeds/{feed}` and `/api/v1/stream` dispatch through `FeedId` and the
core: ordinary new feeds do not need a new forwarding HTTP handler or Tauri command.
Update core/hub exhaustive matches such as filtering and search deliberately.
Operations with a different contract, such as a track or binary grid, need a
specific core method and adapters; do not make them pretend to be snapshots.

Run `cargo test -p lavender-domain` to regenerate TypeScript, then run
`./scripts/check-contracts.sh`. Do not hand-edit `apps/web/src/contracts/generated`.

Add the fire renderer under `apps/web/src/globe/renderers/`, wire it into
`GlobeEngine`, and update feed/entity helpers, layer filters and inspection fields.
The interface calls `LavenderPlatform`; transport implementations remain unchanged
for generic subscriptions. Update the recorded provider only when fixture support
for the new feed has actually been implemented.

## 8. Prove the whole feature

Add unit tests beside the parser/calculation and contract tests in
`crates/lavender-core/tests/providers.rs` using Wiremock. Cover valid, empty and
malformed data; host refusal, timeout, size cap and throttling; missing keys;
commercial licence refusal; stale fallback and cancelled requests. A refusal must
make zero upstream requests. Preserve source credits and real data age.

```bash
cargo test -p lavender-core --test providers
cargo test -p lavender-core --test hub
./scripts/check.sh
PLAYWRIGHT_CHANNEL=chrome pnpm --dir apps/web run e2e
```

Extend a fixture-backed Playwright journey: enable the feed, inspect a record,
change filters, confirm source/age and turn it off. Compare with the legacy fixture
and UI fields. Record outcomes and intended differences in the task and migration
table. Only close the task once all its acceptance criteria are verified.
