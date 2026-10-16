//! FIRMS area CSV: header checks, acquisition times and row-by-row normalisation.

use std::collections::HashMap;
use std::ops::RangeInclusive;

use jiff::Timestamp;
use lavender_domain::FireDetection;
use lavender_domain::text::clean;

use crate::providers::ProviderError;

const REQUIRED_HEADER_FIELDS: [&str; 6] =
    ["latitude", "longitude", "acq_date", "acq_time", "confidence", "frp"];

/// Cheap check whether text is actually FIRMS CSV.
/// HTML error pages and text errors like "Invalid `MAP_KEY`" return false.
#[must_use]
pub fn is_likely_csv(text: &str) -> bool {
    let trimmed = text.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('<') {
        return false;
    }
    let header_line = match trimmed.split_once('\n') {
        Some((first, _)) => first,
        None => trimmed,
    };
    let header_lower = header_line.trim().to_ascii_lowercase();
    let fields: Vec<&str> = header_lower.split(',').map(str::trim).collect();
    REQUIRED_HEADER_FIELDS.iter().all(|req| fields.contains(req))
}

/// Converts FIRMS `acq_date` ("YYYY-MM-DD") and `acq_time` (unpadded "HHMM", UTC)
/// into a UTC [`Timestamp`].
#[must_use]
pub fn parse_acquisition_time(date_str: &str, time_str: &str) -> Option<Timestamp> {
    let date_str = date_str.trim();
    let time_str = time_str.trim();
    if date_str.len() != 10 {
        return None;
    }
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year: i16 = parts[0].parse().ok()?;
    let month: i8 = parts[1].parse().ok()?;
    let day: i8 = parts[2].parse().ok()?;

    if time_str.is_empty() || time_str.len() > 4 || !time_str.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let padded = format!("{time_str:0>4}");
    let hour: i8 = padded[0..2].parse().ok()?;
    let minute: i8 = padded[2..4].parse().ok()?;
    if !(0..=23).contains(&hour) || !(0..=59).contains(&minute) {
        return None;
    }

    let date = jiff::civil::Date::new(year, month, day).ok()?;
    let time = jiff::civil::Time::new(hour, minute, 0, 0).ok()?;
    let dt = jiff::civil::DateTime::from_parts(date, time);
    dt.to_zoned(jiff::tz::TimeZone::UTC).ok().map(|z| z.timestamp())
}

/// Keeps records acquired within `[now - 24 h, now + 2 h]` (inclusive).
/// The 2 h forward slack absorbs upstream/local clock skew.
#[must_use]
pub fn filter_trailing_24h(detections: &[FireDetection], now: Timestamp) -> Vec<FireDetection> {
    let window = trailing_window(now);
    detections.iter().filter(|d| window.contains(&d.acquired_at)).cloned().collect()
}

/// The acquisition times served at `now`: `[now - 24 h, now + 2 h]`.
pub(super) fn trailing_window(now: Timestamp) -> RangeInclusive<Timestamp> {
    let oldest = now.checked_sub(jiff::SignedDuration::from_hours(24)).unwrap_or(now);
    let newest = now.checked_add(jiff::SignedDuration::from_hours(2)).unwrap_or(now);
    oldest..=newest
}

/// Parses a FIRMS CSV payload into normalised detections.
/// Tolerates CRLF, trailing newlines and skips malformed rows.
///
/// # Errors
/// Returns [`ProviderError::Parse`] if input is HTML or non-CSV error text.
pub fn parse_csv(body: &[u8]) -> Result<(Vec<FireDetection>, usize), ProviderError> {
    let text = std::str::from_utf8(body).map_err(|e| ProviderError::parse("NASA FIRMS", &e))?;
    if !is_likely_csv(text) {
        return Err(ProviderError::parse(
            "NASA FIRMS",
            &"non-CSV upstream response or invalid key",
        ));
    }

    let mut lines = text.lines();
    let header_line = loop {
        match lines.next() {
            Some(l) if !l.trim().is_empty() => break l,
            Some(_) => {}
            None => return Ok((Vec::new(), 0)),
        }
    };

    let header: Vec<String> =
        header_line.trim().to_ascii_lowercase().split(',').map(|s| s.trim().to_owned()).collect();

    let mut col = HashMap::new();
    for (i, name) in header.iter().enumerate() {
        col.insert(name.as_str(), i);
    }

    let i_lat = *col
        .get("latitude")
        .ok_or_else(|| ProviderError::parse("NASA FIRMS", &"missing latitude"))?;
    let i_lon = *col
        .get("longitude")
        .ok_or_else(|| ProviderError::parse("NASA FIRMS", &"missing longitude"))?;
    let i_frp =
        *col.get("frp").ok_or_else(|| ProviderError::parse("NASA FIRMS", &"missing frp"))?;
    let i_confidence = *col
        .get("confidence")
        .ok_or_else(|| ProviderError::parse("NASA FIRMS", &"missing confidence"))?;
    let i_acq_date = *col
        .get("acq_date")
        .ok_or_else(|| ProviderError::parse("NASA FIRMS", &"missing acq_date"))?;
    let i_acq_time = *col
        .get("acq_time")
        .ok_or_else(|| ProviderError::parse("NASA FIRMS", &"missing acq_time"))?;
    let i_brightness = col.get("bright_ti4").or_else(|| col.get("brightness")).copied();
    let i_daynight = col.get("daynight").copied();
    let i_satellite = col.get("satellite").copied();

    let mut detections = Vec::new();
    let mut dropped = 0;

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() < header.len() {
            dropped += 1;
            continue;
        }

        let lat: f64 = match parts[i_lat].trim().parse::<f64>() {
            Ok(v) if v.is_finite() && (-90.0..=90.0).contains(&v) => v,
            _ => {
                dropped += 1;
                continue;
            }
        };

        let lon: f64 = match parts[i_lon].trim().parse::<f64>() {
            Ok(v) if v.is_finite() && (-180.0..=180.0).contains(&v) => v,
            _ => {
                dropped += 1;
                continue;
            }
        };

        let frp: f64 = match parts[i_frp].trim().parse::<f64>() {
            Ok(v) if v.is_finite() && v >= 0.0 => v,
            _ => {
                dropped += 1;
                continue;
            }
        };

        let brightness: f64 = i_brightness
            .and_then(|i| parts.get(i))
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v >= 0.0)
            .unwrap_or(0.0);

        let Some(acquired_at) = parse_acquisition_time(parts[i_acq_date], parts[i_acq_time]) else {
            dropped += 1;
            continue;
        };

        let confidence = clean(parts[i_confidence].trim(), 16);
        let satellite =
            i_satellite.map_or(String::new(), |i| clean(parts.get(i).unwrap_or(&"").trim(), 16));
        let day_night =
            i_daynight.map_or(String::new(), |i| clean(parts.get(i).unwrap_or(&"").trim(), 8));

        detections.push(FireDetection {
            lat,
            lon,
            brightness,
            frp,
            confidence,
            satellite,
            acquired_at,
            day_night,
        });
    }

    Ok((detections, dropped))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_CSV: &str =
        include_str!("../../../../../legacy/src/data/fixtures/firms-viirs-noaa20-sample.csv");

    #[test]
    fn parses_sample_viirs_fixture() {
        let (detections, dropped) = parse_csv(SAMPLE_CSV.as_bytes()).unwrap();
        assert_eq!(detections.len(), 45);
        assert_eq!(dropped, 0);

        let first = &detections[0];
        assert!((first.lat - 38.99488).abs() < 1e-5);
        assert!((first.lon - -121.67046).abs() < 1e-5);
        assert!((first.brightness - 303.6).abs() < 1e-4);
        assert!((first.frp - 0.53).abs() < 1e-4);
        assert_eq!(first.confidence, "n");
        assert_eq!(first.satellite, "N20");
        assert_eq!(first.day_night, "N");
        assert_eq!(first.acquired_at.to_string(), "2026-07-16T10:06:00Z");
    }

    #[test]
    fn non_csv_is_rejected_as_parse_error() {
        assert!(parse_csv(b"Invalid MAP_KEY").is_err());
        assert!(parse_csv(b"<html><body>503 Service Unavailable</body></html>").is_err());
        assert!(parse_csv(b"").is_err());
    }

    #[test]
    fn empty_header_only_csv_produces_empty_result() {
        let text = b"latitude,longitude,acq_date,acq_time,confidence,frp\n";
        let (detections, dropped) = parse_csv(text).unwrap();
        assert_eq!(detections.len(), 0);
        assert_eq!(dropped, 0);
    }

    #[test]
    fn skips_malformed_and_truncated_rows() {
        let text = b"latitude,longitude,acq_date,acq_time,confidence,frp\n\
                     30.0,-97.0,2026-07-16,45,n,12.0\n\
                     30.0,-97.0,2026-07-16,1200\n\
                     bad_lat,-97.0,2026-07-16,1200,n,10.0\n\
                     30.0,bad_lon,2026-07-16,1200,n,10.0\n\
                     30.0,-97.0,bad_date,1200,n,10.0\n\
                     30.0,-97.0,2026-07-16,2400,n,10.0\n";
        let (detections, dropped) = parse_csv(text).unwrap();
        assert_eq!(detections.len(), 1);
        assert_eq!(dropped, 5);
        assert_eq!(detections[0].acquired_at.to_string(), "2026-07-16T00:45:00Z");
    }

    #[test]
    fn trailing_24h_window_and_forward_slack() {
        let text = b"latitude,longitude,acq_date,acq_time,confidence,frp\n\
                     30,-97,2026-07-15,1200,n,12\n\
                     30,-97,2026-07-15,1159,n,12\n\
                     30,-97,2026-07-16,1400,h,12\n\
                     30,-97,2026-07-16,1401,l,12\n";
        let (detections, _) = parse_csv(text).unwrap();
        assert_eq!(detections.len(), 4);

        let now: Timestamp = "2026-07-16T12:00:00Z".parse().unwrap();
        let filtered = filter_trailing_24h(&detections, now);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].acquired_at.to_string(), "2026-07-15T12:00:00Z");
        assert_eq!(filtered[1].acquired_at.to_string(), "2026-07-16T14:00:00Z");
    }

    #[test]
    fn recorded_rows_survive_interleaved_malformed_rows() {
        use std::fmt::Write;

        // Every recorded NOAA-20 row is followed by a row that must be skipped; CRLF
        // line ends and blank lines must not matter.
        let malformed = [
            "NaN,-121.6,303.6,0.39,0.36,2026-07-16,1006,N20,VIIRS,n,2.0NRT,290.7,0.5,N",
            "38.9,inf,303.6,0.39,0.36,2026-07-16,1006,N20,VIIRS,n,2.0NRT,290.7,0.5,N",
            "90.1,-121.6,303.6,0.39,0.36,2026-07-16,1006,N20,VIIRS,n,2.0NRT,290.7,0.5,N",
            "38.9,-180.01,303.6,0.39,0.36,2026-07-16,1006,N20,VIIRS,n,2.0NRT,290.7,0.5,N",
            "38.9,-121.6,303.6,0.39,0.36,2026-07-16,1006,N20,VIIRS,n,2.0NRT,290.7,-0.5,N",
            "38.9,-121.6,303.6,0.39,0.36,2026-07-16,1006,N20,VIIRS,n,2.0NRT,290.7,1e400,N",
            "38.9,-121.6,303.6,0.39,0.36,2026-07-16,1006,N20,VIIRS,n,2.0NRT,290.7,NaN,N",
            "38.9,-121.6,303.6,0.39,0.36,2026-02-30,1006,N20,VIIRS,n,2.0NRT,290.7,0.5,N",
            "38.9,-121.6,303.6,0.39,0.36,2026-07-16,960,N20,VIIRS,n,2.0NRT,290.7,0.5,N",
            "38.9,-121.6,303.6,0.39,0.36,2026-07-16,12345,N20,VIIRS,n,2.0NRT,290.7,0.5,N",
            "38.9,-121.6,303.6",
            ",,,,,,,,,,,,,",
        ];
        let mut lines = SAMPLE_CSV.lines();
        let mut text = format!("{}\r\n\r\n", lines.next().unwrap());
        let mut recorded = 0;
        for (i, row) in lines.enumerate() {
            write!(text, "{row}\r\n{}\r\n", malformed[i % malformed.len()]).unwrap();
            recorded += 1;
        }
        let (detections, dropped) = parse_csv(text.as_bytes()).unwrap();
        assert_eq!(detections, parse_csv(SAMPLE_CSV.as_bytes()).unwrap().0);
        assert_eq!(detections.len(), 45);
        assert_eq!(dropped, recorded);
    }

    #[test]
    fn boundary_coordinates_and_largest_finite_frp_are_valid() {
        let text = b"latitude,longitude,acq_date,acq_time,confidence,frp\n\
                     90,180,2026-07-16,0,n,1.7976931348623157e308\n\
                     -90,-180,2026-07-16,2359,n,0\n";
        let (detections, dropped) = parse_csv(text).unwrap();
        assert_eq!(dropped, 0);
        assert_eq!(detections.len(), 2);
        assert_eq!(detections[0].frp, f64::MAX);
    }
}
