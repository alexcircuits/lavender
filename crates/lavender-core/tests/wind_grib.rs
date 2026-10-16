//! Real GRIB2 decoding (ENG-955): NOAA GFS 0.25° (template 5.3, complex packing with
//! spatial differencing) and ECMWF IFS open data 0.25° (template 5.42, CCSDS) messages,
//! downloaded with the provider's own byte ranges, are compared with independent ecCodes
//! reference values, and must be the field their inventory entry names. Corrupt and
//! adversarial headers must fail with an error, never a panic or a header-sized
//! allocation. Provenance: `tests/fixtures/wind/PROVENANCE.md`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lavender_core::providers::wind::{
    ByteRange, DecodedMessage, ForecastField, GribField, MAX_GRID_POINTS, MAX_MESSAGE_BYTES,
    decode_forecast_field, decode_grib_message, gfs_wind_ranges, ifs_wind_ranges, parse_gfs_idx,
    parse_ifs_index, read_grib_header, resample_wind_grid,
};
use lavender_domain::WeatherOverlay;
use serde::Deserialize;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/wind");
const GFS_U: &str = "gfs-20261015-00z-f006-ugrd-10m.grib2";
const GFS_V: &str = "gfs-20261015-00z-f006-vgrd-10m.grib2";
const IFS_U: &str = "ifs-20261015-00z-6h-10u.grib2";
const IFS_V: &str = "ifs-20261015-00z-6h-10v.grib2";

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}/{name}")).unwrap()
}

/// One line of `reference.jsonl` or `reference-scalars.jsonl`, as written by `reference.py`
/// (ecCodes key names).
#[derive(Debug, Deserialize)]
struct Reference {
    file: String,
    #[serde(rename = "dataRepresentationTemplateNumber")]
    data_template: u16,
    #[serde(rename = "Ni")]
    ni: usize,
    #[serde(rename = "Nj")]
    nj: usize,
    #[serde(rename = "latitudeOfFirstGridPointInDegrees")]
    la1: f64,
    #[serde(rename = "longitudeOfFirstGridPointInDegrees")]
    lo1: f64,
    #[serde(rename = "iDirectionIncrementInDegrees")]
    di: f64,
    #[serde(rename = "jDirectionIncrementInDegrees")]
    dj: f64,
    count: usize,
    min: f64,
    max: f64,
    mean: f64,
    rms: f64,
    #[serde(rename = "packingPrecision")]
    packing_precision: f64,
    points: Vec<ReferencePoint>,
}

#[derive(Debug, Deserialize)]
struct ReferencePoint {
    row: usize,
    col: usize,
    index: usize,
    value: f64,
}

fn references_in(name: &str) -> Vec<Reference> {
    std::fs::read_to_string(format!("{FIXTURES}/{name}"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn references() -> Vec<Reference> {
    references_in("reference.jsonl")
}

/// Both decoders unpack the same integers; only the float arithmetic differs. Half the
/// packing step is the error the packing itself allows, plus the f32 rounding of the value,
/// so a single integer decoded wrongly would exceed it.
fn tolerance(reference: &Reference, value: f64) -> f64 {
    reference.packing_precision / 2.0 + value.abs() * f64::from(f32::EPSILON)
}

fn assert_matches_reference(decoded: &DecodedMessage, reference: &Reference) {
    let file = &reference.file;
    assert_eq!((decoded.ni, decoded.nj), (reference.ni, reference.nj), "{file}");
    assert_eq!(decoded.values.len(), reference.count, "{file}");
    assert!((decoded.la1 - reference.la1).abs() < 1e-9, "{file} la1");
    assert!((decoded.lo1 - reference.lo1).abs() < 1e-9, "{file} lo1");
    assert!((decoded.di - reference.di).abs() < 1e-9, "{file} di");
    assert!((decoded.dj - reference.dj).abs() < 1e-9, "{file} dj");
    for point in &reference.points {
        let actual = f64::from(decoded.values[point.index]);
        assert!(
            (actual - point.value).abs() <= tolerance(reference, point.value),
            "{file} index {}: {actual} vs ecCodes {}",
            point.index,
            point.value
        );
    }
    let values: Vec<f64> = decoded.values.iter().copied().map(f64::from).collect();
    let count = f64::from(u32::try_from(values.len()).unwrap());
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mean = values.iter().sum::<f64>() / count;
    let rms = (values.iter().map(|v| v * v).sum::<f64>() / count).sqrt();
    assert!((min - reference.min).abs() <= tolerance(reference, reference.min), "{file} min {min}");
    assert!((max - reference.max).abs() <= tolerance(reference, reference.max), "{file} max {max}");
    let summary = 1e-4 + reference.mean.abs() * f64::from(f32::EPSILON);
    assert!((mean - reference.mean).abs() < summary, "{file} mean {mean}");
    assert!((rms - reference.rms).abs() < summary, "{file} rms {rms}");
}

#[test]
fn real_messages_use_the_expected_packing_templates() {
    let templates: Vec<(String, u16)> =
        references().into_iter().map(|r| (r.file, r.data_template)).collect();
    assert_eq!(
        templates,
        [(GFS_U.into(), 3), (GFS_V.into(), 3), (IFS_U.into(), 42), (IFS_V.into(), 42)]
    );
    for (file, template) in templates {
        let header = read_grib_header(&fixture(&file)).unwrap();
        assert_eq!(header.data_template, template, "{file}");
        assert_eq!(header.grid_points, 1440 * 721, "{file}");
        assert_eq!(header.packed_values, 1440 * 721, "{file}");
    }
}

#[test]
fn real_gfs_and_ifs_messages_match_eccodes_reference_values() {
    for reference in references() {
        let decoded = decode_grib_message(&fixture(&reference.file)).unwrap();
        assert_matches_reference(&decoded, &reference);
    }
}

/// The overlays' companion scalars of the same run (2 m temperature, mean sea level
/// pressure) are not committed (size budget), but their ecCodes values are
/// (`reference-scalars.jsonl`). Download the messages with the ranges and names in
/// PROVENANCE.md into one directory and run
/// `LAVENDER_WIND_SCALAR_MESSAGES=DIR cargo test -p lavender-core --test wind_grib -- --ignored`.
#[test]
#[ignore = "needs the uncommitted scalar messages (see tests/fixtures/wind/PROVENANCE.md)"]
fn real_scalar_messages_match_eccodes_and_are_served_in_display_units() {
    let dir = std::env::var("LAVENDER_WIND_SCALAR_MESSAGES")
        .expect("LAVENDER_WIND_SCALAR_MESSAGES names the directory of the scalar messages");
    let scalars = references_in("reference-scalars.jsonl");
    let templates: Vec<(&str, u16)> =
        scalars.iter().map(|r| (r.file.as_str(), r.data_template)).collect();
    assert_eq!(
        templates,
        [
            ("gfs-20261015-00z-f006-tmp-2m.grib2", 3),
            ("gfs-20261015-00z-f006-prmsl.grib2", 3),
            ("ifs-20261015-00z-6h-2t.grib2", 42),
            ("ifs-20261015-00z-6h-msl.grib2", 42),
        ]
    );
    let decode = |name: &str| decode_grib_message(&fixture(name)).unwrap();
    for reference in &scalars {
        let file = reference.file.as_str();
        // display value = value × scale + offset (K → °C, Pa → hPa)
        let (field, overlay, scale, offset) = if file.contains("tmp-2m") || file.contains("-2t") {
            (ForecastField::Temperature2m, WeatherOverlay::Temperature, 1.0, -273.15)
        } else {
            (ForecastField::PressureMsl, WeatherOverlay::Pressure, 0.01, 0.0)
        };
        let bytes = std::fs::read(format!("{dir}/{file}")).unwrap();
        let scalar = decode_forecast_field(&bytes, field).unwrap();
        assert_matches_reference(&scalar, reference);

        // Served with the same run's wind at 1°: whole-degree nodes are source points.
        let (u, v, first_column) =
            if file.starts_with("gfs") { (GFS_U, GFS_V, 0) } else { (IFS_U, IFS_V, 720) };
        let grid =
            resample_wind_grid(&decode(u), &decode(v), Some(&scalar), overlay, 1.0, 1.0).unwrap();
        let served = grid.scalar.unwrap();
        let mut checked = 0;
        for point in reference.points.iter().filter(|p| p.row % 4 == 0 && p.col % 4 == 0) {
            let at = (point.row / 4) * 360 + ((first_column + point.col) % 1440) / 4;
            let expected = point.value * scale + offset;
            let allowed = tolerance(reference, point.value) * scale
                + expected.abs() * f64::from(f32::EPSILON) * 2.0;
            let actual = f64::from(served[at]);
            assert!((actual - expected).abs() <= allowed, "{file} {at}: {actual} vs {expected}");
            checked += 1;
        }
        assert!(checked >= 20, "{file}: only {checked} points on whole degrees");
    }
}

/// Compares every value with ecCodes. Needs the dumps written by
/// `reference.py --dump-values DIR`; run with
/// `LAVENDER_WIND_REFERENCE_DUMPS=DIR cargo test -p lavender-core --test wind_grib -- --ignored`.
/// With `LAVENDER_WIND_SCALAR_MESSAGES` set as well, the scalar messages are compared too
/// (their dumps in the same `DIR`).
#[test]
#[ignore = "needs local ecCodes value dumps (see tests/fixtures/wind/PROVENANCE.md)"]
fn every_value_matches_an_eccodes_dump() {
    let dir = std::env::var("LAVENDER_WIND_REFERENCE_DUMPS")
        .expect("LAVENDER_WIND_REFERENCE_DUMPS names the --dump-values directory");
    let mut messages: Vec<(Reference, Vec<u8>)> = references()
        .into_iter()
        .map(|r| {
            let bytes = fixture(&r.file);
            (r, bytes)
        })
        .collect();
    if let Ok(scalars) = std::env::var("LAVENDER_WIND_SCALAR_MESSAGES") {
        for reference in references_in("reference-scalars.jsonl") {
            let bytes = std::fs::read(format!("{scalars}/{}", reference.file)).unwrap();
            messages.push((reference, bytes));
        }
    }
    for (reference, bytes) in messages {
        let stem = reference.file.trim_end_matches(".grib2");
        let dump = std::fs::read(format!("{dir}/{stem}.f64le")).unwrap();
        let expected: Vec<f64> =
            dump.as_chunks::<8>().0.iter().map(|chunk| f64::from_le_bytes(*chunk)).collect();
        let decoded = decode_grib_message(&bytes).unwrap();
        assert_eq!(decoded.values.len(), expected.len());
        let mut largest = 0.0f64;
        for (actual, expected) in decoded.values.iter().zip(&expected) {
            let difference = (f64::from(*actual) - expected).abs();
            assert!(
                difference <= tolerance(&reference, *expected),
                "{}: {actual} vs ecCodes {expected}",
                reference.file
            );
            largest = largest.max(difference);
        }
        println!("{}: largest difference from ecCodes {largest:e}", reference.file);
    }
}

#[test]
fn real_messages_describe_the_field_their_inventory_entry_names() {
    let wind = |number| GribField {
        discipline: 0,
        parameter: (2, number),
        surface: 103,
        surface_value: Some(10.0),
    };
    for (file, field, expected) in [
        (GFS_U, wind(2), ForecastField::EastwardWind10m),
        (GFS_V, wind(3), ForecastField::NorthwardWind10m),
        (IFS_U, wind(2), ForecastField::EastwardWind10m),
        (IFS_V, wind(3), ForecastField::NorthwardWind10m),
    ] {
        let bytes = fixture(file);
        assert_eq!(read_grib_header(&bytes).unwrap().field, field, "{file}");
        assert!(decode_forecast_field(&bytes, expected).is_ok(), "{file}");
    }
    // The pressure field is 0.3.1 (PRMSL) at GFS and 0.3.0 (msl) at ECMWF, on surface 101.
    let msl = |number| GribField {
        discipline: 0,
        parameter: (3, number),
        surface: 101,
        surface_value: None,
    };
    assert!(
        ForecastField::PressureMsl.matches(&msl(0)) && ForecastField::PressureMsl.matches(&msl(1))
    );
    assert!(!ForecastField::PressureMsl.matches(&wind(2)));
}

#[test]
fn a_message_that_is_not_the_promised_field_is_refused_and_named() {
    let refused = |bytes: &[u8], expected| {
        decode_forecast_field(bytes, expected).expect_err("wrong field").to_string()
    };
    // V where U was asked for (and the other way round).
    let message = refused(&fixture(GFS_V), ForecastField::EastwardWind10m);
    assert!(message.contains("expected 10 m eastward wind"), "{message}");
    assert!(message.contains("parameter 0.2.3 on surface 103 at 10"), "{message}");
    assert!(decode_forecast_field(&fixture(IFS_U), ForecastField::NorthwardWind10m).is_err());
    // Another level: 100 m instead of 10 m (Section 4 octets 25–28).
    let message =
        refused(&patched(GFS_U, 4, 24, &100u32.to_be_bytes()), ForecastField::EastwardWind10m);
    assert!(message.contains("at 100"), "{message}");
    // Another discipline (Section 0 octet 7): 10 is oceanographic products.
    let mut ocean = fixture(IFS_U);
    ocean[6] = 10;
    assert!(decode_forecast_field(&ocean, ForecastField::EastwardWind10m).is_err());
    // Wind is not temperature or pressure.
    assert!(decode_forecast_field(&fixture(GFS_U), ForecastField::Temperature2m).is_err());
    assert!(decode_forecast_field(&fixture(GFS_U), ForecastField::PressureMsl).is_err());
    // Product templates beyond 4.15 move the parameter octets; they are named, not misread.
    assert_rejected(&patched(IFS_V, 4, 7, &[0, 40]), "4.40");
}

#[test]
fn real_inventories_give_the_exact_ranges_of_the_fixture_messages() {
    let idx = std::fs::read_to_string(format!("{FIXTURES}/gfs-20261015-00z-f006.idx")).unwrap();
    let entries = parse_gfs_idx(&idx).unwrap();
    assert_eq!(entries.len(), 743);
    let ranges = gfs_wind_ranges(&entries, WeatherOverlay::None).unwrap();
    assert_eq!(ranges.u, ByteRange { start: 430_503_386, end: 431_473_669 });
    assert_eq!(ranges.v, ByteRange { start: 431_473_670, end: 432_424_956 });
    assert_eq!(range_len(&ranges.u), fixture(GFS_U).len());
    assert_eq!(range_len(&ranges.v), fixture(GFS_V).len());
    assert!(ranges.scalar.is_none());
    let temperature = gfs_wind_ranges(&entries, WeatherOverlay::Temperature).unwrap();
    assert_eq!(temperature.scalar, Some(ByteRange { start: 425_151_994, end: 426_027_073 }));
    let pressure = gfs_wind_ranges(&entries, WeatherOverlay::Pressure).unwrap();
    assert_eq!(pressure.scalar, Some(ByteRange { start: 0, end: 989_351 }));

    let index = std::fs::read_to_string(format!("{FIXTURES}/ifs-20261015-00z-6h.index")).unwrap();
    let entries = parse_ifs_index(&index).unwrap();
    assert_eq!(entries.len(), 184);
    let ranges = ifs_wind_ranges(&entries, WeatherOverlay::None).unwrap();
    assert_eq!(ranges.u, ByteRange { start: 9_967_974, end: 10_837_157 });
    assert_eq!(ranges.v, ByteRange { start: 12_250_976, end: 13_114_795 });
    assert_eq!(range_len(&ranges.u), fixture(IFS_U).len());
    assert_eq!(range_len(&ranges.v), fixture(IFS_V).len());
    let temperature = ifs_wind_ranges(&entries, WeatherOverlay::Temperature).unwrap();
    assert_eq!(temperature.scalar, Some(ByteRange { start: 18_585_181, end: 19_243_056 }));
}

#[test]
fn the_finest_configurable_spacing_serves_the_native_quarter_degree_grid() {
    // providers.wind.target_dx accepts 0.25, so the resampler must serve 1440 × 721.
    let decode = |name: &str| decode_grib_message(&fixture(name)).unwrap();
    let (u, v) = (decode(GFS_U), decode(GFS_V));
    let grid = resample_wind_grid(&u, &v, None, WeatherOverlay::None, 0.25, 0.25).unwrap();
    assert_eq!((grid.nx, grid.ny, grid.lo1, grid.la1), (1440, 721, 0.0, 90.0));
    // Same origin and spacing as the GFS source: every node is a source point, unchanged.
    assert_eq!(grid.u, u.values);
    assert_eq!(grid.v, v.values);

    // IFS rows start at 180° E: served column c is source column (c + 720) mod 1440.
    let (u, v) = (decode(IFS_U), decode(IFS_V));
    let grid = resample_wind_grid(&u, &v, None, WeatherOverlay::None, 0.25, 0.25).unwrap();
    for (row, column) in [(0, 0), (123, 457), (360, 720), (577, 1439), (720, 1000)] {
        let source = row * 1440 + (column + 720) % 1440;
        assert_eq!(grid.u[row * 1440 + column], u.values[source], "U {row},{column}");
        assert_eq!(grid.v[row * 1440 + column], v.values[source], "V {row},{column}");
    }
}

fn range_len(range: &ByteRange) -> usize {
    usize::try_from(range.end - range.start + 1).unwrap()
}

// ── corrupt and adversarial headers ─────────────────────────────────────────────────────

/// Byte offset of the first section numbered `number` (an independent minimal walk, so the
/// tests do not trust the code under test to locate what they corrupt).
fn section(bytes: &[u8], number: u8) -> usize {
    let mut offset = 16;
    loop {
        let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap());
        if bytes[offset + 4] == number {
            return offset;
        }
        offset += usize::try_from(length).unwrap();
    }
}

fn patched(name: &str, section_number: u8, at: usize, value: &[u8]) -> Vec<u8> {
    let mut bytes = fixture(name);
    let start = section(&bytes, section_number) + at;
    bytes[start..start + value.len()].copy_from_slice(value);
    bytes
}

fn assert_rejected(bytes: &[u8], expected: &str) {
    let error = decode_grib_message(bytes).expect_err("corrupt message must be rejected");
    let message = error.to_string();
    assert!(message.contains(expected), "{message:?} should mention {expected:?}");
}

const HUGE: [u8; 4] = [0x7f, 0xff, 0xff, 0xff];

#[test]
fn claimed_grid_sizes_beyond_the_budget_fail_before_decoding() {
    // Section 3 (template 3.0): number of data points, Ni and Nj.
    assert_rejected(&patched(GFS_U, 3, 6, &HUGE), "grid points");
    assert_rejected(&patched(IFS_U, 3, 6, &HUGE), "grid points");
    assert_rejected(&patched(GFS_U, 3, 30, &HUGE), "grid");
    assert_rejected(&patched(IFS_V, 3, 34, &HUGE), "grid");
    // Section 5: packed value count (CCSDS would allocate count × sample size up front).
    assert_rejected(&patched(IFS_U, 5, 5, &HUGE), "packed values");
    assert_rejected(&patched(GFS_V, 5, 5, &HUGE), "packed values");
    // Bits per packed value beyond 32 (CCSDS sample size, complex packing reference widths).
    assert_rejected(&patched(IFS_U, 5, 19, &[64]), "bits per value");
    assert_rejected(&patched(GFS_U, 5, 19, &[200]), "bits per value");
}

#[test]
fn complex_packing_group_claims_are_checked_before_decoding() {
    // Template 5.3: number of groups, true length of the last group, and group length
    // reference/increment/width (each group is expanded into memory at once).
    assert_rejected(&patched(GFS_U, 5, 31, &HUGE), "groups");
    assert_rejected(&patched(GFS_U, 5, 42, &HUGE), "group length");
    assert_rejected(&patched(GFS_U, 5, 37, &HUGE), "group length");
    assert_rejected(&patched(GFS_V, 5, 46, &[40]), "group length");
}

#[test]
fn broken_message_structure_fails_cleanly() {
    let gfs = fixture(GFS_U);
    assert_rejected(&gfs[..gfs.len() / 2], "length");
    assert_rejected(&gfs[..10], "too short");
    assert_rejected(b"NOT A GRIB MESSAGE AT ALL", "GRIB");
    let mut edition1 = gfs.clone();
    edition1[7] = 1;
    assert_rejected(&edition1, "edition 2");
    let mut no_end = gfs.clone();
    let end = no_end.len() - 4;
    no_end[end..].copy_from_slice(b"7776");
    assert_rejected(&no_end, "7777");
    // A section that claims more bytes than the message holds, or fewer than its header.
    assert_rejected(&patched(GFS_U, 4, 0, &HUGE), "section");
    assert_rejected(&patched(GFS_U, 4, 0, &[0, 0, 0, 0]), "section");
    assert_rejected(&patched(IFS_U, 7, 0, &[0, 0, 0, 2]), "section");
    let mut oversized = vec![0u8; MAX_MESSAGE_BYTES + 1];
    oversized[..4].copy_from_slice(b"GRIB");
    assert_rejected(&oversized, "MiB");
}

#[test]
fn unsupported_templates_and_layouts_are_named() {
    // 5.40 (JPEG 2000) and 5.41 (PNG) are not compiled in; the error names the template.
    assert_rejected(&patched(GFS_U, 5, 9, &[0, 40]), "5.40");
    assert_rejected(&patched(IFS_U, 5, 9, &[0, 41]), "5.41");
    // Grid template 3.40 (Gaussian) is not a regular latitude/longitude grid.
    assert_rejected(&patched(IFS_U, 3, 12, &[0, 40]), "3.40");
    // South-to-north scanning would flip the map; it is refused rather than misread.
    assert_rejected(&patched(GFS_U, 3, 71, &[0x40]), "scanning mode");
    // A bitmap (masked points) is not expected for 10 m wind.
    assert_rejected(&patched(IFS_U, 6, 5, &[0x00]), "bitmap");
}

#[test]
fn a_consistently_truncated_data_section_is_an_error_not_a_panic() {
    // Shrink section 7 of the GFS message to 1 000 bytes and fix every length, so the
    // structure is valid but the packed groups run past the data: the decoder's own
    // slicing would panic without Lavender's containment.
    let original = fixture(GFS_U);
    let data = section(&original, 7);
    let mut bytes = original[..data + 1_000].to_vec();
    bytes[data..data + 4].copy_from_slice(&1_000u32.to_be_bytes());
    bytes.extend_from_slice(b"7777");
    let total = u64::try_from(bytes.len()).unwrap();
    bytes[8..16].copy_from_slice(&total.to_be_bytes());
    assert!(decode_grib_message(&bytes).is_err());
}

#[test]
fn budgets_cover_a_global_quarter_degree_grid() {
    // 1440 × 721 points: the budget must admit the real grids with headroom.
    const { assert!(MAX_GRID_POINTS >= 1440 * 721) };
    assert!(MAX_MESSAGE_BYTES >= fixture(GFS_U).len() * 4);
}
