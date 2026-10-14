# Satellite reference fixtures

These are archived numerical reference cases, not live orbital data. They do not
provide a current ISS prediction.

The ISS two-line elements and observer come from
`legacy/src/data/satellitePass.test.mjs`: NORAD 25544, epoch day 264 of 2008,
Austin at 30.2672°, -97.7431°. The frozen legacy source remains under its existing
licence; no source code is copied into the new application.

`iss-satellite-js-vectors.csv` was generated on 2026-10-13 using the project's
installed satellite.js 7.1.0 (`apps/web/node_modules/satellite.js/dist/index.js`):
`twoline2satrec`, `propagate`, `gstime`, `eciToEcf`, `ecfToLookAngles`. Observer
altitude is 185 metres; times are 0, 30, 60, 120, 600 and 1440 minutes after
2008-09-20T12:30:00Z. The complete numerical output is retained; Rust tests do
not execute JavaScript or regenerate expected values.

`iss-austin-passes.csv` was generated with the same library and the frozen
`legacy/src/data/satellitePass.js::findNextSatellitePass`: start
2008-09-20T12:30:00Z, fixed 24-hour end, elevation threshold 10°, observer altitude
zero. After each result, the next search starts one second after set. It contains
all three geometric passes; sunlight/visibility filtering is disabled.

satellite.js uses WGS72 for SGP4 and WGS84 for observer geometry; Lavender's
propagator uses WGS84 for both. Tests allow 100 metres in TEME/range, 0.01° in
look angles, one second in pass times and 0.1° in peak elevation to cover the
different gravity model and the legacy parabolic peak approximation. These are
regression tolerances, not a claim about accuracy of historic or current elements.

The fixed Vanguard vectors in `satellite_passes.rs` come from the published
[Vallado et al., AIAA 2006-6753 verification data](https://celestrak.org/publications/AIAA/2006-6753/)
(`tcppver.out`, test satellite 00005). They also appear unchanged in sgp4 2.4.0's
`tests/test_cases.toml`. Lavender's WGS84 path is checked against these WGS72
vectors with the same 100-metre model envelope; the existing strict comparison
against sgp4's matching WGS84 construction remains in `orbit.rs`.

Reference transformation API:
[satellite.js coordinate transforms](https://github.com/shashwatak/satellite-js/blob/develop/docs/docs/transforms.md).
The library is MIT licensed; its pinned installation supplies the implementation
used to generate these fixtures.
