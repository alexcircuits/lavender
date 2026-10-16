# Wind GRIB2 fixtures (ENG-955)

These are **real, recorded upstream data from one past model run** (2026-10-15 00 UTC,
forecast +6 h, valid 2026-10-15 06 UTC). They are test inputs only and must never be
presented as a current forecast. Each `.grib2` file is one complete GRIB2 message, the
exact byte range the provider requests, unmodified. The inventories are the complete
published `.idx` / `.index` files of the same run. Downloaded 2026-10-15 with `curl -r`,
following `providers/wind/inventory.rs` (`gfs_urls`/`ifs_urls`, `gfs_wind_ranges`/
`ifs_wind_ranges`); `tests/wind_grib.rs` checks that the recorded inventories yield exactly
these ranges.

| File | Upstream object | Field | Byte range | Bytes | SHA-256 | Packing |
|---|---|---|---|---|---|---|
| `gfs-20261015-00z-f006.idx` | `https://noaa-gfs-bdp-pds.s3.amazonaws.com/gfs.20261015/00/atmos/gfs.t00z.pgrb2.0p25.f006.idx` | inventory (743 messages) | whole file | 40 480 | `b81d3292efaac70d722bccc845db693397129824c765ccd78a48ce5e1e4cb3f9` | text |
| `gfs-20261015-00z-f006-ugrd-10m.grib2` | `https://noaa-gfs-bdp-pds.s3.amazonaws.com/gfs.20261015/00/atmos/gfs.t00z.pgrb2.0p25.f006` | UGRD, 10 m above ground (message 588) | 430503386–431473669 | 970 284 | `6ca04412cc274c77011b4cc6efc056e5436c566f2b3533718f45b87e22566ebd` | 5.3 complex + 2nd-order spatial differencing, 12 bits, D=2 |
| `gfs-20261015-00z-f006-vgrd-10m.grib2` | same object | VGRD, 10 m above ground (message 589) | 431473670–432424956 | 951 287 | `e27fa581f1245f6037c72cbace82089dbe59f9ab04b7401817e51ef8bdc308c2` | 5.3, 13 bits, D=2 |
| `ifs-20261015-00z-6h.index` | `https://data.ecmwf.int/forecasts/20261015/00z/ifs/0p25/oper/20261015000000-6h-oper-fc.index` | inventory (184 fields) | whole file | 40 027 | `26ff187f6e89e5015c2ee2adf0927376030b1af1acf280b8c9b7389d78e41bc0` | JSON lines |
| `ifs-20261015-00z-6h-10u.grib2` | `https://data.ecmwf.int/forecasts/20261015/00z/ifs/0p25/oper/20261015000000-6h-oper-fc.grib2` | 10u (10 m U) | 9967974–10837157 | 869 184 | `89f45ab2a0209f2ae757bd16d7b1ed41b160681fdcfee64820e1d13356abca08` | 5.42 CCSDS, 12 bits, E=−6 |
| `ifs-20261015-00z-6h-10v.grib2` | same object | 10v (10 m V) | 12250976–13114795 | 863 820 | `635ce5d3d266626fdc75660032f79f3d6efadd35e95bf6943cdbb108434898fd` | 5.42 CCSDS, 12 bits, E=−6 |

All four messages: grid template 3.0, 1440 × 721 points at 0.25°, scanning mode 0 (rows
west to east, north to south), no bitmap, product template 4.0. GFS starts at 0° E; IFS
starts at 180° E (`longitudeOfFirstGridPoint` = 180°, last 179.75°). Total size ≈ 3.7 MB.
Section 4 says what each holds: U is parameter 0.2.2 and V 0.2.3 (discipline.category.
number), both on surface type 103 at 10 m; the provider refuses a message that is not the
field its inventory entry named (`decode_forecast_field`).

### Companion scalars (not committed)

The scalars the overlays use, from the same two objects, are **not** committed (size
budget, about 3 MB together). Their ecCodes reference values are, in
`reference-scalars.jsonl`. To run the opt-in tests, download each range into one directory
under exactly these names (`curl -r START-END -o NAME URL`, objects as in the table above):

| Name | Field (Section 4) | Byte range | Bytes | SHA-256 | Packing |
|---|---|---|---|---|---|
| `gfs-20261015-00z-f006-tmp-2m.grib2` | GFS `TMP` 2 m (0.0.0, surface 103 at 2) | 425151994–426027073 | 875 080 | `5aabb7e1da48ff6b9a81eb372512832ebf6664faad2e88f2f2c2ab2b77d64fea` | 5.3, 12 bits, D=2 |
| `gfs-20261015-00z-f006-prmsl.grib2` | GFS `PRMSL` (0.3.1, surface 101); message 1, so the range starts at byte 0 | 0–989351 | 989 352 | `d43dd744a37d2f22f03ccfc4183b8802287f68516b627fa83c4e0ced397f5571` | 5.3, 14 bits, D=1, E=1 |
| `ifs-20261015-00z-6h-2t.grib2` | IFS `2t` (0.0.0, surface 103 at 2) | 18585181–19243056 | 657 876 | `57f6442cd525f1ee101ebfd0b3c3a06ecebf77cdacc9ed072a6ad641bbc497ae` | 5.42, 12 bits, E=−5 |
| `ifs-20261015-00z-6h-msl.grib2` | IFS `msl` (0.3.0, surface 101) | 97113360–97641778 | 528 419 | `6bce85af7ee2f3ecfd7cd44402aeb3630461efbb20c1286e4df2b9595022fbf4` | 5.42, 12 bits, E=2 |

Re-downloaded on 2026-10-16 (HTTP 206); the SHA-256 values matched the 2026-10-15 copies.
Then run
`LAVENDER_WIND_SCALAR_MESSAGES=DIR cargo test -p lavender-core --test wind_grib -- --ignored`
(`real_scalar_messages_match_eccodes_and_are_served_in_display_units` checks the decode
against ecCodes and the served °C / hPa values). On 2026-10-16 the largest differences from
ecCodes over every point were 2.2e-5 K (GFS TMP), 4.7e-3 Pa (GFS PRMSL: f32 rounding near
10⁵ Pa; packing step 0.2 Pa), 0 (IFS 2t) and 0 (IFS msl). The committed tests cover the
overlay path with synthetic messages instead (`tests/wind.rs`, including a PRMSL range that
starts at byte 0), and the service tests leave the real scalar ranges unserved, which
exercises the "wind without scalar" path.

## Licences and attribution

- **NOAA GFS** — U.S. Government work in the public domain, distributed through NOAA Open
  Data Dissemination (https://registry.opendata.aws/noaa-gfs-bdp-pds/; terms
  https://www.weather.gov/disclaimer). Credit: "NOAA Global Forecast System (GFS) via NOAA
  Open Data Dissemination". No endorsement by NOAA/NWS is implied; these files are not
  official NOAA/NWS products.
- **ECMWF IFS open data** — Creative Commons Attribution 4.0 International. Required
  statement (data products, https://apps.ecmwf.int/datasets/licences/general/, checked
  2026-10-15): Copyright © 2026 European Centre for Medium-Range Weather Forecasts (ECMWF).
  Source: www.ecmwf.int. This data is published under a Creative Commons Attribution 4.0
  International (CC BY 4.0) licence, https://creativecommons.org/licenses/by/4.0/. ECMWF
  does not accept any liability whatsoever for any error or omission in the data, their
  availability, or for any loss or damage arising from their use. Modifications: none; the
  files are the listed byte ranges of the published file.

## Reference values (independent decoder)

`reference.py` reads each message with **ecCodes 2.50.0** (PyPI `eccodes==2.50.0`,
`eccodeslib==2.50.0.32`, Python 3.12, isolated) and writes `reference.jsonl`: templates,
grid geometry, min/max/mean/RMS and values at 46 fixed grid indices per field. Command
(run outside this folder; `-I` keeps the script and working directories off `sys.path`):

```sh
uv run --isolated --no-project --python 3.12 \
  --with eccodes==2.50.0 --with eccodeslib==2.50.0.32 \
  python -I crates/lavender-core/tests/fixtures/wind/reference.py \
  crates/lavender-core/tests/fixtures/wind/gfs-20261015-00z-f006-ugrd-10m.grib2 \
  crates/lavender-core/tests/fixtures/wind/gfs-20261015-00z-f006-vgrd-10m.grib2 \
  crates/lavender-core/tests/fixtures/wind/ifs-20261015-00z-6h-10u.grib2 \
  crates/lavender-core/tests/fixtures/wind/ifs-20261015-00z-6h-10v.grib2 \
  > crates/lavender-core/tests/fixtures/wind/reference.jsonl
```

`reference-scalars.jsonl` is the same command over the four scalar messages (in the order
of their table, from `DIR`). Both files were regenerated on 2026-10-16 and came out
byte-identical.

`tests/wind_grib.rs` asserts Lavender's decode matches every recorded point and the
min/max within half the packing step (GFS 0.01 m/s → 0.005; IFS 2⁻⁶ m/s → 0.0078), and
mean/RMS within 1e-4. For a full-field comparison add `--dump-values DIR` to the command
and run
`LAVENDER_WIND_REFERENCE_DUMPS=DIR cargo test -p lavender-core --test wind_grib -- --ignored`
(add `LAVENDER_WIND_SCALAR_MESSAGES` and dump the scalars into the same `DIR` to compare
them too). On 2026-10-15 and again on 2026-10-16 the largest difference over all
1 038 240 points was 1.45e-6 m/s for both GFS fields (f32 arithmetic) and exactly 0 for
both IFS fields.

## If an upstream changes its packing

An unsupported data representation template is refused with an error naming it (for
example `unsupported data representation template 5.40`): the manifest keeps serving the
last good generation marked stale, or reports `unavailable`, and the source's health in
`/api/v1/status` shows the error. The fix, in order of preference (also on
`SUPPORTED_DATA_TEMPLATES` in `src/providers/wind/grib.rs`):

1. 5.40 (JPEG 2000): enable the `grib` crate feature `jpeg2000-unpack-with-hayro` (pure
   Rust); 5.41 (PNG): enable `png-unpack-with-png-crate`. Add the template number to
   `SUPPORTED_DATA_TEMPLATES`, record a real message of the new packing here and add its
   ecCodes reference values.
2. Otherwise, fetch the same field from a mirror that keeps a supported packing (GFS is
   also on NOMADS; ECMWF open data is mirrored on AWS, Azure and Google Cloud).
3. Linking ecCodes through FFI is the last resort: it brings back the C library this
   migration removed and needs a dependency and licence review first.

## Refreshing

Pick a completed run, download the inventory, take the ranges the provider would compute,
fetch each with `curl -r START-END` into its own new directory, check `GRIB` at the start
and `7777` at the end, copy here, rerun `reference.py`, and update this table (URL, run,
range, size, SHA-256). Keep the total under about 4 MB.
