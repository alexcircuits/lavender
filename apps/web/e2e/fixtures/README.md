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
| `cyclones-{status,points,track,cone}.json` | **Synthetic** NOAA-shaped `TEST ALPHA` advisory and GIS records, copied from core test fixtures; multi-part track/cone include dateline parts and a polygon hole. Schema/provenance references: `crates/lavender-core/tests/fixtures/cyclones-provenance.md`. This is not a real forecast. |
| `firms.csv` | **Synthetic**, eight illustrative FIRMS-shaped detections around Helsinki, spanning the five FRP legend bins. Fixed acquisition dates are preserved by fixture replay; these are not real fires or live observations. |
| `perimeters-wfigs.geojson`, `perimeters-inciweb-index.json`, `perimeters-inciweb-<slug>.html` | **Recorded**: seven NIFC WFIGS current perimeters from a 2026-10-15T20:47Z query and the InciWeb index and three incident pages (Internet Archive captures, 2026-08-02 to 2026-10-06), trimmed and redacted; copied from core test fixtures. Replay runs the real parsers, simplification and InciWeb verification and keeps the recorded dates. Provenance and terms: `crates/lavender-core/tests/fixtures/perimeters-provenance.md`. |
| `aisstream.jsonl` | **Synthetic** AISStream replay of invented ships (see `aisstream-provenance.md`). Its first line, `{"MessageType":"LavenderFixtureProvenance","Synthetic":true,…}`, declares it synthetic, so vessel snapshots carry the notice "Synthetic AIS replay: invented test ships at their recorded report times."; a replay without it gets "Vessel positions keep their recorded report times." |

The default browser suite stubs NASA GIBS imagery tiles locally. To separately inspect real
Blue Marble and Black Marble tiles with synthetic fires, run
`LAVENDER_E2E_LIVE_IMAGERY=1 PLAYWRIGHT_CHANNEL=chrome pnpm run e2e fires.spec.ts -g 'optional real NASA'`.
Those opt-in cases count fire pixels against the same view with fires off (real imagery has
orange pixels of its own), record the image responses and screenshots, contact only NASA GIBS
for imagery, and still replay FIRMS locally. The eight-point viewport timing attachment is a
local acceptance measurement, not a live or worldwide performance result. The eight detections
come back individually only for a viewport of at most 5°; wider and unbounded queries group
them into grid cells by design, so `fires.spec.ts` queries a box around Helsinki.

`hazard-imagery.spec.ts` is the ENG-975 review journey: for Blue Marble and Black Marble it
switches the imagery through the map controls and, over the Caribbean, Helsinki and an
Atlantic overview, turns each hazard layer on by itself and checks it draws its expected
hazard ramp colours (against the same view with it off, so one layer cannot stand in for
another), then turns earthquakes, fires and cyclones on together for the screenshots. It
checks the three legends against the ramp and picks a fire cell. Pixel counts paint out the
floating Layers panel, whose icons and legend swatches share the hazard colours. It also times the fire layer from switch to snapshot frame to first
pixels over five fresh loads (ENG-950 evidence, not the ENG-1000 harness). For the visual
review against real tiles, keeping evidence outside the repository:

    LAVENDER_E2E_LIVE_IMAGERY=1 LAVENDER_E2E_EVIDENCE_DIR=/path/outside/the/repo \
      PLAYWRIGHT_CHANNEL=chrome pnpm run e2e hazard-imagery.spec.ts

Its legend check also requires each swatch to draw the globe's shape (quake ring, fire cell,
cyclone spiral). A separate case centres an isolated recorded earthquake (M3.72 north-east of
Cruz Bay) and `TEST ALPHA` under the camera and clicks them: the ring must be open in the
middle yet select through its centre, and a click on the spiral's eye must select the storm.
Another centres the recorded Whites City, NM cluster (an M4.0 about 4 km from an M2.6 and an
M2.9) at 700 km: a click on a small ring's stroke drawn inside the M4.0 ring's open centre
must select the small quake, and a click on the M4.0 stroke must select the M4.0. Each hazard
must also add pixels in the hazard edge's light keyline (`#bcbcc4`) to every view.

Without `LAVENDER_E2E_EVIDENCE_DIR`, screenshots and `fire-toggle-timing.json` go to the
test's output directory. The fire layer must be available in fixture mode; while the server
still requires a FIRMS map key there, export any placeholder `LAVENDER_FIRMS_MAP_KEY` for the
run (fixture mode never contacts FIRMS).

Do not add personal data or anything a provider's terms forbid redistributing.

`vessels.spec.ts` is the ENG-980 journey over the AIS replay (chevrons and dots by pixel,
picking, inspector, track trail, MMSI search; Blue/Black Marble screenshots with the same
`LAVENDER_E2E_LIVE_IMAGERY` / `LAVENDER_E2E_EVIDENCE_DIR` switches). `vessels-perf.spec.ts`
is an opt-in frame-rate measurement (`LAVENDER_E2E_PERF=1`) with 12 000 **synthetic** vessels
generated at run time by `e2e/syntheticVessels.ts` (seeded, invented MMSIs and names) and
delivered through Playwright WebSocket routing; it is lane evidence, not the ENG-1000 harness.
