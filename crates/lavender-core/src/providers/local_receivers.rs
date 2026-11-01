//! Local SDR and Mode-S/UAT ADS-B receiver provider (`dump1090`, `readsb`, `tar1090`, `dump978`).
//!
//! Validates local-only receiver tap addresses (loopback, `RFC1918` private IPv4, `localhost`,
//! and `*.local` mDNS names), parses `LOCAL_RECEIVER_FEEDS` configuration without ever exposing
//! raw receiver URLs to browser clients, normalizes `aircraft.json` payloads, and maintains a
//! thread-safe snapshot engine across configured feeds.

use std::collections::HashMap;
use std::net::IpAddr;
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

/// Validates whether an [`IpAddr`] is loopback or `RFC1918` private IPv4 (SSRF safe).
#[must_use]
pub fn is_safe_local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, _, _] = v4.octets();
            a == 127 || a == 10 || (a == 172 && (16..=31).contains(&b)) || (a == 192 && b == 168)
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() {
                return true;
            }
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_safe_local_ip(IpAddr::V4(v4));
            }
            false
        }
    }
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

/// Message types defined by the Mode-S Beast binary protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeastMessageType {
    /// Mode-AC (2 payload bytes).
    ModeAc,
    /// Mode-S Short (7 payload bytes / 56 bits).
    ModeSShort,
    /// Mode-S Long (14 payload bytes / 112 bits).
    ModeSLong,
}

impl BeastMessageType {
    /// Decodes a Beast message type byte (`'1'`, `'2'`, `'3'` or `0x31`, `0x32`, `0x33`).
    #[must_use]
    pub const fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x31 => Some(Self::ModeAc),
            0x32 => Some(Self::ModeSShort),
            0x33 => Some(Self::ModeSLong),
            _ => None,
        }
    }

    /// Expected payload byte length for this message type.
    #[must_use]
    pub const fn payload_len(self) -> usize {
        match self {
            Self::ModeAc => 2,
            Self::ModeSShort => 7,
            Self::ModeSLong => 14,
        }
    }
}

/// Unpacked Beast binary packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeastPacket {
    /// Decoded message type.
    pub msg_type: BeastMessageType,
    /// 6-byte 12MHz MLAT clock timestamp counter.
    pub timestamp: [u8; 6],
    /// 1-byte signal level (RSSI).
    pub signal_level: u8,
    /// Raw payload bytes.
    pub payload: Vec<u8>,
}

fn hex_val(b: u8) -> Result<u8, CoreError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(CoreError::BadRequest(format!("invalid hex character: {b:02x}"))),
    }
}

fn decode_hex_bytes(hex: &str) -> Result<Vec<u8>, CoreError> {
    let bytes = hex.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err(CoreError::BadRequest("odd number of hex digits".to_owned()));
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.as_chunks::<2>().0 {
        let h = hex_val(chunk[0])?;
        let l = hex_val(chunk[1])?;
        out.push((h << 4) | l);
    }
    Ok(out)
}

/// Parses an AVR format string (`*<hex>;\n` or `@<timestamp><hex>;\n`) into a
/// 7-byte (56-bit) or 14-byte (112-bit) Mode-S byte array.
///
/// # Errors
/// Returns [`CoreError::BadRequest`] if the frame prefix is not `*` or `@`,
/// contains non-hexadecimal characters, or has an invalid length.
pub fn parse_avr_frame(raw: &str) -> Result<Vec<u8>, CoreError> {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix('*') {
        let hex_part = rest.strip_suffix(';').unwrap_or(rest).trim();
        if hex_part.len() != 14 && hex_part.len() != 28 {
            return Err(CoreError::BadRequest(format!(
                "invalid AVR frame length: {} hex chars (expected 14 or 28)",
                hex_part.len()
            )));
        }
        decode_hex_bytes(hex_part)
    } else if let Some(rest) = trimmed.strip_prefix('@') {
        let hex_part = rest.strip_suffix(';').unwrap_or(rest).trim();
        if hex_part.len() != 26 && hex_part.len() != 40 {
            return Err(CoreError::BadRequest(format!(
                "invalid timestamped AVR frame length: {} hex chars (expected 26 or 40)",
                hex_part.len()
            )));
        }
        // Skip 12 hex chars of 12MHz timestamp counter
        decode_hex_bytes(&hex_part[12..])
    } else {
        Err(CoreError::BadRequest("AVR frame must start with '*' or '@'".to_owned()))
    }
}

/// Parses and consumes complete AVR frames from a streaming buffer.
pub fn parse_avr_stream(buffer: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    while let Some(delim_idx) = buffer.iter().position(|&b| b == b';' || b == b'\n') {
        let slice = &buffer[..=delim_idx];
        if let Ok(text) = std::str::from_utf8(slice) {
            let trimmed = text.trim();
            if (trimmed.starts_with('*') || trimmed.starts_with('@'))
                && let Ok(frame) = parse_avr_frame(trimmed)
            {
                frames.push(frame);
            }
        }
        buffer.drain(..=delim_idx);
    }
    if buffer.len() > 65_536 {
        buffer.clear();
    }
    frames
}

/// Extracts all complete Beast packets from a streaming buffer, unescaping `0x1A 0x1A`
/// and removing consumed bytes from the buffer.
pub fn parse_beast_stream(buffer: &mut Vec<u8>) -> Vec<BeastPacket> {
    let mut packets = Vec::new();
    'outer: while !buffer.is_empty() {
        let Some(start_idx) = buffer.iter().position(|&b| b == 0x1A) else {
            buffer.clear();
            break;
        };
        if start_idx > 0 {
            buffer.drain(0..start_idx);
        }
        if buffer.len() < 2 {
            break;
        }
        let type_byte = buffer[1];
        if type_byte == 0x1A {
            buffer.drain(0..1);
            continue 'outer;
        }
        let Some(msg_type) = BeastMessageType::from_byte(type_byte) else {
            buffer.drain(0..1);
            continue 'outer;
        };
        let expected_body_len = 6 + 1 + msg_type.payload_len();
        let mut body = Vec::with_capacity(expected_body_len);
        let mut idx = 2_usize;

        while body.len() < expected_body_len {
            if idx >= buffer.len() {
                break 'outer;
            }
            if buffer[idx] == 0x1A {
                if idx + 1 >= buffer.len() {
                    break 'outer;
                }
                if buffer[idx + 1] == 0x1A {
                    body.push(0x1A);
                    idx += 2;
                } else {
                    buffer.drain(0..idx);
                    continue 'outer;
                }
            } else {
                body.push(buffer[idx]);
                idx += 1;
            }
        }

        if body.len() == expected_body_len {
            let mut ts = [0_u8; 6];
            ts.copy_from_slice(&body[0..6]);
            let signal_level = body[6];
            let payload = body[7..].to_vec();
            packets.push(BeastPacket { msg_type, timestamp: ts, signal_level, payload });
            buffer.drain(0..idx);
        }
    }
    if buffer.len() > 65_536 {
        buffer.clear();
    }
    packets
}

/// Unpacks a single Beast packet from a byte slice.
///
/// # Errors
/// Returns [`CoreError::BadRequest`] if the slice is empty, does not start with `0x1A`,
/// or cannot be decoded into a valid Beast packet.
pub fn parse_beast_packet(data: &[u8]) -> Result<BeastPacket, CoreError> {
    if data.is_empty() {
        return Err(CoreError::BadRequest("empty Beast packet".to_owned()));
    }
    if data[0] != 0x1A {
        return Err(CoreError::BadRequest(
            "Beast packet must begin with escape byte 0x1A".to_owned(),
        ));
    }
    let mut buf = data.to_vec();
    let mut packets = parse_beast_stream(&mut buf);
    match packets.pop() {
        Some(pkt) => Ok(pkt),
        None => Err(CoreError::BadRequest("invalid or incomplete Beast packet".to_owned())),
    }
}

/// Extracted fields from a Mode-S frame.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedModeSFrame {
    /// Downlink format (bits 1..=5).
    pub df: u8,
    /// 24-bit ICAO address formatted as 6 lowercase hex characters (when extractable).
    pub icao: Option<String>,
    /// ADS-B Type Code (bits 33..=37) for extended squitters (DF 17/18).
    pub tc: Option<u8>,
    /// Aircraft callsign (for TC 1..=4).
    pub callsign: Option<String>,
    /// Barometric altitude in feet (for TC 9..=18, 20..=22).
    pub altitude_baro_ft: Option<i32>,
    /// Ground speed in knots (for TC 19).
    pub ground_speed_kt: Option<f64>,
    /// Track heading in degrees (for TC 19).
    pub track_deg: Option<f64>,
    /// Emitter category (for TC 1..=4).
    pub category: Option<String>,
    /// Compact Position Reporting (CPR) flag, lat, lon (for TC 9..=18, 20..=22).
    pub cpr: Option<(bool, u32, u32)>,
}

/// Decodes Mode-S frame fields including Downlink Format, 24-bit ICAO address,
/// Type Code, callsign, altitude, velocity, and CPR coordinates.
///
/// # Errors
/// Returns [`CoreError::BadRequest`] if `frame` is not 7 bytes (56 bits) or 14 bytes (112 bits).
#[allow(clippy::too_many_lines, clippy::cast_precision_loss)]
pub fn extract_mode_s_frame(frame: &[u8]) -> Result<DecodedModeSFrame, CoreError> {
    if frame.len() != 7 && frame.len() != 14 {
        return Err(CoreError::BadRequest(format!(
            "invalid Mode-S frame length: {} bytes (expected 7 or 14)",
            frame.len()
        )));
    }

    let df = (frame[0] >> 3) & 0x1F;
    let icao = if df == 17 || df == 18 || df == 11 || df == 19 {
        Some(format!("{:02x}{:02x}{:02x}", frame[1], frame[2], frame[3]))
    } else {
        None
    };

    let mut tc = None;
    let mut callsign = None;
    let mut altitude_baro_ft = None;
    let mut ground_speed_kt = None;
    let mut track_deg = None;
    let mut category = None;
    let mut cpr = None;

    if (df == 17 || df == 18) && frame.len() == 14 {
        let tc_val = (frame[4] >> 3) & 0x1F;
        tc = Some(tc_val);

        match tc_val {
            1..=4 => {
                let chars = [
                    (frame[5] >> 2) & 0x3F,
                    ((frame[5] & 0x03) << 4) | ((frame[6] >> 4) & 0x0F),
                    ((frame[6] & 0x0F) << 2) | ((frame[7] >> 6) & 0x03),
                    frame[7] & 0x3F,
                    (frame[8] >> 2) & 0x3F,
                    ((frame[8] & 0x03) << 4) | ((frame[9] >> 4) & 0x0F),
                    ((frame[9] & 0x0F) << 2) | ((frame[10] >> 6) & 0x03),
                    frame[10] & 0x3F,
                ];
                let mut cs = String::with_capacity(8);
                for c in chars {
                    match c {
                        1..=26 => cs.push(char::from(b'A' + c - 1)),
                        48..=57 => cs.push(char::from(b'0' + c - 48)),
                        _ => cs.push(' '),
                    }
                }
                let trimmed = cs.trim();
                if !trimmed.is_empty() {
                    callsign = Some(trimmed.to_owned());
                }
                let sub_cat = frame[4] & 0x07;
                let cat_prefix = match tc_val {
                    1 => "B",
                    2 => "D",
                    3 => "C",
                    _ => "A",
                };
                category = Some(format!("{cat_prefix}{sub_cat}"));
            }
            9..=18 | 20..=22 => {
                let alt_field = (u16::from(frame[5]) << 4) | (u16::from(frame[6] >> 4));
                if (alt_field & 0x0010) != 0 {
                    let n = ((alt_field & 0x0FE0) >> 1) | (alt_field & 0x000F);
                    let alt = (i32::from(n) * 25) - 1000;
                    if (-1000..=100_000).contains(&alt) {
                        altitude_baro_ft = Some(alt);
                    }
                }
                let is_odd = ((frame[6] >> 3) & 0x01) == 1;
                let cpr_lat = (u32::from(frame[6] & 0x03) << 15)
                    | (u32::from(frame[7]) << 7)
                    | (u32::from(frame[8]) >> 1);
                let cpr_lon = (u32::from(frame[8] & 0x01) << 16)
                    | (u32::from(frame[9]) << 8)
                    | u32::from(frame[10]);
                cpr = Some((is_odd, cpr_lat, cpr_lon));
            }
            19 => {
                let subtype = frame[4] & 0x07;
                if subtype == 1 || subtype == 2 {
                    let mult = if subtype == 2 { 4.0 } else { 1.0 };
                    let v_ew_sign = if (frame[5] & 0x04) != 0 { -1.0 } else { 1.0 };
                    let v_ew_raw = (u16::from(frame[5] & 0x03) << 8) | u16::from(frame[6]);
                    let v_ns_sign = if (frame[7] & 0x80) != 0 { -1.0 } else { 1.0 };
                    let v_ns_raw = (u16::from(frame[7] & 0x7F) << 3) | u16::from(frame[8] >> 5);
                    if v_ew_raw > 0 && v_ns_raw > 0 {
                        let v_ew = f64::from(v_ew_raw - 1) * mult * v_ew_sign;
                        let v_ns = f64::from(v_ns_raw - 1) * mult * v_ns_sign;
                        let speed = (v_ew * v_ew + v_ns * v_ns).sqrt();
                        let mut track = v_ew.atan2(v_ns).to_degrees();
                        if track < 0.0 {
                            track += 360.0;
                        }
                        ground_speed_kt = Some(speed);
                        track_deg = Some(track);
                    }
                }
            }
            _ => {}
        }
    }

    Ok(DecodedModeSFrame {
        df,
        icao,
        tc,
        callsign,
        altitude_baro_ft,
        ground_speed_kt,
        track_deg,
        category,
        cpr,
    })
}

#[derive(Debug, Clone, Copy)]
struct CprFrame {
    is_odd: bool,
    lat: u32,
    lon: u32,
    time: Timestamp,
}

#[allow(clippy::cast_possible_truncation)]
fn cpr_nl(lat: f64) -> i32 {
    if lat.abs() >= 87.0 {
        return 1;
    }
    let rad = lat.to_radians();
    let cos_lat = rad.cos();
    let num = 1.0 - (1.0 - (std::f64::consts::PI / 30.0).cos()) / (cos_lat * cos_lat);
    if num <= 0.0 {
        return 1;
    }
    let angle = num.sqrt().acos();
    let nl = (2.0 * std::f64::consts::PI / angle).floor() as i32;
    nl.max(1)
}

#[allow(clippy::many_single_char_names, clippy::similar_names, clippy::cast_precision_loss)]
fn decode_cpr_pair(even: &CprFrame, odd: &CprFrame) -> Option<GeoPoint> {
    const TWO_17: f64 = 131_072.0;
    let lat_even = f64::from(even.lat) / TWO_17;
    let lat_odd = f64::from(odd.lat) / TWO_17;

    let dlat_even = 360.0 / 60.0;
    let dlat_odd = 360.0 / 59.0;

    let j = (59.0 * lat_even - 60.0 * lat_odd + 0.5).floor();
    let mut lat_e = dlat_even * (j.rem_euclid(60.0) + lat_even);
    let mut lat_o = dlat_odd * (j.rem_euclid(59.0) + lat_odd);

    if lat_e >= 270.0 {
        lat_e -= 360.0;
    }
    if lat_o >= 270.0 {
        lat_o -= 360.0;
    }

    if !(-90.0..=90.0).contains(&lat_e) || !(-90.0..=90.0).contains(&lat_o) {
        return None;
    }

    let nl_e = cpr_nl(lat_e);
    let nl_o = cpr_nl(lat_o);
    if nl_e != nl_o || nl_e < 1 {
        return None;
    }

    let (use_odd, lat) = if odd.time >= even.time { (true, lat_o) } else { (false, lat_e) };

    let nl = nl_e;
    let lon_even = f64::from(even.lon) / TWO_17;
    let lon_odd = f64::from(odd.lon) / TWO_17;

    let lon = if use_odd {
        let nl_mod = (nl - 1).max(1);
        let dlon = 360.0 / f64::from(nl_mod);
        let m = (lon_even * f64::from(nl - 1) - lon_odd * f64::from(nl) + 0.5).floor();
        let mut l = dlon * (m.rem_euclid(f64::from(nl_mod)) + lon_odd);
        if l >= 180.0 {
            l -= 360.0;
        }
        l
    } else {
        let dlon = 360.0 / f64::from(nl);
        let m = (lon_even * f64::from(nl - 1) - lon_odd * f64::from(nl) + 0.5).floor();
        let mut l = dlon * (m.rem_euclid(f64::from(nl)) + lon_even);
        if l >= 180.0 {
            l -= 360.0;
        }
        l
    };

    GeoPoint::new(lat, lon).ok()
}

#[derive(Debug, Clone)]
struct IngestedFeedState {
    effective_at: Timestamp,
    aircraft: Vec<LocalAdsbAircraft>,
    cpr_frames: HashMap<String, CprFrame>,
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

        guard.ingested.insert(
            feed_id.to_owned(),
            IngestedFeedState { effective_at, aircraft, cpr_frames: HashMap::new() },
        );

        Ok(count)
    }

    /// Ingests a raw Mode-S frame (56-bit or 112-bit) and updates the receiver aircraft state.
    ///
    /// # Errors
    /// Returns [`CoreError::BadRequest`] if the frame length is not 7 or 14 bytes.
    #[allow(clippy::too_many_lines)]
    pub fn ingest_raw_mode_s_frame(
        &self,
        frame: &[u8],
        timestamp: Timestamp,
    ) -> Result<(), CoreError> {
        let decoded = extract_mode_s_frame(frame)?;
        let Some(icao) = decoded.icao else {
            return Ok(());
        };

        let mut guard = self.state.write().unwrap_or_else(PoisonError::into_inner);

        let feed_id = if let Some(feed) =
            guard.feeds.iter().find(|f| f.band == Some(LocalReceiverBand::Band1090) && f.is_valid())
        {
            feed.id.clone()
        } else {
            let id = "raw-1090".to_owned();
            if !guard.feeds.iter().any(|f| f.id == id) {
                guard.feeds.push(LocalReceiverFeedConfig {
                    id: id.clone(),
                    band: Some(LocalReceiverBand::Band1090),
                    label: "1090 MHz Raw".to_owned(),
                    url: Some("tcp://127.0.0.1:30005".to_owned()),
                    reason: None,
                });
            }
            id
        };

        let entry = guard.ingested.entry(feed_id.clone()).or_insert_with(|| IngestedFeedState {
            effective_at: timestamp,
            aircraft: Vec::new(),
            cpr_frames: HashMap::new(),
        });
        entry.effective_at = timestamp;

        if let Some((is_odd, lat, lon)) = decoded.cpr {
            let curr = CprFrame { is_odd, lat, lon, time: timestamp };
            if let Some(prev) = entry.cpr_frames.get(&icao)
                && prev.is_odd != is_odd
            {
                let dt = timestamp.duration_since(prev.time).as_secs_f64().abs();
                if dt <= 10.0 {
                    let (even, odd) = if is_odd { (prev, &curr) } else { (&curr, prev) };
                    if let Some(pos) = decode_cpr_pair(even, odd)
                        && let Some(ac) = entry.aircraft.iter_mut().find(|a| a.hex == icao)
                    {
                        ac.position = Some(pos);
                        ac.seen_pos_s = Some(0.0);
                    }
                }
            }
            entry.cpr_frames.insert(icao.clone(), curr);
        }

        if let Some(ac) = entry.aircraft.iter_mut().find(|a| a.hex == icao) {
            ac.messages = Some(ac.messages.unwrap_or(0) + 1);
            ac.seen_s = 0.0;
            if let Some(cs) = decoded.callsign {
                ac.callsign = Some(cs);
            }
            if let Some(alt) = decoded.altitude_baro_ft {
                ac.altitude_baro_ft = Some(alt);
            }
            if let Some(gs) = decoded.ground_speed_kt {
                ac.ground_speed_kt = Some(gs);
            }
            if let Some(tr) = decoded.track_deg {
                ac.track_deg = Some(tr);
            }
            if let Some(cat) = decoded.category {
                ac.category = Some(cat);
            }
        } else {
            entry.aircraft.push(LocalAdsbAircraft {
                hex: icao,
                band: LocalReceiverBand::Band1090,
                feed_id,
                callsign: decoded.callsign,
                position: None,
                altitude_baro_ft: decoded.altitude_baro_ft,
                altitude_geom_ft: None,
                ground_speed_kt: decoded.ground_speed_kt,
                track_deg: decoded.track_deg,
                vertical_rate_fpm: None,
                squawk: None,
                emergency: None,
                category: decoded.category,
                nic: None,
                nac_p: None,
                rssi_dbfs: None,
                messages: Some(1),
                seen_s: 0.0,
                seen_pos_s: None,
                on_ground: false,
                mlat: false,
                tisb: false,
            });
        }

        Ok(())
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

    #[test]
    fn parses_avr_raw_frames_both_variants() {
        // Mode-S long frame (112-bit / 14-byte) without timestamp
        let long_raw = "*8d4840d6202cc371c32ce0576098;\n";
        let parsed_long = parse_avr_frame(long_raw).unwrap();
        assert_eq!(parsed_long.len(), 14);
        assert_eq!(
            parsed_long,
            vec![
                0x8d, 0x48, 0x40, 0xd6, 0x20, 0x2c, 0xc3, 0x71, 0xc3, 0x2c, 0xe0, 0x57, 0x60, 0x98
            ]
        );

        // Mode-S short frame (56-bit / 7-byte) without timestamp
        let short_raw = "*5d4840d6202cc3;\n";
        let parsed_short = parse_avr_frame(short_raw).unwrap();
        assert_eq!(parsed_short.len(), 7);
        assert_eq!(parsed_short, vec![0x5d, 0x48, 0x40, 0xd6, 0x20, 0x2c, 0xc3]);

        // Mode-S long frame with 12-hex MLAT timestamp (starts with '@')
        let ts_long_raw = "@00000000234a8d4840d6202cc371c32ce0576098;\r\n";
        let parsed_ts_long = parse_avr_frame(ts_long_raw).unwrap();
        assert_eq!(parsed_ts_long, parsed_long);

        // Mode-S short frame with 12-hex MLAT timestamp
        let ts_short_raw = "@00000000234a5d4840d6202cc3;\n";
        let parsed_ts_short = parse_avr_frame(ts_short_raw).unwrap();
        assert_eq!(parsed_ts_short, parsed_short);

        // Without trailing semicolon
        let no_semi = "*8d4840d6202cc371c32ce0576098";
        assert_eq!(parse_avr_frame(no_semi).unwrap(), parsed_long);
    }

    #[test]
    fn rejects_corrupted_avr_frames() {
        // Invalid hex characters
        assert!(parse_avr_frame("*8d4840d6202cc371c32ce05760ZZ;\n").is_err());
        // Missing prefix
        assert!(parse_avr_frame("8d4840d6202cc371c32ce0576098;\n").is_err());
        // Odd number of hex characters
        assert!(parse_avr_frame("*8d4840d6202cc371c32ce057609;\n").is_err());
        // Invalid length (neither 7 nor 14 bytes)
        assert!(parse_avr_frame("*8d4840d620;\n").is_err());
        // Empty
        assert!(parse_avr_frame("").is_err());
        assert!(parse_avr_frame("   ").is_err());
    }

    #[test]
    fn parses_avr_stream_with_resync() {
        let mut stream_buf =
            b"some junk before\n*8d4840d6202cc371c32ce0576098;\n*5d4840d6202cc3;\npartial*frame"
                .to_vec();
        let frames = parse_avr_stream(&mut stream_buf);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].len(), 14);
        assert_eq!(frames[1].len(), 7);
        // Trailing partial frame remains in buffer for next read
        assert_eq!(stream_buf, b"partial*frame");
    }

    #[test]
    fn parses_beast_binary_frames() {
        // Mode-S Long (type 3 / 0x33): 6-byte ts, 1-byte signal, 14-byte payload
        let mut raw = vec![0x1a, 0x33, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0xee];
        let payload = vec![
            0x8d, 0x48, 0x40, 0xd6, 0x20, 0x2c, 0xc3, 0x71, 0xc3, 0x2c, 0xe0, 0x57, 0x60, 0x98,
        ];
        raw.extend_from_slice(&payload);

        let packet = parse_beast_packet(&raw).unwrap();
        assert_eq!(packet.msg_type, BeastMessageType::ModeSLong);
        assert_eq!(packet.timestamp, [0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
        assert_eq!(packet.signal_level, 0xee);
        assert_eq!(packet.payload, payload);

        // Mode-S Short (type 2 / 0x32): 7-byte payload
        let mut short_raw = vec![0x1a, 0x32, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0xcc];
        let short_payload = vec![0x5d, 0x48, 0x40, 0xd6, 0x20, 0x2c, 0xc3];
        short_raw.extend_from_slice(&short_payload);

        let short_pkt = parse_beast_packet(&short_raw).unwrap();
        assert_eq!(short_pkt.msg_type, BeastMessageType::ModeSShort);
        assert_eq!(short_pkt.timestamp, [0x10, 0x20, 0x30, 0x40, 0x50, 0x60]);
        assert_eq!(short_pkt.signal_level, 0xcc);
        assert_eq!(short_pkt.payload, short_payload);

        // Mode-AC (type 1 / 0x31): 2-byte payload
        let ac_raw = vec![0x1a, 0x31, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x88, 0x12, 0x34];
        let ac_pkt = parse_beast_packet(&ac_raw).unwrap();
        assert_eq!(ac_pkt.msg_type, BeastMessageType::ModeAc);
        assert_eq!(ac_pkt.payload, vec![0x12, 0x34]);
    }

    #[test]
    fn handles_beast_escaped_0x1a_bytes() {
        // Packet where timestamp has 0x1a (escaped as 0x1a 0x1a),
        // signal level has 0x1a (escaped as 0x1a 0x1a),
        // and payload has 0x1a (escaped as 0x1a 0x1a).
        let raw = vec![
            0x1a, 0x32, // start + type
            0x00, 0x1a, 0x1a, 0x00, 0x00, 0x01,
            0x02, // 6 ts bytes: [0x00, 0x1a, 0x00, 0x00, 0x01, 0x02]
            0x1a, 0x1a, // signal level 0x1a
            0x5d, 0x1a, 0x1a, 0x40, 0xd6, 0x20, 0x2c, 0xc3, // payload with 0x1a escaped
        ];

        let pkt = parse_beast_packet(&raw).unwrap();
        assert_eq!(pkt.msg_type, BeastMessageType::ModeSShort);
        assert_eq!(pkt.timestamp, [0x00, 0x1a, 0x00, 0x00, 0x01, 0x02]);
        assert_eq!(pkt.signal_level, 0x1a);
        assert_eq!(pkt.payload, vec![0x5d, 0x1a, 0x40, 0xd6, 0x20, 0x2c, 0xc3]);
        assert_eq!(pkt.payload.len(), 7);
    }

    #[test]
    fn handles_beast_corrupted_packets_and_resynchronization() {
        // 1. Incomplete packet: should not parse
        let incomplete = vec![0x1a, 0x33, 0x01, 0x02];
        assert!(parse_beast_packet(&incomplete).is_err());

        // 2. Corrupted packet mid-stream: unescaped 0x1a followed by a valid new packet start
        let mut buf = vec![
            0xff, 0xfe, // junk before
            0x1a, 0x33, 0x01, 0x02, 0x03, // corrupted start
            0x1a, 0x32, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x99, 0x5d, 0x48, 0x40, 0xd6, 0x20,
            0x2c, 0xc3, // valid second packet
        ];
        let packets = parse_beast_stream(&mut buf);
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].msg_type, BeastMessageType::ModeSShort);
        assert_eq!(packets[0].payload.len(), 7);

        // 3. Unknown packet type (e.g. 0x99)
        let invalid_type = vec![0x1a, 0x99, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(parse_beast_packet(&invalid_type).is_err());
    }

    #[test]
    fn extracts_mode_s_basic_frame_fields() {
        // Real-world DF 17 ADS-B message for KLM1023 (TC 4, callsign)
        let klm_frame = vec![
            0x8d, 0x48, 0x40, 0xd6, 0x20, 0x2c, 0xc3, 0x71, 0xc3, 0x2c, 0xe0, 0x57, 0x60, 0x98,
        ];
        let decoded = extract_mode_s_frame(&klm_frame).unwrap();
        assert_eq!(decoded.df, 17);
        assert_eq!(decoded.icao.as_deref(), Some("4840d6"));
        assert_eq!(decoded.tc, Some(4));
        assert_eq!(decoded.callsign.as_deref(), Some("KLM1023"));
        assert_eq!(decoded.category.as_deref(), Some("A0"));

        // DF 11 All-Call acquisition squitter (56-bit / 7-byte)
        let df11_frame = vec![0x5d, 0x48, 0x40, 0xd6, 0x20, 0x2c, 0xc3];
        let decoded_11 = extract_mode_s_frame(&df11_frame).unwrap();
        assert_eq!(decoded_11.df, 11);
        assert_eq!(decoded_11.icao.as_deref(), Some("4840d6"));
        assert_eq!(decoded_11.tc, None);

        // DF 17 Airborne Position with altitude (TC 11)
        // 8d40621258c382d690c8ac2863a7 (TC 11, baro alt)
        let alt_frame = vec![
            0x8d, 0x40, 0x62, 0x12, 0x58, 0xc3, 0x82, 0xd6, 0x90, 0xc8, 0xac, 0x28, 0x63, 0xa7,
        ];
        let decoded_alt = extract_mode_s_frame(&alt_frame).unwrap();
        assert_eq!(decoded_alt.df, 17);
        assert_eq!(decoded_alt.icao.as_deref(), Some("406212"));
        assert_eq!(decoded_alt.tc, Some(11));
        assert!(decoded_alt.altitude_baro_ft.is_some());
        assert!(decoded_alt.cpr.is_some());

        // Rejects invalid frame lengths
        assert!(extract_mode_s_frame(&[0x8d; 6]).is_err());
        assert!(extract_mode_s_frame(&[0x8d; 8]).is_err());
        assert!(extract_mode_s_frame(&[0x8d; 15]).is_err());
    }

    #[test]
    fn snapshot_generation_from_raw_ingest() {
        let engine = LocalReceiverEngine::new("");
        let now: Timestamp = "2026-10-26T10:00:00Z".parse().unwrap();

        // Initially no configured feeds
        let initial_snap = engine.snapshot(now);
        assert!(!initial_snap.configured);
        assert!(initial_snap.stale);
        assert_eq!(initial_snap.aircraft.len(), 0);

        // Ingest raw Mode-S extended squitter for KLM1023
        let raw_frame = vec![
            0x8d, 0x48, 0x40, 0xd6, 0x20, 0x2c, 0xc3, 0x71, 0xc3, 0x2c, 0xe0, 0x57, 0x60, 0x98,
        ];
        engine.ingest_raw_mode_s_frame(&raw_frame, now).unwrap();

        // Snapshot generated from raw ingest
        let snap = engine.snapshot(now);
        assert!(snap.configured);
        assert!(!snap.stale);
        assert_eq!(snap.aircraft.len(), 1);
        let ac = &snap.aircraft[0];
        assert_eq!(ac.hex, "4840d6");
        assert_eq!(ac.callsign.as_deref(), Some("KLM1023"));
        assert_eq!(ac.band, LocalReceiverBand::Band1090);
        assert_eq!(ac.messages, Some(1));
        assert_eq!(ac.seen_s, 0.0);

        // Feed status reflects live feed
        assert_eq!(snap.feeds.len(), 1);
        assert_eq!(snap.feeds[0].id, "raw-1090");
        assert_eq!(snap.feeds[0].aircraft_count, 1);
        assert!(!snap.feeds[0].stale);
    }

    #[test]
    fn validates_safe_local_ips() {
        use std::net::{Ipv4Addr, Ipv6Addr};

        // Safe loopback
        assert!(is_safe_local_ip(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(is_safe_local_ip(IpAddr::V4(Ipv4Addr::new(127, 255, 255, 254))));
        assert!(is_safe_local_ip(IpAddr::V6(Ipv6Addr::LOCALHOST)));

        // Safe RFC1918 private IPv4
        assert!(is_safe_local_ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_safe_local_ip(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_safe_local_ip(IpAddr::V4(Ipv4Addr::new(172, 31, 255, 254))));
        assert!(is_safe_local_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))));

        // Rejected public and metadata IPs
        assert!(!is_safe_local_ip(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_safe_local_ip(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        assert!(!is_safe_local_ip(IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254))));
        assert!(!is_safe_local_ip(IpAddr::V4(Ipv4Addr::new(172, 32, 0, 1))));
        assert!(!is_safe_local_ip(IpAddr::V4(Ipv4Addr::new(192, 169, 0, 1))));
        assert!(!is_safe_local_ip(IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
    }
}
