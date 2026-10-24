//! Normalization, coordinate filtering, and heading derivation helpers for CCTV.

#![allow(
    clippy::collapsible_if,
    clippy::if_not_else,
    clippy::assigning_clones,
    clippy::must_use_candidate,
    clippy::doc_markdown,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]

use super::constants::CAMERA_CODE_MAX_CHARS;

const B64_URL_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encode bytes into URL-safe base64 without padding.
#[must_use]
pub fn base64_url_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity((input.len() * 4).div_ceil(3));
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };

        let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        out.push(B64_URL_ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(B64_URL_ALPHABET[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_URL_ALPHABET[((n >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(B64_URL_ALPHABET[(n & 63) as usize] as char);
        }
    }
    out
}

/// Decode standard or URL-safe base64 string into bytes.
#[must_use]
pub fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let clean = input.trim().trim_end_matches('=');
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;

    for &b in clean.as_bytes() {
        let val = match b {
            b'A'..=b'Z' => u32::from(b - b'A'),
            b'a'..=b'z' => u32::from(b - b'a' + 26),
            b'0'..=b'9' => u32::from(b - b'0' + 52),
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b' ' | b'\r' | b'\n' | b'\t' => continue,
            _ => return None,
        };
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

/// FNV-1a 32-bit hash of a string, used to derive deterministic pseudo-random
/// values (hue for synthetic SVG billboards, fallback heading angles).
#[must_use]
pub fn hash_seed(text: &str) -> u32 {
    let mut h: u32 = 2_166_136_261;
    for &b in text.as_bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(16_777_619);
    }
    h
}

/// Derive a deterministic fallback heading from a camera ID hash.
/// Produces one of 16 evenly-spaced compass directions (0, 22.5, 45, ...).
#[must_use]
pub fn fallback_heading_from_id(camera_id: &str) -> f64 {
    f64::from((hash_seed(camera_id) % 16) * 45) / 2.0
}

/// Escape special XML/HTML characters for safe embedding in SVG text nodes.
#[must_use]
pub fn escape_xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Short display code for the unselected camera label ("CAM-<code>"):
/// the feed's own name trimmed to [`CAMERA_CODE_MAX_CHARS`].
#[must_use]
pub fn camera_display_code(text: &str) -> String {
    let collapsed: Vec<&str> = text.split_whitespace().collect();
    let clean = collapsed.join(" ");
    let char_count = clean.chars().count();
    if char_count <= CAMERA_CODE_MAX_CHARS {
        clean
    } else {
        let prefix: String = clean.chars().take(CAMERA_CODE_MAX_CHARS.saturating_sub(1)).collect();
        format!("{}…", prefix.trim_end())
    }
}

/// Finite, within lat/lon bounds, and not null island (0, 0).
#[must_use]
pub fn is_plausible_lat_lon(lat: f64, lon: f64) -> bool {
    lat.is_finite()
        && lon.is_finite()
        && (-90.0..=90.0).contains(&lat)
        && (-180.0..=180.0).contains(&lon)
        && !(lat.abs() < 1e-6 && lon.abs() < 1e-6)
}

#[must_use]
pub fn is_likely_austin(lat: f64, lon: f64) -> bool {
    is_plausible_lat_lon(lat, lon)
        && (30.02..=30.58).contains(&lat)
        && (-98.12..=-97.4).contains(&lon)
}

#[must_use]
pub fn is_likely_texas(lat: f64, lon: f64) -> bool {
    is_plausible_lat_lon(lat, lon)
        && (25.5..=36.7).contains(&lat)
        && (-107.0..=-93.4).contains(&lon)
}

#[must_use]
pub fn is_likely_bc(lat: f64, lon: f64) -> bool {
    is_plausible_lat_lon(lat, lon)
        && (48.0..=60.5).contains(&lat)
        && (-139.5..=-114.0).contains(&lon)
}

#[must_use]
pub fn is_likely_nsw(lat: f64, lon: f64) -> bool {
    is_plausible_lat_lon(lat, lon)
        && (-38.0..=-28.0).contains(&lat)
        && (140.9..=159.2).contains(&lon)
}

#[must_use]
pub fn is_likely_calgary(lat: f64, lon: f64) -> bool {
    is_plausible_lat_lon(lat, lon)
        && (50.8..=51.25).contains(&lat)
        && (-114.4..=-113.8).contains(&lon)
}

#[must_use]
pub fn is_likely_norway(lat: f64, lon: f64) -> bool {
    is_plausible_lat_lon(lat, lon) && (57.9..=71.3).contains(&lat) && (4.4..=31.3).contains(&lon)
}

#[must_use]
pub fn is_likely_finland(lat: f64, lon: f64) -> bool {
    is_plausible_lat_lon(lat, lon) && (59.5..=70.5).contains(&lat) && (19.0..=32.0).contains(&lon)
}

/// Convert compass direction words or abbreviations to heading degrees [0..360).
#[must_use]
pub fn direction_to_heading(text: &str, allow_bare_cardinal: bool) -> Option<f64> {
    let upper = text.trim().to_uppercase();
    if upper.is_empty() {
        return None;
    }

    // Direct direction abbreviations or names
    if allow_bare_cardinal {
        match upper.as_str() {
            "N" | "NORTH" => return Some(0.0),
            "NNE" | "NORTH-NORTHEAST" | "NORTH NORTHEAST" => return Some(22.5),
            "NE" | "NORTHEAST" | "NORTH-EAST" | "N-E" => return Some(45.0),
            "ENE" | "EAST-NORTHEAST" | "EAST NORTHEAST" => return Some(67.5),
            "E" | "EAST" => return Some(90.0),
            "ESE" | "EAST-SOUTHEAST" | "EAST SOUTHEAST" => return Some(112.5),
            "SE" | "SOUTHEAST" | "SOUTH-EAST" | "S-E" => return Some(135.0),
            "SSE" | "SOUTH-SOUTHEAST" | "SOUTH SOUTHEAST" => return Some(157.5),
            "S" | "SOUTH" => return Some(180.0),
            "SSW" | "SOUTH-SOUTHWEST" | "SOUTH SOUTHWEST" => return Some(202.5),
            "SW" | "SOUTHWEST" | "SOUTH-WEST" | "S-W" => return Some(225.0),
            "WSW" | "WEST-SOUTHWEST" | "WEST SOUTHWEST" => return Some(247.5),
            "W" | "WEST" => return Some(270.0),
            "WNW" | "WEST-NORTHWEST" | "WEST NORTHWEST" => return Some(292.5),
            "NW" | "NORTHWEST" | "NORTH-WEST" | "N-W" => return Some(315.0),
            "NNW" | "NORTH-NORTHWEST" | "NORTH NORTHWEST" => return Some(337.5),
            _ => {}
        }
    }

    // Explicit directional suffix / travel tokens
    if upper.contains("NORTHBOUND")
        || upper.contains(" NB ")
        || upper.ends_with(" NB")
        || upper.starts_with("NB ")
    {
        return Some(0.0);
    }
    if upper.contains("EASTBOUND")
        || upper.contains(" EB ")
        || upper.ends_with(" EB")
        || upper.starts_with("EB ")
    {
        return Some(90.0);
    }
    if upper.contains("SOUTHBOUND")
        || upper.contains(" SB ")
        || upper.ends_with(" SB")
        || upper.starts_with("SB ")
    {
        return Some(180.0);
    }
    if upper.contains("WESTBOUND")
        || upper.contains(" WB ")
        || upper.ends_with(" WB")
        || upper.starts_with("WB ")
    {
        return Some(270.0);
    }

    if upper.contains("LOOKING NORTH")
        || upper.contains("FACING NORTH")
        || upper.contains("TOWARDS NORTH")
    {
        return Some(0.0);
    }
    if upper.contains("LOOKING EAST")
        || upper.contains("FACING EAST")
        || upper.contains("TOWARDS EAST")
    {
        return Some(90.0);
    }
    if upper.contains("LOOKING SOUTH")
        || upper.contains("FACING SOUTH")
        || upper.contains("TOWARDS SOUTH")
    {
        return Some(180.0);
    }
    if upper.contains("LOOKING WEST")
        || upper.contains("FACING WEST")
        || upper.contains("TOWARDS WEST")
    {
        return Some(270.0);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_seed_deterministic() {
        assert_eq!(hash_seed("test"), hash_seed("test"));
        assert_ne!(hash_seed("test1"), hash_seed("test2"));
    }

    #[test]
    fn test_escape_xml() {
        assert_eq!(escape_xml("A & B < C > D ' E \""), "A &amp; B &lt; C &gt; D &#39; E &quot;");
    }

    #[test]
    fn test_camera_display_code() {
        assert_eq!(camera_display_code("  Short   Name  "), "Short Name");
        let long =
            "Very Long Road Name That Exceeds Maximum Characters Permitted For A Camera Code";
        let display = camera_display_code(long);
        assert!(display.ends_with('…'));
        assert!(display.chars().count() <= CAMERA_CODE_MAX_CHARS);
    }

    #[test]
    fn test_direction_to_heading() {
        assert_eq!(direction_to_heading("N", true), Some(0.0));
        assert_eq!(direction_to_heading("N-E", true), Some(45.0));
        assert_eq!(direction_to_heading("W", true), Some(270.0));
        assert_eq!(direction_to_heading("Main St EB", false), Some(90.0));
        assert_eq!(direction_to_heading("I-35 Northbound", false), Some(0.0));
        assert_eq!(direction_to_heading("West Ave", false), None);
    }

    #[test]
    fn test_is_plausible_lat_lon() {
        assert!(is_plausible_lat_lon(30.2, -97.7));
        assert!(!is_plausible_lat_lon(0.0, 0.0));
        assert!(!is_plausible_lat_lon(100.0, 0.0));
        assert!(!is_plausible_lat_lon(f64::NAN, 0.0));
    }
}
