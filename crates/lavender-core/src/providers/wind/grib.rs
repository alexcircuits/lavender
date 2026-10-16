//! GRIB2 checks and decoding in pure Rust (the `grib` crate; no ecCodes C library).
//!
//! # Packing templates
//!
//! Verified on real messages fetched with the provider's own byte ranges (2026-10-15,
//! `tests/wind_grib.rs`, reference values from ecCodes 2.50.0):
//!
//! - NOAA GFS 0.25° `pgrb2` 10 m U/V: data representation template **5.3** (complex
//!   packing with second-order spatial differencing).
//! - ECMWF IFS open data 0.25° 10 m U/V: template **5.42** (CCSDS / AEC, decoded by the
//!   pure-Rust `rust-aec` backend).
//!
//! Templates 5.0 (simple) and 5.2 (complex) are accepted as well. Every other template is
//! refused with an error that names it (for example `5.40`), before any decoding; what to
//! do then is on [`SUPPORTED_DATA_TEMPLATES`].
//!
//! # Field check
//!
//! The provider picks messages by their inventory entry, then [`decode_forecast_field`]
//! checks that the message itself (Sections 0 and 4) describes the field asked for, so an
//! inventory that drifted from its object cannot turn V, another level or another parameter
//! into U wind.
//!
//! # Memory per decode
//!
//! Everything a header could inflate is checked against the bytes actually received
//! before the decoder runs: a message is at most [`MAX_MESSAGE_BYTES`], every section must
//! lie inside it, and the grid may hold at most [`MAX_GRID_POINTS`] points with at most 32
//! bits per packed value. A decode therefore holds the message, copies of its sections
//! (≤ message size), at most `MAX_GRID_POINTS` × 4 bytes of output, and at most
//! `MAX_GRID_POINTS` × 4 bytes of CCSDS samples or × 8 bytes for the largest complex-packing
//! group: about 40 MiB in the worst case (estimate). Measured on 2026-10-15, the whole test
//! process peaked at 19 MB while decoding the four real 0.25° fixtures one after another,
//! and at 3–6 MB while rejecting headers that claim up to 2³¹ points.

use std::io::Cursor;

use crate::providers::ProviderError;

/// Largest GRIB2 message (one field) that is fetched or decoded.
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
/// Largest grid accepted: a global 0.25° grid has 1440 × 721 = 1 038 240 points.
pub const MAX_GRID_POINTS: usize = 2_000_000;
/// Data representation templates (code table 5.0) the build can decode. Real upstream
/// messages use 5.3 (NOAA GFS) and 5.42 (ECMWF IFS), checked against ecCodes on
/// 2026-10-15 fixtures; 5.0 and 5.2 are accepted too.
///
/// # Fallback when an upstream switches to an unsupported template
///
/// The message is refused with an error naming the template (for example
/// `unsupported data representation template 5.40`). The manifest keeps serving the last
/// good generation marked stale, or reports `unavailable`, and the source's health shows
/// the error. The fix, in order of preference:
///
/// 1. 5.40 (JPEG 2000): enable the `grib` feature `jpeg2000-unpack-with-hayro` (pure Rust);
///    5.41 (PNG): enable `png-unpack-with-png-crate`. Add the template number here and a
///    real fixture with ecCodes reference values (`tests/fixtures/wind/PROVENANCE.md`).
/// 2. Otherwise, fetch the same field from a mirror that keeps a supported packing (GFS is
///    also on NOMADS; ECMWF open data is mirrored on AWS, Azure and Google Cloud).
/// 3. Linking ecCodes through FFI is the last resort: it brings back the C library this
///    migration removed (ENG-955) and needs a dependency and licence review first.
pub const SUPPORTED_DATA_TEMPLATES: [u16; 4] = [0, 2, 3, 42];

const MAX_BITS_PER_VALUE: u8 = 32;
const INDICATOR_LEN: usize = 16;
const END_MARKER: &[u8; 4] = b"7777";
const SECTION_HEADER_LEN: usize = 5;
const NO_BITMAP: u8 = 255;
/// Discipline 0 (code table 0.0): meteorological products.
const METEOROLOGICAL: u8 = 0;
/// Fixed surface types (code table 4.5).
const HEIGHT_ABOVE_GROUND: u8 = 103;
const MEAN_SEA_LEVEL: u8 = 101;
/// Product templates 4.0–4.15 place the parameter and the first fixed surface at the same
/// octets (10–28); later templates (chemical constituents, aerosols, …) shift them.
const LAST_PLAIN_PRODUCT_TEMPLATE: u16 = 15;
/// Scanning mode 0: rows run west to east, starting at the northernmost row.
const NORTH_TO_SOUTH_ROWS: u8 = 0;

/// The facts Lavender checks in a message before decoding it.
#[derive(Debug, Clone, PartialEq)]
pub struct GribHeader {
    /// What the message says it holds (Sections 0 and 4).
    pub field: GribField,
    /// Data representation template number (Section 5), for example 3 or 42.
    pub data_template: u16,
    /// Number of grid points (Section 3).
    pub grid_points: usize,
    /// Number of packed values (Section 5).
    pub packed_values: usize,
}

/// A message's own description of its field (WMO code tables 0.0, 4.1, 4.2 and 4.5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GribField {
    /// Discipline; 0 is meteorological products.
    pub discipline: u8,
    /// Parameter category and number, for example `(2, 2)` for eastward wind.
    pub parameter: (u8, u8),
    /// Type of the first fixed surface: 103 height above ground, 101 mean sea level.
    pub surface: u8,
    /// Value of that surface (metres for 103), when the message gives one.
    pub surface_value: Option<f64>,
}

impl std::fmt::Display for GribField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (category, number) = self.parameter;
        write!(f, "parameter {}.{category}.{number} on surface {}", self.discipline, self.surface)?;
        match self.surface_value {
            Some(value) => write!(f, " at {value}"),
            None => Ok(()),
        }
    }
}

/// The fields the wind provider fetches, as GRIB2 encodes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForecastField {
    /// GFS `UGRD`, IFS `10u`: parameter 0.2.2 at 10 m above ground.
    EastwardWind10m,
    /// GFS `VGRD`, IFS `10v`: parameter 0.2.3 at 10 m above ground.
    NorthwardWind10m,
    /// GFS `TMP`, IFS `2t`: parameter 0.0.0 at 2 m above ground.
    Temperature2m,
    /// GFS `PRMSL` (0.3.1) or IFS `msl` (0.3.0), at mean sea level.
    PressureMsl,
}

impl ForecastField {
    /// Whether a message's own description is this field.
    #[must_use]
    pub fn matches(self, field: &GribField) -> bool {
        let at_height = |metres: f64| {
            field.surface == HEIGHT_ABOVE_GROUND
                && field.surface_value.is_some_and(|value| (value - metres).abs() < 1e-6)
        };
        field.discipline == METEOROLOGICAL
            && match self {
                Self::EastwardWind10m => field.parameter == (2, 2) && at_height(10.0),
                Self::NorthwardWind10m => field.parameter == (2, 3) && at_height(10.0),
                Self::Temperature2m => field.parameter == (0, 0) && at_height(2.0),
                Self::PressureMsl => {
                    matches!(field.parameter, (3, 0 | 1)) && field.surface == MEAN_SEA_LEVEL
                }
            }
    }
}

impl std::fmt::Display for ForecastField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::EastwardWind10m => "10 m eastward wind",
            Self::NorthwardWind10m => "10 m northward wind",
            Self::Temperature2m => "2 m temperature",
            Self::PressureMsl => "mean sea level pressure",
        })
    }
}

/// One decoded field on a regular latitude/longitude grid, rows north to south.
#[derive(Debug, Clone)]
pub struct DecodedMessage {
    pub ni: usize,
    pub nj: usize,
    /// Longitude of the first point, degrees east.
    pub lo1: f64,
    /// Latitude of the first (northernmost) row, degrees.
    pub la1: f64,
    pub di: f64,
    pub dj: f64,
    pub values: Vec<f32>,
}

fn invalid(reason: impl std::fmt::Display) -> ProviderError {
    ProviderError::parse("GRIB2 upstream", &reason)
}

/// Checks the structure of one single-field GRIB2 message and every size its header
/// claims, without decoding it.
///
/// # Errors
/// Returns [`ProviderError::Parse`] for anything Lavender cannot decode safely: oversized
/// or truncated messages, sections outside the message, grids above the point budget,
/// unsupported grid, product or data templates, or bitmaps.
pub fn read_grib_header(bytes: &[u8]) -> Result<GribHeader, ProviderError> {
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(invalid(format!(
            "message exceeds the {} MiB limit",
            MAX_MESSAGE_BYTES / (1024 * 1024)
        )));
    }
    if bytes.len() < INDICATOR_LEN + END_MARKER.len() {
        return Err(invalid("message is too short"));
    }
    if &bytes[..4] != b"GRIB" {
        return Err(invalid("missing the GRIB indicator"));
    }
    if bytes[7] != 2 {
        return Err(invalid("only GRIB edition 2 is supported"));
    }
    let total = u64::from_be_bytes(bytes[8..16].try_into().map_err(invalid)?);
    if usize::try_from(total).ok() != Some(bytes.len()) {
        return Err(invalid(format!(
            "message length {total} does not match the {} bytes received",
            bytes.len()
        )));
    }
    if &bytes[bytes.len() - END_MARKER.len()..] != END_MARKER {
        return Err(invalid("missing the 7777 end section"));
    }
    let sections = Sections::find(bytes)?;
    let field = product_field(bytes[6], sections.product)?;
    let grid_points = grid_points(sections.grid)?;
    let (data_template, packed_values) = representation(sections.representation, grid_points)?;
    if sections.bitmap.get(5) != Some(&NO_BITMAP) {
        return Err(invalid("bitmap-masked fields are not supported"));
    }
    Ok(GribHeader { field, data_template, grid_points, packed_values })
}

/// Sections 3 to 6 of a single-field message (1 and 7 must be present too).
struct Sections<'a> {
    grid: &'a [u8],
    product: &'a [u8],
    representation: &'a [u8],
    bitmap: &'a [u8],
}

impl<'a> Sections<'a> {
    fn find(bytes: &'a [u8]) -> Result<Self, ProviderError> {
        let body_end = bytes.len() - END_MARKER.len();
        let mut found: [Option<&[u8]>; 8] = [None; 8];
        let mut previous = 0u8;
        let mut offset = INDICATOR_LEN;
        while offset < body_end {
            let header = bytes
                .get(offset..offset + SECTION_HEADER_LEN)
                .filter(|_| offset + SECTION_HEADER_LEN <= body_end)
                .ok_or_else(|| invalid("section header runs past the end of the message"))?;
            let number = header[4];
            let claimed = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
            let end = usize::try_from(claimed)
                .ok()
                .filter(|length| *length >= SECTION_HEADER_LEN)
                .and_then(|length| offset.checked_add(length))
                .filter(|end| *end <= body_end)
                .ok_or_else(|| {
                    invalid(format!("section {number} claims {claimed} bytes outside the message"))
                })?;
            if !(1..=7).contains(&number) || number <= previous {
                return Err(invalid(format!(
                    "unexpected section {number}; only single-field GRIB2 messages are supported"
                )));
            }
            found[usize::from(number)] = Some(&bytes[offset..end]);
            previous = number;
            offset = end;
        }
        match found {
            [
                _,
                Some(_),
                _,
                Some(grid),
                Some(product),
                Some(representation),
                Some(bitmap),
                Some(_),
            ] => Ok(Self { grid, product, representation, bitmap }),
            _ => Err(invalid("message lacks a required section")),
        }
    }
}

fn be_u32(section: &[u8], at: usize) -> Option<u32> {
    let bytes = section.get(at..at + 4)?;
    Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn be_u16(section: &[u8], at: usize) -> Option<u16> {
    let bytes = section.get(at..at + 2)?;
    Some(u16::from_be_bytes([bytes[0], bytes[1]]))
}

/// Section 4 (templates 4.0–4.15): the parameter and the first fixed surface.
fn product_field(discipline: u8, product: &[u8]) -> Result<GribField, ProviderError> {
    let short = || invalid("product definition section is too short");
    let template = be_u16(product, 7).ok_or_else(short)?;
    if template > LAST_PLAIN_PRODUCT_TEMPLATE {
        return Err(invalid(format!("unsupported product definition template 4.{template}")));
    }
    let category = *product.get(9).ok_or_else(short)?;
    let number = *product.get(10).ok_or_else(short)?;
    let surface = *product.get(22).ok_or_else(short)?;
    let scale = *product.get(23).ok_or_else(short)?;
    let scaled = be_u32(product, 24).ok_or_else(short)?;
    Ok(GribField {
        discipline,
        parameter: (category, number),
        surface,
        surface_value: surface_value(scale, scaled),
    })
}

/// A fixed surface's value, `scaled × 10^−scale`. Both are sign-and-magnitude numbers; all
/// bits set means the surface has no value (mean sea level, for example).
fn surface_value(scale: u8, scaled: u32) -> Option<f64> {
    if scale == u8::MAX || scaled == u32::MAX {
        return None;
    }
    let magnitude = f64::from(scaled & 0x7fff_ffff);
    let value = if scaled & 0x8000_0000 == 0 { magnitude } else { -magnitude };
    let exponent = i32::from(scale & 0x7f);
    let exponent = if scale & 0x80 == 0 { exponent } else { -exponent };
    Some(value / 10f64.powi(exponent))
}

/// Section 3: only template 3.0 (regular latitude/longitude) with no point list.
fn grid_points(grid: &[u8]) -> Result<usize, ProviderError> {
    let short = || invalid("grid definition section is too short");
    let template = be_u16(grid, 12).ok_or_else(short)?;
    if template != 0 {
        return Err(invalid(format!(
            "unsupported grid definition template 3.{template}; expected 3.0 (latitude/longitude)"
        )));
    }
    if grid.get(10) != Some(&0) {
        return Err(invalid("quasi-regular grids are not supported"));
    }
    let points = be_u32(grid, 6).ok_or_else(short)?;
    usize::try_from(points).ok().filter(|points| *points <= MAX_GRID_POINTS).ok_or_else(|| {
        invalid(format!("message claims {points} grid points; the limit is {MAX_GRID_POINTS}"))
    })
}

/// Section 5: a supported template, a packed value count within the grid, at most 32 bits
/// per value and, for complex packing, group counts and lengths the data could hold.
fn representation(section: &[u8], grid_points: usize) -> Result<(u16, usize), ProviderError> {
    let short = || invalid("data representation section is too short");
    let packed = be_u32(section, 5).ok_or_else(short)?;
    let template = be_u16(section, 9).ok_or_else(short)?;
    if !SUPPORTED_DATA_TEMPLATES.contains(&template) {
        return Err(invalid(format!(
            "unsupported data representation template 5.{template} (supported: 5.0, 5.2, 5.3, 5.42)"
        )));
    }
    let packed =
        usize::try_from(packed).ok().filter(|packed| *packed <= grid_points).ok_or_else(|| {
            invalid(format!("message claims {packed} packed values for {grid_points} grid points"))
        })?;
    let bits = *section.get(19).ok_or_else(short)?;
    if bits > MAX_BITS_PER_VALUE {
        return Err(invalid(format!("{bits} bits per value exceeds {MAX_BITS_PER_VALUE}")));
    }
    if template == 2 || template == 3 {
        check_complex_groups(section, packed)?;
    }
    Ok((template, packed))
}

/// The decoder expands each complex-packing group into memory at once, so no group may be
/// longer than the field (templates 5.2/5.3, octets 32–47).
fn check_complex_groups(section: &[u8], packed: usize) -> Result<(), ProviderError> {
    let short = || invalid("complex packing parameters are missing");
    let packed = u64::try_from(packed).map_err(invalid)?;
    let groups = u64::from(be_u32(section, 31).ok_or_else(short)?);
    if groups == 0 || groups > packed {
        return Err(invalid(format!("message claims {groups} groups for {packed} packed values")));
    }
    let reference = u64::from(be_u32(section, 37).ok_or_else(short)?);
    let increment = u64::from(*section.get(41).ok_or_else(short)?);
    let last = u64::from(be_u32(section, 42).ok_or_else(short)?);
    let length_bits = u32::from(*section.get(46).ok_or_else(short)?);
    let longest = 1u64
        .checked_shl(length_bits)
        .filter(|_| length_bits < 32)
        .and_then(|span| increment.checked_mul(span - 1))
        .and_then(|scaled| scaled.checked_add(reference))
        .unwrap_or(u64::MAX);
    if longest > packed || last > packed {
        return Err(invalid(format!(
            "complex packing group length exceeds the {packed} packed values"
        )));
    }
    Ok(())
}

/// Decodes one single-field GRIB2 message after [`read_grib_header`] accepted it.
///
/// # Errors
/// Returns [`ProviderError::Parse`] for messages [`read_grib_header`] refuses, for grids
/// that are not regular, north-to-south latitude/longitude grids, and for data the decoder
/// cannot unpack or that contains non-finite values.
pub fn decode_grib_message(bytes: &[u8]) -> Result<DecodedMessage, ProviderError> {
    let header = read_grib_header(bytes)?;
    decode_contained(bytes, &header)
}

/// Decodes a message after checking that it describes `expected` (module docs: field
/// check).
///
/// # Errors
/// As [`decode_grib_message`], and when the message holds another parameter or level.
pub fn decode_forecast_field(
    bytes: &[u8],
    expected: ForecastField,
) -> Result<DecodedMessage, ProviderError> {
    let header = read_grib_header(bytes)?;
    if !expected.matches(&header.field) {
        return Err(invalid(format!(
            "expected {expected}, but the message holds {}",
            header.field
        )));
    }
    decode_contained(bytes, &header)
}

fn decode_contained(bytes: &[u8], header: &GribHeader) -> Result<DecodedMessage, ProviderError> {
    // grib 0.19.1 slices packed data without bounds checks in places: a message that is
    // well-formed in size but inconsistent inside must be an error, not a crash.
    std::panic::catch_unwind(|| decode_checked(bytes, header))
        .unwrap_or_else(|_| Err(invalid("the decoder could not unpack the data section")))
}

fn decode_checked(bytes: &[u8], header: &GribHeader) -> Result<DecodedMessage, ProviderError> {
    let grib2 = grib::from_reader(Cursor::new(bytes)).map_err(invalid)?;
    let (_, submessage) = grib2.iter().next().ok_or_else(|| invalid("no data field found"))?;
    let geometry = lat_lon_geometry(&submessage, header.grid_points)?;
    let decoder = grib::Grib2SubmessageDecoder::from(submessage).map_err(invalid)?;
    let values: Vec<f32> = decoder.dispatch().map_err(invalid)?.take(header.grid_points).collect();
    if values.len() != header.grid_points {
        return Err(invalid(format!(
            "expected {} values, decoded {}",
            header.grid_points,
            values.len()
        )));
    }
    if !values.iter().all(|value| value.is_finite()) {
        return Err(invalid("decoded values contain non-finite numbers"));
    }
    Ok(DecodedMessage { values, ..geometry })
}

fn lat_lon_geometry<R>(
    submessage: &grib::SubMessage<'_, R>,
    grid_points: usize,
) -> Result<DecodedMessage, ProviderError> {
    let grib::GridDefinitionTemplateValues::Template0(template) =
        grib::GridDefinitionTemplateValues::try_from(submessage.grid_def()).map_err(invalid)?
    else {
        return Err(invalid("expected grid definition template 3.0"));
    };
    let lat_lon = &template.lat_lon;
    let grid = &lat_lon.grid;
    let ni = usize::try_from(grid.ni).map_err(invalid)?;
    let nj = usize::try_from(grid.nj).map_err(invalid)?;
    if ni < 2 || nj < 2 || ni.checked_mul(nj) != Some(grid_points) {
        return Err(invalid(format!(
            "grid {ni}×{nj} does not match its {grid_points} grid points"
        )));
    }
    if grid.initial_production_domain_basic_angle != 0 {
        return Err(invalid("grids with a non-default basic angle are not supported"));
    }
    if lat_lon.scanning_mode.0 != NORTH_TO_SOUTH_ROWS {
        return Err(invalid(format!(
            "unsupported scanning mode {:#04x}; expected west-to-east rows from north to south",
            lat_lon.scanning_mode.0
        )));
    }
    Ok(DecodedMessage {
        ni,
        nj,
        lo1: sign_magnitude_degrees(grid.first_point_lon),
        la1: f64::from(grid.first_point_lat) / 1e6,
        di: f64::from(lat_lon.i_direction_inc) / 1e6,
        dj: f64::from(lat_lon.j_direction_inc) / 1e6,
        values: Vec::new(),
    })
}

/// GRIB2 marks negative numbers by the most significant bit (sign and magnitude); the
/// crate reads longitudes as plain unsigned values, so `-180°` would otherwise read as
/// about 2327°. Units are micro-degrees.
fn sign_magnitude_degrees(raw: u32) -> f64 {
    let magnitude = f64::from(raw & 0x7fff_ffff) / 1e6;
    if raw & 0x8000_0000 == 0 { magnitude } else { -magnitude }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longitudes_use_sign_and_magnitude() {
        assert!((sign_magnitude_degrees(0x0aba_9500) - 180.0).abs() < 1e-9); // 180 000 000 µ°
        assert!((sign_magnitude_degrees(0x8000_0000 | 0x0aba_9500) + 180.0).abs() < 1e-9);
        assert!(sign_magnitude_degrees(0).abs() < f64::EPSILON);
    }

    #[test]
    fn surface_values_are_scaled_sign_and_magnitude_numbers() {
        assert_eq!(surface_value(0, 10), Some(10.0));
        assert_eq!(surface_value(1, 100), Some(10.0));
        assert_eq!(surface_value(0x81, 1), Some(10.0)); // scale factor −1
        assert_eq!(surface_value(0, 0x8000_0002), Some(-2.0));
        assert_eq!(surface_value(u8::MAX, u32::MAX), None);
    }

    #[test]
    fn group_lengths_are_bounded_by_the_field() {
        // Octets 32–47 of template 5.3: groups, width ref/bits, length ref/inc/last/bits.
        let mut section = vec![0u8; 49];
        section[31..35].copy_from_slice(&10u32.to_be_bytes());
        section[37..41].copy_from_slice(&1u32.to_be_bytes());
        section[41] = 1;
        section[42..46].copy_from_slice(&5u32.to_be_bytes());
        section[46] = 3; // longest group: 1 + 1 × 7 = 8
        assert!(check_complex_groups(&section, 100).is_ok());
        assert!(check_complex_groups(&section, 7).is_err());
        section[46] = 32;
        assert!(check_complex_groups(&section, 100).is_err());
        section[46] = 3;
        section[31..35].copy_from_slice(&0u32.to_be_bytes());
        assert!(check_complex_groups(&section, 100).is_err());
    }
}
