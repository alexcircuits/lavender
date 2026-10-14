# Lavender Development Roadmap

The goal is a useful geospatial intelligence workspace with a reusable Rust core,
one TypeScript interface, honest source/age information, and web plus desktop
adapters. The backend already runs; the remaining work is feature parity,
operational readiness and measured quality. Do not restart the foundations.

This roadmap was reconciled with source and Flokzy on **2026-10-13**. Flokzy holds
current status, ownership and dependency links. No delivery dates were supplied.
[Task Index](development/task-index.md) lists all existing tasks; the
[original detailed breakdown](roadmap-detail.md) preserves L/WS/P references and
technical requirements. Milestones below are readable groupings, not duplicate
Flokzy projects or fabricated sprint records.

## 1. Make the existing application easy to develop

**Goal:** a contributor can run, understand and change the current app using docs.

**Current state:** Rust foundations/server, USGS, frontend platform, globe, tokens,
primitives, feed controller and source/inspection UI are implemented. Their completed
tasks stay completed. ENG-1012 is now Done with this pass's verification evidence;
monorepo CI work ENG-1004 is Todo.

**Required work:** maintain the contributor guides and verified local workflows
delivered in ENG-1012, then resolve hosted monorepo CI inclusion without changing
other products (ENG-1004).

**Dependencies:** existing core/frontend foundations; monorepo CI changes require
coordination because Lavender is a separate nested Git repository.

**Verification:** documented setup starts the app; Rust/frontend checks and fixture
E2E pass; generated TS matches Rust; new contributor can trace an aircraft record.
CI task completion additionally requires the actual tracked/published integration.

## 2. Finish the three working feeds and search

**Goal:** earthquakes, aircraft, satellites and search meet their full task scope.

**Current state:** earthquakes are complete (ENG-947). Aircraft and satellite feeds,
tracks for satellites, browser renderers and coordinate/entity search exist.
ENG-949 is Done for provider, tracks, passes and cache validation. ENG-948 and
ENG-952 remain In Progress; renderer tasks ENG-974/976 and shell
ENG-983 are In Review. Their remaining acceptance criteria are not cancelled.

**Required work:** optional OpenSky/enrichment, live pacing validation and future
vessel search. Bounded aircraft
history, geometric observer passes/reference checks and configured place search
now have integrated implementations. Finish reviews
and the fixture journeys (ENG-1003: In Review).

**Dependencies:** existing hub, geo, registry and adapters. Provider agreements or
geocoder configuration must be established for the relevant source (ENG-938).

**Verification:** targeted provider/core tests, explicit parity comparisons, fixture
journeys, failure/cancellation cases and source attribution; no missing feature is
hidden behind a completed basic-feed claim.

## 3. Add the next live layers one at a time

**Goal:** fires and cyclones, then ships/weather/wind and other first-wave layers,
work through the same core and interface.

**Current state:** fire/cyclone providers ENG-950/951 are Todo. ENG-975 is In Progress
because earthquake rendering exists, while fire/cyclone rendering remains. Provider
work ENG-953–959, vessel renderer ENG-980 and first-wave interface ENG-990 are Todo.

**Required work:** one provider contract and renderer journey per layer. Start the
wind decoder trial (ENG-955) early with real legally reusable GRIB fixtures. Add
perimeters, launches, static context and terrain after their actual inputs are ready.

**Dependencies:** core availability is complete. ENG-957's satellite dependency
is now Done; ENG-980 waits for vessels; ENG-990 waits for provider/rendering links.
Fire credentials, source terms and fixtures are substantive prerequisites too.

**Verification:** each source's parser, errors, limits, cache age and provenance are
tested; a recorded browser journey proves the layer; planned performance targets
are measured once the harness exists. Use [Add a Provider](development/add-provider.md).

## 4. Restore the remaining analyst workflows

**Goal:** remaining useful upstream layers and workflows are covered with clear parity.

**Current state:** second-wave providers ENG-960–968 and interface ENG-991 are Todo.
Share links, settings, tracking, annotations, keyed imagery and label improvements
are separate existing tasks (ENG-977–979, 988–989). ENG-972 share/restore is Done,
with validated v1 links and recorded browser verification.

**Required work:** migrate traffic, transit, bike-share, directions, permitted cameras,
Mapillary, safe map-feature queries, licensed cables and recent imagery individually.
Implement one meaningful workflow per review; the wave parents summarize outcomes,
not one enormous implementation task. Defer MCP/voice/radio and event packs until
required foundations, access controls and owner scope are ready (ENG-992–994).

**Dependencies:** follow the existing task links. Do not merge unrelated features
into a provider ticket; share/annotation and rendering work has its own prerequisites.

**Verification:** the [parity matrix](audit/feature-parity-matrix.md) identifies what
works, what differs intentionally and what remains; error, privacy and attribution
behavior are verified alongside the happy path.

## 5. Make desktop reliable across platforms

**Goal:** the shared frontend/core run securely in supported desktop environments.

**Current state:** desktop commands/core task ENG-995 is Done; TauriPlatform and a
successful macOS rendering self-check exist. ENG-996 remains In Progress in Flokzy.
Keychain and Windows/Linux tasks ENG-997/998 are Todo.

**Required work:** reconcile ENG-996 with its evidence, implement native secret
storage, verify Windows/Linux WebViews and report platform limitations. Packaging,
signing, updates and deep links (ENG-999) follow platform readiness and owner decisions.

**Dependencies:** shared adapters/feed slice; native prerequisites and key custody.

**Verification:** adapter contracts, native rendered-frame reports, per-OS builds,
capability/CSP controls and keys never entering browser preferences or plain files.
A web E2E run alone is not desktop evidence.

## 6. Measure, harden and release

**Goal:** release decisions rest on tested functionality, terms and measurements.

**Current state:** security controls and many regression tests exist; ENG-1002's
complete suite and ENG-1000's measurement harness are Todo. Optimization ENG-1001,
operator/user guides ENG-1005/1006, deployment ENG-1007, licensing sign-off ENG-1008
and legacy removal ENG-1009 remain Todo.

**Required work:** establish repeatable baselines, address measured bottlenecks,
complete security coverage and operator/user guides, then deploy after hosting and
licensing decisions. Remove legacy only after parity and required notices are signed off.

**Dependencies:** complete relevant source/workflow migrations, platform tests,
provider permissions and owner decisions. Keep license/privacy gates throughout.

**Verification:** measured performance targets, security regressions, clean builds,
operational dry run, release notices and the full accepted parity matrix. Passing
local tests is not production or commercial sign-off.

## What to work on next

Read current task comments/links before claiming work. On 2026-10-13, these **Todo**
tasks have all recorded `blocked_by` tasks completed:

| Task | A focused next deliverable | Before starting |
|---|---|---|
| [ENG-989](https://app.flokzy.com/app/tasks/7787f083-5295-434f-9f96-d4fa07e87d44) | Add persistent annotations and measurements | Extend the workspace document's reserved annotations field with bounded geometry and keyboard list tests. |
| [ENG-957](https://app.flokzy.com/app/tasks/6c40bdde-a62a-4eba-8a89-d41053a00090) | Add launch data and satellite matching | Satellite dependency is Done; verify current LL2 terms and hourly budget before ingestion. |
| [ENG-950](https://app.flokzy.com/app/tasks/3f879345-de9b-4766-9d1f-87820918cdaf) | Add FIRMS fire ingestion and aggregation | Confirm key/terms and lawful CSV fixtures; then coordinate renderer ENG-975. |
| [ENG-951](https://app.flokzy.com/app/tasks/7ad4d567-da74-4ca3-ace7-3f112d5c6d87) | Add NHC/CPHC cyclone data | Capture active, empty and malformed fixtures; preserve strict schema validation. |
| [ENG-955](https://app.flokzy.com/app/tasks/4651a15f-676e-4c01-90cd-cae4618548b2) | Prove the wind decoder on real forecasts | Keep the first review to the trial/evidence; do not assume a decoder supports every packing template. |
| [ENG-1002](https://app.flokzy.com/app/tasks/ed0b7065-cb9f-416d-8120-fe79f24eec8a) | Fill gaps in the existing security tests | Inventory current regression coverage before adding duplicates. |
| [ENG-1004](https://app.flokzy.com/app/tasks/3625e8eb-c541-499d-a58b-6d53a739f8ae) | Resolve CI inclusion | Inspect the nested repository boundary and coordinate monorepo-owned files. |

Direct task links are generated from the live snapshot in the [Task Index](development/task-index.md).
ENG-1012 records completed readability work. ENG-948/952/996/975 are already
In Progress; coordinate before extending them. ENG-976 remains In Review with
all recorded prerequisites Done, but its 5,000-satellite performance requirement
still needs measurement.

## Whole-rewrite definition of done

Useful upstream capabilities have verified replacements or explicit retirement;
backend logic stays reusable in Rust; frontend remains TypeScript; source terms,
provenance, security, web/desktop behavior and performance are verified; deployment
and documentation are usable by humans; Flokzy reflects that evidence. Preserve
completed history and legitimate remaining acceptance criteria.
