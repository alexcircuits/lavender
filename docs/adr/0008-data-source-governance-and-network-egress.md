# ADR 0008 — Data-source governance: the Rust core is the egress, licences are enforced

- Status: Accepted
- Date: 2026-10-11
- Evidence: `docs/audit/data-sources-and-licensing.md`, `docs/audit/upstream-layers.md` §6, `docs/audit/upstream-backend.md` §5.3, §9

## Problem

GEV lets the browser call about 20 external hosts directly. It ships three
provider keys inside its JavaScript bundle and allows any `https:` origin in its
CSP. Its default map and several of its keyless feeds are not licensed for
commercial use (OpenSky, Esri World Imagery, the public OSM servers, Open-Meteo,
Cesium ion's free plan, CelesTrak without a grant). The licensing knowledge
lives in prose (`DATA_SOURCES.md`), not in code. Lavender may be sold, and must
show users where every datum came from.

## Decision

### 1. One egress for data

All **data** requests (feeds, search, enrichment, computation inputs) go
through `lavender-core`'s fetcher. Each provider declares an allowlist of hosts.
The fetcher enforces:

- HTTPS only;
- no redirects off the allowlist;
- connect and total timeouts;
- a maximum body size;
- per-host concurrency;
- a `User-Agent` identifying Lavender with a contact address;
- (where needed) DNS resolution checks that reject private, loopback and
  link-local addresses.

No upstream URL is ever taken from a request parameter. Base URLs come from
configuration.

**Map tiles are the only exception.** Imagery, terrain and 3D Tiles are fetched
by Cesium in the WebView, from hosts listed in the catalog, because proxying
them would multiply bandwidth and violate some providers' terms. The CSP's
`img-src`/`connect-src` lists exactly those tile hosts, generated from the
catalog for the desktop build and configured for web deployments.

### 2. Licences are data, and a gate

Every provider registers a `SourceInfo`:

```text
id, name, homepage, attribution (exact mandated text), licence { name, url },
commercial: Allowed | RequiresAgreement | NonCommercialOnly | Unknown,
requires_key, privacy: Normal | Sensitive, notes
```

- The UI's attribution panel and the inspector's provenance row render from
  this registry, never from hand-written strings.
- `licence_mode` is configuration: `evaluation` (the default for development)
  or `commercial`. In `commercial` mode, the core refuses to enable any source
  whose `commercial` is not `Allowed`, unless the operator lists it in
  `licence_acknowledged = ["opensky", …]`, recording that an agreement exists.
  The refusal is visible in `/status` as "disabled by licence policy".
- `privacy: Sensitive` sources (CCTV, ALPR) are off by default in every mode and
  carry an in-product notice when enabled.

### 3. Keys

- Server-held keys never leave the server (ADR 0006).
- Keys that a tile provider requires the *browser* to send (e.g. a Google Map
  Tiles key restricted by referrer) are configured as `public_*` values,
  delivered at runtime in the catalog, and labelled public in the docs.

### 4. Default keyless stack (commercially safe)

| Need | Default | Licence |
|---|---|---|
| Base imagery (online) | NASA GIBS: Blue Marble shaded relief (day) / VIIRS Black Marble (night) | US Gov / attribution |
| Base imagery (offline) | Cesium's bundled Natural Earth II | public domain |
| Earthquakes | USGS GeoJSON feeds | public domain |
| Aircraft | adsb.lol | ODbL 1.0 |
| Fires | NASA FIRMS (free `MAP_KEY`) | NASA open data |
| Launches | Launch Library 2 | free tier, rate-limited |
| Cyclones | NOAA NHC / CPHC | public domain |

Sources with restricted terms (OpenSky, CelesTrak, AISStream, Google, Cesium ion,
TomTom, the public OSM services) are implemented but default to "requires
configuration" or "evaluation only".

## Consequences

- The licence matrix becomes executable and testable. A unit test fails if a
  registered source lacks attribution or licence data.
- Some GEV defaults (the Esri basemap, the OpenSky global feed) are not
  defaults in Lavender; they remain available behind configuration.
- Satellites need a decision. CelesTrak grants no licence, and Space-Track
  requires an account and has redistribution limits. The provider is built,
  marked `Unknown`, and the question is recorded in the product specification's
  open questions.
