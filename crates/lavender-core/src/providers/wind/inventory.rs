//! Which model run and forecast step to request, where it lives, and which byte ranges of
//! the GRIB2 file hold the wanted fields (from the GFS `.idx` / IFS `.index` inventories).

use lavender_domain::WeatherOverlay;
use url::Url;

use crate::providers::ProviderError;

/// Select the latest GFS cycle available given reference epoch milliseconds.
#[must_use]
pub fn select_latest_gfs_cycle(now_ms: i64, lag_ms: i64) -> (String, u32) {
    latest_six_hourly_cycle(now_ms, lag_ms)
}

/// Select the latest IFS cycle available given reference epoch milliseconds.
#[must_use]
pub fn select_latest_ifs_cycle(now_ms: i64, lag_ms: i64) -> (String, u32) {
    latest_six_hourly_cycle(now_ms, lag_ms)
}

fn latest_six_hourly_cycle(now_ms: i64, lag_ms: i64) -> (String, u32) {
    let shifted = now_ms.saturating_sub(lag_ms);
    let ts = jiff::Timestamp::from_millisecond(shifted).unwrap_or(jiff::Timestamp::UNIX_EPOCH);
    let dt = ts.to_zoned(jiff::tz::TimeZone::UTC).datetime();
    let date = format!("{:04}{:02}{:02}", dt.year(), dt.month(), dt.day());
    let hour = ((u32::try_from(dt.hour()).unwrap_or(0)) / 6) * 6;
    (date, hour)
}

/// Round hours elapsed since GFS cycle run time to the nearest published step.
/// Hourly through 120 h, then 3-hourly through 384 h.
#[must_use]
pub fn nearest_gfs_step(hours: f64) -> u32 {
    let val = if hours.is_finite() { hours.clamp(0.0, 384.0) } else { 0.0 };
    let step =
        if val <= 120.0 { nearest_integer(val, 120) } else { nearest_integer(val / 3.0, 128) * 3 };
    step.min(384)
}

/// Round hours elapsed since IFS cycle run time to nearest published step.
/// 3-hourly through 144 h, then 6-hourly to 360 h (for 00/12z) or 144 h (for 06/18z).
#[must_use]
pub fn nearest_ifs_step(hours: f64, cycle_hour: u32) -> u32 {
    let max = if cycle_hour == 6 || cycle_hour == 18 { 144 } else { 360 };
    let val = if hours.is_finite() && hours > 0.0 { hours } else { 0.0 };
    let step = if val <= 144.0 {
        nearest_integer(val / 3.0, max / 3) * 3
    } else {
        nearest_integer(val / 6.0, max / 6) * 6
    };
    step.min(max)
}

// Published forecast steps are a small bounded integer range. Comparing half-step
// thresholds preserves round-to-nearest semantics without a lossy float-to-int cast.
fn nearest_integer(value: f64, maximum: u32) -> u32 {
    (0..=maximum).take_while(|step| f64::from(*step) <= value + 0.5).last().unwrap_or(0)
}

pub(super) fn cycle_timestamp(
    date: &str,
    hour: u32,
    source_name: &'static str,
) -> Result<jiff::Timestamp, ProviderError> {
    let parse_error = |reason: String| ProviderError::Parse { source_name, reason };
    let hour = i8::try_from(hour).map_err(|error| parse_error(error.to_string()))?;
    let year = date_part(date, 0..4, source_name)?;
    let month = date_part(date, 4..6, source_name)?;
    let day = date_part(date, 6..8, source_name)?;
    jiff::civil::Date::new(year, month, day)
        .map_err(|error| parse_error(error.to_string()))?
        .at(hour, 0, 0, 0)
        .to_zoned(jiff::tz::TimeZone::UTC)
        .map(|zoned| zoned.timestamp())
        .map_err(|error| parse_error(error.to_string()))
}

fn date_part<T: std::str::FromStr>(
    date: &str,
    range: std::ops::Range<usize>,
    source_name: &'static str,
) -> Result<T, ProviderError> {
    date.get(range).and_then(|text| text.parse().ok()).ok_or_else(|| ProviderError::Parse {
        source_name,
        reason: format!("invalid cycle date {date}"),
    })
}

pub(super) fn elapsed_hours(now_ms: i64, run_ms: i64) -> f64 {
    // Only the model's maximum 384-hour horizon matters when choosing a step.
    let elapsed_ms = now_ms.saturating_sub(run_ms).clamp(0, 384 * 3_600_000);
    f64::from(u32::try_from(elapsed_ms).unwrap_or(0)) / 3_600_000.0
}

/// Build GFS object URLs for a cycle and forecast hour.
#[must_use]
pub fn gfs_urls(base: &Url, date: &str, hour: u32, forecast_hour: u32) -> (Url, Url) {
    let hh = format!("{hour:02}");
    let fff = format!("{forecast_hour:03}");
    let key = format!("gfs.{date}/{hh}/atmos/gfs.t{hh}z.pgrb2.0p25.f{fff}");
    let grib_url = base.join(&key).unwrap_or_else(|_| base.clone());
    let idx_url = base.join(&format!("{key}.idx")).unwrap_or_else(|_| base.clone());
    (grib_url, idx_url)
}

/// Build IFS object URLs for a cycle and forecast hour.
#[must_use]
pub fn ifs_urls(base: &Url, date: &str, hour: u32, forecast_hour: u32) -> (Url, Url) {
    let hh = format!("{hour:02}");
    let stem = format!("{date}/{hh}z/ifs/0p25/oper/{date}{hh}0000-{forecast_hour}h-oper-fc");
    let grib_url = base.join(&format!("{stem}.grib2")).unwrap_or_else(|_| base.clone());
    let idx_url = base.join(&format!("{stem}.index")).unwrap_or_else(|_| base.clone());
    (grib_url, idx_url)
}

/// An inclusive HTTP byte range (`Range: bytes=start-end`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

/// The single-field GRIB2 messages one forecast variant needs.
#[derive(Debug, Clone)]
pub struct WindMessageRanges {
    pub u: ByteRange,
    pub v: ByteRange,
    pub scalar: Option<ByteRange>,
}

#[derive(Debug, Clone)]
pub struct GfsIndexEntry {
    pub index: usize,
    pub offset: u64,
    pub variable: String,
    pub level: String,
}

/// Parse a GFS `.idx` inventory text into ordered message records.
///
/// # Errors
/// Returns [`ProviderError::Parse`] when no valid lines are present.
pub fn parse_gfs_idx(text: &str) -> Result<Vec<GfsIndexEntry>, ProviderError> {
    let mut messages = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() < 6 {
            continue;
        }
        let Ok(index) = parts[0].parse::<usize>() else {
            continue;
        };
        let Ok(offset) = parts[1].parse::<u64>() else {
            continue;
        };
        let variable = parts[3].trim();
        let level = parts[4].trim();
        if variable.is_empty() || level.is_empty() {
            continue;
        }
        messages.push(GfsIndexEntry {
            index,
            offset,
            variable: variable.to_owned(),
            level: level.to_owned(),
        });
    }
    if messages.is_empty() {
        return Err(ProviderError::Parse {
            source_name: "noaa-gfs",
            reason: "malformed GFS index".into(),
        });
    }
    Ok(messages)
}

/// Extract GFS byte ranges for 10 m wind and optional companion scalar.
///
/// # Errors
/// Returns [`ProviderError::Parse`] if required variables are missing.
pub fn gfs_wind_ranges(
    entries: &[GfsIndexEntry],
    overlay: WeatherOverlay,
) -> Result<WindMessageRanges, ProviderError> {
    let find_range = |var: &str, lvl: &str| -> Result<ByteRange, ProviderError> {
        let idx =
            entries.iter().position(|e| e.variable == var && e.level == lvl).ok_or_else(|| {
                ProviderError::Parse {
                    source_name: "noaa-gfs",
                    reason: format!("missing {var} at {lvl}"),
                }
            })?;
        let cur = &entries[idx];
        let next = entries.get(idx + 1).ok_or_else(|| ProviderError::Parse {
            source_name: "noaa-gfs",
            reason: format!("missing message following {var} at {lvl}"),
        })?;
        if next.offset <= cur.offset {
            return Err(ProviderError::Parse {
                source_name: "noaa-gfs",
                reason: format!("invalid offsets for {var} at {lvl}"),
            });
        }
        Ok(ByteRange { start: cur.offset, end: next.offset - 1 })
    };

    let u = find_range("UGRD", "10 m above ground")?;
    let v = find_range("VGRD", "10 m above ground")?;
    let scalar = match overlay {
        WeatherOverlay::None => None,
        WeatherOverlay::Temperature => Some(find_range("TMP", "2 m above ground")?),
        WeatherOverlay::Pressure => Some(find_range("PRMSL", "mean sea level")?),
    };
    Ok(WindMessageRanges { u, v, scalar })
}

#[derive(Debug, Clone)]
pub struct IfsIndexEntry {
    pub param: String,
    pub offset: u64,
    pub length: u64,
}

/// Parse the IFS JSON Lines inventory into `{param, offset, length}` records.
///
/// # Errors
/// Returns [`ProviderError::Parse`] when no valid lines are present.
pub fn parse_ifs_index(text: &str) -> Result<Vec<IfsIndexEntry>, ProviderError> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(param) = v.get("param").and_then(|p| p.as_str()) else {
            continue;
        };
        let Some(offset) = v.get("_offset").and_then(serde_json::Value::as_u64) else {
            continue;
        };
        let Some(length) = v.get("_length").and_then(serde_json::Value::as_u64) else {
            continue;
        };
        entries.push(IfsIndexEntry { param: param.to_owned(), offset, length });
    }
    if entries.is_empty() {
        return Err(ProviderError::Parse {
            source_name: "ecmwf-ifs",
            reason: "malformed IFS index".into(),
        });
    }
    Ok(entries)
}

/// Extract IFS byte ranges for 10 m wind and optional companion scalar.
///
/// # Errors
/// Returns [`ProviderError::Parse`] if required variables are missing.
pub fn ifs_wind_ranges(
    entries: &[IfsIndexEntry],
    overlay: WeatherOverlay,
) -> Result<WindMessageRanges, ProviderError> {
    let find_range = |param: &str| -> Result<ByteRange, ProviderError> {
        let entry = entries.iter().find(|e| e.param == param).ok_or_else(|| {
            ProviderError::Parse { source_name: "ecmwf-ifs", reason: format!("missing {param}") }
        })?;
        let end = entry.offset.checked_add(entry.length).filter(|_| entry.length > 0);
        let Some(end) = end else {
            return Err(ProviderError::Parse {
                source_name: "ecmwf-ifs",
                reason: format!("invalid length for {param}"),
            });
        };
        Ok(ByteRange { start: entry.offset, end: end - 1 })
    };

    let u = find_range("10u")?;
    let v = find_range("10v")?;
    let scalar = match overlay {
        WeatherOverlay::None => None,
        WeatherOverlay::Temperature => Some(find_range("2t")?),
        WeatherOverlay::Pressure => Some(find_range("msl")?),
    };
    Ok(WindMessageRanges { u, v, scalar })
}
