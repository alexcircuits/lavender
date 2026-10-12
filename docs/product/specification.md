# Lavender — product specification

- Product: Lavender, a NoirSystems product
- Status: v1 specification, 2026-10-11
- Derived from: God's Eye View (MIT), audited in `docs/audit/`

## 1. Vision

Lavender is a geospatial intelligence workspace. It puts live public signals
(aircraft, vessels, satellites, earthquakes, fires, weather, infrastructure and
more) on one accurate 3D globe, tells the analyst where each datum came from and
how fresh it is, and lets them inspect, track, search, compare and record what
they find.

Its long-term direction is multi-source analysis: entities, relationships,
timelines, saved investigations and collaboration, in the spirit of analytical
platforms such as Palantir Gotham, built independently with its own identity.

Lavender is the opposite of a cinematic globe. Upstream's night-vision filters,
fake thermal readouts, classification banners and radar sweeps are gone. What
remains has to earn its pixels by helping someone understand the world.

## 2. Principles

1. **Information over spectacle.** No effect without analytical value. Motion
   explains state changes and never decorates.
2. **Every datum is attributable.** Source, licence, fetch time, freshness state
   and known uncertainty are one click away, always.
3. **Honest states.** Live, stale, degraded, polling, unconfigured, disabled by
   licence policy, failed. Lavender never shows stale data as live, and never
   hides a source failure.
4. **Spatial first.** Geography is the primary organising axis; everything
   selectable has a location, a footprint or a track.
5. **Operator productivity.** Keyboard-first, dense but readable, nothing modal
   that does not need to be.
6. **Responsible use.** Sensitive sources (public cameras, licence-plate
   readers) are off by default, labelled, and governed. Lavender does not
   aggregate personal data.
7. **One product, two homes.** The same interface runs in a browser against a
   hosted server and as a desktop application with an embedded core.

## 3. Users

| Persona | Needs |
|---|---|
| **OSINT analyst** (journalism, research, NGOs) | Watch an area; inspect aircraft, vessels and events; correlate them in time and space; export evidence with provenance |
| **Situational-awareness operator** (crisis response, logistics, security) | A reliable live picture of hazards (quakes, fires, storms) and traffic around assets; alerts later |
| **Researcher / educator** | Explore satellites, launches and global patterns; reproducible views |
| **Desktop power user** | Runs Lavender locally with their own provider keys, offline-tolerant |

Not a target: covert surveillance of individuals. Lavender does not resolve
identities of private persons, and keeps its sensitive layers within their
providers' terms.

## 4. Scope

The specification separates four kinds of scope, because they are planned and
verified differently.

### 4.1 Existing capability to migrate (parity)

The full matrix is in `docs/audit/feature-parity-matrix.md`. In summary:

| Group | Capabilities |
|---|---|
| Globe | 3D globe, imagery stacks, terrain, photorealistic 3D Tiles (keyed), camera navigation, place framing |
| Live movement | Aircraft (civil + military, one engine), vessels (AIS), satellites (orbits, passes), transit vehicles, bike-share |
| Hazards & environment | Earthquakes, active fires (FIRMS), fire perimeters, tropical cyclones, weather radar/satellite/lightning imagery, wind fields |
| Infrastructure & context | Datacenters, dams, submarine cables (licence-gated), military installations, regions/boundaries, regional briefing |
| Sensors & media | Public cameras (sensitive, off by default), street-level imagery (Mapillary), recent satellite imagery (NASA HLS/GIBS) with A/B comparison |
| Mobility | Road traffic flow (TomTom), directions (OSRM) |
| Interaction | Search (places, coordinates incl. DMS/MGRS, live entities), select/inspect/track/follow, annotations & measurement, share links |
| Automation | Command registry used by keyboard, palette, voice and MCP; MCP tools for external agents; voice control (deferred) |

### 4.2 Improvements included in the rewrite

- A Rust backend that is the only data egress, with typed contracts, server-side
  spatial filtering, coalesced refresh, bounded caches and honest health.
- A redesigned interface (`docs/design/design-system.md`): navigation rail, layer
  explorer, context inspector, command palette, status bar, attribution panel.
- Provenance and freshness on every entity and layer.
- Licence-aware source registry with a commercial-mode gate (ADR 0008).
- Unified aircraft engine (civil + military), R-tree-indexed vessel store,
  server-side fire aggregation.
- Accessibility (keyboard, screen-reader announcements for live counts, reduced motion).
- Performance targets measured against GEV's recorded baseline (`docs/performance/specification.md`).

### 4.3 Infrastructure for future expansion (built now, used later)

- Transport-independent Rust core usable from Axum and Tauri (ADR 0002).
- `LavenderPlatform` frontend boundary with web and Tauri adapters (ADR 0005).
- Data-time model in the core: every snapshot carries observation and fetch
  times, and stores are time-indexed where cheap. This is the seed for timeline
  playback.
- A workspace document schema (saved views, layers, annotations), versioned with
  migrations, persisted through the core.
- One command registry (actions with schemas) that later drives voice, MCP and
  AI assistance.
- Audit-log hook points in the core service facade.

### 4.4 Roadmap (not in the rewrite)

Entity relationship graphs; historical archives and timeline playback; saved
searches and geospatial alerts; event correlation; investigations, notes and
reports; collaboration; AI-assisted analysis; role-based access control and
enterprise SSO; multi-tenant deployment; offline packs for desktop.

## 5. Core workflows (acceptance-level)

1. **Orient.** Open Lavender; the shell paints in under 1 s and the globe within
   2.5 s. A status bar shows connection, feed health and UTC time.
2. **Turn on a layer.** Open the layer explorer, toggle Aircraft. Within one
   refresh interval contacts appear, and the layer row shows count, source and
   freshness. If the source is unconfigured, the row says so and links to
   configuration help.
3. **Search.** Press `/` or ⌘K and type a place, coordinates (`60.17, 24.94`,
   DMS, MGRS) or a callsign. Results are grouped (Places, Live entities,
   Actions). Choosing one flies there or selects the entity.
4. **Inspect.** Click a contact. The inspector shows identity, kinematics,
   source, observation time and age, and actions (track, follow, copy
   coordinates, open source record). `Esc` clears the selection.
5. **Track.** Track an aircraft. Its trail draws; if the feed loses it, the
   state becomes "lost (last seen 2 min ago)" and is not silently extrapolated
   beyond a bounded dead-reckoning window.
6. **Understand provenance.** The attribution panel lists every active source
   with its mandated attribution text and licence. The provenance row of any
   entity links to the same record.
7. **Share.** Copy a link that restores camera, layers, filters and selection.

## 6. Non-functional requirements

| Area | Requirement |
|---|---|
| Performance | See `docs/performance/specification.md` (startup, frame rate, layer activation, memory, backend latency targets) |
| Availability | A provider failure degrades only its layer; the app never blanks |
| Security | No secrets in frontend bundles; SSRF-safe egress; strict CSP without `unsafe-eval`; least-privilege Tauri capabilities (`docs/architecture/security.md`) |
| Privacy | No analytics or telemetry by default; sensitive layers opt-in; viewport never sent to third parties except tile hosts |
| Accessibility | WCAG 2.2 AA for the application chrome; full keyboard operation; reduced-motion support |
| Platforms | Web: current Chrome, Edge, Firefox and Safari (WebGL2). Desktop (future): Windows 10+ (WebView2), macOS 13+ (WKWebView), Linux with WebKitGTK 4.1 |
| Licensing | Every source registered with licence and attribution; commercial mode enforced |

## 7. Out of scope for the rewrite

- Cinematic effects, fake sensor modes and military theming (see
  `docs/audit/upstream-ui-and-systems.md` §2 for the full list).
- The one-off Bhote Koshi event pack as core content. It becomes the first
  example of a generic event-pack format later (its CC BY-NC data stays
  evaluation-only).
- Borrowing ChatGPT/Codex credentials for voice.
- The Pinokio launcher.

## 8. Open questions (owner or legal decisions)

1. **Satellites:** CelesTrak grants no explicit licence and Space-Track limits
   redistribution. Commercial basis for orbital elements?
2. **Aircraft:** keep OpenSky only as an evaluation source, or license it?
   adsb.lol (ODbL) is the default.
3. **Base map detail:** for street-level detail, buy a commercial imagery/3D
   provider (Google Map Tiles, Cesium ion commercial, MapTiler…)? NASA GIBS
   tops out around 250–500 m resolution.
4. **Public cameras:** which jurisdictions' feeds may be redistributed?
   (DriveBC requires a licence; several have no published terms.)
5. **Brand:** accent colour and typeface are proposed in the design system
   (lavender accent, Geist/Geist Mono) and await owner confirmation.
6. **Hosting:** where the web deployment runs, and whether it is single-tenant
   per customer or shared.
