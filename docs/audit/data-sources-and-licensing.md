# Lavender — data sources, licensing and attribution audit

| | |
|---|---|
| Product | Lavender (NoirSystems), derived from God's Eye View (GEV), https://github.com/bilawalsidhu/gods-eye-view, MIT |
| Upstream snapshot audited | `products/lavender/legacy/` (GEV `package.json` version 0.2.1; npm dependencies not installed) |
| Audit date | 2026-10-11. Every external licence or terms fact below was checked on this date unless marked otherwise. |
| Question asked of every item | Can Lavender use and redistribute it in a product that NoirSystems **sells**, and under what attribution and conditions? |
| Owner policy | **Do not delete or auto-delete non-commercial assets.** Each one is recorded with its restriction and a mitigation option. Choosing a mitigation is a human decision (§8). |
| Status | Engineering audit, not legal advice. Items marked "unverified" could not be confirmed from an official page (blocked, 403/404, or no terms published). |

How to read the risk levels used below:

- **HIGH** — blocks a commercial sale as currently implemented: a non-commercial licence, a terms-of-service breach, or no right to use at all.
- **MEDIUM** — usable commercially only after a paid plan, written permission, self-hosting, or clarifying terms that are unclear or unpublished.
- **LOW** — an open licence or public-domain source; attribution and/or a usage policy is enough.

## 1. Summary verdict

### 1.1 Safe for commercial use, subject to the listed attribution and notices

- **GEV source code (MIT).** Keep the upstream notice (§2.1). Also add the missing MIT notice for code adapted from `skylight` (§2.2).
- **All runtime npm dependencies** (86 packages, all permissive: MIT/Apache-2.0/ISC/BSD/0BSD/Unlicense/Zlib). The only copyleft is LGPL-3.0 in `sharp`'s optional libvips binaries, a dev-only dependency that is never shipped (§3).
- **Bundled data:**
  - Natural Earth, US Census counties and DataSF neighbourhoods (public domain / PDDL).
  - OSM/Overture-derived datacenters, dams and military names (ODbL 1.0: attribution plus share-alike on the data).
  - The nine Sketchfab aircraft and ship models (CC BY 4.0: credit, link and modification notice).
- **US Government public-domain feeds:**
  - USGS earthquakes.
  - NOAA GFS, nowCOAST radar/satellite/lightning density, and NHC/CPHC.
  - NASA FIRMS, GIBS, CMR and HLS.
  - NIFC WFIGS perimeters (public domain per catalog mirrors) and InciWeb links.
  - Constraints that apply: no endorsement, no use of agency logos, FIRMS MAP_KEY quota.
- **Other openly licensed live sources:**
  - ECMWF IFS open data (CC BY 4.0 plus the mandated statement).
  - GDELT (citation and link).
  - Launch Library 2: add value, respect the rate limit or buy a key.
  - Radio Browser directory (public domain).
  - OpenFreeMap/OpenMapTiles tiles.
  - Re:Earth/Mapterhorn terrain (CC BY 4.0, no SLA).
  - DeFlock ALPR tiles (MIT tooling, ODbL data, no SLA).
  - adsb.lol (ODbL; the operator asks production users to make contact).
  - Mapillary (CC BY-SA 4.0 imagery; API commercial use limited to the scope in Mapillary terms §12).
- **Openly licensed camera and transit feeds:**
  - Fintraffic/Digitraffic (CC BY 4.0).
  - Ontario 511 (OGL-Ontario).
  - Open Calgary (OGL-Calgary).
  - Statens vegvesen (NLOD).
  - TfL JamCams (TfL OGL-based licence).
  - Live Traffic NSW (CC BY, version unclear; see the endpoint issue in §5).
  - Entur (NLOD), HSL and TransLink SEQ (CC BY 4.0).
- **OpenAI API** used with NoirSystems' own API key under the OpenAI Services Agreement.
- **CesiumJS** the library (Apache-2.0). Cesium **ion** is a separate service; see 1.2.

### 1.2 Needs a commercial licence, paid plan, written permission or self-hosting before sale

| Item | Why |
|---|---|
| **Google Maps Platform** (Photorealistic 3D Tiles, Geocoding, Places, Street View Static) | Commercial use is allowed on a billed account, but the legacy design breaches the terms in four ways (§5, §8 Q4). (1) It stores heights derived from 3D Tiles. (2) It shows Google content with or near non-Google maps (Esri/OSM/Bing stacks, Street View next to non-Google maps; non-EEA terms). (3) It sends screenshots that can contain 3D Tiles or Street View to OpenAI. (4) It can use Google 3D Tiles with non-Google geocoders (Photon/Nominatim fallback). |
| **Cesium ion** (Google 3D via ion, Bing imagery, Cesium World Terrain) | The Community plan is personal/non-commercial. Commercial plans cover use *within your organization*. Putting ion into a product sold to other organizations needs a separate agreement with Cesium. |
| **Bing Maps imagery via ion** | Cannot be combined with non-Bing imagery or Google 3D Tiles. It is being retired; availability after Sept 2026 is unverified. |
| **Esri World Imagery** (the default keyless basemap) | No keyless commercial grant. Use requires an ArcGIS Location Platform/ArcGIS account (key-based, metered) under the Esri Master Agreement. |
| **OpenSky Network** (primary flight feed) | Licensed for non-profit research/education only. For-profit use, and any operational use of the REST API in a live product, needs a written licence. The terms also forbid passing data on to third parties. |
| **adsbdb** (enrichment) | Route data may not be copied or incorporated into other databases without David J Taylor's permission. PlaneBase aircraft data needs PlaneBase permission for commercial export. |
| **AISStream.io** (vessels) | No terms of service or data licence are published, and commercial-terms requests on GitHub have no answer. Get written permission or switch to a commercial AIS provider. |
| **Open-Meteo** (cockpit weather and effects) | The keyless `api.open-meteo.com` endpoint the legacy code calls is "only … for non-commercial purposes". A paid plan is needed. |
| **TomTom Traffic** (BYOK) | Production and caching terms for the freemium tier could not be read from TomTom's terms page. Confirm the plan; the legacy code keeps a 120 s disk cache. |
| **CelesTrak** | The usage policy covers rate limits only; there is no licence grant and no statement on commercial use. Ask, or move to Space-Track/own SGP data. |
| **Public OSM-community servers**: `tile.openstreetmap.org`, Nominatim, FOSSGIS OSRM (`routing.openstreetmap.de`), Photon (`photon.komoot.io`), public Overpass | Their usage policies (Nominatim's 1 req/s absolute; FOSSGIS: no use as "a substantial part of an online offering"; tiles may be withdrawn from commercial users) do not fit a sold product. Self-host or use a commercial provider; the data stays ODbL. |
| **DriveBC webcam images** | DataBC lists OGL-BC, but the Ministry's developer notice says images "may not be rebroadcast or republished without express authorization" and offers a licence agreement. |
| **OpenAI via a user's ChatGPT/Codex sign-in** ("USE CHATGPT OAUTH") | Not a sanctioned route. Sign in with ChatGPT is a gated partner trial, does not cover Realtime/audio, and Codex CLI tokens are not for third-party apps. |

### 1.3 Non-commercial only, or no usable licence — recorded, not deleted

| Item | Restriction | Mitigation options (human decision) |
|---|---|---|
| **TeleGeography submarine cables** — `src/data/local_data/telegeography_submarine_cables/` | Upstream labels it CC BY-NC-SA 3.0. TeleGeography's current licence page grants CC BY-SA 4.0 for map *images/screenshots* only and says "Access to the underlying databases remains restricted to paying subscribers". **No commercial right. Even the non-commercial basis is unverified.** | Buy TeleGeography's geocoded map-data licence; or ship only in a non-commercial/evaluation edition; or use a reviewed build-time exclusion flag (files stay in the repo); or use an alternative source (OSM submarine cables, ODbL). |
| **Bhote Koshi 2026 event pack** — `public/events/bhote-koshi-2026/` (Vantor imagery, GeoPera reconstruction) **plus** derived coordinates compiled into `src/data/bhoteKoshiFloodPath.js` and related modules | CC BY-NC 4.0 (Vantor Open Data; GeoPera derived data "© Planet Labs PBC, © Vantor") | Written permission from Vantor (and Planet) and GeoPera; or a non-commercial edition; or build-time exclusion of the whole Nepal scene (assets, data modules, tool registration). |
| **Google News RSS** (cockpit headlines) | Google News terms: "for your own personal use (i.e., non-commercial use)"; robots are prohibited | Use GDELT (already the fallback, commercially licensed) or a licensed news API. |
| **OpenSky** as currently used | Non-profit research/education licence | Written OpenSky licence; or adsb.lol primary (ODbL, contact operator); or a commercial ADS-B provider. |
| **Open-Meteo free endpoint** | Non-commercial only | Open-Meteo paid plan; or NOAA/NWS API (public domain, US); or MET Norway (CC BY 4.0, commercial allowed). |
| **Cesium ion Community plan** | Personal/non-commercial | A Cesium commercial/integration agreement, or a no-ion architecture (Google 3D direct, Re:Earth terrain, licensed imagery). |
| **GEV README media** — `docs/media/**` (17 GIFs, 2 PNGs) | © Bilawal Sidhu; redistribution only with the upstream repo; "Commercial reuse outside this repository requires separate permission" | Keep under `legacy/`; ask permission before any reuse in Lavender docs or marketing. |

### 1.4 Non-compliant as implemented, regardless of licence (must be re-engineered for Lavender)

1. **`src/data/local_data/cctv_ground_heights/cctv_ground_heights.json`** was sampled from Google Photorealistic 3D Tiles and stored. This breaches GMP Terms §3.2.3(a) (no pre-fetching or storing; no bulk download of elevation values) and §3.2.3(c) (no creating content from Google Maps Content, e.g. terrain from elevation values). It also breaches the Map Tiles policy, which allows no non-visualisation use. Regenerate it from open terrain.
2. **Google content shown with or near non-Google maps.** Covers 3D Tiles toggled against Esri/OSM/Bing stacks, Places/Geocoding results drawn over Esri/OSM, and Street View fallback frames in the scene (non-EEA GMP §3.2.3(e); EEA terms are stricter still for Places and Geocoding).
3. **Viewport screenshots sent to OpenAI** (`src/voice/realtimeViewport.js`, `input_image`). They can contain Google 3D Tiles, Street View and CCTV frames showing people. Google bars "image analysis / machine interpretation" of Map Tiles and resharing; this is also a privacy issue (§7).
4. **Codex/ChatGPT OAuth voice path** (`server/providers/openai/codex-auth.js`) — not permitted for a product.
5. **Live Traffic NSW frames.** The proxy deliberately presents a browser User-Agent to `webcams.transport.nsw.gov.au`, an image host that returns a placeholder to non-browser clients (`DATA_SOURCES.md`). It also reads the Live Traffic website feed rather than the keyed Open Data Hub API. Circumventing a host's client filtering is a terms risk; use the official API.
6. **Upstream identity strings** in `User-Agent`, `Referer`, `Digitraffic-User` and `ET-Client-Name` headers still name GEV (§2.1). Lavender must identify itself.

### 1.5 Unknown — terms not published or not retrievable (permission recommended before sale)

- **CCTV:**
  - City of Austin frames (`cctv.austinmobility.io`; no dataset licence found).
  - TxDOT ITS snapshots.
  - Caltrans CWWP2: a portal for commercial redistributors, but the Conditions of Use could not be retrieved.
  - DelDOT live HLS: no terms found; upstream says use beyond personal viewing "needs confirmation".
  - City of Tallinn `ristmikud.tallinn.ee` (no terms).
  - Transpordiamet/Tarktee (no licence found).
  - Stadt Warendorf webcam (no terms; German image and privacy law).
- **Transit:**
  - MBTA/MassDOT Developers License Agreement (PDF blocked).
  - CapMetro (licence grants use and redistribution but is silent on commercial use).
  - Metro Transit (no terms found).
  - OVapi ("free to use", no named licence).
- **GBFS feeds** (Lyft, BCycle, PBSC, Bluebikes, Biketown): no `license_url` in the Lyft feed. Lyft-run Divvy terms allow use "in your product or service" but forbid redistributing the data as a stand-alone dataset and using trademarks.
- **Other:**
  - Pilot camera pack videos (Google Chromecast promo clips).
  - TomTom tile test fixture.
  - Provenance of the voice WAV fixture, the street JPG fixture and the ALPR marker PNGs.
  - The Cesium-bundled moon texture (© James Hastings-Trew).
  - The exact copyright line for `skylight`.

## 2. Source-code licence

### 2.1 Upstream MIT licence — what Lavender must carry

God's Eye View (GEV) is MIT-licensed (`legacy/LICENSE`, `legacy/package.json` `"license": "MIT"`).
MIT allows commercial use, modification, sublicensing and sale. Its single condition is
that "the above copyright notice and this permission notice shall be included in all copies
or substantial portions of the Software". Lavender therefore **must reproduce the following
text verbatim** wherever it distributes code that is, or is derived from, GEV code. That
covers source releases, the shipped web bundle and server binaries/containers. Put it in a
`LICENSES/` or `THIRD_PARTY_NOTICES` file, and in an in-app "Open-source licences" screen
for distributed clients:

```text
MIT License

Copyright (c) 2026 Bilawal Sidhu

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

Notes:

- **Contributions are MIT too.** `legacy/CONTRIBUTING.md` line 168 says "By contributing, you agree your contributions are licensed under the project's MIT License". Community PR code (for example the weather contributions credited in `DATA_SOURCES.md`) is covered by the same notice.
- **Lavender's own code** (the Rust core and new client code) may carry any NoirSystems licence, including a proprietary one. GEV-derived files keep the MIT notice above. A port to another language that follows the original structure closely should still be treated as a "substantial portion".
- **The carve-out paragraph in `legacy/LICENSE`** (after the `---`) is not part of the MIT grant. It is a notice that third-party data and models are excluded from MIT. Carry it forward as informational text, because it describes exactly the exclusions recorded in §4 of this audit.
- **No trademark rights.** MIT grants copyright permission only. Do not use the "God's Eye View" name, `public/logo.svg` (its `<title>` is "God's Eye View"), or the "Halfpixel" brand in Lavender's product UI, store listings or marketing, and do not suggest endorsement by Bilawal Sidhu or Halfpixel. The code also carries upstream identity strings: HTTP `User-Agent`/`Referer` values such as `gods-eye-view/0.1 (+https://github.com/bilawalsidhu/gods-eye-view)`, `gods-eye-view-cctv-proxy/1.0` and `gods-eye-view-celestrak-proxy/1.0`. They also include `DIGITRAFFIC_USER = 'gods-eye-view'`, `ET-Client-Name: gods-eye-view-transit` and the MCP server name `gods-eye-view`. They appear in `server/providers/{gbfs,cctv,cctv/*,aircraft/*,overpass/constants,regional/place,space/celestrak,places/routes}.js`, `src/data/transitFeeds.js`, `src/data/transitProxy.js` and `server/mcp/server.js`. Several providers (Nominatim, OVapi, Entur, Digitraffic, CelesTrak) require the client to identify itself truthfully. Lavender must replace all of these with its own identity and contact address. Sending the upstream's identity would misidentify the requester to those providers.
- **README media is not MIT.** `legacy/docs/media/README.md` says the 17 capture GIFs and 2 PNGs are © Bilawal Sidhu: "Permission is limited to their inclusion and redistribution with this repository … Commercial reuse outside this repository requires separate permission". The GIFs also show Google Photorealistic 3D Tiles. Do not reuse them in Lavender marketing or documentation without written permission (see §8).

### 2.2 Third-party code embedded in GEV source files

`legacy/THIRD_PARTY_NOTICES.md` lists five npm packages: `@jtarrio/webrtlsdr` 3.0.6 (Apache-2.0), `@jtarrio/signals` 0.10.1 (Apache-2.0), `mapillary-js` 4.1.2 (MIT, © Facebook, Inc. and its affiliates), `rxjs` 7.8.2 (Apache-2.0) and `three` 0.134.0 (MIT). Lavender must carry those entries forward. It should also replace them with a complete generated inventory (see §3), because the upstream file covers only "the packages added for the browser-local SDR feature and the Street Level viewer".

**Gap found: code adapted from `skylight` (MIT, https://github.com/cpaczek/skylight) is not listed in `THIRD_PARTY_NOTICES.md`.** Source comments credit it in:

| File | What was adapted |
|---|---|
| `src/data/aircraftClass.js` | ICAO type-designator sets ("adapted from skylight … web/src/display/aircraftGlyph.ts") |
| `src/data/motionModel.js` | turn-rate and arc integration (`shared/src/aim.ts`) |
| `src/layers/flights/motion.js`, `src/layers/military/motion.js` | arc math |
| `src/data/aircraftMeta.js` | "sticky enrichment" pattern (`server/src/datasource.ts`) |
| `src/data/routePlausible.js` | `routePlausible()` (`web/src/display/renderer.ts`) |
| `src/data/satellitePass.js` | `nextISSPass` (`shared/src/celestial.ts`) |
| `server/providers/aircraft/enrichment.js` | adsbdb route proxy (`server/src/enrich/routes.ts`) |
| `server/providers/space/celestrak.js` | TLE store (`TleStore`) |

MIT requires skylight's own copyright line and permission notice in Lavender's notices. The skylight README states MIT (checked 2026-10-11). The exact copyright line must be copied from `github.com/cpaczek/skylight/blob/main/LICENSE` before release; see open question Q14.

### 2.3 Apache-2.0 obligations (CesiumJS, hls.js, eccodes-wasm, rxjs, webrtlsdr and others)

When Lavender distributes these packages in its bundle, Apache-2.0 §4 requires four things: give recipients a copy of the licence, keep copyright and attribution notices, mark modified files, and pass on any `NOTICE` file. CesiumJS also ships a `LICENSE.md` that lists its own bundled third-party code and assets. Those include Natural Earth imagery (public domain), NASA sky-box images and the moon texture from JHT's Planetary Pixel Emporium (© James Hastings-Trew). They also include BSD/MIT/ISC libraries and the Source Sans Pro font (SIL OFL 1.1). Ship Cesium's `LICENSE.md` unchanged with the build (https://github.com/CesiumGS/cesium/blob/main/LICENSE.md, checked 2026-10-11). Because `vite-plugin-cesium` copies all of Cesium's `Assets/` into the build output, those assets are redistributed even though GEV never configures them explicitly.

## 3. Dependency licence matrix

Source: `legacy/package.json` and `legacy/package-lock.json` (lockfile v3, 243 resolved packages). Licences were read from the lockfile's `license` fields. Seven packages without that field were resolved from the npm registry's legacy `licenses` field (checked 2026-10-11). Nothing was installed.

### 3.1 Direct dependencies

| Package | Range in package.json | Resolved | Scope | Licence | Commercial use | Notes |
|---|---|---|---|---|---|---|
| `cesium` (+ `@cesium/engine` 22.3.0, `@cesium/widgets` 14.3.0) | ^1.124.0 | 1.138.0 | runtime (browser) | Apache-2.0 | Yes | Ship Cesium `LICENSE.md` (bundles third-party code and assets, §2.3). Cesium **ion** is a separate paid service (§5). |
| `hls.js` | ^1.7.3 | 1.7.3 | runtime (browser) | Apache-2.0 | Yes | CCTV live HLS playback |
| `mapillary-js` | ^4.1.2 | 4.1.2 | runtime (browser, lazy) | MIT | Yes | Pulls in `three` 0.134.0 (MIT), `rxjs` 7.8.2 (Apache-2.0), `virtual-dom` (MIT) and others |
| `satellite.js` | ^6.0.2 | 6.0.2 | runtime | MIT | Yes | SGP4 propagation |
| `@meri-imperiumi/eccodes-wasm` | ^2.48.2 | 2.48.2 | runtime (server) | Apache-2.0 | Yes | README lists compiled-in ecCodes (Apache-2.0, ECMWF), OpenJPEG (BSD-2-Clause) and libaec (BSD-2-Clause); carry all three notices (https://github.com/meri-imperiumi/eccodes-wasm, checked 2026-10-11) |
| `@jtarrio/webrtlsdr` | ^3.0.6 | 3.0.6 | runtime (browser) | Apache-2.0 | Yes | Portions © Google Inc. (per upstream notice) |
| `@jtarrio/signals` | ^0.10.0 | 0.10.1 | runtime (browser) | Apache-2.0 | Yes | — |
| `@mapbox/vector-tile` | ^3.0.0 | 3.0.0 | runtime | BSD-3-Clause | Yes | Keep the copyright notice; no endorsement by Mapbox |
| `pbf` | ^5.1.2 | 5.1.2 (3.3.0 nested under mapillary-js) | runtime | BSD-3-Clause | Yes | — |
| `mgrs` | ^2.1.0 | 2.1.0 | runtime | MIT | Yes | — |
| `egm96-universal` | ^1.1.1 | 1.1.1 | runtime | MIT | Yes | Contains EGM96 geoid data (NGA, U.S. Government work) |
| `vite` | ^6.0.0 | 6.4.3 | dev/build | MIT | Yes | Not shipped |
| `vite-plugin-cesium` | ^1.2.23 | 1.2.23 | dev/build | MIT | Yes | Copies Cesium Assets into the build |
| `puppeteer` | ^25.10.0 | 25.10.0 | dev (QA scripts) | Apache-2.0 | Yes | Downloads Chrome for Testing at install time (Google terms); never shipped |
| `sharp` | ^0.35.4 | 0.35.4 | dev (asset scripts) | Apache-2.0 | Yes | Prebuilt `@img/sharp-libvips-*` binaries are **LGPL-3.0-or-later** (see 3.2) |
| `prettier` | 3.9.6 | 3.9.6 | dev | MIT | Yes | Not shipped |
| `ws` | ^8.21.0 | 8.21.3 | dev (QA/MCP scripts) | MIT | Yes | Not shipped |

### 3.2 Transitive summary

| Licence | All packages | Runtime (non-dev) packages |
|---|---:|---:|
| MIT | 155 | 43 |
| Apache-2.0 | 34 | 15 |
| ISC | 20 | 10 |
| BSD-3-Clause | 8 | 6 |
| No `license` field in lockfile; registry metadata says **MIT** (`dom-walk`, `error`, `ev-store`, `individual`, `string-template`, `x-is-array`, `x-is-string`, all via `mapillary-js → virtual-dom`) | 7 | 7 |
| `(MPL-2.0 OR Apache-2.0)` (`dompurify` via Cesium; choose Apache-2.0) | 1 | 1 |
| `(MIT AND Zlib)` (`pako` via Cesium) | 1 | 1 |
| `(MIT or Apache-2 or ISC)` (`s2-geometry` via mapillary-js; informal SPDX expression, all options permissive) | 1 | 1 |
| Unlicense (`robust-predicates`) | 1 | 1 |
| 0BSD (`tslib`) | 1 | 1 |
| **LGPL-3.0-or-later** (`@img/sharp-libvips-*` ×10) | 10 | 0 |
| **Apache-2.0 AND LGPL-3.0-or-later** (`@img/sharp-win32-*` ×3) / **+ MIT** (`@img/sharp-wasm32`) | 4 | 0 |
| **Total** | **243** | **86** |

Findings:

- **No copyleft or non-OSI licence in the runtime dependency tree.** The only copyleft code is LGPL-3.0-or-later in `sharp`'s optional, platform-specific libvips binaries. They are a dev-only dependency used by asset-processing scripts, so they are not part of the browser bundle or the server runtime. If Lavender ever ships `sharp` in a product, LGPL obligations apply: provide the licence text and libvips source or an offer, and allow relinking, which the dynamically linked prebuilt binaries already permit.
- **MPL-2.0 is offered only as one side of an `OR`.** `dompurify` can be taken under Apache-2.0.
- **Seven MIT packages carry no SPDX `license` field.** They declare MIT only in the deprecated `licenses` array. Licence scanners such as `license-checker` and Syft may report them as "UNKNOWN", so add them to an explicit allow-list with this evidence.
- **The Rust core will need the same process.** This audit covers only the legacy npm tree. Run `cargo-deny` (licences + advisories) and generate a combined notices file in CI. The npm side can use `license-checker --production --json` or an SBOM (CycloneDX) from the lockfile.
- **Upstream notices are incomplete.** `THIRD_PARTY_NOTICES.md` covers five packages only. Generate the complete notice set automatically for every distributed build.

## 4. Bundled dataset and asset matrix

"Commercial use" means use inside a product NoirSystems sells. Per the product owner's policy, **nothing below is to be deleted or auto-deleted**. Restricted items are recorded with their restriction and a mitigation option. Every mitigation is a human decision (see §8).

| # | Item | Path | Origin | Licence | Commercial use | Redistribution | Attribution required | Notes / mitigation |
|---|---|---|---|---|---|---|---|---|
| B1 | TeleGeography submarine cables (712 cables) and landing points (1,917) | `src/data/local_data/telegeography_submarine_cables/` (`cable-geo.json` 728 KB, `landing-point-geo.json` 359 KB, `source.json`, `README.md`) | `submarinecablemap.com/api/v3/...`, downloaded 2026-05-24 (`source.json` sha256 recorded) | Upstream claims **CC BY-NC-SA 3.0**. **Not confirmed:** TeleGeography's licence page (https://www2.telegeography.com/license-telegeography-map, checked 2026-10-11) puts only map *images/screenshots* under CC BY-SA 4.0 and says "Access to the underlying databases remains restricted to paying subscribers" | **NO** — commercial use needs TeleGeography's paid geocoded-data licence | Unverified even non-commercially (the NC claim likely traces to TeleGeography's deleted pre-2022 GitHub repo, not to this API v3 snapshot) | "© TeleGeography — submarinecablemap.com" + licence link | **RESTRICTED.** Mitigation: (a) buy a commercial licence from TeleGeography; (b) ship the layer only in a non-commercial/evaluation edition; (c) build-time exclusion flag for commercial artifacts (files stay in the repo); (d) commercial alternative: OSM `communication=line` / `submarine=yes` cables (ODbL), less complete. Note that `source.json` `license_note` says "Check TeleGeography terms before redistribution or commercial use". Q1. |
| B2 | Bhote Koshi 2026 event imagery | `public/events/bhote-koshi-2026/pre.webp` (1.35 MB), `post.webp` (1.00 MB) | Vantor (formerly Maxar) Open Data Program, WorldView-2 scene `10300100C86CED00` (2021-10-16) and WorldView-3 `B040001100881410` (2026-08-27), cropped and resampled | **CC BY-NC 4.0** | **NO** | Yes, non-commercially | Vantor Open Data + CC BY-NC 4.0 link + modification note (see `README.md` there) | **RESTRICTED.** Mitigation: written commercial permission from Vantor; or non-commercial edition only; or build-time exclusion of the Nepal scene (event pack + data modules below + `get_bhote_koshi_flood` tool registration in `src/tools/queries/events.js`). Files stay in the repo. |
| B3 | Bhote Koshi reconstruction (river centerline, chainage/elevation corridor, 16 evidence records) | `public/events/bhote-koshi-2026/event.json` (42 KB) | GeoPera reconstruction commit `59ee5bd` (github.com/geo-pera/bhotekoshi-2026-reconstruction) plus links to witness posts and the GeoGeorgeShadrach map on X | **CC BY-NC 4.0** (derived centerline); linked posts keep their owners' terms | **NO** | Yes, non-commercially (links only for social posts) | GeoPera + CC BY-NC 4.0 | Same as B2 |
| B4 | Bhote Koshi coordinates compiled into JS | `src/data/bhoteKoshiFloodPath.js` (19 KB, header says "CC BY-NC 4.0 (derived data; Planet Labs PBC / Vantor open data)"). Related modules: `bhoteKoshiEvent.js`, `bhoteKoshiLocator.js`, `bhoteKoshiIncidentPlaces.js` (curated place coordinates), `bhoteKoshiShotPaths.js`, `bhoteKoshiEmbeddedMedia.js` (YouTube/Facebook/X embed URLs) | GeoPera centerline (every 4th vertex) | **CC BY-NC 4.0** for the coordinate data, even though it sits in a `.js` file; the surrounding code is MIT | **NO** | Non-commercial only | Same as B3 | Upstream warns: "Deleting only `public/events/bhote-koshi-2026/` does not remove all restricted data". Same mitigation as B2. The embedded-media module loads third-party scripts (`www.youtube.com`, `connect.facebook.net`, `platform.twitter.com`), which is a privacy/cookie-consent issue (§7). |
| B5 | Datacenters (4,351 features) | `src/data/local_data/datacenters/datacenters.geojsonl` (2.56 MB) | OpenStreetMap extract. Contact tags removed. Extraction date and query **not recorded** | **ODbL 1.0** | Yes, with conditions | Yes; derivative database must stay ODbL and be offered | "© OpenStreetMap contributors" (+ link to osm.org/copyright) | OK. ODbL share-alike applies to the database: publish or offer Lavender's modified extract under ODbL. Record extraction date/query on the next refresh. |
| B6 | Dams (704 features) | `src/data/local_data/dams/dams.geojson` + `dams.geojsonl` (730 KB each) | Open Infrastructure Map / OSM PostGIS layers. Contact fields removed | **ODbL 1.0** | Yes, with conditions | Yes (ODbL) | "© OpenStreetMap contributors" + "Open Infrastructure Map" | OK (as B5). |
| B7 | Military area names (36,466 rows) | `src/data/local_data/osm_military_names/names.json` (3.86 MB) + `names.sha256` | OSM via Overture Maps `base/land_use` release 2026-09-23.1 (OSM snapshot 2026-09-06) | **ODbL 1.0** | Yes, with conditions | Yes (ODbL) | "© OpenStreetMap contributors" + "Overture Maps Foundation" | OK for licensing; sensitive content (§7). |
| B8 | Natural Earth physical regions, marine, countries, states/provinces | `src/data/local_data/natural_earth/{regions,marine,countries,states_provinces}.json` (2.0 / 0.63 / 0.95 / 3.65 MB) | `nvkelso/natural-earth-vector` commit `ca96624…`, SHA-256 pinned | **Public domain** | Yes | Yes | None required; courtesy "Made with Natural Earth" | OK. Country boundaries are politically sensitive (disputed areas); Natural Earth shows de facto boundaries. Product decision, not licensing. |
| B9 | US counties (3,235) | `src/data/local_data/us_census_counties/counties.json` (1.54 MB) | US Census Bureau `cb_2025_us_county_5m`, SHA-256 pinned | **Public domain** (17 U.S.C. §105) | Yes | Yes | None; courtesy "U.S. Census Bureau" | OK. |
| B10 | SF Analysis Neighborhoods (41) | `src/data/local_data/neighborhoods/san-francisco.json` (222 KB) | DataSF `j2bu-swwd`, retrieved 2026-07-30 | **PDDL 1.0** | Yes | Yes | None; courtesy credit | OK. |
| B11 | CCTV ground heights (3,445 cameras × 10 points) | `src/data/local_data/cctv_ground_heights/cctv_ground_heights.json` (1.99 MB) | **Sampled from Google Photorealistic 3D Tiles**: `"provider": "google-3d-tiles"`, generated 2026-09-14 by `scripts/precompute-cctv-heights.mjs` through headless Chrome `scene.sampleHeightMostDetailed()` | No licence; **derived from Google Maps Content** | **NO — prohibited by the Google Maps Platform Terms**: §3.2.3(a) bars storing Google Maps Content and bulk-downloading "elevation values"; §3.2.3(c) bars creating content from it (terrain from elevation values); the Map Tiles policy bars non-visualisation use. See §5.1 | Not redistributable | n/a | **HIGH RISK, unlabelled upstream.** `DATA_SOURCES.md` lists the licence as "—". Mitigation: regenerate the same file from an openly licensed surface (Re:Earth/Mapterhorn terrain CC BY 4.0, Copernicus DEM, or OSM building heights); or keep runtime sampling only (the code already falls back to runtime terrain placement when no entry matches). Keep the file in the repo, but do not ship it in commercial builds until a legal decision is made (Q3). |
| B12 | Aircraft and ship 3D models (9) | `public/models/{airplane,jet,ship,bell206,c172,citation2,mq9,b789,atr72}.glb` (88–562 KB each) | Sketchfab authors listed in `public/models/README.md` (zairiq-123, Nick the Name, Javier_Fernandez, terran4627, e737, BlenderCommunityHead, IProZenoN, Nobilis 2, Oyan3D) | **CC BY 4.0** each | Yes | Yes | Per model: title, author, source link, licence link, and a statement that it was modified | OK. Lavender must display these credits (in-app "Credits/Licences" screen + notices file) and keep the modification notes. Aircraft manufacturer trademarks (Boeing, Cessna, ATR, Bell, General Atomics MQ-9) are depicted; low risk for a generic map visualisation, but do not use them in marketing. |
| B13 | Pilot CCTV pack (Tokyo/Shinjuku) | `config/cctv_sources.shinjuku.json` | Points to Google Chromecast promo clips `storage.googleapis.com/gtv-videos-bucket/sample/ForBigger*.mp4` placed at fictitious Tokyo camera positions | Unknown: Google promotional clips with no published licence; the file says "Demo sample stream for projection pipeline testing" | **Unknown — treat as not licensed** | Hot-linked, not bundled | n/a | Not referenced by code (only loaded through `CCTV_SOURCES_FILE`). Presenting promo videos as cameras at real locations is also misleading. Mitigation: test-only; use CC-licensed clips (Blender open movies, CC BY) if a demo pack is needed. |
| B14 | Curated CCTV catalogs | `config/cctv_sources.tallinn.json` (172 KB, City of Tallinn intersection cameras: id, name, coordinates, heading/pitch/FOV priors, image URLs on `ristmikud.tallinn.ee`); `config/cctv_sources.warendorf.json` (1 camera, Marktplatz; pose derived from OSM geometry, ODbL); `config/cctv_sources.austin.json` (`[]`) | Hand-curated by GEV contributors from provider URLs and OSM | Catalog metadata: curated facts plus ODbL-derived pose (Warendorf). Image rights stay with the camera operators (§5) | Catalog: yes (facts + ODbL attribution). **Frames: unknown** (§5) | Catalog: yes | "© OpenStreetMap contributors" for OSM-derived poses; operator credit for frames | See §5 (Tallinn, Warendorf) and §7. |
| B15 | OSM/OpenFreeMap test fixtures | `src/data/fixtures/ofm-*.pbf`, `osm-alpr-austin-11-467-843.pbf` (from `tiles.dontgetflocked.com`), `traffic-road-access.json` | OpenFreeMap / OpenMapTiles tiles and the DeFlock extract, retrieved 2026-09-23/25 | **ODbL 1.0** | Yes (test-only) | Yes (ODbL) | OSM + OpenMapTiles | OK; keep out of product bundles. |
| B16 | TomTom traffic-flow tile fixture | `src/data/fixtures/tomtom-flow-austin-12-935-1686.pbf` (23 KB, captured 2026-07-16) | `api.tomtom.com` | **© TomTom**, proprietary | **Not covered by any licence we hold** | Stored TomTom content in a repo; TomTom terms generally restrict storage | © TomTom | Test-only. Mitigation: keep it out of product artifacts and public mirrors; or ask TomTom; or replace with a synthetic MVT. |
| B17 | FIRMS sample CSV, FIRMS cases | `src/data/fixtures/firms-viirs-noaa20-sample.csv`, `firms-csv-cases.json` | NASA FIRMS | U.S. public domain / CC0 (NASA) | Yes | Yes | NASA FIRMS acknowledgement (requested) | OK (test-only). |
| B18 | ADS-B receiver captures | `src/data/fixtures/adsb-austin-*.json`, `adsb-austin-frames.txt` | Radio frames received over Austin TX by a contributor | Facts from a public broadcast; no licence | Yes (test-only) | Yes | None | OK. Contains aircraft ICAO addresses and callsigns, so low personal-data risk. |
| B19 | Voice QA fixture | `scripts/fixtures/voice/full-globe-turn-on-radio.wav` (884 KB) | "Owner-generated" synthetic speech; provenance in WAV metadata; "approved for repository QA use by the user on 2026-08-05" | Unspecified; generator usage terms embedded in metadata | Test-only; unknown for redistribution | Unknown | Unknown | Not shipped. Inspect the embedded metadata (`ffprobe`) before any reuse (Q15). |
| B20 | Street-level fixture photo | `scripts/fixtures/street-level/street-640x320.jpg` (3.7 KB) | Undocumented | Unknown | Test-only | Unknown | Unknown | Not shipped; provenance undocumented. |
| B21 | ALPR marker icons | `src/layers/alpr/assets/alpr-marker-{normal,selected,selected-brackets}.png` | Undocumented; presumably original GEV artwork | Presumed MIT (repo) | Presumed yes | Presumed yes | MIT notice | Provenance not recorded; confirm or redraw (Q15). |
| B22 | UI SVG icons and logo | `public/{logo,pin,mic,location,visual-presets}.svg` | GEV project | MIT (code repo) | Yes for icons; **logo = GEV brand** | Yes | MIT notice | Replace `logo.svg` with Lavender branding (no trademark licence). |
| B23 | Web fonts (hot-linked, not bundled) | `index.html` loads Inter, JetBrains Mono and Material Symbols Outlined from `fonts.googleapis.com`/`fonts.gstatic.com` | Google Fonts | SIL OFL 1.1 (Inter, JetBrains Mono); Apache-2.0 (Material Symbols) | Yes | Yes (self-hosting allowed) | Licence text when self-hosting | Recommend self-hosting: no runtime Google request, which avoids the GDPR IP-transfer issue (§7). |
| B24 | Cesium bundled assets (copied by `vite-plugin-cesium`) | Cesium `Assets/` in the build output: Natural Earth II imagery, NASA sky-box (Tycho), `moonSmall.jpg`, approximate terrain heights, Source Sans Pro font, widget icons | CesiumJS 1.138.0 | Listed in Cesium `LICENSE.md`. Natural Earth is public domain. NASA sky-box falls under NASA media guidelines. Moon texture © James Hastings-Trew (JHT Planetary Pixel Emporium). Source Sans Pro is OFL 1.1 | Yes, except the moon texture, which is **uncertain** for commercial redistribution | Ship Cesium `LICENSE.md` | Per Cesium `LICENSE.md` | The JHT site requires the author's permission to package the maps into distributed software. Cesium redistributes the texture, but its licence for downstream commercial redistribution is not stated. Mitigation: disable `scene.moon`, or replace the texture with a public-domain NASA LRO moon texture (Q13). |
| B25 | README media (repo docs, not app) | `docs/media/*.gif`, `docs/media/start-here/*.gif`, `youtube-popular-videos.png`, `open-source-survey.png` | © Bilawal Sidhu; promotional captures containing Google 3D Tiles | **Not MIT.** Redistribution only "with this repository and its project documentation"; "Commercial reuse outside this repository requires separate permission" | **NO** (outside the upstream repo) | Only as part of the upstream repo copy | Keep in-frame attribution | Kept under `legacy/` as the upstream snapshot. Do not use in Lavender docs or marketing without permission (Q12). |

## 5. External data-source matrix

**How this list was built.** It comes from a host grep over `legacy/server`, `legacy/src`, `legacy/build` and `legacy/config`, excluding tests and fixtures, plus `DATA_SOURCES.md` and `.env.example`.

**Not used by the code:**
- ADS-B Exchange, Space-Track, Windy, RainViewer.
- The public Overpass instances: `OVERPASS_UPSTREAMS` is empty by default and only operator-supplied endpoints are used.

**Refresh values** are the legacy code's constants: client `updateInterval`, server cache TTLs.

**Target location** is where the fetch should live in Lavender:
- **Core** = Rust core provider: server-side fetch, key custody, caching, rate governance, and attribution metadata delivered to the client.
- **Client** = client-side tile/imagery fetch straight from the provider. Licence terms are unchanged; only keys meant for the browser are exposed.

### 5.1 Basemaps, 3D, terrain and imagery

| Provider | Purpose | Data format | API key / auth | Refresh used | Licence / terms (URL, checked 2026-10-11) | Commercial use | Attribution text required | Rate limits / usage policy | Legacy location | Target in Lavender | Risk |
|---|---|---|---|---|---|---|---|---|---|---|---|
| **Google Map Tiles API — Photorealistic 3D Tiles** | Photoreal 3D globe; authored scene shots | OGC 3D Tiles (glTF/b3dm), session-based | `GOOGLE_MAPS_API_KEY` (browser, referrer-restricted) or server-minted short-lived tokens (`googleTokens.js`) | Streamed on camera movement; one root session covers roughly 3 h of rendering (README) | https://cloud.google.com/maps-platform/terms (mod. 2026-08-26) · https://cloud.google.com/maps-platform/terms/maps-service-terms · https://developers.google.com/maps/documentation/tile/policies | **Yes, billed**, with conditions: no caching or storage beyond headers; no derived content (B11); not "with or near a non-Google Map" (non-EEA §3.2.3(e)); no non-visualisation use ("Image analysis, Machine interpretation"); no AI-model improvement (§3.2.3(c)(vii)) | Google Maps logo (16–19 dp; text "Google Maps" only when space is short) **plus all tile data attributions aggregated, sorted and displayed in a line on screen** (Cesium `showCreditsOnScreen: true`). The logo must not overlap the Cesium logo. App ToS must reference the Google Maps Additional Terms and the Google Privacy Policy | Per-project quotas and billing; free monthly root-session allowance (verify current pricing) | `src/maps/google3d.js`, `src/maps/googleTokens.js`, `src/maps/defaultSources.js`, `src/maps/catalog.js` | **Client** (tiles direct from Google); **Core** only mints tokens and enforces budgets; never caches tiles | **HIGH** |
| **Google Geocoding API** | Named-place fly-to, reverse geocode | JSON | Browser key | Per search | Same GMP terms; Geocoding service terms §6.2 | Yes, billed; **not with a non-Google map** (non-EEA); EEA: content (except lat/lng/place_id) not "With any Map" | Google Maps logo where results appear | lat/lng cache ≤30 days; addresses cached only per end user | `src/search/http.js`, `src/search/defaults.js` | **Core** | **HIGH** (results flown to on Esri/OSM stacks) |
| **Google Places API (New)** — Nearby / Text Search | Nearby-installations search; voice scene context | JSON | `GOOGLE_MAPS_SERVER_API_KEY` (server, IP-restricted) | On request; `Cache-Control: private, max-age=300`; per-IP limit 120/min (`GEV_RATELIMIT_GOOGLE_PER_MIN`) | Places policies: https://developers.google.com/maps/documentation/places/web-service/policies | Yes, billed; **not with a non-Google map**; photo/review author attributions; **not to be fed to an LLM** (only via Maps Grounding Lite API) | Google Maps logo; author attribution (name, avatar, link) for photos/reviews; links back to Google Maps | place_id storable indefinitely; lat/lng ≤30 days | `server/providers/places/google.js`, `server/providers/places.js` | **Core** | **HIGH** |
| **Google Street View Static API** | Fallback frame when a CCTV upstream has no image (also projected onto the in-scene camera plane) | JPEG | Server key | Only on CCTV fallback | Street View policies: https://developers.google.com/maps/documentation/streetview/policies | Yes, billed; **may not show Street View and non-Google maps "on the same screen"**; no modification | Google logo + "Report a problem" link; label clearly as Street View | GMP quotas | `server/providers/cctv.js` (≈ line 85–100, 500–530) | **Core**, behind a flag (disabled by default unless the active basemap is Google) | **HIGH** |
| **Cesium ion** (Google 3D via ion, Cesium World Terrain, ion geocoder) | Alternative 3D route; terrain; ion stacks | 3D Tiles, quantized-mesh, JSON | `CESIUM_ION_TOKEN` (browser, `assets:read`, URL-restricted) | Streamed | https://cesium.com/legal/terms-of-service · https://cesium.com/platform/cesium-ion/pricing · https://cesium.com/learn/ion/content-usage-and-attribution-guide · https://cesium.com/legal/terms-for-google | Community = "Personal and non-commercial use". Commercial/Premium plans = use "within your organization". **Integrating ion into a solution sold to other organizations needs a Cesium agreement.** Google terms flow down. No storing ion output offline | **"Cesium ion" logo "prominently displayed on the main application window"** (CesiumJS adds it; never hide it) plus asset credits | Plan quotas; no export/caching of ion geocodes | `src/maps/google3d.js` (`createGoogleIonTileset`), `src/maps/terrain.js`, `src/maps/imagery.js` (`createIonImagery`) | **Client** (or drop ion entirely) | **HIGH** |
| **Bing Maps Aerial / Aerial-with-Labels via Cesium ion** (assets 2/3) | Map stacks "Bing Aerial", "Bing Labels" | Raster tiles | Via ion token | Streamed | https://cesium.com/legal/terms-for-bing | Ion licence required (above); display only; **"may not combine Bing Maps assets … with non-Bing maps or with non-Bing imagery, including Google Photorealistic 3D Tiles"**; service being retired (ion promised access "at least through September 2026"; current status unverified) | Bing/Microsoft logo and imagery credits (Cesium credit display) | Ion quotas | `src/maps/catalog.js` (`bing-aerial`, `bing-labels`), `src/maps/imagery.js` | Do not carry forward without a decision | **HIGH** |
| **Esri World Imagery** (classic keyless endpoint) | **Default** keyless satellite basemap; Nepal comparison shots | Raster tiles (ArcGIS MapServer) | None (keyless) | Streamed | Item `10df2279f9684e4a9f6a7f08febac2a9`: "licensed under the Esri Master License Agreement"; "not intended to be used to export tiles for offline". https://www.esri.com/en-us/legal/terms/full-master-agreement · https://location.arcgis.com/pricing | **No affirmative keyless commercial grant.** Use ArcGIS Location Platform (API key; free monthly tile allowance, then metered) | "Powered by Esri" (Esri linked to esri.com) + service copyright text, currently **"Source: Esri, Vantor, Earthstar Geographics, and the GIS User Community"**. Legacy hard-codes "Maxar"; read it from service metadata instead | Location Platform metering; no offline export | `src/maps/imagery.js` (`createEsriImagery`), `src/maps/defaultSources.js` | **Client** with an Esri key, or a different licensed imagery provider | **HIGH** |
| **OpenStreetMap standard tiles** (`tile.openstreetmap.org`) | Keyless fallback basemap (OSM stack; Esri failure fallback) | PNG raster tiles | None | Streamed | https://operations.osmfoundation.org/policies/tiles/ | Not banned, but "Commercial services … should be especially aware that access may be withdrawn at any point"; no bulk or offline use | "© OpenStreetMap contributors" linked to https://www.openstreetmap.org/copyright, visible on the map | Unique app User-Agent, valid Referer, honour caching (≥7 days); proxies "not recommended"; heavy use prohibited | `src/maps/imagery.js` (`createOsmImagery`) | **Client** against a commercial or self-hosted OSM tile service | **MEDIUM** |
| **OpenFreeMap** (`tiles.openfreemap.org`, OpenMapTiles schema) | Vector roads (traffic), military polygons, place geometry | Mapbox Vector Tiles | None | On demand; legacy caches decoded tiles | https://openfreemap.org (FAQ "Is commercial usage allowed? Yes."; ToS updated 2026-09-09) · https://openmaptiles.org | **Yes** | "OpenFreeMap © OpenMapTiles Data from OpenStreetMap" ("OpenFreeMap" optional; **"© OpenMapTiles" linked to openmaptiles.org and OSM credit required**) | No published limits; ToS bans automated data collection without permission; no SLA | `src/sources/openFreeMap.js`, `src/sources/vectorTiles.js`, `src/sources/militaryTileGeometry.js` | **Client**, or **Core** for server-side feature extraction (ODbL share-alike applies to extracted databases) | **LOW** |
| **Re:Earth Terrain / Mapterhorn** (`terrain.reearth.land`) | Terrain for keyless stacks; `/api/terrain/heights` ellipsoidal heights | Quantized-mesh; JSON heights | None | Mesh streamed; heights proxy disk TTL 30 days | https://terrain.reearth.land · https://mapterhorn.com (data CC BY 4.0; sources all open) | **Yes** (CC BY 4.0); "Best-effort uptime, no SLA"; "Not a production SaaS" | **"Re:Earth Terrain · Mapterhorn (CC BY 4.0)"** (+ "Protomaps · © OpenStreetMap contributors (ODbL)" if the water mask is used); EGM2008 (NGA) for the geoid | Rate limits may be added without notice | `src/maps/terrain.js`, `server/providers/terrain.js`, `src/data/terrainHeightsProxy.js` | **Client** for the mesh; **Core** for heights. Self-host Mapterhorn for production | **LOW** (licence) / MEDIUM (availability) |
| **NASA GIBS WMTS + Worldview Snapshots; NASA CMR** | Recent Imagery layer: HLS S30/L30 and VIIRS true colour; HLS granule catalogue | WMTS tiles (JPEG/PNG); CMR JSON | None (CORS `*`) | On demand per box; ≤6 tile requests in flight | https://www.earthdata.nasa.gov/engage/open-data-services-software-policies/data-use-guidance ; HLS: https://www.earthdata.nasa.gov/data/catalog/lpcloud-hlss30-2.0 ("openly shared, without restriction") | **Yes** (NASA data CC0/public domain; no implied NASA endorsement; no NASA insignia) | "We acknowledge the use of imagery provided by services from NASA's Global Imagery Browse Services (GIBS), part of NASA's Earth Science Data and Information System (ESDIS)." For HLS S30 (Copernicus Sentinel-2-derived), also recommend "Contains modified Copernicus Sentinel data [year]" | No bulk use; GIBS fair use | `src/layers/recentImagery/{source,model,catalog,rendering}.js` | **Client** for tiles; **Core** for CMR queries (caching) | **LOW** |
| **Mapillary** (vector tiles, Graph API, MapillaryJS viewer) | Street Level coverage, image cones, viewer | MVT; JSON; JPEG imagery | `MAPILLARY_CLIENT_TOKEN` (browser by design; server tile proxy) | Coverage tiles cached on disk 24 h; Graph on demand | https://www.mapillary.com/terms (effective 2024-02-15) · https://www.mapillary.com/developer/api-documentation | **With conditions:** commercial use limited to the scope in terms §12; imagery **CC BY-SA 4.0** (share-alike on adapted imagery); no re-identification or unblurring | **"© Mapillary contributors, CC BY-SA 4.0"** + per-image author credit and licence link | 50,000 tile requests/day; 60,000/min entity calls; 10,000/min search | `server/providers/mapillary{,/*}.js`, `src/layers/streetLevel/providers/mapillary/*` | **Client** (viewer, Graph) + **Core** (coverage-tile cache) | **MEDIUM** |

### 5.2 Search, geocoding, routing and OSM data services

| Provider | Purpose | Data format | API key / auth | Refresh used | Licence / terms | Commercial use | Attribution | Rate limits / usage policy | Legacy location | Target | Risk |
|---|---|---|---|---|---|---|---|---|---|---|---|
| **Photon (komoot public instance)** | Keyless place search (fallback when no Google key) | GeoJSON | None | Per search, browser-direct | https://photon.komoot.io ("please be fair - extensive usage will be throttled"); data ODbL | Unclear (no commercial clause, no SLA) — **self-host Photon** for a product | "Photon (komoot)" + © OpenStreetMap contributors | Fair use, throttled | `src/keylessGeocoder.js` | **Core** (self-hosted Photon) | **MEDIUM** |
| **Nominatim (OSMF public)** | `/api/geocode` place search; regional briefing place resolution | JSON | None; identifying UA/Referer | 1 request per 1.1 s queue; 5-min cache | https://operations.osmfoundation.org/policies/nominatim/ | Not for product scale: "an absolute maximum of 1 request per second" across all users; no autocomplete, systematic queries or resale; "Code generated by LLMs must adhere to all terms" | © OpenStreetMap contributors | 1 req/s absolute; UA required | `server/providers/regional/place.js`, `src/search/nominatim.js`, `src/sources/nominatim.js`, `src/nominatimGeocode.js` | **Core** (self-hosted Nominatim or commercial geocoder) | **MEDIUM** |
| **OSRM on FOSSGIS** (`routing.openstreetmap.de`) | Directions layer and voice routes (`/api/route`) | JSON (OSRM) | None; identifying UA | Per route; 10-min cache; ≥1 s between upstream calls; per-client 60/min | https://routing.openstreetmap.de/about.html · https://www.fossgis.de/arbeitsgruppen/osm-server/nutzungsbedingungen/ | **Only if "the use of the services does not constitute a substantial part of an online offering"**; "Websites with high traffic volumes are generally not permitted" | Data © OpenStreetMap contributors (ODbL) + link "fix the map" → https://www.openstreetmap.org/fixthemap | 1 req/s; no mass downloads | `server/providers/places/routes.js` | **Core** (self-hosted OSRM/Valhalla) | **MEDIUM** |
| **Overpass API** (operator-configured only) | Roads for traffic, military installations, annotations, ALPR fallback | JSON | None | 24 h memory / 7-day disk cache; military 5 min fresh, 1 h stale, 30-day disk | https://dev.overpass-api.de/overpass-doc/en/preface/commons.html · FOSSGIS terms (above) | Public instances: ~10,000 requests/day and <1 GB/day guidance; commercial users should self-host. **Legacy default uses none.** | © OpenStreetMap contributors | As above | `server/providers/overpass{,/*}.js`, `server/providers/military-installations{,/*}.js` | **Core** (self-hosted Overpass or planet extract) | **LOW** (default off) |
| **OpenStreetMap data (via the above, bundled extracts and fixtures)** | Roads, military areas, ALPR, places, outlines | Various | — | — | ODbL 1.0: https://opendatacommons.org/licenses/odbl/1-0/ · https://osmfoundation.org/wiki/Licence/Attribution_Guidelines | **Yes.** A rendered map is a Produced Work (notice only). A bundled or extracted **Derivative Database used publicly must be ODbL and offered in machine-readable form** (§4.6) | "© OpenStreetMap contributors" (or "© OpenStreetMap") linked to https://www.openstreetmap.org/copyright, visible on the map | — | many | Both | **LOW** |

### 5.3 Aviation, maritime and space

| Provider | Purpose | Data format | API key / auth | Refresh used | Licence / terms | Commercial use | Attribution | Rate limits / usage policy | Legacy location | Target | Risk |
|---|---|---|---|---|---|---|---|---|---|---|---|
| **OpenSky Network** (`/api/states/all`, `/api/tracks/all`) | Primary worldwide live flights; tracks | JSON | `OPENSKY_AUTH_MODE` (default `oauth`: `OPENSKY_CLIENT_ID/SECRET` via `auth.opensky-network.org`), basic, or anonymous | Client every 30 s; server cache 9 s; tracks 60 s; credit governor | https://opensky-network.org/about/terms-of-use (read via curl; WebFetch 403) | **No without a written licence:** "Any use by a for-profit or commercial entity requires written permission and a license"; "Operational use of the REST API in any live product … also requires a written license"; no passing data to third parties | Cite Schäfer, Strohmeier, Lenders, Martinovic, Wilhelm, "Bringing up OpenSky: A large-scale ADS-B sensor network for research", ACM/IEEE IPSN 2014 (+ optional "The OpenSky Network, https://opensky-network.org") | Anonymous 400 credits/day; standard user 4,000/day; feeder 8,000/day; licensed 14,400/hour | `server/providers/aircraft/opensky.js`, `server/providers/aircraft/tracks.js`, `src/layers/flights/*` | **Core** (only after licence) | **HIGH** |
| **adsb.lol** (`/v2/mil`, `/v2/lat/{lat}/lon/{lon}/dist/{r}`, `/data/traces/`) | Military flights; regional flight fallback; aircraft traces | JSON | None today ("In the future, you will require an API key which you can get by feeding") | `/v2/mil` client every 15 s, server cache 12 s; point fallback cache 12 s; traces on selection | https://adsb.lol/privacy-license/ · https://api.adsb.lol/api/openapi.json | **Yes with conditions:** data ODbL 1.0; "If you want to use the API for production purposes, please contact me"; provided as is, with an indemnity | ODbL credit, e.g. "adsb.lol (ODbL 1.0)" | Dynamic limits; `traces` path is undocumented (historical traces are published as ODbL GitHub releases) | `server/providers/aircraft/adsb-lol.js`, `server/providers/aircraft/opensky.js` (fallback), `server/providers/aircraft/tracks.js` | **Core** | **MEDIUM** |
| **adsbdb** (`api.adsbdb.com`) | Aircraft type/registration by ICAO hex; callsign → route | JSON | None | Once per key; 24 h disk cache `.gev-cache/adsbdb.json`; ≤4 in flight, ~5/s | https://github.com/mrjackwills/adsbdb (code MIT) · https://www.planebase.biz/terms21.php | **Code yes; data no without permission:** route data "may not be copied, published, or incorporated into other databases without the explicit permission of David J Taylor, Edinburgh"; PlaneBase: "express permission is required … to export data on a commercial basis" | adsbdb + PlaneBase + "route data … David Taylor, Edinburgh, and Jim Mason, Glasgow" (as in `dataCredits.js`) | 512 requests/60 s per IP (source code), longer block above 1,024 | `server/providers/aircraft/enrichment.js`, `server/providers/live.js`, `src/layers/flights/enrichment.js` | **Core** (after permission), or replace with an openly licensed registry (e.g. national aircraft registers) | **HIGH** |
| **AISStream.io** (`wss://stream.aisstream.io`) | Live global vessels | WebSocket JSON (AIS messages) | `AISSTREAM_API_KEY` (server only) | Persistent socket; server tick 15 s; client polls `/api/vessels` every 60 s; entries stale after 30 min | https://aisstream.io/documentation (no ToS published; `/terms` 404) | **Unclear — no written terms**; get written permission or use a commercial AIS provider | Courtesy "AISStream.io" | 3 subscribed connections per account, 3 per IP; ≤1 subscription update/s; ≤200 MMSI filters; "Direct browser connections are not permitted"; bandwidth limits on uncompressed connections from Sept 2026 | `server/providers/vessels/ais-live.js`, `server/providers/vessels/ais-store.js`, `src/data/aisStreamAdapter.js` | **Core** | **HIGH** |
| **CelesTrak** (`gp.php?GROUP=…&FORMAT=tle`) | Satellite TLEs (SGP4) | TLE text | None (identifying UA) | Server TTL 6 h with serve-stale; client refresh 5 min (served from cache) | https://celestrak.org/usage-policy.php (updated 2026-05-22) | **Unclear:** policy covers resource use only; no licence grant or citation clause. Source data originates from 18 SDS/Space-Track, whose redistribution terms were not verified | Courtesy "CelesTrak (celestrak.org), Dr. T.S. Kelso" | Each GP data set at most once per 2-hour update; >100 MB/day per IP may be firewalled; stop on non-200. **TLE format cannot carry catalogue numbers above 99999 (exhausted 2026-07-11) — move to OMM/CSV** | `server/providers/space/celestrak.js`, `src/layers/satellites/source.js`, `src/data/spaceProviderRequests.js` | **Core** | **MEDIUM** |
| **The Space Devs — Launch Library 2 v2.3** | Space missions (30 days): launches, payloads, recovery | JSON | Optional `LL2_API_TOKEN` (paid via Patreon tiers) | Server cache 15 min (memory + disk), serve-stale; client every 5 min | https://github.com/TheSpaceDevs/Tutorials/blob/main/faqs/faq_TSD.md · https://ll.thespacedevs.com/docs/ | **Yes:** "free to use the data in any way, shape, or form … Please refrain from forwarding it without adding value" | Encouraged, not mandatory: "Launch Library 2 — The Space Devs" | 15 calls/hour per IP anonymous; higher with a key; use a server cache | `server/providers/space/launch-library.js` | **Core** | **LOW** |
| **Local receivers** (WebUSB RTL-SDR; local dump1090 `aircraft.json`) | Local ADS-B and FM radio | Raw IQ/Mode-S; JSON | None (local devices) | 1 s | Radio received by the user; no third-party licence | Yes | None | `LOCAL_RECEIVER_FEEDS` restricted to loopback/private hosts | `server/providers/local-receivers.js`, `src/layers/localAdsb/*`, `src/sdr/*` | **Client** (WebUSB) + **Core** relay | **LOW** (check local radio-reception law per jurisdiction) |

### 5.4 Earth observation, weather and hazards

| Provider | Purpose | Data format | API key / auth | Refresh used | Licence / terms | Commercial use | Attribution | Rate limits / usage policy | Legacy location | Target | Risk |
|---|---|---|---|---|---|---|---|---|---|---|---|
| **USGS Earthquake Hazards** (`earthquake.usgs.gov/.../summary/all_day.geojson`) | Earthquakes | GeoJSON | None | Client every 60 s, browser-direct | https://earthquake.usgs.gov/earthquakes/feed/v1.0/geojson.php · https://www.usgs.gov/information-policies-and-instructions/copyrights-and-credits | **Yes** (U.S. public domain); no USGS logo or endorsement | Suggested "Credit: U.S. Geological Survey" (legacy uses "Data courtesy of the U.S. Geological Survey", also fine) | Feeds update each minute; no published limit | `src/layers/earthquakes/source.js` | **Core** (one fetch for all clients) | **LOW** |
| **NOAA GFS** (NOAA Open Data Dissemination, `noaa-gfs-bdp-pds.s3.amazonaws.com`) | Wind (10 m), optional 2 m temperature and MSLP | GRIB2 byte ranges (decoded with ecCodes WASM) | None | Hourly cache per cycle; client 1 h | https://registry.opendata.aws/noaa-gfs-bdp-pds/ ; https://www.weather.gov/disclaimer | **Yes** (public domain; "may be used without charge for any lawful purpose"); no implied endorsement; NWS name/logo need permission | Courtesy "NOAA Global Forecast System (GFS)" | AWS open data; none | `server/providers/wind/{gfs,catalog,decode,grid}.js` | **Core** | **LOW** |
| **ECMWF IFS Open Data** (`data.ecmwf.int/forecasts`) | Alternative wind model | GRIB2 byte ranges | None | Hourly | https://www.ecmwf.int/en/forecasts/datasets/open-data ("governed by the Creative Commons CC-BY-4.0 licence") + ECMWF Terms of Use https://apps.ecmwf.int/datasets/licences/general/ | **Yes** (CC BY 4.0) | **"This service is based on data and products of the European Centre for Medium-Range Weather Forecasts (ECMWF)"** + CC BY 4.0 link + modification notice + ECMWF liability disclaimer (legacy text in `dataCredits.js` `ecmwf-ifs` is complete) | Portal limited to 500 simultaneous connections; ECMWF recommends cloud mirrors (AWS/Azure/GCP) | `server/providers/wind/ifs.js` | **Core** (prefer a cloud mirror) | **LOW** |
| **NOAA nowCOAST** (radar MRMS, GOES IR, global IR, lightning density) | Observed weather layers | WMS 1.1.1 images | None | Metadata every 2 min (radar/sat) and 10 min (lightning); exact-time images cached ≤24 h | https://www.weather.gov/disclaimer · https://oceanservice.noaa.gov/disclaimer.html · lightning: https://ocean.weather.gov/lightning/lightning_pdd.php | **Yes** (public domain). Lightning density is a NOAA Level-5 derived product "acceptable for distribution to the public" — **do not redistribute raw Vaisala detections** | "NOAA nowCOAST · NWS/OAR MRMS radar; NESDIS GOES and global satellite partners"; lightning "NOAA/NWS nowCOAST; derived from Vaisala NLDN/GLD360" | NWS asks for limited request rates; abusive traffic blocked | `server/providers/weather.js`, `src/layers/weather/*` | **Core** | **LOW** |
| **NOAA NHC / CPHC** (`CurrentStorms.json`; `mapservices.weather.noaa.gov` tropical MapServer) | Cyclone advisories, tracks, cones | JSON; ArcGIS REST JSON | None | 5-min cache; client 5 min | https://www.weather.gov/disclaimer | **Yes** (public domain; no endorsement) | "NOAA/NWS NHC / CPHC" | As NWS | `server/providers/cyclones.js`, `src/layers/cyclones/*` | **Core** | **LOW** |
| **NASA FIRMS** (VIIRS NOAA-20/21, S-NPP; MODIS NRT) | Active fires (24 h) | CSV | `FIRMS_MAP_KEY` (free, server only) | Server TTL 30 min; client 10 min | https://firms.modaps.eosdis.nasa.gov/api/map_key/ ; NASA data-use guidance (above) | **Yes** (NASA CC0/public domain; no endorsement) | Requested: "We acknowledge the use of data and/or imagery from NASA's Fire Information for Resource Management System (FIRMS) (https://earthdata.nasa.gov/firms), part of NASA's Earth Observing System Data and Information System (EOSDIS)." | **5,000 transactions per 10-minute interval** per MAP_KEY (multi-day requests count as several) | `server/providers/firms.js`, `src/data/firmsCsv.js`, `src/layers/firms/*` | **Core** | **LOW** |
| **NIFC WFIGS Interagency Fire Perimeters** (`services3.arcgis.com`) | Fire perimeters, containment | ArcGIS feature JSON (paged) | None | 5-min server cache; client 5 min | https://data-nifc.opendata.arcgis.com/ (public domain per Data.gov/catalog mirrors; the hub's own licence field was not retrieved) | **Yes** (U.S. Government data) — verify the item licence field | "Wildfire perimeters: National Interagency Fire Center (WFIGS)" | Service refreshes every 5 min; warns against repeated relative-date queries | `server/providers/firePerimeters.js`, `src/layers/perimeters/*` | **Core** | **LOW** |
| **InciWeb** (`inciweb.wildfire.gov`) | Links to incident pages; catalogue matched by name | HTML/JSON metadata | None | Catalogue cached 1 h; publication metadata 30 min | U.S. Government public incident information | **Yes** (links; page content stays InciWeb's) | "Incident information: InciWeb (inciweb.wildfire.gov)" | — | `server/providers/firePerimeters.js`, `src/layers/perimeters/inciweb.js` | **Core** | **LOW** |
| **Open-Meteo** (`api.open-meteo.com/v1/forecast`) | Cockpit Local Info current conditions; dynamic weather effects | JSON | None (free endpoint) | 5-min cache per 0.1° cell; stale ≤30 min | https://open-meteo.com/en/terms · https://open-meteo.com/en/licence | **No on the free endpoint:** "You may only use the free API services for non-commercial purposes". **Paid plan (Standard/Professional/Enterprise) required** | Data CC BY 4.0; **"You must include a link next to any location Open-Meteo data are displayed"**: `<a href="https://open-meteo.com/">Weather data by Open-Meteo.com</a>` | Free tier: <10,000/day, 5,000/h, 600/min | `server/providers/regional/weather.js`, `server/providers/regional/weather-effects.js` | **Core** (paid customer endpoint and key, or replace) | **HIGH** |

### 5.5 News, traffic, mobility, radio, AI

| Provider | Purpose | Data format | API key / auth | Refresh used | Licence / terms | Commercial use | Attribution | Rate limits / usage policy | Legacy location | Target | Risk |
|---|---|---|---|---|---|---|---|---|---|---|---|
| **Google News RSS** (`news.google.com/rss/search`) | Primary location-matched cockpit headlines | RSS XML | None | 5-min cache per 0.1° cell; stale ≤1 h | https://www.google.com/intl/en_us/terms_google_news.html | **No:** "You may only display the content of the Service for your own personal use (i.e., non-commercial use)"; robots prohibited | n/a (publisher links keep publisher terms) | — | `server/providers/regional/news.js` | Replace (GDELT or licensed news API) | **HIGH** |
| **GDELT Project DOC 2.0** (`api.gdeltproject.org/api/v2/doc/doc`) | Fallback headlines | JSON | None | Same cache as above | https://www.gdeltproject.org/about.html#termsofuse | **Yes:** "unlimited and unrestricted use for any academic, commercial, or governmental use" | **Citation to the GDELT Project and a link to https://www.gdeltproject.org/** (required); article links keep publisher terms | Unpublished; keep cached | `server/providers/regional/news.js` | **Core** | **LOW** |
| **TomTom Traffic Flow** (`api.tomtom.com/traffic/map/4/tile/flow/...pbf`) | Live flow speeds and congestion (optional, BYOK) | Vector tiles (MVT) | `TOMTOM_API_KEY` (server only) | Tile TTL 120 s (disk `.gev-cache`); daily budget 6,000 tiles | https://docs.tomtom.com/legal/terms-and-conditions (page body did not render for verification) · https://docs.tomtom.com/pricing/ | **Unverified.** TomTom states freemium can be "commercially deployed", but caching limits and production terms could not be read. Confirm the plan in writing | "Traffic flow data © TomTom" when shown (logo requirements unverified) | Free allowance (legacy note: 200,000 tile requests/month) | `server/providers/traffic.js`, `src/layers/traffic/*`, `src/data/tomtomTiles.js` | **Core** | **MEDIUM** |
| **GBFS bikeshare** (Lyft `gbfs.lyft.com` NYC/Chicago/DC/SF; `gbfs.bcycle.com`; `*.publicbikesystem.net` Austin/Honolulu/Chattanooga; Bluebikes; Biketown; CoGo) | Bikeshare station availability | GBFS 2.x JSON | None | `station_status` every 60 s; `station_information` 5 min | Per operator. Lyft feeds carry no `license_url`. Lyft-run Divvy licence: https://divvybikes.com/data-license-agreement ("use the Data for any lawful purpose", "distribute in your product or service"; not "as a stand-alone dataset"; no trademarks; no implied endorsement) | **Likely yes (per-operator; unverified for most)** | Credit the operator; no use of system trademarks | Respect feed `ttl` | `server/providers/gbfs.js`, `src/layers/bikeshare/registry.js`, `src/data/gbfsSource.js` | **Core** | **MEDIUM** |
| **GTFS-Realtime transit** (7 feeds) | Live transit vehicles | Protobuf | Entur: `ET-Client-Name` header; others none | Client poll 15 s; MBTA 15-min in-memory history | MBTA: MassDOT Developers License Agreement (PDF 403, **unverified**) · CapMetro: https://www.capmetro.org/developertools (Open Data License Agreement: "use, reproduce, and redistribute"; commercial not addressed; no CMTA trademarks) · Metro Transit (Minneapolis): **no terms found** · OVapi: https://gtfs.ovapi.nl/README ("You are free to use this data … best-effort"; don't impersonate agencies) · Entur: NLOD (https://developer.entur.org) · TransLink SEQ: CC BY 4.0, no State endorsement · HSL: CC BY 4.0 (https://www.hsl.fi/en/hsl/open-data) | **Yes** for Entur/TransLink/HSL; **unverified** for MBTA/CapMetro/Metro Transit/OVapi | Per feed (`transitFeedCredit()`); CC BY/NLOD statements; no endorsement | Entur: strict limits without `ET-Client-Name` ("<company>-<application>"); OVapi: identify in User-Agent, cache when polling more than once per minute, gzip | `src/data/transitFeeds.js`, `src/data/transitProxy.js`, `server/providers/transit.js`, `server/providers/transitHistory.js` | **Core** | **MEDIUM** (mixed) |
| **Radio Browser** (`all.api.radio-browser.info` mirrors) | Internet-radio station directory; click counter | JSON | None | 45-min cache; stale ≤7 days; mirror list 6 h | https://api.radio-browser.info ("Data license: public domain") | **Yes** ("You may use it in free and non free software"); streams and favicons belong to the stations | Courtesy "Radio Browser" + link to the broadcaster | DNS-discover mirrors (no hard-coding), "speaking" User-Agent, report clicks | `server/providers/radio{,/*}.js`, `src/sources/radioBrowser.js` | **Core** (directory); **Client** plays streams directly (exposes listener IP) | **LOW** |
| **DeFlock ALPR extract** (`tiles.dontgetflocked.com/cameras-{us,ca}-hourly.json`) | Mapped ALPR camera locations (OSM `surveillance:type=ALPR`) | TileJSON + MVT (PMTiles on Cloudflare R2) | None | Upstream regenerated hourly; decoded-tile TTL 1 h | https://github.com/flockhopper3/deflock-data ("Anyone can use the data"; tooling MIT; data © OpenStreetMap contributors, ODbL) | **Yes** (ODbL), hobby endpoint with no SLA | **"© OpenStreetMap contributors"** (TileJSON carries no attribution; Lavender must add it) | "No rate limits beyond Cloudflare's defaults" | `src/layers/alpr/source.js` | **Core** (mirror/self-generate from OSM) | **LOW** (licence) / sensitive (§7) |
| **OpenAI API** (Realtime voice; Responses for HUD summary, `gpt-5-nano`; transcription `gpt-4o-mini-transcribe`) | Voice control, scene Q&A with viewport screenshots, HUD summary | WebRTC/WebSocket; JSON; `input_image` | `OPENAI_API_KEY` (server); browser gets ephemeral Realtime client secret | Realtime per session; HUD summary every 15 s while a tab is open; per-IP limit 30/min | OpenAI Services Agreement https://cdn.openai.com/osa/openai-services-agreement.pdf (v.010126) | **Yes with API key:** customer owns output; no training on customer content by default; no credential sharing or key transfer | None mandated | Account rate limits; `$5` hard cap in legacy (API-key mode) | `server/providers/openai{,/*}.js`, `src/voice/*` | **Core** | **MEDIUM** (data-flow issues §7) |
| **OpenAI via ChatGPT/Codex OAuth** (`~/.codex/auth.json`) | "USE CHATGPT OAUTH" voice mode | — | User's Codex CLI tokens read from disk | — | https://developers.openai.com (Sign in with ChatGPT: "currently available to selected commercial partners through a limited trial"; audio not supported) | **No** for a commercial or hosted product | — | — | `server/providers/openai/codex-auth.js`, `realtime.js` | Do not port | **HIGH** |
| **Google Fonts** (`fonts.googleapis.com`, `fonts.gstatic.com`) | Inter, JetBrains Mono, Material Symbols | CSS/WOFF2 | None | Page load | SIL OFL 1.1 / Apache-2.0 (https://github.com/google/fonts) | **Yes** | Licence text when self-hosting | — | `index.html`, CSP in `build/vite.js` | **Self-host** in the client bundle | **LOW** (GDPR: LG München I, 20.01.2022, 3 O 17493/20) |
| **YouTube / Facebook / X embeds** | Bhote Koshi witness posts | Embedded players/widgets | None | On scene | Platform embed terms; content stays with owners | Embedding via official players is permitted by the platforms; the content is not licensed to Lavender | Platform UI | Third-party cookies and scripts | `src/data/bhoteKoshiEmbeddedMedia.js`, CSP in `build/vite.js` | Consent-gated client embeds (only if the Nepal scene ships) | **MEDIUM** (privacy) |

### 5.6 Public camera (CCTV) feeds

All camera packs share one implementation:
- Catalogue loaders: `server/providers/cctv/sources.js`, `catalog.js`, `constants.js`.
- Frame proxy: `media.js`, with a server-registered URL allow-list.
- HLS relay: `stream.js`.
- Routes and Street View fallback: `server/providers/cctv.js`.
- Client: `src/layers/cctv/*`.

Catalogues are cached 15 min. Frames refresh every 10 s for the active camera and 60 s when idle; Fintraffic ambient frames refresh every 600 s. No frame is stored. **Target in Lavender for all: Core** (catalogue + allow-listed frame proxy, no storage), with per-jurisdiction kill switches (the legacy `CCTV_*_ENABLED` flags).

| Provider | Feed | Auth | Licence / terms | Commercial use | Attribution | Usage policy | Risk |
|---|---|---|---|---|---|---|---|
| **City of Austin** | Catalogue `data.austintexas.gov` (Socrata) + JPEG frames `cctv.austinmobility.io/image/{id}.jpg` | None | City of Austin Open Data Terms of Use (https://data.austintexas.gov/stories/s/ranj-cccq). Catalog mirrors show "No license information" for the dataset; frame terms not found (**unverified**) | **Unknown** | "City of Austin, TX — data.austintexas.gov" | — | **MEDIUM** |
| **TxDOT ITS** | `its.txdot.gov` district catalogues + base64 JPEG snapshots | None | No terms found (**unverified**) | **Unknown** | "Texas Department of Transportation" (courtesy) | Default districts AUS, SAT | **MEDIUM** |
| **Caltrans CWWP2** | `cwwp2.dot.ca.gov` CCTV JSON + stills | None | "Commercial Wholesale Web Portal" for third-party developers. "Please read the Conditions Of Use" — page unreachable from this audit (**unverified**). A 1997 Caltrans/CHP policy bars broadcasting personal identifiers visible in video | **Likely yes, conditions unverified** | "Caltrans — cwwp2.dot.ca.gov" | Districts 4, 7, 11, 3 by default | **MEDIUM** |
| **DelDOT** | `tmc.deldot.gov/json/videocamera.json` + live HLS `video.deldot.gov` relayed | None | No terms found; upstream: "deployment beyond personal local viewing needs confirmation" | **Unknown — ask DelDOT** (live video relay) | "DelDOT — Delaware Department of Transportation" | Bounded in-memory relay | **HIGH** |
| **TfL JamCams** | `api.tfl.gov.uk` catalogue + S3 frames | Optional app key | TfL Transport Data Service licence (OGL 2.0-based): https://tfl.gov.uk/corporate/terms-and-conditions/transport-data-service (403 to fetch; via search excerpts) | **Yes** ("Exploit the Information commercially and non-commercially") | **"Powered by TfL Open Data"** + OS/Geomni map-data credits where included (e.g. "Contains OS data © Crown copyright and database rights"; verify current year text); no TfL logos or implied endorsement | Registration/app key recommended for rate | **LOW** |
| **Ontario 511** | `511on.ca/api/v2/get/cameras` + stills | None | https://511on.ca/developers/resources — use of developer content = acceptance of the Open Government Licence – Ontario | **Yes** | "Contains information licensed under the Open Government Licence – Ontario" + "Ontario 511"; the 511 logo is an AASHTO-derived trademark | **10 calls per 60 s** | **LOW** |
| **Fintraffic / Digitraffic weathercams** | `tie.digitraffic.fi` stations + `weathercam.digitraffic.fi` stills | `Digitraffic-User` header | CC BY 4.0: https://www.digitraffic.fi/en/terms-of-service/ (images assumed in scope; not stated explicitly) | **Yes** | **"Source: Fintraffic / digitraffic.fi, license CC 4.0 BY"** + licence link + modification note | Identify with `Digitraffic-User`; gzip | **LOW** |
| **DriveBC** | `www.drivebc.ca/api/webcams/` + `/images/{id}.jpg` | None | DataBC "BC HighwayCams": OGL-BC. **But** the Ministry webcam developer page: images "may not be rebroadcast or republished without express authorization of the Ministry"; licence agreement available. Partner cameras (TransLink, Vancouver, Surrey, Parks Canada) carry their own credits | **Permission recommended** | "DriveBC. Contains information licensed under the Open Government Licence – British Columbia" + per-camera partner credit | 250 nearest Vancouver/Victoria by default | **MEDIUM** |
| **Open Calgary** | Socrata `k7p9-kppz` + `trafficcam.calgary.ca` stills | None | Open Government Licence – City of Calgary (terms page did not render; upstream text) | **Yes** (OGL family; excludes personal information and third-party rights) | **"Contains information licensed under the Open Government Licence – City of Calgary"** | — | **LOW** |
| **Statens vegvesen** | OGC API Features DATEX CCTV + `kamera.atlas.vegvesen.no` stills + `kamera.vegvesen.no` HLS | None | NLOD 2.0 (dataset "Webkamera" on https://dataut.vegvesen.no lists NLOD) | **Yes** | **"Contains data under the Norwegian licence for Open Government data (NLOD) distributed by Statens vegvesen"** | DATEX registration normally required (legacy uses the OGC view) | **LOW** |
| **City of Tallinn** | Curated `config/cctv_sources.tallinn.json` + `ristmikud.tallinn.ee/last/camNNN.jpg` | None | **No terms or licence found** | **Unknown** | "City of Tallinn — ristmikud.tallinn.ee" (courtesy) | — | **MEDIUM** (EU GDPR) |
| **Transpordiamet / Tarktee** | DATEX2 `roadCameraLocations` / `roadCameraImages` | None | Smart Road open data; **licence not found** | **Unknown** | "Transpordiamet / Tarktee" (courtesy) | Images refresh ~10 min | **MEDIUM** |
| **Stadt Warendorf** | `config/cctv_sources.warendorf.json`, `http://webcam.warendorf.de/image/jpeg.cgi` (plain HTTP) | None | **No terms found**; German KUG/DSGVO apply to people visible on a market square | **Unknown** | "Stadt Warendorf" (courtesy); OSM credit for the derived pose | — | **MEDIUM** |
| **Live Traffic NSW** | `data.livetraffic.com/cameras/traffic-cam.json` + `webcams.transport.nsw.gov.au` stills | None (the official Open Data Hub API normally uses a key) | Transport for NSW Open Data "Live Traffic Cameras": Creative Commons Attribution (portal label; 3.0 AU vs 4.0 inconsistent across mirrors) | **Yes via the official Open Data Hub**; current access path is a website feed with **browser-UA spoofing** for the image host | "Live Traffic NSW — Transport for NSW" + CC BY | Use the Open Data Hub API and key | **MEDIUM** |
| **Google Street View fallback** | See §5.1 | Server key | GMP | Restricted | Google logo + "Report a problem" | — | **HIGH** |
| **Pilot pack (Shinjuku)** | `config/cctv_sources.shinjuku.json` → `storage.googleapis.com/gtv-videos-bucket/sample/ForBigger*.mp4` | None | Google Chromecast promo clips; no licence found | **Unknown — not licensed** | — | Not loaded unless configured | **MEDIUM** |

## 6. Attribution the Lavender UI must display

**How the legacy app shows credits.**
- Required Google/Cesium credits go on the on-globe credit line (`#cesium-credits`).
- Every other credit goes into a collapsed "Data attribution" popover (`src/data/dataCredits.js`, `showOnScreen=false`).
- Conditional credits (TomTom, Natural Earth, Census, transit feeds, Bhote Koshi) are registered when their source activates.

Lavender should keep this model with three changes:
1. Credits that providers require to be **visible on the map** must be on screen whenever that provider's content is visible — never only in a popover. This holds in clean-view and recording modes too.
2. Credits should be driven by **what is currently displayed**, not registered permanently.
3. Lavender needs an in-app **"Licences & credits" screen**: open-source notices (§2–3), model credits (B12), and dataset licences (§4).

### 6.1 On-screen, always visible while the content is displayed (provider-mandated)

| Provider | Exact wording / element | Source of requirement |
|---|---|---|
| Google Photorealistic 3D Tiles | **Google Maps logo** (16–19 dp high) + the **data attributions returned by the tiles, aggregated, sorted and shown in one line** on screen (`showCreditsOnScreen: true`); must not overlap other logos; in mixed scenes the UI must make clear which content is Google's | GMP ToS §3.2.2; Map Tiles policies (updated 2026-10-08) |
| Google Places / Geocoding / Street View | Google Maps logo with the content; Places photo/review **author name, avatar and link**; Street View **"Report a problem"** link | GMP Places / Street View policies |
| Cesium ion assets | **"Cesium ion" logo**, "prominently displayed on the main application window" (CesiumJS default credit) + per-asset credits | Cesium ion ToS / content usage guide |
| Bing imagery via ion | Bing/Microsoft logo and imagery copyright lines (supplied by the Cesium credit) | Cesium terms for Bing |
| Esri World Imagery | **"Powered by Esri"** (the word Esri linked to https://www.esri.com) + **"Source: Esri, Vantor, Earthstar Geographics, and the GIS User Community"** (take the live `copyrightText` from service metadata) | Esri attribution guidance |
| OpenStreetMap (tiles, vector tiles, routing, ALPR, bundled OSM data) | **"© OpenStreetMap contributors"** (or "© OpenStreetMap") linked to **https://www.openstreetmap.org/copyright** | OSMF Attribution Guidelines; ODbL §4.3 |
| OpenFreeMap / OpenMapTiles | **"OpenFreeMap © OpenMapTiles Data from OpenStreetMap"** — at minimum "© OpenMapTiles" linked to https://openmaptiles.org/ | OpenFreeMap FAQ; OpenMapTiles licence |
| FOSSGIS OSRM routes (if still used) | OSM credit + **"fix the map"** link → https://www.openstreetmap.org/fixthemap | FOSSGIS usage policy |
| Open-Meteo (if retained under a paid plan) | **`<a href="https://open-meteo.com/">Weather data by Open-Meteo.com</a>`** next to the displayed values | Open-Meteo licence ("next to any location Open-Meteo data are displayed") |
| TomTom traffic flow | **"Traffic flow data © TomTom"** (plus any logo the TomTom plan terms require — unverified) | TomTom terms |
| Mapillary | **"© Mapillary contributors, CC BY-SA 4.0"** + the image author and licence link in the viewer | Mapillary terms; CC BY-SA 4.0 |
| Re:Earth / Mapterhorn terrain | **"Re:Earth Terrain · Mapterhorn (CC BY 4.0)"** | Re:Earth Terrain page |

### 6.2 In the "Data attribution" panel, shown while the source is active

| Provider | Wording |
|---|---|
| ECMWF IFS | "This service is based on data and products of the European Centre for Medium-Range Weather Forecasts (ECMWF)." + CC BY 4.0 link + "Modified: resampled 10 m IFS forecast vectors and animated display." + "ECMWF does not accept any liability whatsoever for any error or omission in the data, their availability, or for any loss or damage arising from their use." |
| GDELT | "GDELT Project" with a link to https://www.gdeltproject.org/ (citation + link required) |
| NASA GIBS | "We acknowledge the use of imagery provided by services from NASA's Global Imagery Browse Services (GIBS), part of NASA's Earth Science Data and Information System (ESDIS)." |
| NASA HLS (Sentinel-2-derived S30) | Recommended: "Contains modified Copernicus Sentinel data [year]" + HLS DOI 10.5067/HLS/HLSS30.002 |
| NASA FIRMS | "We acknowledge the use of data and/or imagery from NASA's Fire Information for Resource Management System (FIRMS) (https://earthdata.nasa.gov/firms), part of NASA's Earth Observing System Data and Information System (EOSDIS)." |
| USGS | "Credit: U.S. Geological Survey" (or "Data courtesy of the U.S. Geological Survey") |
| NOAA / NWS | "NOAA nowCOAST · NWS/OAR MRMS radar; NESDIS GOES and global satellite partners"; "NOAA/NWS nowCOAST; derived from Vaisala NLDN/GLD360"; "NOAA/NWS NHC / CPHC"; "NOAA Global Forecast System (GFS)" — plus a no-endorsement statement; no NOAA/NWS logos |
| NIFC / InciWeb | "Wildfire perimeters: National Interagency Fire Center (WFIGS)"; "Incident information: InciWeb (inciweb.wildfire.gov)" |
| adsb.lol | "adsb.lol (ODbL 1.0)" |
| adsbdb (only if permitted) | "adsbdb — aircraft data from PlaneBase · route data is the work of David Taylor, Edinburgh, and Jim Mason, Glasgow …" (as legacy) |
| OpenSky (only if licensed) | Schäfer et al., "Bringing up OpenSky: A large-scale ADS-B sensor network for research", IPSN 2014 · https://opensky-network.org |
| AISStream (only if permitted) | "AISStream.io" |
| CelesTrak | "CelesTrak (celestrak.org), Dr. T.S. Kelso" |
| Launch Library 2 | "Launch Library 2 — The Space Devs" |
| Radio Browser | "Radio Browser" + link to the selected broadcaster |
| Overture (military names) | "© OpenStreetMap contributors, Overture Maps Foundation" |
| Open Infrastructure Map (dams) | "Open Infrastructure Map" + OSM credit |
| Natural Earth / U.S. Census / DataSF | Courtesy: "Made with Natural Earth"; "U.S. Census Bureau"; "City & County of San Francisco — DataSF" |
| TfL | **"Powered by TfL Open Data"** + "Contains OS data © Crown copyright and database rights" (+ Geomni line where applicable; confirm current text) |
| Ontario 511 | "Ontario 511 — Contains information licensed under the Open Government Licence – Ontario" |
| Fintraffic | **"Source: Fintraffic / digitraffic.fi, license CC 4.0 BY"** |
| DriveBC | "DriveBC. Contains information licensed under the Open Government Licence – British Columbia" + per-camera partner credit |
| Open Calgary | "Contains information licensed under the Open Government Licence – City of Calgary" |
| Statens vegvesen | "Contains data under the Norwegian licence for Open Government data (NLOD) distributed by Statens vegvesen" |
| Live Traffic NSW | "Live Traffic NSW — Transport for NSW" + CC BY licence link |
| City of Austin / TxDOT / Caltrans / DelDOT / Tallinn / Tarktee / Warendorf | Courtesy credits as in `dataCredits.js` until the terms are confirmed |
| Transit feeds | Per feed: "TransLink — Queensland Government (CC BY 4.0)" (and no statement implying State endorsement); "HSL (Helsinki Region Transport), CC BY 4.0"; "Entur — data under NLOD"; MBTA / CapMetro / Metro Transit / OVapi courtesy credits |
| GBFS | Operator name (e.g. "Austin BCycle") + its `license_url` where published; no operator trademarks used as branding |
| Bundled 3D models (B12) | Per model: "“<title>” by <author> (<Sketchfab URL>), licensed under CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/); modified for Lavender" — in the Licences screen, and reachable from the model when shown |
| TeleGeography / Vantor / GeoPera (only in builds that include them) | "© TeleGeography — submarinecablemap.com"; "Vantor Open Data Program … CC BY-NC 4.0"; "GeoPera … CC BY-NC 4.0" |

### 6.3 Legal text outside the map

- **Product terms of service and privacy policy.** They must reference the **Google Maps/Google Earth Additional Terms of Service** and the **Google Privacy Policy** (GMP ToS §3.2.2(a)).
- **Open-source notices** for distributed builds:
  - the MIT notice (§2.1);
  - the skylight MIT notice (§2.2);
  - Apache-2.0 licence text, plus Cesium `LICENSE.md` and the eccodes-wasm third-party list;
  - BSD/ISC/MIT notices of all runtime packages (generate them from the SBOM).
- **ODbL offer.** For each OSM-derived database Lavender publishes or embeds (B5–B7, any Overpass extracts), state that it is available under ODbL 1.0 and how to obtain it (§4.6 of ODbL).

## 7. Privacy- and ethics-sensitive sources

| Source | What is sensitive | Legacy safeguards (keep) | Recommended additional safeguards for Lavender |
|---|---|---|---|
| **Public CCTV / webcams** (14 jurisdictions) | Frames can show identifiable people, faces, vehicles and licence plates. EU/UK/EEA jurisdictions (Estonia, Finland, Norway, Germany, UK) bring GDPR/UK GDPR. Germany adds KUG (right to one's image) for the Warendorf market square. Caltrans/CHP policy bars broadcasting visible personal identifiers. | Frames fetched live and never written to disk; no face, plate or object detection; server-registered URL allow-list (no client-supplied URLs); indoor Kreis Warendorf cameras excluded; per-pack kill switches; "people are not a query type" policy (README) | Do a DPIA before EU/UK launch. Keep a per-jurisdiction enable list, default-off where terms are unknown (§1.5). Add an in-product notice that frames are third-party and live. Consider server-side face/plate blurring for EU/UK packs, or stills-only at reduced resolution. **Never send camera frames to an LLM by default** (legacy voice screenshots include on-screen camera planes) — make it opt-in per request and strip camera planes from the capture. No recording, export or timelapse features. Rate-limit frame access per user. Label Street View fallback frames clearly so they are not mistaken for live camera images. Exclude cameras near sensitive sites (schools, clinics, shelters, places of worship) on request. Write an abuse/takedown process. |
| **ALPR camera locations** (OSM via DeFlock) | Surveillance-infrastructure mapping; politically charged; potential misuse (evasion, vandalism) versus public-interest transparency | Shows mapped locations and tags only; no plate records; OSM edit dates not presented as verification; viewport-bounded reads; caps | Keep it read-only and neutral, with "community-mapped, may be incomplete or wrong" wording. Never join it with vehicle or plate data or movement inference. Keep OSM attribution. Get a legal review per market, because some jurisdictions treat mapping of enforcement cameras differently (e.g. speed-camera warning restrictions in DE/FR/CH). |
| **Military installations and military aircraft** (OSM/Overture names B7, Overpass installations, adsb.lol `/v2/mil`, MQ-9 model, "military" layers) | National-security sensitivity. Some countries criminalise mapping or photographing military sites. Live display of military flights. Possible export-control questions for a product marketed as "geospatial intelligence" | Data is public OSM/ADS-B; README: "Mapped data does not establish current activity or complete coverage" | Get export-control/sanctions classification advice (EAR/ITAR, EU dual-use; Q21). Use market-specific geofencing/disable switches. Add an acceptable-use policy that forbids targeting and harassment. Never infer "activity" from mapped polygons. Do not market as targeting-capable. |
| **Private aircraft** (OpenSky/adsb.lol flights, adsbdb registration lookup, cockpit tracking) | Business and private jets can be tied to identifiable owners, so tracking them is tracking individuals. This conflicts with the upstream line "no tracking individuals". | adsbdb lookups limited to the selected aircraft | Honour the FAA **LADD/PIA** lists and an owner opt-out/blocklist. Suppress registration/owner enrichment for non-commercial (general aviation) aircraft by default. Retain no per-aircraft history beyond the session. |
| **AIS vessels** | MMSI and vessel names of small private craft (yachts) can be personal data in the EU | Server cache only; no archive | Same opt-out/blocklist approach; no historical track storage without a lawful basis; limit identity display for class-B (small craft) transponders. |
| **Voice / OpenAI** | User voice audio, transcripts, viewport screenshots (which can contain Google content and CCTV frames of people), Places context sent to OpenAI. Legacy debug log `.gev-logs/realtime-conversations.jsonl` (rotated at 32 MB, never expired; content omitted unless `GEV_VOICE_LOG_CONTENT=1`) | Ephemeral client secrets; content logging off by default; spend caps | Get explicit consent for the microphone and for sending imagery. Sign a DPA with OpenAI (enterprise/zero-retention options). Set a retention period for logs. Do not include Google Maps Content in prompts or images (GMP). Remove the Codex OAuth path. |
| **Geocoding and search queries** | Users' searched places sent to Google / Photon / Nominatim / OSRM | Server-side proxying for most; Photon is browser-direct | Proxy every search through the core (IP not exposed to third parties), self-host OSM services, and disclose processors in the privacy policy. |
| **Radio playback** | Direct playback exposes the listener's IP address to the broadcaster | Documented | Disclose it, or offer proxied playback (bandwidth cost and broadcaster terms permitting). |
| **Third-party embeds and fonts** (YouTube, Facebook SDK, X widgets in the Nepal scene; Google Fonts) | Third-party cookies and IP transfer without consent (GDPR; LG München I 3 O 17493/20 for Google Fonts) | CSP restricts hosts | Self-host fonts. Put social embeds behind a consent click (two-click embeds), or link out instead of embedding. |
| **Disaster event pack** (Bhote Koshi) | Witness posts by private individuals; casualty and hazard implications | "Not an official hazard model"; no clips bundled; links only | If ever shipped: keep the interpretive caveats, link rather than embed, and provide a takedown path for witness-post owners. |
| **Local receivers (SDR)** | Reception or decoding of some radio traffic is regulated in some countries | Data stays in the browser/LAN | Show a jurisdictional notice in the SDR feature. |

## 8. Open questions requiring a human or legal decision

| # | Question | Context / options |
|---|---|---|
| Q1 | **TeleGeography cables (B1):** licence, non-commercial edition, or build-exclusion flag? | The upstream "CC BY-NC-SA 3.0" claim could not be verified. TeleGeography now says the underlying data is "restricted to paying subscribers". Even non-commercial redistribution of the 2026-05-24 API v3 snapshot may lack a licence. Options: TeleGeography geocoded map-data licence (https://www2.telegeography.com/license-geocoded-map-data); OSM cables (ODbL). Files stay in the repo either way. |
| Q2 | **Bhote Koshi pack (B2–B4):** seek Vantor/Planet/GeoPera permission, ship only in a non-commercial edition, or exclude the Nepal scene from commercial builds? | CC BY-NC 4.0. Exclusion must cover the event pack, the six `bhoteKoshi*.js` modules and the `get_bhote_koshi_flood` tool registration. |
| Q3 | **CCTV ground heights (B11):** confirm with counsel that the Google-derived JSON may not ship; choose the replacement source (Re:Earth/Mapterhorn, Copernicus DEM GLO-30, OSM building heights) | Google ToS §3.2.3(a) and (c) and the Map Tiles policy; the file is unlabelled upstream. |
| Q4 | **Google Maps Platform architecture.** (a) Billing entity in or out of the EEA (the terms differ). (b) Is Google 3D the only basemap whenever Google content is on screen, or does Lavender drop Google content when users switch to Esri/OSM? (c) Remove or redesign the Street View fallback. (d) Stop sending Google content and screenshots to OpenAI. (e) Use only the Google geocoder alongside Google 3D Tiles. | Commercial use is allowed only inside these constraints. |
| Q5 | **Cesium ion:** negotiate an integration/OEM agreement, or run without ion (CesiumJS + Google direct + Re:Earth/self-hosted terrain + licensed imagery)? Drop Bing stacks (retiring; no mixing allowed)? | Community plan is non-commercial; Commercial plans cover internal use only. |
| Q6 | **Default satellite basemap:** ArcGIS Location Platform account (metered) for Esri imagery, or another licensed imagery provider? | The keyless Esri endpoint has no commercial grant. Note: EOX Sentinel-2 cloudless years after 2016 are CC BY-NC-SA, so do not substitute them without checking the year's licence. |
| Q7 | **Flight data:** buy an OpenSky licence; make adsb.lol primary (ODbL; contact the operator for production; a future key may require feeding); or contract a commercial ADS-B provider? | OpenSky forbids for-profit and operational live-product use without a written licence. |
| Q8 | **AIS:** written permission from AISStream, or a commercial AIS feed? | No published terms. |
| Q9 | **Aircraft enrichment:** request permission from David J Taylor (routes) and PlaneBase (aircraft data), or replace with openly licensed registries? | adsbdb data is not openly licensed. |
| Q10 | **Weather for cockpit/effects:** Open-Meteo paid plan, or switch to NOAA/NWS (US, public domain) / MET Norway (CC BY 4.0, commercial allowed, User-Agent required)? | The free Open-Meteo endpoint is non-commercial. |
| Q11 | **Headlines:** drop Google News RSS (personal, non-commercial only) and keep GDELT, or license a news API? | — |
| Q12 | **Branding and media:** confirm Lavender uses no GEV/Halfpixel names, logos or README media; ask Bilawal Sidhu if any media reuse is wanted. | MIT grants no trademark rights; `docs/media` reuse needs permission. |
| Q13 | **Cesium moon texture** (© James Hastings-Trew): accept as distributed by Cesium, disable `scene.moon`, or replace with a public-domain NASA texture? | JHT terms require permission to package maps into distributed software. |
| Q14 | **skylight attribution:** copy the exact copyright line from `github.com/cpaczek/skylight/blob/main/LICENSE` into Lavender's notices. | Eight GEV files adapt skylight code (§2.2); the upstream notices omit it. |
| Q15 | **Unprovenanced assets:** voice WAV fixture (check embedded generator terms), `street-640x320.jpg`, ALPR marker PNGs, TomTom tile fixture. | Test-only or presumed-original; confirm before any redistribution of the repository. |
| Q16 | **CCTV jurisdictions with unknown or restrictive terms:** obtain written confirmation from City of Austin, TxDOT, Caltrans (CWWP2 Conditions of Use), DelDOT (live video), City of Tallinn, Transpordiamet, Stadt Warendorf, and a DriveBC licence agreement. Move NSW to the official Open Data Hub API with a key and stop UA spoofing. Respect the Ontario 511 limit of 10 calls/min. | Ship packs default-off until confirmed. |
| Q17 | **Transit and bikeshare:** review the MassDOT Developers License Agreement PDF and the CapMetro, Metro Transit, OVapi and per-operator GBFS terms (Lyft/Divvy: no stand-alone redistribution, no trademarks). | Mostly permissive in practice; commercial clauses unverified. |
| Q18 | **OSM community services:** self-host (Nominatim, Photon, OSRM/Valhalla, Overpass, raster or vector tiles) or contract commercial OSM-based providers? How does Lavender publish ODbL derivative databases (offer mechanism)? | Public instances' policies do not fit a product. |
| Q19 | **TomTom:** confirm in writing that the chosen plan permits production commercial use, the 120 s disk cache, and the attribution/logo form. | Terms page body could not be verified. |
| Q20 | **OpenAI:** remove the ChatGPT/Codex OAuth path; sign a DPA; decide voice-log retention; consent model for images and audio. | Codex tokens are not a permitted route for third-party products. |
| Q21 | **Export control, sanctions and acceptable use** for a commercial "geospatial intelligence" product with military-installation and military-aircraft layers; LADD/PIA honouring; customer AUP and KYC. | Not a licensing question, but it gates sale. |
| Q22 | **CelesTrak:** ask Dr. T.S. Kelso about commercial use, or source GP data from Space-Track under its own user agreement (verify redistribution terms). Migrate from TLE to OMM/CSV (5-digit catalogue numbers were exhausted 2026-07-11). | No licence grant published. |
| Q23 | **Mapillary:** confirm Lavender's use fits Mapillary terms §12 (commercial scope). Decide whether adapted imagery (projection onto 3D) triggers CC BY-SA share-alike. | — |
| Q24 | **Hobby or no-SLA endpoints** (Re:Earth Terrain, DeFlock tiles, OpenFreeMap, Photon, adsb.lol): self-host or mirror for production reliability? | Licences permit it; availability is the risk. |
| Q25 | **Rust core licence policy:** choose the licence-allow-list (e.g. `cargo-deny` + npm license checker) and the SBOM/notice generation step for releases. | Prevents copyleft or non-commercial drift in new dependencies. |

---

### Sources consulted (all checked 2026-10-11)

- **Google:**
  - https://cloud.google.com/maps-platform/terms
  - https://cloud.google.com/maps-platform/terms/maps-service-terms
  - https://cloud.google.com/terms/maps-platform/eea
  - https://developers.google.com/maps/documentation/tile/policies
  - https://developers.google.com/maps/documentation/places/web-service/policies
  - https://developers.google.com/maps/documentation/streetview/policies
  - https://www.google.com/intl/en_us/terms_google_news.html
- **Cesium:**
  - https://github.com/CesiumGS/cesium/blob/main/LICENSE.md
  - https://cesium.com/legal/terms-of-service
  - https://cesium.com/platform/cesium-ion/pricing
  - https://cesium.com/learn/ion/content-usage-and-attribution-guide
- **Esri:**
  - https://services.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer?f=json
  - https://location.arcgis.com/pricing
- **OSM and OSM-based services:**
  - https://operations.osmfoundation.org/policies/tiles/
  - https://operations.osmfoundation.org/policies/nominatim/
  - https://osmfoundation.org/wiki/Licence/Attribution_Guidelines
  - https://opendatacommons.org/licenses/odbl/1-0/
  - https://www.fossgis.de/arbeitsgruppen/osm-server/nutzungsbedingungen/
  - https://openfreemap.org
  - https://photon.komoot.io
  - https://docs.overturemaps.org/attribution
- **Terrain and street-level imagery:**
  - https://terrain.reearth.land
  - https://mapterhorn.com
  - https://www.mapillary.com/terms
- **Aviation and AIS:**
  - https://opensky-network.org/about/terms-of-use
  - https://adsb.lol/privacy-license/
  - https://api.adsb.lol/api/openapi.json
  - https://github.com/mrjackwills/adsbdb
  - https://www.planebase.biz/terms21.php
  - https://aisstream.io/documentation
- **Space:**
  - https://celestrak.org/usage-policy.php
  - https://github.com/TheSpaceDevs/Tutorials/blob/main/faqs/faq_TSD.md
- **Earthquakes, weather and fire:**
  - https://www.usgs.gov/information-policies-and-instructions/copyrights-and-credits
  - https://www.weather.gov/disclaimer
  - https://ocean.weather.gov/lightning/lightning_pdd.php
  - https://www.ecmwf.int/en/forecasts/datasets/open-data
  - https://firms.modaps.eosdis.nasa.gov/api/map_key/
  - https://www.earthdata.nasa.gov/engage/open-data-services-software-policies/data-use-guidance
  - https://www.earthdata.nasa.gov/data/catalog/lpcloud-hlss30-2.0
  - https://www.nasa.gov/nasa-brand-center/images-and-media/
  - https://open-meteo.com/en/terms
  - https://open-meteo.com/en/licence
- **News and traffic:**
  - https://www.gdeltproject.org/about.html
  - https://docs.tomtom.com/legal/terms-and-conditions
- **Bundled-data and asset owners:**
  - https://www2.telegeography.com/license-telegeography-map
  - https://registry.opendata.aws/vantor-open-data
  - https://github.com/geo-pera/bhotekoshi-2026-reconstruction
  - https://help.sketchfab.com (CC attribution guidance)
  - https://github.com/cpaczek/skylight
  - https://github.com/meri-imperiumi/eccodes-wasm
  - https://planetpixelemporium.com/planets.html
- **Radio, ALPR, AI:**
  - https://api.radio-browser.info
  - https://github.com/flockhopper3/deflock-data
  - https://cdn.openai.com/osa/openai-services-agreement.pdf
  - https://developers.openai.com (Sign in with ChatGPT)
- **CCTV:**
  - https://www.digitraffic.fi/en/terms-of-service/
  - https://511on.ca/developers/resources
  - https://dataut.vegvesen.no
  - https://opendata.transport.nsw.gov.au/dataset/live-traffic-cameras
  - https://tfl.gov.uk/corporate/terms-and-conditions/transport-data-service
  - DriveBC webcam developer notice (legacy.drivebc.ca)
- **Transit and bikeshare:**
  - https://divvybikes.com/data-license-agreement
  - https://www.capmetro.org/developertools
  - https://gtfs.ovapi.nl/README
  - https://developer.entur.org/pages-intro-authentication
