# Glossary

| Term | Meaning in Lavender |
|---|---|
| Provider | Concrete code that fetches one source and produces validated records. |
| Source | Attribution, licensing and privacy metadata for where records or imagery originate. |
| Feed | A typed collection shown as a layer: currently aircraft, earthquakes or satellites. |
| Core | Reusable Rust application logic called by both HTTP and Tauri adapters. |
| Adapter | Transport-specific code that translates HTTP/WebSocket or Tauri IPC into core calls. |
| Snapshot | Records plus source IDs, age, freshness, coverage, counts and notices for one query. |
| Hub | The shared scheduler/cache that fetches each feed/region once for its viewers. |
| Region / cell | A portion of the map used to share upstream queries; global feeds use one region. |
| Slot | Hub state and one refresh task for a feed/region key. |
| Lease | A subscription's hold on a shared slot and its client budget; released on drop. |
| Linger | Brief idle lifetime of a slot so rapid pans or repeated snapshots can reuse data. |
| Latest-value channel | A Tokio `watch` channel keeps the newest state; readers can skip intermediate states. |
| Freshness | Live, stale, degraded or unavailable data status; use it alongside HTTP status. |
| Coverage | Complete or partial coverage of the requested area, with a reason for partial results. |
| BBox | Bounding box in west,south,east,north order; it can cross longitude ±180°. |
| OMM / orbital elements | The CelesTrak document and numeric orbit description used to calculate satellite positions. |
| NORAD ID | Satellite catalog identifier used for deduplication and track queries. |
| SGP4 | The orbital propagation calculation implemented in Rust and in browser satellite.js. |
| IPC | In-process app communication between the Tauri WebView and Rust commands/channels. |
| Provenance | Where a record came from and the source/time context needed to interpret it. |
| Parity | Verified preservation of useful upstream behavior and data, with intentional differences documented. |

For how these pieces interact, read [Architecture](architecture.md). Definitions
here describe implemented concepts rather than a new abstraction layer.
