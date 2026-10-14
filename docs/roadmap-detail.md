# Lavender — original implementation breakdown

> Historical requirements and L/WS/P cross-references, preserved from 2026-10-11.
> Use the [current roadmap](roadmap.md) and [Rust rewrite status](development/rust-rewrite.md)
> for present implementation and task status. Proposed APIs here may not exist yet.

- Status: v1, 2026-10-11
- Tracking: Flokzy project **Lavender** (team ENG). The `L-xx` ids below are this document's
  plan ids; the Flokzy identifiers are recorded in the table once tasks exist
  (`docs/PROGRESS.md` keeps the live mapping).
- No dates: none were given. Order comes from dependencies and risk.

## Phases

| Phase | Outcome | Exit criteria |
|---|---|---|
| P0 Discovery | Audits, research, specifications, spike | Specs in `docs/`, Flokzy plan exists (**done**) |
| P1 Foundations | Rust workspace, domain, core runtime, server skeleton, frontend scaffold, platform interface, tokens, desktop skeleton | `cargo test`, `pnpm test`, `pnpm build` green; server serves the SPA; desktop app boots the same `dist` |
| P2 First vertical slices | Earthquakes, aircraft, satellites, fires, cyclones, search, shell, explorer, inspector, attribution | Live layers end-to-end on web **and** one through Tauri; E2E + rendering smoke green |
| P3 Wave A | Vessels, weather, wind, perimeters, launches, infrastructure, tracking, keyed imagery, annotations, share links | Parity rows for wave A verified |
| P4 Wave B | Traffic, transit, bikeshare, directions, CCTV, street level, installations, cables, recent imagery | Parity rows for wave B verified |
| P5 Automation & deferred | Command registry → MCP, voice, radio, local ADS-B, space missions, event packs | As each feature's acceptance criteria |
| P6 Desktop readiness | Keychain, Windows/Linux builds, WebKitGTK safeguards | `docs/architecture/tauri.md` §8 met |
| P7 Optimisation & hardening | Performance targets met, security suite complete | `docs/performance/specification.md` targets measured and met |
| P8 Release readiness | Licensing verified, deployment, parity sign-off, `legacy/` removed | Definition of done (below) |

The order is risk-driven. The riskiest assumption, "one frontend + one core over
two transports with Cesium in WebViews", is validated first: the spike in P0,
then one live layer through Tauri in P2. The riskiest provider (GRIB2 wind)
starts early in P3 rather than last.

## Workstreams and tasks

Legend: priority (U/H/M/L), type label, phase, depends on.

### WS1 Specifications and decisions
| Id | Task | Pri | Label | Phase | Depends |
|---|---|---|---|---|---|
| L01 | Audit God's Eye View and publish the Lavender specification set | H | Documentation | P0 | — (done) |
| L02 | Validate CesiumJS inside a Tauri 2 WebView on macOS (spike) | H | Improvement | P0 | — (done) |
| L03 | Record owner decisions on licensing, brand and hosting questions | H | Documentation | P1 | L01 |

### WS2 Rust foundations
| L04 | Lavender Cargo workspace with lint, format and dependency policy, registered in Nx | H | Improvement | P1 | — |
| L05 | `lavender-domain` contract types with generated TypeScript bindings | H | Feature | P1 | L04 |
| L06 | `lavender-geo`: geodesy, antimeridian-safe bboxes, region keys, DMS/MGRS parsing | H | Feature | P1 | L04 |

### WS4 Shared Rust core
| L07 | Egress-safe `Fetcher` (allowlists, HTTPS, redirects, timeouts, size caps, retries) | H | Security | P1 | L04 |
| L08 | Feed hub: coalesced refresh, latest-value subscriptions, bbox filtering, cancellation | H | Feature | P1 | L05, L06 |
| L09 | Source registry with licence-mode gate and provider health | H | Feature | P1 | L05 |
| L10 | `LavenderCore` facade with typed config, secret and settings stores | H | Feature | P1 | L07, L08, L09 |

### WS3 Provider and API migration
| L11 | `lavender-server`: `/api/v1`, WebSocket stream, SPA hosting, security layers | H | Feature | P1 | L10 |
| L12 | Earthquakes from USGS through the Rust core | H | Feature | P2 | L10 |
| L13 | Aircraft from adsb.lol (civil + military merged), OpenSky as evaluation source, tracks | H | Feature | P2 | L10 |
| L14 | Satellites: orbital elements, SGP4 propagation, ground tracks and passes | H | Feature | P2 | L06, L10 |
| L15 | Active fires (FIRMS) with server-side aggregation | M | Feature | P2 | L10 |
| L16 | Tropical cyclones from NHC/CPHC | M | Feature | P2 | L10 |
| L17 | Search service: coordinates, configurable geocoder, live-entity search | H | Feature | P2 | L06, L10 |
| L18 | Vessels: AISStream ingest, watchdog and R-tree store | M | Feature | P3 | L10 |
| L19 | Weather imagery (nowCOAST) manifest and bounded tile proxy | M | Feature | P3 | L10 |
| L20 | Wind fields: pure-Rust GRIB2 decoding and grids | M | Feature | P3 | L10 |
| L21 | Fire perimeters (WFIGS) with InciWeb links | M | Feature | P3 | L10 |
| L22 | Launches (Launch Library 2) with payload→NORAD matching | L | Feature | P3 | L14 |
| L23 | Static context datasets: datacenters, dams, regions lookup | M | Feature | P3 | L10 |
| L24 | Terrain heights service | L | Feature | P3 | L10 |
| L25 | Traffic flow (TomTom) proxy with budget and road matching | L | Feature | P4 | L10 |
| L26 | Transit vehicles (GTFS-Realtime) with history | L | Feature | P4 | L10 |
| L27 | Bike-share stations (GBFS registry) | L | Feature | P4 | L10 |
| L28 | Directions (OSRM) with throttling | L | Feature | P4 | L10 |
| L29 | Public cameras: catalog and media relay, off by default | L | Feature | P4 | L10 |
| L30 | Street-level imagery (Mapillary) coverage and Graph proxy | L | Feature | P4 | L10 |
| L31 | Overpass proxy and military installations (sanitizer port) | L | Security | P4 | L10 |
| L32 | Submarine cables (licence-gated) and regional briefing | L | Feature | P4 | L10 |
| L33 | Recent satellite imagery search (CMR/HLS) | L | Feature | P4 | L10 |

### WS5 Frontend architecture
| L34 | `apps/web` scaffold: Vite, React, strict TS, Biome, Vitest, Playwright, Cesium assets | H | Improvement | P1 | — |
| L35 | `LavenderPlatform` interface and `WebPlatform` adapter | H | Feature | P1 | L05, L34 |
| L36 | Workspace/feed/UI stores and the `FeedController` | H | Feature | P2 | L35 |
| L37 | Share links and versioned workspace document | M | Feature | P3 | L36 |

### WS6 Geospatial rendering
| L38 | `GlobeEngine`: CesiumWidget, imagery catalog, camera, picking, render-error recovery | H | Feature | P1 | L34 |
| L39 | Aircraft renderer with keyed diffing | H | Feature | P2 | L36, L38 |
| L40 | Hazard renderers: earthquakes, fires, cyclones | H | Feature | P2 | L36, L38 |
| L41 | Satellite renderer with frame-time propagation | H | Feature | P2 | L14, L38 |
| L42 | Tracking and follow camera with trails and lost/stale states | M | Feature | P3 | L39 |
| L43 | Keyed imagery: Google Photorealistic 3D Tiles and Cesium ion via the runtime catalog | M | Feature | P3 | L38 |
| L44 | Label declutter engine | M | Performance | P3 | L38 |
| L45 | Vessel renderer | M | Feature | P3 | L18, L38 |

### WS7 Design system
| L46 | Design tokens, fonts, squircles and motion in code | H | UX | P1 | L34 |
| L47 | Primitive component library with tests | H | UX | P1 | L46 |

### WS8 Application interface redesign
| L48 | Application shell: navigation rail, panel layout, status bar, responsive | H | UX | P2 | L47 |
| L49 | Layer explorer with freshness and honest states | H | UX | P2 | L36, L48 |
| L50 | Context inspector with provenance and actions | H | UX | P2 | L48 |
| L51 | Search field and command palette with command registry v1 | H | UX | P2 | L17, L48 |
| L52 | Sources and attribution panel generated from the registry | H | UX | P2 | L09, L48 |
| L53 | Settings panel: preferences and desktop provider keys | M | UX | P3 | L48 |
| L54 | Annotations and measurement, persisted | M | Feature | P3 | L37 |

### WS9 Layer and feature migration
| L55 | Wave A layers in the interface | M | Feature | P3 | L18–L23, L45 |
| L56 | Wave B layers in the interface | L | Feature | P4 | L25–L33 |
| L57 | Command registry exposed as a Rust MCP server | L | Feature | P5 | L51 |
| L58 | Voice control through OpenAI Realtime (API key only) | L | Feature | P5 | L57 |
| L59 | Deferred features: radio, local ADS-B, space missions view, event packs | L | Feature | P5 | L55 |

### WS10 Tauri compatibility
| L60 | Desktop app on the shared core: commands, channels, CSP, capabilities | H | Feature | P1 | L10, L34 |
| L61 | `TauriPlatform` adapter with IPC contract tests and a live layer end-to-end | H | Feature | P2 | L35, L60, L12 |
| L62 | Keychain secret store and desktop settings | M | Security | P6 | L60 |
| L63 | Windows and Linux desktop builds with WebKitGTK safeguards and diagnostics | M | Improvement | P6 | L61 |
| L64 | Desktop packaging, signing, updater and deep links | L | Feature | P8 | L63, L03 |

### WS11 Performance
| L65 | Performance harness and first baseline (Rust benches, startup, fps, heap) | M | Performance | P2 | L39, L11 |
| L66 | Optimisation pass to meet the performance targets | M | Performance | P7 | L65, L55 |

### WS12 Security and testing
| L67 | Security test suite: host/origin, WebSocket limits, rate limits, CSP, fetch policy | H | Security | P2 | L11 |
| L68 | End-to-end and rendering smoke tests on a fixture-backed server | H | Improvement | P2 | L11, L39 |
| L69 | Monorepo CI integration: Nx targets, cargo deny, legacy CI handling | M | Improvement | P1 | L04, L34 |

### WS13 Documentation
| L70 | Operator guide: configuration, keys, licence mode, deployment | M | Documentation | P3 | L11 |
| L71 | User guide: workflows and keyboard shortcuts | L | Documentation | P4 | L51 |

### WS14 Release readiness
| L73 | Production deployment of `lavender-server` | M | Feature | P8 | L03, L70 |
| L74 | Licensing compliance verification before commercial release | H | Documentation | P8 | L03 |
| L75 | Parity sign-off, full regression and removal of `legacy/` | M | Improvement | P8 | L55, L56, L74 |

## Critical path to the first usable product (P1–P2)

```
L04 ─┬─ L05 ─┬─ L08 ─┐
     │       └─ L09 ─┤
     ├─ L06 ─────────┤
     └─ L07 ─────────┴─ L10 ─┬─ L11 ─┬─ L67, L68
                             ├─ L12 ─┤
L34 ─┬─ L35 ─ L36 ─┐         ├─ L13 ─┤
     ├─ L38 ───────┼─ L39/L40/L41    │
     └─ L46 ─ L47 ─ L48 ─ L49/L50/L51/L52
L10 + L34 ─ L60 ─ L61 (live layer through Tauri)
```

## Definition of done (whole rewrite)

Mirrors the brief: backend logic migrated to Rust and transport-independent;
interface fully redesigned; useful features preserved per the parity matrix;
Tauri readiness criteria met; tests, performance measurements, security
controls and licensing verified; Flokzy statuses reflect reality.
