# Place geocoder contract fixtures

These are saved API documentation examples, not live endpoint downloads. Tests
only serve them from Wiremock. They establish wire formats and coordinate order;
they are not claims of current map data.

- `nominatim-london.json`: JSON example from
  <https://nominatim.org/release-docs/latest/api/Output/>, read 2026-10-13;
  optional icon/extratags and unused address fields omitted.
- `photon-berlin.json`: search example from
  <https://github.com/komoot/photon/blob/master/docs/api-v1.md>, read 2026-10-13.

Data © OpenStreetMap contributors, ODbL 1.0:
<https://www.openstreetmap.org/copyright>. Photon service credit: komoot.
Malformed, empty, unavailable and quota responses in the tests are synthetic.
