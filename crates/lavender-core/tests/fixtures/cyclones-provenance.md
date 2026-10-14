# Cyclone contract fixtures

These JSON documents are **synthetic**, not recorded advisories or real storms.
TEST ALPHA, advisory 6, coordinates, intensity and dates were authored for deterministic
validation on 2026-10-14. The deliberately split track/cone and polygon hole test
preservation of GeoJSON topology; they do not represent an actual forecast.

Schema checked on 2026-10-14 against the official NOAA endpoints:
- https://www.nhc.noaa.gov/CurrentStorms.json
- https://mapservices.weather.noaa.gov/tropical/rest/services/tropical/NHC_tropical_weather_summary/MapServer/5?f=pjson (points)
- same service layers 6 (track) and 7 (cone), queried with f=geojson/outSR=4326.
- https://www.weather.gov/disclaimer (NOAA data public domain absent specific annotation;
  no endorsement; modified displays must not be presented as official products).

The empty fixture tests an inactive season. The malformed fixture is intentionally
invalid and must be rejected. No live service is contacted by automated tests.
