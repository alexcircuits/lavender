# Upstream delta 2026-10: 1095 commits imported, what they carry, what to port

Record of the import and the port backlog derived from it. Written 2026-10-24,
after merge `30047df2` brought `upstream/main` (`9d3548c2`) under `legacy/`.
`git log HEAD..upstream/main` is empty; `legacy/` is byte-identical to
upstream/main (verified by `git diff upstream/main HEAD:legacy`).

## What the 1095 commits are

- 1095 commits total, 596 non-merge, 1199 files, +138k/−22k lines.
- Merge-base was `591f299d` (PR #986); upstream since merged ~350 PRs
  (#987–#1342).
- Largest change areas: `src/data` (12% of files), `src/ui` (11%+styles 2.5%),
  `src/layers` (10.6% + per-layer dirs), `server/providers` (5%), `scripts`
  (5.9%), `changelog.d` (6.4%).
- Named scopes with the most commits: street-level (70), i18n incl. zh-CN (49),
  qa-cctv-v2 (3), recent-imagery (2), air-quality (2). Most subjects are
  unscoped; port feature-level, not commit-by-commit.

## New upstream subsystems beyond the layer list

| Path | What it is | Lavender status |
|---|---|---|
| `src/sdr` | rtl-sdr IQ read pipeline, ADS-B frames → CPR decode → local feed (`adsbDecoder`, `gain`, `readPipeline`) | Not started; Zig kernels (`iq.zig`, `cpr.zig`, `adsb.zig`) already implement the maths for the browser |
| `server/providers/local-receivers.js` + `local.js` | Local receiver ingestion (SDR / local ADS-B) | Not started |
| `src/voice` | Voice narration, analyst cards, action schemas, cost tiers | Deferred (ENG-993, behind MCP ENG-992) |
| `src/scenes` | Director-led scene tours: camera motion, evidence data packs (e.g. `nepalEvidencePack`) | Not started; natural fit for a "missions" feature |
| `src/maps` | Map imagery source catalog, availability, credits | Partially covered by imagery_search provider; catalog/credits model worth porting |
| `src/tools/queries` | Analyst query packs: atmosphere, aviation, brief, cables… | Not started; overlaps DuckDB analytics (CEP/queries in core) |
| `server/providers/{gemini,openai}.js` | AI enrichment providers | Not started; owner-scope decision needed (like ENG-992) |
| `server/providers/gnssIntegrity.js` | GNSS integrity monitoring feed | Not started |
| `src/i18n` (+ zh-CN) | Full app localisation | Not started; decide i18n framework before UI grows further |

## Layer coverage matrix (upstream layer → Lavender)

Status key: **done** = provider + renderer + tests verified · **landed** = code
in tree from the 2026-10-22 WIP (checkpoint `0eaebbb3`), verification per
provider still owed · **partial** = some slices done · **—** = not started.

| Upstream layer | Rust provider | Web renderer | Status | Tasks |
|---|---|---|---|---|
| aircraft | adsb_lol + opensky + adsbdb enrichment | aircraft (+ history) | landed | ENG-948 |
| earthquakes | usgs | earthquakes | done | ENG-947 |
| satellites | celestrak + catalog/passes | satellites (+ Zig SGP4 wasm) | landed | ENG-949/976 |
| cyclones | cyclones | cyclones (tracks/cones) | landed | ENG-951/975 |
| firms | firms | fires | landed | ENG-950/975 |
| launches | launches (LL2) | LaunchPadRenderer | landed | ENG-957 |
| vessels | aisstream + vessel_store | vessels (+ symbology) | landed | ENG-953/980 |
| perimeters | perimeters | perimeters | landed | ENG-956 |
| bikeshare | bikeshare (GBFS) | bikeshare | landed | — |
| transit | transit (GTFS-RT) | transit (+ trails, Zig RDP) | landed | — |
| traffic | traffic (TomTom MVT) | traffic | landed | ENG-960 |
| cctv | cctv (15 cities + Mojo vision IPC) | cctv (+ frustums, detections) | landed | ENG-964 |
| weather | weather | weather | landed | ENG-954 |
| wind | wind (GRIB decode) | windStreamlines (+ Zig wasm) | landed | ENG-955 |
| submarineCables | cables (Telegeography embed) | cables | landed | ENG-967 |
| recentImagery | imagery_search | recent imagery types | landed | ENG-968 |
| installations | installations + infrastructure | (via boundaries/regions UI) | landed | ENG-966 |
| directions | routing (OSRM) | — | landed | ENG-963 |
| military | overpass (+ sanitiser) | — | landed | ENG-966 |
| streetLevel | mapillary | (streetLevel UI partial) | partial | ENG-965 |
| localAdsb | opensky (network) · SDR local | — | partial | new |
| airQuality | — | — | — | new |
| alpr | cctv detections carry `plate_text` | — | partial | new |
| awareness | — | — | — | new |
| flights (motion model) | — | — | — | new (see backlog LB-111) |
| gnss | — | — | — | new |
| radio | — | — | — | deferred (ENG-994 family) |

Static context: boundaries (Natural Earth admin-0, `crates/lavender-core/data/natural_earth/`),
regions (admin-1 + marine), terrain heights, us_census_counties,
neighborhoods, osm_military_names, sky (star field) all have Rust providers or
embedded data from the WIP; sky/starfield renderer not started.

## Port backlog (ranked; owner priorities first)

1. **LB-110 Borders, divisions, region drawing** (owner priority). Rust
   boundaries/regions providers are landed. Port upstream's
   `src/data/adminBoundaries.js` + `naturalEarthRegions.js` behaviour: division
   hierarchy display (admin-0 → admin-1 → counties/neighbourhoods), per-class
   styling (disputed, breakaway, maritime indicative), and a globe geofence
   drawing tool (polygon/box draw, named regions, persistence in workspace
   annotations).
2. **LB-111 Flight motion + simulation** (owner priority). Port upstream
   `src/layers/flights/{motion,controller,lifecycle}` dead-reckoning model and
   `src/cockpitMath.js` (heading slew, cockpit anchor correction) into
   lavender-core (predictor between ADS-B fixes) + a synthetic aircraft
   simulation mode for demos/offline.
3. **LB-112 Launch ascent simulation** (owner priority). Extends the launches
   provider: ascent trajectory integration (staging events, orbit insertion),
   rendered as a time-scrubbable arc from pad.
4. **LB-113 Kanban mission boards** (owner priority, Gotham-inspired). In-app
   analyst boards; see `kanban.md`.
5. **LB-114 airQuality provider** (new upstream layer; likely OpenAQ or
   equivalent — check `legacy/src/layers/airQuality/ingestion.js`).
6. **LB-115 gnss integrity provider** (`server/providers/gnssIntegrity.js`).
7. **LB-116 streetLevel remainder**: Mapillary deep integration + KartaView
   (impossible-timestamp fixes #1341 hint at both).
8. **LB-117 SDR/local receivers**: Zig kernels exist; wire an SDR ingestion
   path (native sidecar or browser via WASM) behind a feature flag.
9. **LB-118 scenes/evidence tours**: director + camera motion + data packs as
   "missions replay".
10. **LB-119 maps catalog/credits**: port the imagery-source availability model
    into the source registry UI.
11. **LB-120 i18n decision + framework**: upstream now ships zh-CN; decide
    before the interface grows.
12. **LB-121 query tool packs** over DuckDB analytics (atmosphere/aviation/brief).
13. **LB-122 AI enrichment providers** (Gemini/OpenAI): owner decision on
    scope/keys (mirrors ENG-992 gating).
14. **LB-123 awareness layer** (focus/history/navigation model — read
    `legacy/src/layers/awareness/model.js` before designing).

Deferred: voice (ENG-993), radio (ENG-994), ALPR beyond current detection
fields.

## Verification owed on the landed wave

Every "landed" provider above needs the checks the roadmap requires before its
task can be called done: parser tests on recorded fixtures, error/limit/cache
behaviour, licence/credit lines in the source registry, a renderer journey
(browser fixture), and performance sanity. `check.sh` (fmt, clippy −D warnings,
tests, deny, contracts) and `pnpm --dir apps/web run test` both pass on the
checkpoint; per-provider fixture journeys are the gap.
