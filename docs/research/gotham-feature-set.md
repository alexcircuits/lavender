# Palantir Gotham — feature inventory and Lavender fit

Researched 2026-10-24 (web agent pass over official Palantir pages/docs, the UK
G-Cloud Service Definition Document, the public Gotham API reference, the
official Python SDK and the AI-Enabled Operations white paper). Feeds the
LB-113 kanban boards and the LB-12x Gotham-inspired backlog. Weapons-specific
fields (aimpoints, effectors) are inventoried for fidelity but out of scope for
a public-data tool.

## Capability areas → Lavender fit (value)

| Area | What Gotham does | Lavender-fit | Value |
|---|---|---|---|
| Ontology | Dynamic ontology maps raw records to operational objects (people, orgs, events, locations, vessels) with properties and links; Objects API | Small Rust "ontology" module: entity types (vessel/aircraft/satellite/event/geo-feature), relation table in DuckDB; feeds map to entities at ingest | High |
| Entity resolution | Resolve/unresolve objects; AI-assisted record linking; analysts merge federated records into canonical entities | Deterministic Rust resolver: aircraft hex/registration/callsign, vessel MMSI/IMO/name; alias table + merge/split with provenance | Med |
| Ingestion & provenance | Federated sources without copying; every source and action tracked with version history; immutable audit logs | Extend licence/source registry into per-record lineage (URL, fetch time, licence, transformation chain) in the inspector; append-only analyst audit log | High |
| Search & investigation | Federated keyword search; pivot entity → related; Object Explorer drill-down with histograms and live alerts | DuckDB FTS over entities+events; saved searches; "pivot" = entity → observations, relations, sources | High |
| Link/graph (Graph app) | Interactive canvas, Search Around multi-hop expansion, histogram helpers, HTML export | Lightweight graph view over the relation table (co-location, shared-route, feed edges); off-the-shelf JS graph lib | Med |
| Geospatial (Gaia) | Shared editable maps; geo-search by radius/route/polygon with temporal+property filters; heatmaps; custom tiles; SHP/KMZ/KML; MIL-STD-2525C; WFS/KMZ export | Cesium polygon/route/radius select → Rust point-in-polygon → inspector results; KMZ/GeoJSON import+export; heatmap via Zig WASM binning | High (center of gravity) |
| Watchlists & Inbox | Alert inbox: saved-search feeds, object watch, **geofence enter/exit alerts**; shareable alerts; TargetWorkbench with Target Boards (kanban: IDENTIFIED→PRIORITIZED→IN COORDINATION→IN EXECUTION→COMPLETE) | Watchlists + polygon geofences evaluated in Rust against live feeds (vessel enters zone, satellite overflight via SGP4, quake in region); alert inbox; monitoring boards without weapons fields | High |
| Timeline & tracks (Geotime) | Track objects with observation history; timeline of objects/properties/links; replay; video soak heatmaps | DuckDB observation store + Cesium playback slider (vessel/aircraft/cyclone history); CZML interpolation; SGP4 time-slider | High |
| Missions & sensors | AI-driven sensor tasking; mission plans overlaid with live sensors; Europa video detection tagging | "Monitoring missions" = saved globe state (layers+geofences+watchlists+schedule) with a Rust rule engine; SGP4 overflight alerts as the public-data tasking analog; Mojo sidecar tags detections into the entity store. Skip kinetic/effector concepts | Med |
| Collaboration | Secure chat with objects; comments on entities; shared graphs/maps; graph history | Comment threads on entities/annotations; shared saved searches/missions; presence later | Med |
| Reporting & export | Slides (live-data briefings → PPTX/PDF); Dossier documents with live entity links; graph HTML; map KMZ/WFS | One-click briefing: map snapshot + layers + entity tables + auto-cited sources → PDF/HTML; GeoJSON/KMZ/CSV export | High |
| Security & access | Attribute-level ACLs; five access levels; PKI/SAML/MFA; MLS | Simple RBAC + API keys; per-source licence gates (display vs redistribute); audit log | Low-Med |
| APIs | REST + YAML-declared APIs; Python SDK; WFS; ATAK/MapBox interop | Typed REST/WS over the Rust core (entities, search, observations, geofences); GeoJSON/CoT emission | Med-High |
| Mobile / edge | Chat to mobile units; PFCS Forward disconnected edge | Read-only responsive map + webhook/Pushover alerts; offline tile packs | Low-Med |

## Top-10 build list (analyst value × feasibility)

1. Polygon/radius/route geo-search in Cesium → Rust spatial query → inspector.
2. Geofence + entity watchlist alerting on live feeds (enter/exit/SGP4 overflight) with an alert inbox.
3. Timeline replay: DuckDB observation history + Cesium playback slider.
4. Per-record provenance & licence view in the inspector + audit log.
5. Saved searches as persistent feeds ("alert me on new matches").
6. Entity resolution across feeds (hex/reg/callsign; MMSI/IMO/name).
7. Briefing export: PDF/HTML map+layers+selection with auto-citations; GeoJSON/KMZ/CSV.
8. Heatmaps + histogram widgets (DuckDB aggregates, WASM binning).
9. Monitoring missions: saved composite state, shareable.
10. Graph view with search-around over co-occurrence relations.

Board columns for LB-113 (from TargetWorkbench, adapted to OSINT):
`Detected → Watching → Monitoring → Analysing → Reported`, plus High-Priority
watch lists.

Sources: [Gotham product page](https://www.palantir.com/platforms/gotham) ·
[G-Cloud Service Definition Document (PDF)](https://assets.applytosupply.digitalmarketplace.service.gov.uk/g-cloud-14/documents/92736/801146272055049-service-definition-document-2024-11-26-1253.pdf) ·
[Gotham API reference](https://palantir.com/docs/gotham/api/v1/revdb-resources/objects/search-objects) ·
[gotham-platform-python SDK](https://github.com/palantir/gotham-platform-python) ·
[AI-Enabled Operations white paper](https://www.palantir.com/assets/xrfr7uokpv1b/3A0y10xksgXENpRMNaAsUu/ed8f7f1ed534c0101f64536a85f7297b/Gotham_AI-Enabled_Operations_White_Paper.pdf) ·
[NSArchive platform doc](https://nsarchive.gwu.edu/sites/default/files/documents/3891748/Palantir-The-Palantir-Platform-The-Platform-for.pdf) ·
[SEC S-1](https://www.sec.gov/Archives/edgar/data/1321655/000119312520241694/d904406ds1a.htm) ·
[Europa](https://www.palantir.com/platforms/gotham/europa) ·
[PFCS Forward](https://blog.palantir.com/introducing-pfcs-forward-d8755d34c429)
