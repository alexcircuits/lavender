# Third-party notices — Lavender

Lavender itself is proprietary (see `LICENSE`). The components below are used under
their own licences, whose notices must accompany the code that includes them.

Lavender is a NoirSystems product. Parts of it are derived from, or depend on, third-party
work whose licences require notices. Data sources and their attribution requirements are
recorded in `docs/audit/data-sources-and-licensing.md` and enforced at runtime by the
source registry (ADR 0008); this file covers code and bundled software.

## God's Eye View (upstream project)

Lavender is derived from God's Eye View (https://github.com/bilawalsidhu/gods-eye-view).
Its source code is used under the MIT License:

```
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

The MIT licence covers the upstream **source code only**. The upstream's bundled data
and assets carry their own licences (some non-commercial); see
`docs/audit/data-sources-and-licensing.md` §4. The "God's Eye View" name and logo are not
licensed to Lavender.

### skylight (via God's Eye View)

Upstream code in `legacy/src/data/aircraftClass.js` and related modules is adapted from
skylight (https://github.com/cpaczek/skylight), MIT-licensed. Its exact copyright line
must be copied from the skylight LICENSE before any release that includes code derived
from it (open question in the licensing audit; tracked by ENG-1008).

## CesiumJS

Lavender renders the globe with `@cesium/engine` and `@cesium/core` (CesiumJS), Apache
License 2.0, https://github.com/CesiumGS/cesium. Cesium's own third-party notices ship with the
package (`ThirdParty.json`).

## NOAA NHC / CPHC data

Cyclone status, forecast tracks and cones use NOAA/NWS NHC / CPHC products
([National Hurricane Center](https://www.nhc.noaa.gov/),
[Central Pacific Hurricane Center](https://www.nhc.noaa.gov/?cpac)). These U.S.
Government products are public domain under the
[National Weather Service disclaimer](https://www.weather.gov/disclaimer).
Lavender's use does not imply NOAA or NWS endorsement. Data-source notice updated
2026-10-14; rights to basemap imagery are governed separately.

## Rust dependencies

Rust dependencies are checked by `cargo deny` against a permissive allow-list
(`deny.toml`). The desktop application additionally includes these crates under the
**Mozilla Public License 2.0**, used unmodified: `cssparser`, `cssparser-macros`,
`selectors`, `dtoa-short`, `option-ext`. Their source code is available at
https://crates.io/crates/cssparser, https://crates.io/crates/cssparser-macros,
https://crates.io/crates/selectors, https://crates.io/crates/dtoa-short and
https://crates.io/crates/option-ext.

## Frontend dependencies

Frontend npm dependencies and their licences are recorded in `apps/web/pnpm-lock.yaml`
and verified before release (ENG-1008).

### satellite.js

The globe propagates satellite orbits in the browser with `satellite.js` 7.1.0
(https://github.com/shashwatak/satellite-js), MIT License, Copyright (C) 2013 Shashwat
Kandadai, UCSC Jack Baskin School of Engineering. The full licence text ships with the
package (`node_modules/satellite.js/LICENSE.md`) and must accompany distributed builds.
Only its JavaScript SGP4 code is bundled; its optional WASM bulk propagator is excluded
at build time (`apps/web/satellite-js-without-wasm.ts`).
