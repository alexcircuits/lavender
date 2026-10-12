//! UTM and MGRS on the WGS84 ellipsoid (Snyder's transverse Mercator series; accurate to
//! well under a metre inside a zone). Polar regions (UPS) are not supported: MGRS bands
//! cover latitudes -80° to 84°.

use lavender_domain::{DomainError, GeoPoint};

const A: f64 = 6_378_137.0;
const F: f64 = 1.0 / 298.257_223_563;
const K0: f64 = 0.9996;
const FALSE_EASTING: f64 = 500_000.0;
const FALSE_NORTHING_SOUTH: f64 = 10_000_000.0;
const BANDS: &[u8; 20] = b"CDEFGHJKLMNPQRSTUVWX";
const ROW_LETTERS: &[u8; 20] = b"ABCDEFGHJKLMNPQRSTUV";
const COLUMN_SETS: [&[u8; 8]; 3] = [b"ABCDEFGH", b"JKLMNPQR", b"STUVWXYZ"];

fn e2() -> f64 {
    F * (2.0 - F)
}

fn ep2() -> f64 {
    e2() / (1.0 - e2())
}

/// A UTM coordinate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Utm {
    pub zone: u8,
    pub northern: bool,
    pub easting: f64,
    pub northing: f64,
}

/// The UTM zone for a position, including the Norway and Svalbard exceptions.
#[must_use]
pub fn utm_zone(lat: f64, lon: f64) -> u8 {
    // floor((lon + 180) / 6) + 1 is in 1..=61 for valid longitudes; 61 only at lon = 180.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let mut zone = (((lon + 180.0) / 6.0).floor() as u8 + 1).min(60);
    if (56.0..64.0).contains(&lat) && (3.0..12.0).contains(&lon) {
        zone = 32;
    }
    if (72.0..=84.0).contains(&lat) {
        zone = match lon {
            l if (0.0..9.0).contains(&l) => 31,
            l if (9.0..21.0).contains(&l) => 33,
            l if (21.0..33.0).contains(&l) => 35,
            l if (33.0..42.0).contains(&l) => 37,
            _ => zone,
        };
    }
    zone
}

fn central_meridian(zone: u8) -> f64 {
    f64::from(zone) * 6.0 - 183.0
}

fn meridian_arc(phi: f64) -> f64 {
    let e2 = e2();
    let e4 = e2 * e2;
    let e6 = e4 * e2;
    A * ((1.0 - e2 / 4.0 - 3.0 * e4 / 64.0 - 5.0 * e6 / 256.0) * phi
        - (3.0 * e2 / 8.0 + 3.0 * e4 / 32.0 + 45.0 * e6 / 1024.0) * (2.0 * phi).sin()
        + (15.0 * e4 / 256.0 + 45.0 * e6 / 1024.0) * (4.0 * phi).sin()
        - (35.0 * e6 / 3072.0) * (6.0 * phi).sin())
}

/// Projects a position into a given UTM zone.
#[must_use]
#[allow(clippy::many_single_char_names)] // Names follow Snyder's published formulas.
pub fn to_utm_in_zone(point: &GeoPoint, zone: u8) -> Utm {
    let phi = point.lat.to_radians();
    let lambda = point.lon.to_radians();
    let lambda0 = central_meridian(zone).to_radians();
    let e2 = e2();
    let ep2 = ep2();
    let n = A / (1.0 - e2 * phi.sin().powi(2)).sqrt();
    let t = phi.tan().powi(2);
    let c = ep2 * phi.cos().powi(2);
    let a = phi.cos() * (lambda - lambda0);
    let m = meridian_arc(phi);
    let easting = K0
        * n
        * (a + (1.0 - t + c) * a.powi(3) / 6.0
            + (5.0 - 18.0 * t + t * t + 72.0 * c - 58.0 * ep2) * a.powi(5) / 120.0)
        + FALSE_EASTING;
    let mut northing = K0
        * (m + n
            * phi.tan()
            * (a * a / 2.0
                + (5.0 - t + 9.0 * c + 4.0 * c * c) * a.powi(4) / 24.0
                + (61.0 - 58.0 * t + t * t + 600.0 * c - 330.0 * ep2) * a.powi(6) / 720.0));
    let northern = point.lat >= 0.0;
    if !northern {
        northing += FALSE_NORTHING_SOUTH;
    }
    Utm { zone, northern, easting, northing }
}

/// Projects a position into its own UTM zone.
#[must_use]
pub fn to_utm(point: &GeoPoint) -> Utm {
    to_utm_in_zone(point, utm_zone(point.lat, point.lon))
}

/// Converts a UTM coordinate back to a position.
///
/// # Errors
/// Returns an error when the result is not a valid position.
#[allow(clippy::many_single_char_names)] // Names follow Snyder's published formulas.
pub fn from_utm(utm: &Utm) -> Result<GeoPoint, DomainError> {
    let e2 = e2();
    let ep2 = ep2();
    let e1 = (1.0 - (1.0 - e2).sqrt()) / (1.0 + (1.0 - e2).sqrt());
    let x = utm.easting - FALSE_EASTING;
    let y = if utm.northern { utm.northing } else { utm.northing - FALSE_NORTHING_SOUTH };
    let m = y / K0;
    let mu = m / (A * (1.0 - e2 / 4.0 - 3.0 * e2 * e2 / 64.0 - 5.0 * e2.powi(3) / 256.0));
    let phi1 = mu
        + (3.0 * e1 / 2.0 - 27.0 * e1.powi(3) / 32.0) * (2.0 * mu).sin()
        + (21.0 * e1 * e1 / 16.0 - 55.0 * e1.powi(4) / 32.0) * (4.0 * mu).sin()
        + (151.0 * e1.powi(3) / 96.0) * (6.0 * mu).sin()
        + (1097.0 * e1.powi(4) / 512.0) * (8.0 * mu).sin();
    let n1 = A / (1.0 - e2 * phi1.sin().powi(2)).sqrt();
    let t1 = phi1.tan().powi(2);
    let c1 = ep2 * phi1.cos().powi(2);
    let r1 = A * (1.0 - e2) / (1.0 - e2 * phi1.sin().powi(2)).powf(1.5);
    let d = x / (n1 * K0);
    let lat = phi1
        - (n1 * phi1.tan() / r1)
            * (d * d / 2.0
                - (5.0 + 3.0 * t1 + 10.0 * c1 - 4.0 * c1 * c1 - 9.0 * ep2) * d.powi(4) / 24.0
                + (61.0 + 90.0 * t1 + 298.0 * c1 + 45.0 * t1 * t1 - 252.0 * ep2 - 3.0 * c1 * c1)
                    * d.powi(6)
                    / 720.0);
    let lon = central_meridian(utm.zone).to_radians()
        + (d - (1.0 + 2.0 * t1 + c1) * d.powi(3) / 6.0
            + (5.0 - 2.0 * c1 + 28.0 * t1 - 3.0 * c1 * c1 + 8.0 * ep2 + 24.0 * t1 * t1)
                * d.powi(5)
                / 120.0)
            / phi1.cos();
    GeoPoint::new(lat.to_degrees(), lon.to_degrees())
}

fn band_letter(lat: f64) -> Option<u8> {
    if !(-80.0..=84.0).contains(&lat) {
        return None;
    }
    // (lat + 80) / 8 is in 0..=20 here; X (index 19) is 12° tall, so clamp.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let i = (((lat + 80.0) / 8.0).floor() as usize).min(19);
    Some(BANDS[i])
}

fn column_set(zone: u8) -> usize {
    usize::from((zone - 1) % 3)
}

fn row_offset(zone: u8) -> usize {
    if zone.is_multiple_of(2) { 5 } else { 0 }
}

/// Formats a position as MGRS with `digits` digits per axis (0–5; 5 = 1 m precision).
///
/// # Errors
/// Returns an error outside the MGRS latitude bands (polar regions).
pub fn to_mgrs(point: &GeoPoint, digits: u8) -> Result<String, DomainError> {
    let digits = digits.min(5);
    let band = band_letter(point.lat).ok_or(DomainError::OutOfRange {
        field: "lat",
        reason: "MGRS covers latitudes -80° to 84°".to_owned(),
    })?;
    let utm = to_utm(point);
    // Easting is 100 000..900 000 inside a zone, so the 100 km column is 1..=8.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let col = ((utm.easting / 100_000.0).floor() as usize).clamp(1, 8);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let row = (utm.northing / 100_000.0).floor() as usize;
    let col_letter = COLUMN_SETS[column_set(utm.zone)][col - 1];
    let row_letter = ROW_LETTERS[(row + row_offset(utm.zone)) % 20];
    let divisor = 10f64.powi(5 - i32::from(digits));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let e = ((utm.easting % 100_000.0) / divisor).floor() as u64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = ((utm.northing % 100_000.0) / divisor).floor() as u64;
    let square =
        format!("{}{}{}{}", utm.zone, band as char, col_letter as char, row_letter as char);
    if digits == 0 {
        // A bare 100 km square. (`{:00$}` would still print "0" for each axis, which reads
        // back as a 10 km square in the square's corner.)
        return Ok(square);
    }
    let width = usize::from(digits);
    Ok(format!("{square}{e:0width$}{n:0width$}"))
}

/// Converts a small table index to `f64` (indices here are below 20).
fn small(i: usize) -> f64 {
    f64::from(u8::try_from(i).unwrap_or(u8::MAX))
}

fn parse_err(reason: impl Into<String>) -> DomainError {
    DomainError::Parse { what: "MGRS", reason: reason.into() }
}

/// The latitude span of a band (X, the last, is 12° tall).
fn band_range(band_idx: usize) -> (f64, f64) {
    let south = -80.0 + 8.0 * small(band_idx);
    let north = if band_idx + 1 == BANDS.len() { 84.0 } else { south + 8.0 };
    (south, north)
}

/// The latitude extent of a UTM square. Lines of constant northing curve, so besides the
/// corners the extremes can lie where the square's north and south edges are closest to
/// the central meridian.
fn square_latitudes(
    zone: u8,
    northern: bool,
    (easting, northing): (f64, f64),
    size: f64,
) -> Result<(f64, f64), DomainError> {
    let near_meridian = FALSE_EASTING.clamp(easting, easting + size);
    let mut lats = Vec::with_capacity(6);
    for e in [easting, easting + size, near_meridian] {
        for n in [northing, northing + size] {
            lats.push(from_utm(&Utm { zone, northern, easting: e, northing: n })?.lat);
        }
    }
    let min = lats.iter().copied().fold(f64::INFINITY, f64::min);
    let max = lats.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    Ok((min, max))
}

/// Parses an MGRS string (spaces allowed) and returns the centre of the referenced square.
///
/// The referenced square must lie in the stated latitude band. A coarse square (for
/// example a bare 100 km square, `18SUJ`) may straddle a band edge and is accepted when
/// any part of it lies in the band; a 1 m square effectively has to match the band.
///
/// # Errors
/// Returns an error for malformed strings, unknown letters, odd digit counts, or a square
/// outside the stated band (for example `18SUL…`, which lies in band T).
pub fn from_mgrs(input: &str) -> Result<GeoPoint, DomainError> {
    let s: String = input.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase();
    let bytes = s.as_bytes();
    let zone_len = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if !(1..=2).contains(&zone_len) {
        return Err(parse_err("expected a 1–2 digit zone"));
    }
    let zone: u8 = s[..zone_len].parse().map_err(|_| parse_err("bad zone"))?;
    if !(1..=60).contains(&zone) {
        return Err(parse_err("zone must be 1–60"));
    }
    let rest = &bytes[zone_len..];
    let [band, col_letter, row_letter, digits @ ..] = rest else {
        return Err(parse_err("expected band and two square letters"));
    };
    let band_idx = BANDS.iter().position(|b| b == band).ok_or_else(|| parse_err("bad band"))?;
    if !digits.iter().all(u8::is_ascii_digit) || digits.len() % 2 != 0 || digits.len() > 10 {
        return Err(parse_err("expected an even number of digits (0–10)"));
    }
    let half = digits.len() / 2;
    let col = COLUMN_SETS[column_set(zone)]
        .iter()
        .position(|c| c == col_letter)
        .ok_or_else(|| parse_err("square column letter not valid in this zone"))?;
    let row_pos =
        ROW_LETTERS.iter().position(|r| r == row_letter).ok_or_else(|| parse_err("bad row"))?;
    let row = (row_pos + 20 - row_offset(zone)) % 20;

    let precision = 10f64.powi(5 - i32::try_from(half).map_err(|_| parse_err("digits"))?);
    let parse_num =
        |d: &[u8]| -> f64 { d.iter().fold(0.0, |acc, b| acc * 10.0 + f64::from(b - b'0')) };
    // Indices come from 8- and 20-letter tables, so the conversions are exact.
    let (col, row) = (small(col), small(row));
    let easting = (col + 1.0) * 100_000.0 + parse_num(&digits[..half]) * precision;
    let mut northing = row * 100_000.0 + parse_num(&digits[half..]) * precision;

    let northern = band_idx >= 10;
    let (band_south, band_north) = band_range(band_idx);
    let min_northing = to_utm_in_zone(
        &GeoPoint { lat: band_south, lon: central_meridian(zone), alt_m: None },
        zone,
    )
    .northing;
    // Bring the 2 000 km-periodic row northing up to the band's southern edge.
    while northing < min_northing - 100_000.0 {
        northing += 2_000_000.0;
    }
    let band_letter = *band as char;
    let (square_south, square_north) =
        square_latitudes(zone, northern, (easting, northing), precision)?;
    // A micro-degree of slack absorbs floating-point error in the projection round trip.
    if square_north < band_south - 1e-6 || square_south > band_north + 1e-6 {
        return Err(parse_err(format!(
            "the square lies outside latitude band {band_letter} ({band_south}° to {band_north}°)"
        )));
    }
    let centre = precision / 2.0;
    from_utm(&Utm { zone, northern, easting: easting + centre, northing: northing + centre })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geodesy::distance_m;
    use proptest::prelude::*;

    fn p(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    // Expected values were computed independently with Krüger's n-series (the formulation
    // on Wikipedia's UTM page): White House → 18N 323390.07 E 4307393.06 N; origin →
    // 31N 166021.44 E 0 N; Vienna → 33N 605004.39 E 5344997.70 N.
    #[test]
    fn white_house() {
        assert_eq!(to_mgrs(&p(38.897_676, -77.036_548), 5).unwrap(), "18SUJ2339007393");
        let back = from_mgrs("18SUJ2339007393").unwrap();
        assert!(distance_m(&back, &p(38.897_676, -77.036_548)) < 1.5);
    }

    #[test]
    fn origin() {
        assert_eq!(to_mgrs(&p(0.0, 0.0), 5).unwrap(), "31NAA6602100000");
    }

    #[test]
    fn zero_digits_is_the_bare_100_km_square() {
        assert_eq!(to_mgrs(&p(38.897_676, -77.036_548), 0).unwrap(), "18SUJ");
        let back = from_mgrs("18SUJ").unwrap();
        assert!(distance_m(&back, &p(38.897_676, -77.036_548)) < 75_000.0);
    }

    #[test]
    fn ten_kilometre_precision() {
        assert_eq!(to_mgrs(&p(48.249_49, 16.414_50), 1).unwrap(), "33UXP04");
        assert_eq!(to_mgrs(&p(48.249_49, 16.414_50), 5).unwrap(), "33UXP0500444997");
        let back = from_mgrs("33UXP04").unwrap();
        assert!(distance_m(&back, &p(48.249_49, 16.414_50)) < 10_000.0);
    }

    #[test]
    fn southern_hemisphere_and_spaces() {
        let sydney = p(-33.8568, 151.2153);
        let m = to_mgrs(&sydney, 5).unwrap();
        assert!(m.starts_with("56H"), "{m}");
        let spaced = format!("{} {} {} {}", &m[..3], &m[3..5], &m[5..10], &m[10..]);
        assert!(distance_m(&from_mgrs(&spaced).unwrap(), &sydney) < 1.5);
    }

    #[test]
    fn norway_and_svalbard_zones() {
        assert_eq!(utm_zone(60.0, 5.0), 32);
        assert_eq!(utm_zone(78.0, 15.0), 33);
        assert_eq!(utm_zone(78.0, 25.0), 35);
    }

    #[test]
    fn a_square_outside_its_stated_band_is_rejected() {
        // Review finding 11: "18SUL…" decoded to 40.70°N, which is band T, not S.
        let err = from_mgrs("18SUL2339007393").unwrap_err();
        assert!(err.to_string().contains("band S"), "{err}");
        let right = from_mgrs("18TUL2339007393").unwrap();
        assert_eq!(band_letter(right.lat), Some(b'T'));
        assert!((right.lat - 40.70).abs() < 0.05, "{}", right.lat);
    }

    #[test]
    fn coarse_squares_may_straddle_a_band_edge_but_fine_ones_may_not() {
        // 39.95°N lies in band S; its 100 km square reaches north of 40°, into band T.
        let point = p(39.95, -75.0);
        let coarse = to_mgrs(&point, 0).unwrap();
        assert!(coarse.starts_with("18S"), "{coarse}");
        let in_t = coarse.replacen("18S", "18T", 1);
        let (a, b) = (from_mgrs(&coarse).unwrap(), from_mgrs(&in_t).unwrap());
        assert!(distance_m(&a, &b) < 1.0, "same square, either band");
        let fine = to_mgrs(&point, 5).unwrap().replacen("18S", "18T", 1);
        assert!(from_mgrs(&fine).is_err(), "a 1 m square at 39.95°N is not in band T");
    }

    #[test]
    fn rejects_garbage_and_polar() {
        assert!(from_mgrs("hello").is_err());
        assert!(from_mgrs("18SUJ233").is_err());
        assert!(from_mgrs("61SUJ").is_err());
        assert!(from_mgrs("18SIJ").is_err());
        assert!(to_mgrs(&p(85.0, 0.0), 5).is_err());
    }

    proptest! {
        #[test]
        fn round_trip_within_a_metre(lat in -79.9f64..83.9, lon in -179.9f64..179.9) {
            let point = p(lat, lon);
            let m = to_mgrs(&point, 5).unwrap();
            let back = from_mgrs(&m).unwrap();
            prop_assert!(distance_m(&back, &point) < 1.5, "{} -> {} -> {:?}", lat, m, back);
        }
    }
}
