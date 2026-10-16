"""Independent ecCodes reference values for Lavender's real GRIB2 wind fixtures.

Run from a directory outside the fixture folder, isolated (`-I` keeps the script's and
the working directory off `sys.path`), with the GRIB files as arguments:

    UV_CACHE_DIR=<scratch>/uv-cache UV_PYTHON_INSTALL_DIR=<scratch>/uv-python \
      uv run --isolated --no-project --python 3.12 \
      --with eccodes==2.50.0 --with eccodeslib==2.50.0.32 \
      python -I <fixtures>/reference.py \
      <fixtures>/gfs-20261015-00z-f006-ugrd-10m.grib2 \
      <fixtures>/gfs-20261015-00z-f006-vgrd-10m.grib2 \
      <fixtures>/ifs-20261015-00z-6h-10u.grib2 \
      <fixtures>/ifs-20261015-00z-6h-10v.grib2 > <fixtures>/reference.jsonl

It prints one JSON object per file: the template and packing ecCodes reports, the grid
geometry, summary statistics and values at fixed grid indices. `reference.jsonl` is that
output for the four wind fixtures, and `reference-scalars.jsonl` for the four uncommitted
scalar messages listed in PROVENANCE.md; both are read by
`crates/lavender-core/tests/wind_grib.rs`. With `--dump-values DIR` it also
writes every decoded value as little-endian f64 (`<name>.f64le`) for the ignored
full-field comparison test; those dumps are not committed.
"""

import json
import math
import struct
import sys
from pathlib import Path

import eccodes

KEYS = [
    "edition",
    "centre",
    "shortName",
    "typeOfLevel",
    "level",
    "dataDate",
    "dataTime",
    "stepRange",
    "gridDefinitionTemplateNumber",
    "dataRepresentationTemplateNumber",
    "packingType",
    "bitsPerValue",
    "binaryScaleFactor",
    "decimalScaleFactor",
    "referenceValue",
    "Ni",
    "Nj",
    "numberOfValues",
    "numberOfDataPoints",
    "latitudeOfFirstGridPointInDegrees",
    "longitudeOfFirstGridPointInDegrees",
    "latitudeOfLastGridPointInDegrees",
    "longitudeOfLastGridPointInDegrees",
    "iDirectionIncrementInDegrees",
    "jDirectionIncrementInDegrees",
    "scanningMode",
    "bitmapPresent",
]


def sample_indices(ni: int, nj: int) -> list[tuple[int, int]]:
    """(row, column) pairs: corners, edges, equator, prime/anti meridian, interior points."""
    rows = [0, 1, nj // 4, nj // 2, (3 * nj) // 4, nj - 2, nj - 1]
    cols = [0, 1, ni // 4, ni // 2, (3 * ni) // 4, ni - 1]
    picks = [(r, c) for r in rows for c in cols]
    # A few irregular interior points so a row/column swap cannot pass by symmetry.
    picks += [(123, 457), (301, 1001), (577, 89), (640, 1399 % ni)]
    return picks


def describe(path: Path, dump_dir: Path | None) -> dict:
    with path.open("rb") as handle:
        gid = eccodes.codes_grib_new_from_file(handle)
        if gid is None:
            raise SystemExit(f"{path}: no GRIB message")
        try:
            meta = {}
            for key in KEYS:
                try:
                    meta[key] = eccodes.codes_get(gid, key)
                except eccodes.KeyValueNotFoundError:
                    meta[key] = None
            values = eccodes.codes_get_values(gid)
        finally:
            eccodes.codes_release(gid)
    ni, nj = int(meta["Ni"]), int(meta["Nj"])
    count = len(values)
    mean = math.fsum(values) / count
    rms = math.sqrt(math.fsum(v * v for v in values) / count)
    precision = (2.0 ** meta["binaryScaleFactor"]) / (10.0 ** meta["decimalScaleFactor"])
    points = [
        {"row": r, "col": c, "index": r * ni + c, "value": float(values[r * ni + c])}
        for (r, c) in sample_indices(ni, nj)
    ]
    if dump_dir is not None:
        out = dump_dir / f"{path.stem}.f64le"
        out.write_bytes(struct.pack(f"<{count}d", *values))
    return {
        "file": path.name,
        "eccodes": eccodes.codes_get_api_version(),
        **meta,
        "count": count,
        "min": float(min(values)),
        "max": float(max(values)),
        "mean": mean,
        "rms": rms,
        "packingPrecision": precision,
        "points": points,
    }


def main(argv: list[str]) -> None:
    dump_dir = None
    if "--dump-values" in argv:
        at = argv.index("--dump-values")
        dump_dir = Path(argv[at + 1])
        argv = argv[:at] + argv[at + 2 :]
    for name in argv:
        print(json.dumps(describe(Path(name), dump_dir)))


if __name__ == "__main__":
    main(sys.argv[1:])
