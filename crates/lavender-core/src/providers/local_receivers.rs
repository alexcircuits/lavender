//! Local SDR and Mode-S/UAT ADS-B receiver provider (`dump1090`, `readsb`, `tar1090`, `dump978`).
//!
//! Validates local-only receiver tap addresses (loopback, `RFC1918` private IPv4, `localhost`,
//! and `*.local` mDNS names), parses `LOCAL_RECEIVER_FEEDS` configuration without ever exposing
//! raw receiver URLs to browser clients, normalizes `aircraft.json` payloads, and maintains a
//! thread-safe snapshot engine across configured feeds.

use std::collections::HashMap;
use std::sync::{PoisonError, RwLock};

use jiff::{SignedDuration, Timestamp};
use lavender_domain::GeoPoint;
use lavender_domain::entities::{
    LocalAdsbAircraft, LocalReceiverBand, LocalReceiverFeedStatus, LocalReceiverSnapshot,
};
use lavender_domain::text::clean_opt;
use serde_json::Value;
use url::Url;

use crate::CoreError;

/// Maximum number of local receiver feeds accepted from configuration.
pub const MAX_LOCAL_RECEIVER_FEEDS: usize = 8;

/// Maximum age in seconds of a feed's `now` clock before the feed is marked stale.
pub const LOCAL_RECEIVER_STALE_SECS: f64 = 10.0;

/// Maximum age in seconds since an aircraft's last message before it is dropped from snapshots.
pub const LOCAL_ADSB_MESSAGE_STALE_SECS: f64 = 60.0;

/// Parsed configuration for a single local receiver feed entry.
///
/// The validated `url` is retained only inside [`LocalReceiverEngine`] for backend polling and is
/// never copied into [`LocalReceiverFeedStatus`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalReceiverFeedConfig {
    /// Deterministic feed identifier (`"feed-1"`, `"feed-2"`, ...).
    pub id: String,
    /// Decoded frequency band when valid.
    pub band: Option<LocalReceiverBand>,
    /// Human-readable label (`"1090 MHz"`, `"978 MHz UAT"`, with `#1`/`#2` ordinals on repeats).
    pub label: String,
    /// Validated local `aircraft.json` URL when the entry passed all security checks.
    pub url: Option<String>,
    /// Rejection reason when the entry failed validation.
    pub reason: Option<String>,
}

impl LocalReceiverFeedConfig {
    /// Whether this feed entry passed validation and can be polled or ingested.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.url.is_some() && self.reason.is_none()
    }

    /// Converts this configuration entry into a public [`LocalReceiverFeedStatus`] without
    /// exposing the underlying receiver URL.
    #[must_use]
    pub fn to_status(&self, stale: bool, aircraft_count: usize) -> LocalReceiverFeedStatus {
        LocalReceiverFeedStatus {
            id: self.id.clone(),
            band: self.band,
            label: self.label.clone(),
            configured: self.is_valid(),
            stale,
            reason: self.reason.clone(),
            aircraft_count,
        }
    }
}

/// Checks whether a dotted-quad IPv4 string is loopback (`127.0.0.0/8`) or `RFC1918` private
/// (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`).
#[must_use]
fn is_local_ipv4(host: &str) -> bool {
    let mut octets = [0_u8; 4];
    let mut count = 0_usize;
    for part in host.split('.') {
        if count >= 4
            || part.is_empty()
            || part.len() > 3
            || !part.bytes().all(|b| b.is_ascii_digit())
        {
            return false;
        }
        let Ok(val) = part.parse::<u8>() else {
            return false;
        };
        octets[count] = val;
        count += 1;
    }
    if count != 4 {
        return false;
    }
    let [a, b, _, _] = octets;
    a == 127 || a == 10 || (a == 172 && (16..=31).contains(&b)) || (a == 192 && b == 168)
}

/// Validates whether `host` is an allowed local receiver tap target:
/// - Loopback (`127.0.0.0/8`, `::1`, `[::1]`, or IPv4-mapped `::ffff:127.x.x.x`)
/// - `RFC1918` private IPv4 (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`)
/// - `localhost`
/// - Valid mDNS `*.local` hostname
///
/// Rejects public IP addresses, link-local/cloud metadata (`169.254.0.0/16`), empty strings,
/// and invalid hostname characters.
#[must_use]
pub fn is_local_receiver_host(host: &str) -> bool {
    if host.is_empty() || host.bytes().any(|b| b.is_ascii_whitespace() || b.is_ascii_control()) {
        return false;
    }

    let unbracketed = host.strip_prefix('[').and_then(|s| s.strip_suffix(']')).unwrap_or(host);
    let lower = unbracketed.to_ascii_lowercase();

    if lower.is_empty() || lower.len() > 253 {
        return false;
    }

    if lower == "::1" {
        return true;
    }

    if let Some(mapped) = lower.strip_prefix("::ffff:") {
        return is_local_ipv4(mapped);
    }

    if lower.contains(':') {
        return false;
    }

    if lower == "localhost" || is_local_ipv4(&lower) {
        return true;
    }

    if let Some(prefix) = lower.strip_suffix(".local") {
        if prefix.is_empty() {
            return false;
        }
        return prefix.split('.').all(|label| {
            (1..=63).contains(&label.len())
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        });
    }

    false
}

/// Validates a single `band=url` feed entry.
fn parse_feed_entry(entry: &str) -> (Option<LocalReceiverBand>, Result<String, &'static str>) {
    let Some(sep) = entry.find('=') else {
        return (None, Err("expected band=url"));
    };
    if sep == 0 {
        return (None, Err("expected band=url"));
    }

    let band_raw = entry[..sep].trim();
    let raw_url = entry[sep + 1..].trim();

    let Some(band) = LocalReceiverBand::parse(band_raw) else {
        return (None, Err("band must be 1090 or 978"));
    };

    if raw_url.contains('\\') {
        return (Some(band), Err("not a valid URL"));
    }

    let Ok(url) = Url::parse(raw_url) else {
        return (Some(band), Err("not a valid URL"));
    };

    if url.scheme() != "http" && url.scheme() != "https" {
        return (Some(band), Err("scheme must be http or https"));
    }

    if !url.username().is_empty() || url.password().is_some() {
        return (Some(band), Err("credentials are not allowed in the URL"));
    }

    if url.query().is_some()
        || url.fragment().is_some()
        || raw_url.contains('?')
        || raw_url.contains('#')
    {
        return (Some(band), Err("query and fragment are not allowed"));
    }

    let Some(host_str) = url.host_str() else {
        return (Some(band), Err("host must be loopback, RFC1918, localhost or *.local"));
    };

    if !is_local_receiver_host(host_str) {
        return (Some(band), Err("host must be loopback, RFC1918, localhost or *.local"));
    }

    let path = url.path();
    if !(path == "/aircraft.json" || path.ends_with("/aircraft.json")) {
        return (Some(band), Err("path must end in aircraft.json"));
    }

    let port = url.port_or_known_default().unwrap_or(80);
    let host_lower = host_str.to_ascii_lowercase();
    let normalized = format!("{}://{host_lower}:{port}{path}", url.scheme());
    (Some(band), Ok(normalized))
}

/// Parses a comma-separated `LOCAL_RECEIVER_FEEDS` configuration string (`band=url[,band=url...]`).
///
/// Enforces:
/// - At most [`MAX_LOCAL_RECEIVER_FEEDS`] (8) feeds
/// - Band `1090` (`"1090 MHz"`) or `978` (`"978 MHz UAT"`)
/// - Ordinal suffixes (`#1`, `#2`, ...) when a band repeats
/// - `http://` or `https://` scheme, no credentials, no query or fragment
/// - Valid local receiver host via [`is_local_receiver_host`]
/// - Path ending in `aircraft.json`
#[must_use]
pub fn parse_local_receiver_feeds(raw: &str) -> Vec<LocalReceiverFeedConfig> {
    let entries: Vec<&str> = raw.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();

    let mut feeds = Vec::with_capacity(entries.len());
    let mut per_band: HashMap<Option<LocalReceiverBand>, usize> = HashMap::new();

    for (index, entry) in entries.into_iter().enumerate() {
        let (band, outcome) = if index < MAX_LOCAL_RECEIVER_FEEDS {
            parse_feed_entry(entry)
        } else {
            (None, Err("at most 8 feeds are read"))
        };

        let ordinal = per_band.get(&band).copied().unwrap_or(0) + 1;
        per_band.insert(band, ordinal);

        let base = band.map_or_else(|| format!("entry {}", index + 1), |b| b.label().to_owned());
        let label = if band.is_some() && ordinal > 1 { format!("{base} #{ordinal}") } else { base };

        let (url, reason) = match outcome {
            Ok(valid_url) => (Some(valid_url), None),
            Err(msg) => (None, Some(msg.to_owned())),
        };

        feeds.push(LocalReceiverFeedConfig {
            id: format!("feed-{}", index + 1),
            band,
            label,
            url,
            reason,
        });
    }

    // Add `#1` to the first feed of any band that repeated.
    for feed in &mut feeds {
        if let Some(band) = feed.band
            && per_band.get(&Some(band)).copied().unwrap_or(0) > 1
            && !feed.label.contains('#')
        {
            feed.label = format!("{} #1", feed.label);
        }
    }

    feeds
}

fn normalize_icao_hex(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.len() == 6 && trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(trimmed)
    } else {
        None
    }
}

fn normalize_callsign(raw: &str) -> Option<String> {
    let replaced: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { ' ' })
        .collect();
    clean_opt(Some(&replaced), 16)
}

fn normalize_adsb_category(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_ascii_uppercase();
    let bytes = trimmed.as_bytes();
    if bytes.len() == 2 && (b'A'..=b'D').contains(&bytes[0]) && (b'0'..=b'7').contains(&bytes[1]) {
        Some(trimmed)
    } else {
        None
    }
}

#[allow(clippy::cast_possible_truncation)]
fn finite_i32(val: &Value) -> Option<i32> {
    if let Some(i) = val.as_i64() {
        i32::try_from(i).ok()
    } else if let Some(f) = val.as_f64() {
        if f.is_finite() && f >= f64::from(i32::MIN) && f <= f64::from(i32::MAX) {
            Some(f.round() as i32)
        } else {
            None
        }
    } else {
        None
    }
}

fn finite_f64(val: &Value) -> Option<f64> {
    val.as_f64().filter(|v| v.is_finite())
}

fn flag_or_nonempty_array(val: Option<&Value>) -> bool {
    match val {
        Some(Value::Bool(b)) => *b,
        Some(Value::Array(arr)) => !arr.is_empty(),
        _ => false,
    }
}

/// Parses and normalizes a `dump1090` / `readsb` / `tar1090` / `dump978` `aircraft.json` payload.
///
/// Returns `(feed_now_s, aircraft)` where `feed_now_s` is the receiver's reported epoch timestamp
/// in seconds when present and finite. Malformed individual aircraft rows or non-ICAO (`~`) entries
/// are skipped without failing the document.
///
/// # Errors
/// Returns [`CoreError::BadRequest`] if `payload` is not valid JSON or does not contain a top-level
/// `aircraft` array.
pub fn normalize_dump1090_json(
    payload: &[u8],
    band: LocalReceiverBand,
    feed_id: &str,
) -> Result<(Option<f64>, Vec<LocalAdsbAircraft>), CoreError> {
    let doc: Value = serde_json::from_slice(payload)
        .map_err(|e| CoreError::BadRequest(format!("invalid dump1090 JSON: {e}")))?;

    let Some(rows) = doc.get("aircraft").and_then(Value::as_array) else {
        return Err(CoreError::BadRequest("dump1090 document missing aircraft array".to_owned()));
    };

    let now_s = doc.get("now").and_then(finite_f64);
    let mut aircraft = Vec::with_capacity(rows.len());

    for entry in rows {
        let Some(obj) = entry.as_object() else {
            continue;
        };

        let Some(hex) = obj.get("hex").and_then(Value::as_str).and_then(normalize_icao_hex) else {
            continue;
        };

        let callsign = obj
            .get("flight")
            .or_else(|| obj.get("callsign"))
            .and_then(Value::as_str)
            .and_then(normalize_callsign);

        let (on_ground, altitude_baro_ft) = match obj.get("alt_baro") {
            Some(Value::String(s)) if s.trim().eq_ignore_ascii_case("ground") => (true, Some(0)),
            Some(v) => (false, finite_i32(v)),
            None => (false, None),
        };

        let altitude_geom_ft = obj.get("alt_geom").and_then(finite_i32);

        let seen_s = obj.get("seen").and_then(finite_f64).unwrap_or(0.0).max(0.0);
        let seen_pos_s = obj.get("seen_pos").and_then(finite_f64).map(|v| v.max(0.0));

        let position =
            match (obj.get("lat").and_then(finite_f64), obj.get("lon").and_then(finite_f64)) {
                (Some(lat), Some(lon)) => GeoPoint::new(lat, lon).ok(),
                _ => None,
            };

        let ground_speed_kt = obj.get("gs").and_then(finite_f64).filter(|v| *v >= 0.0);

        let track_deg =
            obj.get("track").and_then(finite_f64).map(|t| ((t % 360.0) + 360.0) % 360.0);

        let vertical_rate_fpm = obj
            .get("baro_rate")
            .and_then(finite_i32)
            .or_else(|| obj.get("geom_rate").and_then(finite_i32));

        let squawk = clean_opt(obj.get("squawk").and_then(Value::as_str), 8);

        let emergency = clean_opt(obj.get("emergency").and_then(Value::as_str), 16)
            .filter(|e| !e.eq_ignore_ascii_case("none"));

        let category =
            obj.get("category").and_then(Value::as_str).and_then(normalize_adsb_category);

        let nic = obj.get("nic").and_then(Value::as_u64).and_then(|v| u8::try_from(v).ok());

        let nac_p = obj.get("nac_p").and_then(Value::as_u64).and_then(|v| u8::try_from(v).ok());

        let rssi_dbfs = obj.get("rssi").and_then(finite_f64);
        let messages = obj.get("messages").and_then(Value::as_u64);

        let mlat = flag_or_nonempty_array(obj.get("mlat"));
        let tisb = flag_or_nonempty_array(obj.get("tisb"));

        aircraft.push(LocalAdsbAircraft {
            hex,
            band,
            feed_id: feed_id.to_owned(),
            callsign,
            position,
            altitude_baro_ft,
            altitude_geom_ft,
            ground_speed_kt,
            track_deg,
            vertical_rate_fpm,
            squawk,
            emergency,
            category,
            nic,
            nac_p,
            rssi_dbfs,
            messages,
            seen_s,
            seen_pos_s,
            on_ground,
            mlat,
            tisb,
        });
    }

    Ok((now_s, aircraft))
}

#[derive(Debug, Clone)]
struct IngestedFeedState {
    effective_at: Timestamp,
    aircraft: Vec<LocalAdsbAircraft>,
}

#[derive(Debug)]
struct EngineState {
    feeds: Vec<LocalReceiverFeedConfig>,
    ingested: HashMap<String, IngestedFeedState>,
}

/// Thread-safe engine managing local ADS-B receiver feed configurations and ingested aircraft.
#[derive(Debug)]
pub struct LocalReceiverEngine {
    state: RwLock<EngineState>,
}

impl Default for LocalReceiverEngine {
    fn default() -> Self {
        Self::new("")
    }
}

impl LocalReceiverEngine {
    /// Creates a new [`LocalReceiverEngine`] by parsing a `LOCAL_RECEIVER_FEEDS` specification.
    #[must_use]
    pub fn new(raw_feeds: &str) -> Self {
        Self::from_configs(parse_local_receiver_feeds(raw_feeds))
    }

    /// Creates a new [`LocalReceiverEngine`] from pre-parsed feed configurations.
    #[must_use]
    pub fn from_configs(feeds: Vec<LocalReceiverFeedConfig>) -> Self {
        Self { state: RwLock::new(EngineState { feeds, ingested: HashMap::new() }) }
    }

    /// Returns a copy of the configured local receiver feeds.
    #[must_use]
    pub fn feeds(&self) -> Vec<LocalReceiverFeedConfig> {
        let guard = self.state.read().unwrap_or_else(PoisonError::into_inner);
        guard.feeds.clone()
    }

    /// Ingests a raw `aircraft.json` payload for `feed_id` on `band`.
    ///
    /// Returns the number of normalized aircraft decoded from the payload.
    ///
    /// # Errors
    /// Returns [`CoreError::BadRequest`] if the payload is malformed or if `feed_id` refers to an
    /// invalid/rejected feed configuration.
    pub fn ingest_feed_payload(
        &self,
        feed_id: &str,
        band: LocalReceiverBand,
        payload: &[u8],
        now: Timestamp,
    ) -> Result<usize, CoreError> {
        let (feed_now_s, aircraft) = normalize_dump1090_json(payload, band, feed_id)?;
        let count = aircraft.len();

        // Anchor to the feed's own clock when valid and not ahead of `now`; otherwise use `now`.
        let effective_at = feed_now_s
            .filter(|&s| (946_684_800.0..4_102_444_800.0).contains(&s))
            .and_then(|s| SignedDuration::try_from_secs_f64(s).ok())
            .and_then(|d| Timestamp::UNIX_EPOCH.checked_add(d).ok())
            .map_or(now, |ts| if ts <= now { ts } else { now });

        let mut guard = self.state.write().unwrap_or_else(PoisonError::into_inner);

        if let Some(cfg) = guard.feeds.iter().find(|f| f.id == feed_id) {
            if !cfg.is_valid() {
                return Err(CoreError::BadRequest(format!(
                    "cannot ingest into invalid local receiver feed {feed_id}"
                )));
            }
        } else {
            guard.feeds.push(LocalReceiverFeedConfig {
                id: feed_id.to_owned(),
                band: Some(band),
                label: band.label().to_owned(),
                url: Some(format!("http://127.0.0.1:8080/{feed_id}/aircraft.json")),
                reason: None,
            });
        }

        guard.ingested.insert(feed_id.to_owned(), IngestedFeedState { effective_at, aircraft });

        Ok(count)
    }

    /// Computes a merged point-in-time [`LocalReceiverSnapshot`] across all feeds at `now`.
    #[must_use]
    pub fn snapshot(&self, now: Timestamp) -> LocalReceiverSnapshot {
        let guard = self.state.read().unwrap_or_else(PoisonError::into_inner);

        let configured = !guard.feeds.is_empty();
        let mut feed_statuses = Vec::with_capacity(guard.feeds.len());
        let mut merged_by_hex: HashMap<String, LocalAdsbAircraft> = HashMap::new();
        let mut any_live_feed = false;

        for feed in &guard.feeds {
            if !feed.is_valid() {
                feed_statuses.push(feed.to_status(true, 0));
                continue;
            }

            if let Some(ingested) = guard.ingested.get(&feed.id) {
                let age_s = now.duration_since(ingested.effective_at).as_secs_f64().max(0.0);
                let is_stale = age_s > LOCAL_RECEIVER_STALE_SECS;
                if !is_stale {
                    any_live_feed = true;
                }

                let mut active_count = 0_usize;
                for ac in &ingested.aircraft {
                    let effective_seen_s = ac.seen_s + age_s;
                    if effective_seen_s >= LOCAL_ADSB_MESSAGE_STALE_SECS {
                        continue;
                    }
                    active_count += 1;

                    let mut adjusted = ac.clone();
                    adjusted.seen_s = effective_seen_s;
                    adjusted.seen_pos_s = ac.seen_pos_s.map(|sp| sp + age_s);

                    match merged_by_hex.get(&adjusted.hex) {
                        None => {
                            merged_by_hex.insert(adjusted.hex.clone(), adjusted);
                        }
                        Some(current) => {
                            if is_aircraft_newer(&adjusted, current) {
                                merged_by_hex.insert(adjusted.hex.clone(), adjusted);
                            }
                        }
                    }
                }

                feed_statuses.push(feed.to_status(is_stale, active_count));
            } else {
                feed_statuses.push(LocalReceiverFeedStatus {
                    id: feed.id.clone(),
                    band: feed.band,
                    label: feed.label.clone(),
                    configured: true,
                    stale: true,
                    reason: Some("awaiting initial payload".to_owned()),
                    aircraft_count: 0,
                });
            }
        }

        let mut aircraft: Vec<LocalAdsbAircraft> = merged_by_hex.into_values().collect();
        aircraft.sort_by(|a, b| a.hex.cmp(&b.hex));

        LocalReceiverSnapshot {
            as_of: now,
            configured,
            stale: !configured || !any_live_feed,
            feeds: feed_statuses,
            aircraft,
        }
    }
}

fn is_aircraft_newer(candidate: &LocalAdsbAircraft, current: &LocalAdsbAircraft) -> bool {
    match (candidate.seen_pos_s, current.seen_pos_s) {
        (Some(cand_pos), Some(curr_pos)) => {
            if (cand_pos - curr_pos).abs() > f64::EPSILON {
                return cand_pos < curr_pos;
            }
        }
        (Some(_), None) => return true,
        (None, Some(_)) => return false,
        (None, None) => {}
    }
    candidate.seen_s < current.seen_s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_local_receiver_hosts() {
        // Loopback IPv4 & IPv6
        assert!(is_local_receiver_host("127.0.0.1"));
        assert!(is_local_receiver_host("127.255.255.254"));
        assert!(is_local_receiver_host("::1"));
        assert!(is_local_receiver_host("[::1]"));
        assert!(is_local_receiver_host("::ffff:127.0.0.1"));
        assert!(is_local_receiver_host("::ffff:192.168.1.10"));

        // RFC1918 private ranges
        assert!(is_local_receiver_host("10.0.0.1"));
        assert!(is_local_receiver_host("10.255.255.255"));
        assert!(is_local_receiver_host("172.16.0.1"));
        assert!(is_local_receiver_host("172.31.255.254"));
        assert!(is_local_receiver_host("192.168.0.1"));
        assert!(is_local_receiver_host("192.168.254.100"));

        // Localhost and mDNS *.local
        assert!(is_local_receiver_host("localhost"));
        assert!(is_local_receiver_host("LOCALHOST"));
        assert!(is_local_receiver_host("readsb.local"));
        assert!(is_local_receiver_host("piaware-1090.shack.local"));

        // Rejects public IPs, link-local / cloud metadata, and invalid hosts
        assert!(!is_local_receiver_host(""));
        assert!(!is_local_receiver_host("   "));
        assert!(!is_local_receiver_host("8.8.8.8"));
        assert!(!is_local_receiver_host("1.1.1.1"));
        assert!(!is_local_receiver_host("172.15.255.255"));
        assert!(!is_local_receiver_host("172.32.0.1"));
        assert!(!is_local_receiver_host("169.254.169.254"));
        assert!(!is_local_receiver_host("192.169.1.1"));
        assert!(!is_local_receiver_host("999.1.1.1"));
        assert!(!is_local_receiver_host("adsb.lol"));
        assert!(!is_local_receiver_host(".local"));
        assert!(!is_local_receiver_host("-bad.local"));
        assert!(!is_local_receiver_host("bad-.local"));
        assert!(!is_local_receiver_host("bad host.local"));
    }

    #[test]
    fn parses_local_receiver_feeds_with_ordinals_and_security_rules() {
        let raw = "1090=http://192.168.1.10:8080/data/aircraft.json, \
                   978=http://skyaware.local:8978/data/aircraft.json, \
                   1090=https://localhost:8443/tar1090/data/aircraft.json";
        let feeds = parse_local_receiver_feeds(raw);
        assert_eq!(feeds.len(), 3);

        assert_eq!(feeds[0].id, "feed-1");
        assert_eq!(feeds[0].band, Some(LocalReceiverBand::Band1090));
        assert_eq!(feeds[0].label, "1090 MHz #1");
        assert_eq!(feeds[0].url.as_deref(), Some("http://192.168.1.10:8080/data/aircraft.json"));
        assert!(feeds[0].is_valid());

        assert_eq!(feeds[1].id, "feed-2");
        assert_eq!(feeds[1].band, Some(LocalReceiverBand::Band978));
        assert_eq!(feeds[1].label, "978 MHz UAT");
        assert!(feeds[1].is_valid());

        assert_eq!(feeds[2].id, "feed-3");
        assert_eq!(feeds[2].band, Some(LocalReceiverBand::Band1090));
        assert_eq!(feeds[2].label, "1090 MHz #2");
        assert!(feeds[2].is_valid());

        // Verify public status never contains the raw URL
        let status = feeds[0].to_status(false, 12);
        let serialized = serde_json::to_string(&status).unwrap();
        assert!(!serialized.contains("192.168.1.10"));
        assert_eq!(status.aircraft_count, 12);
    }

    #[test]
    fn rejects_invalid_feeds_and_enforces_max_eight_cap() {
        let raw = [
            "1090=ftp://127.0.0.1/aircraft.json",
            "1090=http://user:pass@127.0.0.1/aircraft.json",
            "1090=http://127.0.0.1/aircraft.json?foo=1",
            "1090=http://127.0.0.1/aircraft.json#frag",
            "1090=http://8.8.8.8/aircraft.json",
            "1090=http://127.0.0.1/status.json",
            "1234=http://127.0.0.1/aircraft.json",
            "missing_equals",
            "1090=http://127.0.0.1:8080/aircraft.json",
        ]
        .join(",");

        let feeds = parse_local_receiver_feeds(&raw);
        assert_eq!(feeds.len(), 9);
        assert_eq!(feeds[0].reason.as_deref(), Some("scheme must be http or https"));
        assert_eq!(feeds[1].reason.as_deref(), Some("credentials are not allowed in the URL"));
        assert_eq!(feeds[2].reason.as_deref(), Some("query and fragment are not allowed"));
        assert_eq!(feeds[3].reason.as_deref(), Some("query and fragment are not allowed"));
        assert_eq!(
            feeds[4].reason.as_deref(),
            Some("host must be loopback, RFC1918, localhost or *.local")
        );
        assert_eq!(feeds[5].reason.as_deref(), Some("path must end in aircraft.json"));
        assert_eq!(feeds[6].reason.as_deref(), Some("band must be 1090 or 978"));
        assert_eq!(feeds[7].reason.as_deref(), Some("expected band=url"));
        assert_eq!(feeds[8].reason.as_deref(), Some("at most 8 feeds are read"));
        assert!(feeds.iter().all(|f| !f.is_valid()));
    }

    #[test]
    fn normalizes_dump1090_aircraft_json_and_handles_ground_state() {
        let payload = br#"{
            "now": 1760000000.5,
            "messages": 98123,
            "aircraft": [
                {
                    "hex": "A1B2C3",
                    "flight": " UAL123  ",
                    "alt_baro": 34000,
                    "alt_geom": 34650,
                    "gs": 462.5,
                    "track": 275.4,
                    "baro_rate": -512,
                    "squawk": "1200",
                    "emergency": "none",
                    "category": "a3",
                    "lat": 37.6188,
                    "lon": -122.375,
                    "nic": 8,
                    "nac_p": 10,
                    "rssi": -8.4,
                    "messages": 420,
                    "seen": 0.4,
                    "seen_pos": 1.1,
                    "mlat": [],
                    "tisb": []
                },
                {
                    "hex": "ae0456",
                    "flight": "RCH99",
                    "alt_baro": "ground",
                    "gs": 14.0,
                    "track": -10.0,
                    "geom_rate": 0,
                    "emergency": "general",
                    "category": "A5",
                    "lat": 38.8512,
                    "lon": -77.0402,
                    "seen": 1.0,
                    "seen_pos": 1.5,
                    "mlat": ["lat", "lon"],
                    "tisb": false
                },
                {
                    "hex": "~3f9001",
                    "lat": 37.0,
                    "lon": -122.0,
                    "seen": 0.1
                }
            ]
        }"#;

        let (now_s, aircraft) =
            normalize_dump1090_json(payload, LocalReceiverBand::Band1090, "feed-1").unwrap();
        assert_eq!(now_s, Some(1_760_000_000.5));
        assert_eq!(aircraft.len(), 2);

        let first = &aircraft[0];
        assert_eq!(first.hex, "a1b2c3");
        assert_eq!(first.band, LocalReceiverBand::Band1090);
        assert_eq!(first.feed_id, "feed-1");
        assert_eq!(first.callsign.as_deref(), Some("UAL123"));
        assert!(!first.on_ground);
        assert_eq!(first.altitude_baro_ft, Some(34_000));
        assert_eq!(first.altitude_geom_ft, Some(34_650));
        assert_eq!(first.ground_speed_kt, Some(462.5));
        assert_eq!(first.track_deg, Some(275.4));
        assert_eq!(first.vertical_rate_fpm, Some(-512));
        assert_eq!(first.squawk.as_deref(), Some("1200"));
        assert_eq!(first.emergency, None);
        assert_eq!(first.category.as_deref(), Some("A3"));
        assert_eq!(first.nic, Some(8));
        assert_eq!(first.nac_p, Some(10));
        assert_eq!(first.rssi_dbfs, Some(-8.4));
        assert_eq!(first.messages, Some(420));
        assert!(!first.mlat);
        assert!(!first.tisb);

        let second = &aircraft[1];
        assert_eq!(second.hex, "ae0456");
        assert!(second.on_ground);
        assert_eq!(second.altitude_baro_ft, Some(0));
        assert_eq!(second.track_deg, Some(350.0));
        assert_eq!(second.emergency.as_deref(), Some("general"));
        assert!(second.mlat);
        assert!(!second.tisb);
    }

    #[test]
    fn engine_ingests_and_builds_merged_snapshot() {
        let engine = LocalReceiverEngine::new(
            "1090=http://127.0.0.1:8080/data/aircraft.json,978=http://127.0.0.1:8978/data/aircraft.json",
        );
        let now: Timestamp = "2026-10-26T10:00:05Z".parse().unwrap();

        let payload_1090 = serde_json::to_vec(&serde_json::json!({
            "now": now.as_second(),
            "aircraft": [
                {"hex": "a00001", "flight": "N100A", "lat": 30.2, "lon": -97.7, "seen": 0.5, "seen_pos": 0.5}
            ]
        }))
        .unwrap();

        let count = engine
            .ingest_feed_payload("feed-1", LocalReceiverBand::Band1090, &payload_1090, now)
            .unwrap();
        assert_eq!(count, 1);

        let snap = engine.snapshot(now);
        assert!(snap.configured);
        assert!(!snap.stale);
        assert_eq!(snap.feeds.len(), 2);
        assert_eq!(snap.feeds[0].aircraft_count, 1);
        assert!(!snap.feeds[0].stale);
        assert!(snap.feeds[1].stale);
        assert_eq!(snap.aircraft.len(), 1);
        assert_eq!(snap.aircraft[0].hex, "a00001");
    }
}
