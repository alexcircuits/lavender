# Fire perimeter fixtures (ENG-956)

Recorded NIFC WFIGS and InciWeb documents for `providers/perimeters`, plus synthetic
malformed cases. Automated tests serve them from disk or Wiremock; no test contacts a
live service. The same recorded files are copied to `apps/web/e2e/fixtures/` for fixture
mode (`providers/fixture.rs`).

## NIFC WFIGS (recorded)

| File | Source and time | Changes |
|---|---|---|
| `perimeters-wfigs.geojson` | `GET https://services3.arcgis.com/T4QMspbfLg3qTGWY/arcgis/rest/services/WFIGS_Interagency_Perimeters_Current/FeatureServer/0/query` with exactly the provider's query (`where=1=1`, the 15 card `outFields`, `maxAllowableOffset=0.0002`, `geometryPrecision=5`, `outSR=4326`, `orderByFields=OBJECTID`, `resultOffset=0`, `resultRecordCount=1000`, `f=geojson`). HTTP 200, `Date: Thu, 15 Oct 2026 20:47:31 GMT`, `Last-Modified: Thu, 15 Oct 2026 20:42:11 GMT`. | The service returned 112 features (1.85 MB). Trimmed to 7 in their original order: `2026-COGMF-000099` Gold Mountain, `2026-ORPRD-000446` Camel Hump (Rowe Creek Complex, two holes), `2026-WANES-001791` Sinlahekin, `2026-MTBDF-266319` Sand Creek (434 parts), `2026-AKTAS-613487` Mukluk (Alaska), `2026-GACHP-000103` a prescribed fire with null attributes, `2026-CACDD-011826` ORANGE2 (0.01 acres, a four-position ring). Values unchanged; JSON re-serialised without whitespace. |
| `perimeters-wfigs-empty.geojson` | Same layer and parameters with `where=1=0`, HTTP 200, 2026-10-15T21:03:46Z. | None. |
| `perimeters-wfigs-error.json` | Same layer with an invalid `outFields`, 2026-10-15T21:03:46Z. ArcGIS answered **HTTP 200** with this error body, which is why error bodies are rejected by shape, not status. | None. |

Layer facts checked the same day (`…/FeatureServer/0?f=json`): `maxRecordCount` 2000,
`hasZ` false, pagination and `orderByFields` supported, `cacheMaxAge` 300. The item
description states "Data are refreshed every 5 minutes", the basis of the 300 s cadence
(legacy also used 5 minutes).

## InciWeb (recorded through the Internet Archive)

`inciweb.wildfire.gov` answered HTTP 403 from its AWS load balancer to every request from
the recording workstation's network on 2026-10-15 (home page, `robots.txt`, `api/…`,
`node/…`, with a descriptive non-browser User-Agent). The documents below are therefore the
Internet Archive's captures of the original responses, fetched with the `id_` modifier,
which returns the archived bytes without archive additions. Retrieved 2026-10-15.

| File | Original URL | Capture |
|---|---|---|
| `perimeters-inciweb-index.json` | `https://inciweb.wildfire.gov/api/map_data` | `https://web.archive.org/web/20261006203643id_/https://inciweb.wildfire.gov/api/map_data` (original `Date: Tue, 06 Oct 2026 20:36:43 GMT`) |
| `perimeters-inciweb-mtbdf-sand-creek.html` | `https://inciweb.wildfire.gov/incident-information/mtbdf-sand-creek` | `https://web.archive.org/web/20260929045909id_/…` (2026-09-29T04:59:09Z) |
| `perimeters-inciweb-cogmf-gold-mountain-fire.html` | `https://inciweb.wildfire.gov/incident-information/cogmf-gold-mountain-fire` | `https://web.archive.org/web/20260908012018id_/…` (2026-09-08T01:20:18Z) |
| `perimeters-inciweb-wanes-sinlahekin-fire.html` | `https://inciweb.wildfire.gov/incident-information/wanes-sinlahekin-fire` | `https://web.archive.org/web/20260802150210id_/…` (2026-08-02T15:02:10Z) |

Changes: the index (167 active incidents) is trimmed to 8 rows (Gold Mountain, Sinlahekin,
Sand Creek, Bobcat Lakes, Rowe Creek Complex, both same-name "Cottonwood Fire" incidents in
Utah and Oregon, and a prescribed burn), with the free-text `field_incident_overview` and
`field_incident_description` blanked (size; they can name people), then pretty-printed by
Biome (whitespace only; the web lint checks fixture JSON). Each page keeps its
`<head>` metadata (scripts and stylesheet links removed) and its first "Basic Information"
details table; the "Incident Commander" cell is replaced by `[redacted]`. The recorded
pages carry `dcterms.date` but no `og:updated_time`, which the legacy scraper read.

## Synthetic

`perimeters-wfigs-malformed.geojson` is written for the tests (2099 ids, invented shapes):
one valid triangle with wrong-typed, out-of-range, future and control-character attributes;
an open ring; a latitude of 95; a `LineString`; `geometry: null`; a three-position ring;
string coordinates; a duplicate id; null properties; a non-`Feature` member; a flat
(zero-area) exterior; a `MultiPolygon` with one real hole, one flat hole and a flat second
polygon; and a feature identified only by its GeoJSON `id`. Malformed InciWeb pages and
index rows, rate-limit bodies and HTTP failures are written inline in the tests.

## Terms

- **NIFC WFIGS**: item `d1c32af3212341869b3c810f1a215824` ("WFIGS Current Interagency Fire
  Perimeters", owner `NIFC_Authoritative`, public), read 2026-10-15 from
  `https://www.arcgis.com/sharing/rest/content/items/d1c32af3212341869b3c810f1a215824?f=json`.
  Its terms of use are a disclaimer without use restrictions: NIFC is not liable for
  improper use; the data are not legal documents, are dynamic and carry no warranty.
  Credits: DOI Office of Wildland Fire (IRWIN), USDA Forest Service, National Park Service,
  Fish and Wildlife Service, Bureau of Indian Affairs, Bureau of Land Management, National
  Association of State Foresters, US Fire Administration, National Wildfire Coordinating
  Group. Attribution used: "Wildfire perimeters: National Interagency Fire Center (WFIGS)".
- **InciWeb**: U.S. Government interagency incident information. Its About page (archived
  2026-10-02) states the information is for information purposes only and refers readers to
  the local wildfire agency for official information. Lavender stores only incident ids,
  titles, unit codes, page paths and page dates, and links to the pages.
