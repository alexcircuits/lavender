//! Coordinate recognition for search: decimal degrees, degrees-minutes-seconds with
//! hemispheres, and MGRS. Parsing never touches the network, so coordinates resolve
//! instantly even when no geocoder is configured.

use lavender_domain::GeoPoint;

use crate::mgrs::from_mgrs;

/// How an input was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateFormat {
    Decimal,
    Dms,
    Mgrs,
}

impl CoordinateFormat {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Decimal => "decimal",
            Self::Dms => "dms",
            Self::Mgrs => "mgrs",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParsedCoordinate {
    pub point: GeoPoint,
    pub format: CoordinateFormat,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    Number(f64),
    Hemisphere(char),
}

fn tokenize(input: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() || c == '.' || ((c == '-' || c == '+') && tokens_allow_sign(&tokens))
        {
            let mut s = String::new();
            s.push(c);
            chars.next();
            while let Some(&d) = chars.peek() {
                if d.is_ascii_digit() || d == '.' {
                    s.push(d);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(Token::Number(s.parse().ok()?));
        } else if matches!(c.to_ascii_uppercase(), 'N' | 'S' | 'E' | 'W') {
            tokens.push(Token::Hemisphere(c.to_ascii_uppercase()));
            chars.next();
        } else if c.is_whitespace()
            || matches!(c, ',' | ';' | ':' | '°' | 'º' | '\'' | '"' | '′' | '″' | '’' | '”')
        {
            chars.next();
        } else {
            return None;
        }
    }
    Some(tokens)
}

fn tokens_allow_sign(_tokens: &[Token]) -> bool {
    true
}

/// Degrees from 1–3 numbers (degrees, minutes, seconds). Minutes and seconds must be
/// non-negative and below 60, and only the last component may carry a fraction.
fn dms_value(parts: &[f64]) -> Option<f64> {
    let (&deg, rest) = parts.split_first()?;
    if rest.len() > 2 {
        return None;
    }
    let all_but_last = &parts[..parts.len() - 1];
    if all_but_last.iter().any(|v| v.fract() != 0.0) {
        return None;
    }
    let sign = if deg.is_sign_negative() { -1.0 } else { 1.0 };
    let mut value = deg.abs();
    for (i, &v) in rest.iter().enumerate() {
        if !(0.0..60.0).contains(&v) {
            return None;
        }
        value += v / if i == 0 { 60.0 } else { 3600.0 };
    }
    Some(sign * value)
}

fn from_hemispheres(tokens: &[Token]) -> Option<ParsedCoordinate> {
    let letters: Vec<(usize, char)> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| match t {
            Token::Hemisphere(h) => Some((i, *h)),
            Token::Number(_) => None,
        })
        .collect();
    let [(i1, h1), (i2, h2)] = letters[..] else {
        return None;
    };
    let numbers = |range: std::ops::Range<usize>| -> Vec<f64> {
        tokens[range]
            .iter()
            .filter_map(|t| match t {
                Token::Number(n) => Some(*n),
                Token::Hemisphere(_) => None,
            })
            .collect()
    };
    let prefix = i1 == 0;
    let (g1, g2) = if prefix {
        (numbers(1..i2), numbers(i2 + 1..tokens.len()))
    } else {
        if i2 != tokens.len() - 1 {
            return None;
        }
        (numbers(0..i1), numbers(i1 + 1..i2))
    };
    if g1.is_empty() || g2.is_empty() || g1.iter().chain(&g2).any(|v| v.is_sign_negative()) {
        return None;
    }
    let format = if g1.len() > 1 || g2.len() > 1 {
        CoordinateFormat::Dms
    } else {
        CoordinateFormat::Decimal
    };
    let v1 = dms_value(&g1)?;
    let v2 = dms_value(&g2)?;
    let signed = |v: f64, h: char| if matches!(h, 'S' | 'W') { -v } else { v };
    let (lat, lon) = match (h1, h2) {
        ('N' | 'S', 'E' | 'W') => (signed(v1, h1), signed(v2, h2)),
        ('E' | 'W', 'N' | 'S') => (signed(v2, h2), signed(v1, h1)),
        _ => return None,
    };
    GeoPoint::new(lat, lon).ok().map(|point| ParsedCoordinate { point, format })
}

/// Recognises coordinates in `input`. Unmarked decimal pairs are read as latitude then
/// longitude, the convention of every mainstream map product.
#[must_use]
pub fn parse_coordinates(input: &str) -> Option<ParsedCoordinate> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return None;
    }
    let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.chars().next().is_some_and(|c| c.is_ascii_digit())
        && compact.chars().any(|c| c.is_ascii_alphabetic())
        && let Ok(point) = from_mgrs(&compact)
    {
        return Some(ParsedCoordinate { point, format: CoordinateFormat::Mgrs });
    }
    let tokens = tokenize(trimmed)?;
    if tokens.iter().any(|t| matches!(t, Token::Hemisphere(_))) {
        return from_hemispheres(&tokens);
    }
    let numbers: Vec<f64> = tokens
        .iter()
        .filter_map(|t| match t {
            Token::Number(n) => Some(*n),
            Token::Hemisphere(_) => None,
        })
        .collect();
    let (lat, lon, format) = match numbers.len() {
        2 => (numbers[0], numbers[1], CoordinateFormat::Decimal),
        4 | 6 => {
            let half = numbers.len() / 2;
            (dms_value(&numbers[..half])?, dms_value(&numbers[half..])?, CoordinateFormat::Dms)
        }
        _ => return None,
    };
    GeoPoint::new(lat, lon).ok().map(|point| ParsedCoordinate { point, format })
}

/// `60.1699° N, 24.9384° E`
#[must_use]
pub fn format_decimal(point: &GeoPoint) -> String {
    let ns = if point.lat < 0.0 { 'S' } else { 'N' };
    let ew = if point.lon < 0.0 { 'W' } else { 'E' };
    format!("{:.4}° {ns}, {:.4}° {ew}", point.lat.abs(), point.lon.abs())
}

/// `60°10′12″ N 24°56′18″ E`
#[must_use]
pub fn format_dms(point: &GeoPoint) -> String {
    fn part(v: f64) -> String {
        let total = (v.abs() * 3600.0).round();
        let d = (total / 3600.0).floor();
        let m = ((total - d * 3600.0) / 60.0).floor();
        let s = total - d * 3600.0 - m * 60.0;
        format!("{d:.0}°{m:02.0}′{s:02.0}″")
    }
    let ns = if point.lat < 0.0 { 'S' } else { 'N' };
    let ew = if point.lon < 0.0 { 'W' } else { 'E' };
    format!("{} {ns} {} {ew}", part(point.lat), part(point.lon))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geodesy::distance_m;
    use proptest::prelude::*;

    fn close(parsed: Option<ParsedCoordinate>, lat: f64, lon: f64, format: CoordinateFormat) {
        let parsed = parsed.expect("should parse");
        assert_eq!(parsed.format, format);
        assert!((parsed.point.lat - lat).abs() < 1e-4, "lat {}", parsed.point.lat);
        assert!((parsed.point.lon - lon).abs() < 1e-4, "lon {}", parsed.point.lon);
    }

    #[test]
    fn decimal_pairs() {
        close(parse_coordinates("60.1699, 24.9384"), 60.1699, 24.9384, CoordinateFormat::Decimal);
        close(
            parse_coordinates("-33.8568 151.2153"),
            -33.8568,
            151.2153,
            CoordinateFormat::Decimal,
        );
        close(parse_coordinates("60.17N 24.94E"), 60.17, 24.94, CoordinateFormat::Decimal);
        close(parse_coordinates("33.86 s, 151.21 e"), -33.86, 151.21, CoordinateFormat::Decimal);
    }

    #[test]
    fn dms_forms() {
        let helsinki = (60.0 + 10.0 / 60.0 + 12.0 / 3600.0, 24.0 + 56.0 / 60.0 + 18.0 / 3600.0);
        close(
            parse_coordinates("60°10'12\"N 24°56'18\"E"),
            helsinki.0,
            helsinki.1,
            CoordinateFormat::Dms,
        );
        close(
            parse_coordinates("60°10′12″ N, 24°56′18″ E"),
            helsinki.0,
            helsinki.1,
            CoordinateFormat::Dms,
        );
        close(
            parse_coordinates("N 60 10 12 E 24 56 18"),
            helsinki.0,
            helsinki.1,
            CoordinateFormat::Dms,
        );
        close(parse_coordinates("E24°56.3' N60°10.2'"), 60.17, 24.938_333, CoordinateFormat::Dms);
        close(
            parse_coordinates("60 10 12 24 56 18"),
            helsinki.0,
            helsinki.1,
            CoordinateFormat::Dms,
        );
    }

    #[test]
    fn mgrs_form() {
        let parsed = parse_coordinates("18S UJ 23390 07393").unwrap();
        assert_eq!(parsed.format, CoordinateFormat::Mgrs);
        assert!(distance_m(&parsed.point, &GeoPoint::new(38.897_676, -77.036_548).unwrap()) < 1.5);
    }

    #[test]
    fn rejects_non_coordinates() {
        for s in [
            "Helsinki",
            "",
            "91, 0",
            "60 70 00 N 24 E",
            "1 2 3",
            "N 60 N 24",
            "SAS123",
            "60.1.2, 3",
        ] {
            assert!(parse_coordinates(s).is_none(), "{s}");
        }
    }

    #[test]
    fn formats() {
        let p = GeoPoint::new(60.17, -24.94).unwrap();
        assert_eq!(format_decimal(&p), "60.1700° N, 24.9400° W");
        assert_eq!(format_dms(&GeoPoint::new(60.17, 24.9384).unwrap()), "60°10′12″ N 24°56′18″ E");
    }

    proptest! {
        #[test]
        fn dms_label_round_trips(lat in -89.0f64..89.0, lon in -179.0f64..179.0) {
            let p = GeoPoint::new(lat, lon).unwrap();
            let parsed = parse_coordinates(&format_dms(&p)).unwrap();
            prop_assert!(distance_m(&parsed.point, &p) < 40.0);
            let parsed = parse_coordinates(&format_decimal(&p)).unwrap();
            prop_assert!(distance_m(&parsed.point, &p) < 15.0);
        }
    }
}
