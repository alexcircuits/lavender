# End-to-end fixtures

Recorded or synthetic provider documents that `lavender-server` replays in fixture mode
(`server.fixtures_dir`, see `crates/lavender-core/src/providers/fixture.rs`). With them the
end-to-end tests exercise the real parsers, hub, API, stream and interface without
contacting a live source. Every snapshot served from here carries the notice
"Recorded fixture data, not live."

| File | Provenance |
|---|---|
| `usgs.geojson` | Recorded USGS `2.5_day` summary feed (45 events), copied from `crates/lavender-core/tests/fixtures/usgs-2.5_day.geojson` |
| `adsb-lol.json` | **Synthetic**, written for these tests: six contacts around Helsinki and Tallinn in adsb.lol v2 shape, including one military flag (`dbFlags: 1`), one on the ground and one malformed row the parser must skip. Registrations and hex codes are illustrative, not real flights. |
| `celestrak-stations.json` | **Synthetic** OMM element sets: `TEST STATION` (the set the Rust provider tests use, ISS-like orbit), `TEST POLAR` (sun-synchronous-like) and `TEST MEO` (navigation-like). Epoch 2026-10-10. |

Do not add personal data or anything a provider's terms forbid redistributing.
