//! Radio Browser and tactical communications directory provider.
//!
//! Normalizes and validates georeferenced public radio stations from `RadioBrowser` payloads,
//! classifies tactical and broadcast categories (`aviation`, `marine`, `emergency`, `scanner`,
//! `weather`, `traffic`, `news`, `talk`, `general`), enforces public `HTTPS` stream policies and
//! browser-playable audio codecs, and maintains a thread-safe catalog seeded with a global
//! tactical baseline so the layer operates offline out-of-the-box.

use std::collections::HashMap;
use std::sync::{PoisonError, RwLock};

use jiff::Timestamp;
use lavender_domain::entities::{RadioCatalogSnapshot, RadioCategory, RadioStation};
use lavender_domain::text::{clean, clean_opt};
use lavender_domain::{BBox, GeoPoint};
use serde_json::Value;
use url::Url;

use crate::CoreError;

/// Default maximum stations returned in a catalog snapshot when no smaller limit is requested.
pub const RADIO_DEFAULT_CATALOG_LIMIT: usize = 750;

/// Normalizes a raw radio station tag into a clean lowercase space-separated token.
#[must_use]
pub fn normalize_radio_tag(raw: &str) -> String {
    let folded: String = raw
        .chars()
        .map(|c| if c == '_' || c == '-' { ' ' } else { c.to_ascii_lowercase() })
        .collect();
    clean(&folded, 80)
}

fn tokenize_words(input: &str) -> Vec<String> {
    input
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

fn matches_keyword(normalized_phrases: &[String], words: &[String], keyword: &str) -> bool {
    if keyword.contains(' ') {
        normalized_phrases.iter().any(|p| p.contains(keyword))
    } else if keyword.len() <= 3 {
        // Short acronyms (`atc`, `vhf`, `ems`, `ham`) must match whole tokens to avoid
        // false-positives inside words like `hamburg`, `watch`, or `items`.
        words.iter().any(|w| w == keyword) || normalized_phrases.iter().any(|p| p == keyword)
    } else {
        words.iter().any(|w| w == keyword || w.starts_with(keyword))
            || normalized_phrases.iter().any(|p| p.contains(keyword))
    }
}

/// Classifies a radio station into a [`RadioCategory`] from its normalized tags and display name.
#[must_use]
pub fn classify_radio_category(tags: &[String], name: &str) -> RadioCategory {
    let mut phrases: Vec<String> = tags.iter().map(|t| normalize_radio_tag(t)).collect();
    let cleaned_name = normalize_radio_tag(name);
    if !cleaned_name.is_empty() {
        phrases.push(cleaned_name);
    }

    let words: Vec<String> = phrases.iter().flat_map(|p| tokenize_words(p)).collect();

    let has_any = |keywords: &[&str]| -> bool {
        keywords.iter().any(|kw| matches_keyword(&phrases, &words, kw))
    };

    if has_any(&["aviation", "atc", "airband", "air traffic", "airport", "atis", "tower"]) {
        RadioCategory::Aviation
    } else if has_any(&[
        "marine",
        "maritime",
        "vhf",
        "coast guard",
        "coastguard",
        "navtex",
        "harbor",
        "harbour",
    ]) {
        RadioCategory::Marine
    } else if has_any(&[
        "emergency",
        "ems",
        "fire",
        "civil defense",
        "civil defence",
        "public safety",
        "dispatch",
        "police",
    ]) {
        RadioCategory::Emergency
    } else if has_any(&["scanner", "ham", "amateur", "repeater", "shortwave"]) {
        RadioCategory::Scanner
    } else if has_any(&["weather", "noaa", "volmet", "meteorology", "storm"]) {
        RadioCategory::Weather
    } else if has_any(&["traffic", "transit", "highway", "commuter"]) {
        RadioCategory::Traffic
    } else if has_any(&["news", "current affairs", "journalism", "world service", "public radio"]) {
        RadioCategory::News
    } else if has_any(&["talk", "spoken word", "interview", "podcast"]) {
        RadioCategory::Talk
    } else {
        RadioCategory::General
    }
}

/// Returns `true` if `hostname` is a non-global or private IPv4 address.
#[must_use]
pub fn is_non_global_ipv4(hostname: &str) -> bool {
    let mut octets = [0_u8; 4];
    let mut count = 0_usize;
    for piece in hostname.split('.') {
        if count >= 4
            || piece.is_empty()
            || piece.len() > 3
            || !piece.bytes().all(|b| b.is_ascii_digit())
        {
            return false;
        }
        let Ok(val) = piece.parse::<u8>() else {
            return true;
        };
        octets[count] = val;
        count += 1;
    }
    if count != 4 {
        return false;
    }
    let [a, b, c, _] = octets;
    a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0)
        || (a == 192 && b == 88 && c == 99)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
}

/// Validates that `raw` is a safe public `HTTPS` URL without credentials or private/local hosts.
#[must_use]
#[allow(clippy::case_sensitive_file_extension_comparisons)]
pub fn public_radio_https_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut url = Url::parse(trimmed).ok()?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let host = url
        .host_str()?
        .to_ascii_lowercase()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_owned();

    if host.is_empty()
        || host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.contains(':')
        || !host.contains('.')
        || is_non_global_ipv4(&host)
    {
        return None;
    }

    url.set_fragment(None);
    Some(url.into())
}

fn normalize_browser_codec(raw: &str) -> Option<String> {
    let cleaned = clean(raw, 16).to_ascii_uppercase();
    match cleaned.as_str() {
        "MP3" => Some("MP3".to_owned()),
        "AAC" | "AAC-LC" => Some("AAC".to_owned()),
        "AAC+" | "AAC-HE" | "HE-AAC" => Some("AAC+".to_owned()),
        "OGG" | "VORBIS" => Some("OGG".to_owned()),
        "OPUS" => Some("OPUS".to_owned()),
        "FLAC" => Some("FLAC".to_owned()),
        _ => None,
    }
}

fn value_as_f64(val: Option<&Value>) -> Option<f64> {
    match val? {
        Value::Number(n) => n.as_f64().filter(|v| v.is_finite()),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() { None } else { t.parse::<f64>().ok().filter(|v| v.is_finite()) }
        }
        _ => None,
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn value_as_u32_clamped(val: Option<&Value>, max: u32) -> u32 {
    let raw = match val {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0),
        Some(Value::String(s)) => s.trim().parse::<i64>().unwrap_or(0),
        _ => 0,
    };
    if raw <= 0 { 0 } else { u32::try_from(raw).unwrap_or(max).min(max) }
}

fn is_lastcheckok(val: Option<&Value>) -> bool {
    match val {
        Some(Value::Number(n)) => n.as_i64() == Some(1),
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s.trim() == "1" || s.trim().eq_ignore_ascii_case("true"),
        _ => false,
    }
}

/// Validates and normalizes a single `RadioBrowser` station JSON object into a [`RadioStation`].
///
/// Enforces:
/// - `lastcheckok == 1`
/// - Validated public `HTTPS` `url_resolved` or `url` (no credentials, no private/local hosts)
/// - Permitted browser audio codec (`MP3`, `AAC`, `AAC+`, `OGG`, `OPUS`, `FLAC`)
/// - Finite `geo_lat` and `geo_long` within valid WGS84 bounds, rejecting `(0, 0)` null-island
/// - Cleaned non-empty station `name` and normalized `tags`
#[must_use]
pub fn normalize_radio_browser_station(raw: &Value) -> Option<RadioStation> {
    let obj = raw.as_object()?;

    if !is_lastcheckok(obj.get("lastcheckok")) {
        return None;
    }

    let id = obj
        .get("stationuuid")
        .or_else(|| obj.get("id"))
        .and_then(Value::as_str)
        .map(|s| clean(s, 64).to_ascii_lowercase())
        .filter(|s| !s.is_empty())?;

    let name =
        obj.get("name").and_then(Value::as_str).map(|s| clean(s, 140)).filter(|s| !s.is_empty())?;

    let stream_url = obj
        .get("url_resolved")
        .and_then(Value::as_str)
        .and_then(public_radio_https_url)
        .or_else(|| obj.get("url").and_then(Value::as_str).and_then(public_radio_https_url))?;

    let codec = obj.get("codec").and_then(Value::as_str).and_then(normalize_browser_codec)?;

    let lat = value_as_f64(obj.get("geo_lat").or_else(|| obj.get("lat")))?;
    let lon = value_as_f64(obj.get("geo_long").or_else(|| obj.get("lon")))?;

    // Reject (0, 0) null-island and out-of-range coordinates.
    if lat.abs() < 1e-5 && lon.abs() < 1e-5 {
        return None;
    }
    let position = GeoPoint::new(lat, lon).ok()?;

    let homepage = obj.get("homepage").and_then(Value::as_str).and_then(public_radio_https_url);

    let mut tags: Vec<String> = Vec::new();
    match obj.get("tags") {
        Some(Value::String(s)) => {
            for part in s.split(',') {
                let tag = normalize_radio_tag(part);
                if !tag.is_empty() && !tags.contains(&tag) && tags.len() < 24 {
                    tags.push(tag);
                }
            }
        }
        Some(Value::Array(arr)) => {
            for item in arr {
                if let Some(s) = item.as_str() {
                    let tag = normalize_radio_tag(s);
                    if !tag.is_empty() && !tags.contains(&tag) && tags.len() < 24 {
                        tags.push(tag);
                    }
                }
            }
        }
        _ => {}
    }

    let category = classify_radio_category(&tags, &name);

    let country_code = obj
        .get("countrycode")
        .or_else(|| obj.get("country_code"))
        .and_then(Value::as_str)
        .map(|s| clean(s, 2).to_ascii_uppercase())
        .filter(|s| s.len() == 2 && s.bytes().all(|b| b.is_ascii_uppercase()))
        .unwrap_or_else(|| "XX".to_owned());

    let state = clean_opt(obj.get("state").and_then(Value::as_str), 80);

    let language =
        clean_opt(obj.get("language").or_else(|| obj.get("languages")).and_then(Value::as_str), 80);

    let bitrate_kbps =
        value_as_u32_clamped(obj.get("bitrate").or_else(|| obj.get("bitrate_kbps")), 10_000);
    let votes = value_as_u32_clamped(obj.get("votes"), 100_000_000);
    let click_count =
        value_as_u32_clamped(obj.get("clickcount").or_else(|| obj.get("click_count")), 100_000_000);

    Some(RadioStation {
        id,
        name,
        stream_url,
        homepage,
        country_code,
        state,
        language,
        tags,
        category,
        codec,
        bitrate_kbps,
        votes,
        click_count,
        position,
    })
}

#[allow(clippy::too_many_lines)]
fn curated_baseline_stations() -> Vec<RadioStation> {
    struct Seed {
        id: &'static str,
        name: &'static str,
        stream_url: &'static str,
        homepage: &'static str,
        country_code: &'static str,
        state: Option<&'static str>,
        language: &'static str,
        tags: &'static [&'static str],
        category: RadioCategory,
        codec: &'static str,
        bitrate_kbps: u32,
        votes: u32,
        click_count: u32,
        lat: f64,
        lon: f64,
    }

    let seeds = [
        Seed {
            id: "seed-kjfk-tower-1090",
            name: "JFK Tower & Approach Airband",
            stream_url: "https://streams.radiomast.io/kjfk-atc-main.mp3",
            homepage: "https://www.faa.gov",
            country_code: "US",
            state: Some("New York"),
            language: "english",
            tags: &["aviation", "atc", "airband", "tower"],
            category: RadioCategory::Aviation,
            codec: "MP3",
            bitrate_kbps: 64,
            votes: 420,
            click_count: 1850,
            lat: 40.6413,
            lon: -73.7781,
        },
        Seed {
            id: "seed-eham-schiphol-airband",
            name: "Amsterdam Schiphol Approach ATC",
            stream_url: "https://streams.radiomast.io/eham-atc-approach.mp3",
            homepage: "https://www.lvnl.nl",
            country_code: "NL",
            state: Some("North Holland"),
            language: "english",
            tags: &["aviation", "atc", "airband"],
            category: RadioCategory::Aviation,
            codec: "MP3",
            bitrate_kbps: 64,
            votes: 310,
            click_count: 1420,
            lat: 52.3105,
            lon: 4.7683,
        },
        Seed {
            id: "seed-uscg-sector-sf-vhf",
            name: "San Francisco Bay Marine VHF Ch 16",
            stream_url: "https://streams.radiomast.io/sf-marine-vhf16.aac",
            homepage: "https://www.navcen.uscg.gov",
            country_code: "US",
            state: Some("California"),
            language: "english",
            tags: &["marine", "maritime", "vhf", "coast guard"],
            category: RadioCategory::Marine,
            codec: "AAC",
            bitrate_kbps: 48,
            votes: 275,
            click_count: 980,
            lat: 37.8083,
            lon: -122.4756,
        },
        Seed {
            id: "seed-fdny-citywide-dispatch",
            name: "NYC Metro Fire & EMS Dispatch",
            stream_url: "https://streams.radiomast.io/nyc-fire-ems.mp3",
            homepage: "https://www.nyc.gov/fdny",
            country_code: "US",
            state: Some("New York"),
            language: "english",
            tags: &["emergency", "fire", "ems", "dispatch"],
            category: RadioCategory::Emergency,
            codec: "MP3",
            bitrate_kbps: 32,
            votes: 510,
            click_count: 2300,
            lat: 40.6943,
            lon: -73.9866,
        },
        Seed {
            id: "seed-w6ek-sierra-ham-repeater",
            name: "W6EK Sierra Amateur Radio Repeater 145.430",
            stream_url: "https://streams.radiomast.io/w6ek-repeater.mp3",
            homepage: "https://www.w6ek.org",
            country_code: "US",
            state: Some("California"),
            language: "english",
            tags: &["scanner", "ham", "amateur", "repeater"],
            category: RadioCategory::Scanner,
            codec: "MP3",
            bitrate_kbps: 48,
            votes: 190,
            click_count: 740,
            lat: 38.8986,
            lon: -121.0769,
        },
        Seed {
            id: "seed-noaa-kwo35-wx",
            name: "NOAA Weather Radio KWO35 New York",
            stream_url: "https://wxradio.org/streams/kwo35.mp3",
            homepage: "https://www.weather.gov/nwr",
            country_code: "US",
            state: Some("New York"),
            language: "english",
            tags: &["weather", "noaa", "storm"],
            category: RadioCategory::Weather,
            codec: "MP3",
            bitrate_kbps: 64,
            votes: 380,
            click_count: 1610,
            lat: 40.7580,
            lon: -73.9855,
        },
        Seed {
            id: "seed-shannon-volmet-wx",
            name: "Shannon Aeradio North Atlantic VOLMET",
            stream_url: "https://streams.radiomast.io/shannon-volmet.ogg",
            homepage: "https://www.airnav.ie",
            country_code: "IE",
            state: Some("Clare"),
            language: "english",
            tags: &["weather", "volmet", "meteorology"],
            category: RadioCategory::Weather,
            codec: "OGG",
            bitrate_kbps: 64,
            votes: 260,
            click_count: 890,
            lat: 52.7019,
            lon: -8.9248,
        },
        Seed {
            id: "seed-bbc-world-service-london",
            name: "BBC World Service London",
            stream_url: "https://stream.live.vc.bbcmedia.co.uk/bbc_world_service",
            homepage: "https://www.bbc.co.uk/worldserviceradio",
            country_code: "GB",
            state: Some("London"),
            language: "english",
            tags: &["news", "world service", "current affairs"],
            category: RadioCategory::News,
            codec: "AAC",
            bitrate_kbps: 96,
            votes: 4800,
            click_count: 19500,
            lat: 51.5186,
            lon: -0.1436,
        },
        Seed {
            id: "seed-france-info-paris",
            name: "franceinfo Paris",
            stream_url: "https://icecast.radiofrance.fr/franceinfo-midfi.mp3",
            homepage: "https://www.francetvinfo.fr",
            country_code: "FR",
            state: Some("Ile-de-France"),
            language: "french",
            tags: &["news", "journalism"],
            category: RadioCategory::News,
            codec: "MP3",
            bitrate_kbps: 128,
            votes: 2100,
            click_count: 8400,
            lat: 48.8524,
            lon: 2.2782,
        },
        Seed {
            id: "seed-kqed-public-talk-sf",
            name: "KQED 88.5 FM Public Radio & Talk",
            stream_url: "https://streams.kqed.org/kqedradio",
            homepage: "https://www.kqed.org",
            country_code: "US",
            state: Some("California"),
            language: "english",
            tags: &["talk", "interview", "spoken word"],
            category: RadioCategory::Talk,
            codec: "MP3",
            bitrate_kbps: 128,
            votes: 1450,
            click_count: 6200,
            lat: 37.7627,
            lon: -122.4098,
        },
        Seed {
            id: "seed-autoroute-info-1077",
            name: "Autoroute INFO 107.7 Traffic",
            stream_url: "https://media.autorouteinfo.fr/direct_nord.mp3",
            homepage: "https://www.autorouteinfo.fr",
            country_code: "FR",
            state: Some("Bourgogne"),
            language: "french",
            tags: &["traffic", "highway", "commuter"],
            category: RadioCategory::Traffic,
            codec: "MP3",
            bitrate_kbps: 128,
            votes: 630,
            click_count: 2750,
            lat: 47.3220,
            lon: 5.0415,
        },
        Seed {
            id: "seed-soma-fm-mission-control",
            name: "SomaFM Mission Control Ambient",
            stream_url: "https://ice1.somafm.com/missioncontrol-128-mp3",
            homepage: "https://somafm.com/missioncontrol",
            country_code: "US",
            state: Some("California"),
            language: "english",
            tags: &["ambient", "electronic", "space"],
            category: RadioCategory::General,
            codec: "MP3",
            bitrate_kbps: 128,
            votes: 920,
            click_count: 4100,
            lat: 37.7897,
            lon: -122.4014,
        },
    ];

    seeds
        .into_iter()
        .filter_map(|s| {
            let position = GeoPoint::new(s.lat, s.lon).ok()?;
            Some(RadioStation {
                id: s.id.to_owned(),
                name: s.name.to_owned(),
                stream_url: s.stream_url.to_owned(),
                homepage: Some(s.homepage.to_owned()),
                country_code: s.country_code.to_owned(),
                state: s.state.map(str::to_owned),
                language: Some(s.language.to_owned()),
                tags: s.tags.iter().map(|t| (*t).to_owned()).collect(),
                category: s.category,
                codec: s.codec.to_owned(),
                bitrate_kbps: s.bitrate_kbps,
                votes: s.votes,
                click_count: s.click_count,
                position,
            })
        })
        .collect()
}

#[derive(Debug)]
struct CatalogState {
    as_of: Timestamp,
    instance: String,
    generation: u64,
    stale: bool,
    stations: Vec<RadioStation>,
}

/// Thread-safe georeferenced radio station catalog engine.
#[derive(Debug)]
pub struct RadioCatalogEngine {
    state: RwLock<CatalogState>,
}

impl Default for RadioCatalogEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RadioCatalogEngine {
    /// Creates a new [`RadioCatalogEngine`] seeded with curated global tactical, weather,
    /// aviation, marine, and news stations so the layer works offline out-of-the-box.
    #[must_use]
    pub fn new() -> Self {
        let mut stations = curated_baseline_stations();
        sort_stations(&mut stations);
        Self {
            state: RwLock::new(CatalogState {
                as_of: Timestamp::now(),
                instance: "lavender-radio-catalog-v1".to_owned(),
                generation: 1,
                stale: false,
                stations,
            }),
        }
    }

    /// Ingests a `RadioBrowser` JSON station list payload, normalizing and merging valid stations
    /// into the catalog and advancing the catalog generation.
    ///
    /// Returns the number of valid stations accepted from `payload`.
    ///
    /// # Errors
    /// Returns [`CoreError::BadRequest`] if `payload` is not valid JSON or not a station array.
    pub fn ingest_radio_browser_json(
        &self,
        payload: &[u8],
        now: Timestamp,
    ) -> Result<usize, CoreError> {
        let parsed: Value = serde_json::from_slice(payload)
            .map_err(|e| CoreError::BadRequest(format!("invalid Radio Browser JSON: {e}")))?;

        let rows = if let Some(arr) = parsed.as_array() {
            arr
        } else if let Some(arr) = parsed.get("stations").and_then(Value::as_array) {
            arr
        } else {
            return Err(CoreError::BadRequest(
                "Radio Browser payload must be a JSON array of stations".to_owned(),
            ));
        };

        let mut accepted: Vec<RadioStation> = Vec::new();
        for row in rows {
            if let Some(station) = normalize_radio_browser_station(row) {
                accepted.push(station);
            }
        }

        let accepted_count = accepted.len();
        let mut guard = self.state.write().unwrap_or_else(PoisonError::into_inner);

        let mut by_id: HashMap<String, RadioStation> =
            guard.stations.drain(..).map(|s| (s.id.clone(), s)).collect();

        for station in accepted {
            by_id.insert(station.id.clone(), station);
        }

        let mut merged: Vec<RadioStation> = by_id.into_values().collect();
        sort_stations(&mut merged);

        guard.stations = merged;
        guard.as_of = now;
        guard.generation = guard.generation.saturating_add(1);
        guard.stale = false;

        Ok(accepted_count)
    }

    /// Produces a filtered [`RadioCatalogSnapshot`] constrained by an optional bounding box,
    /// optional [`RadioCategory`], and maximum station count `limit`.
    #[must_use]
    pub fn snapshot(
        &self,
        bbox: Option<&BBox>,
        category: Option<RadioCategory>,
        limit: usize,
    ) -> RadioCatalogSnapshot {
        let guard = self.state.read().unwrap_or_else(PoisonError::into_inner);
        let total_stations = guard.stations.len();

        let stations: Vec<RadioStation> = guard
            .stations
            .iter()
            .filter(|s| bbox.is_none_or(|b| b.contains(&s.position)))
            .filter(|s| category.is_none_or(|cat| s.category == cat))
            .take(limit)
            .cloned()
            .collect();

        RadioCatalogSnapshot {
            as_of: guard.as_of,
            instance: guard.instance.clone(),
            generation: guard.generation,
            stale: guard.stale,
            total_stations,
            stations,
        }
    }
}

fn sort_stations(stations: &mut [RadioStation]) {
    stations.sort_by(|a, b| {
        b.click_count
            .cmp(&a.click_count)
            .then_with(|| b.votes.cmp(&a.votes))
            .then_with(|| a.name.cmp(&b.name))
    });
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn classifies_tactical_and_broadcast_categories() {
        assert_eq!(
            classify_radio_category(&["airband".to_owned(), "atc".to_owned()], "LiveATC KJFK"),
            RadioCategory::Aviation
        );
        assert_eq!(
            classify_radio_category(&["marine".to_owned(), "vhf".to_owned()], "Coast Guard Ch 16"),
            RadioCategory::Marine
        );
        assert_eq!(
            classify_radio_category(&["fire".to_owned(), "ems".to_owned()], "Metro Dispatch"),
            RadioCategory::Emergency
        );
        assert_eq!(
            classify_radio_category(&["amateur".to_owned(), "ham".to_owned()], "W6EK Repeater"),
            RadioCategory::Scanner
        );
        assert_eq!(
            classify_radio_category(&["noaa".to_owned(), "weather".to_owned()], "WX Radio"),
            RadioCategory::Weather
        );
        assert_eq!(
            classify_radio_category(&["volmet".to_owned()], "Shannon VOLMET"),
            RadioCategory::Weather
        );
        assert_eq!(
            classify_radio_category(&["traffic".to_owned()], "Highway 107.7"),
            RadioCategory::Traffic
        );
        assert_eq!(
            classify_radio_category(&["news".to_owned()], "BBC World Service"),
            RadioCategory::News
        );
        assert_eq!(
            classify_radio_category(&["talk".to_owned(), "podcast".to_owned()], "Morning Talk"),
            RadioCategory::Talk
        );
        // "Radio Hamburg" must not false-positive on "ham"
        assert_eq!(
            classify_radio_category(&["pop".to_owned(), "rock".to_owned()], "Radio Hamburg"),
            RadioCategory::General
        );
    }

    #[test]
    fn normalizes_radio_browser_station_and_enforces_security_policy() {
        let valid = json!({
            "stationuuid": "9617a958-0601-11e8-ae97-52543be04c81",
            "name": "  Geneva Tower Airband \n ",
            "url_resolved": "https://streams.example.org/lsgg-atc.mp3#frag",
            "homepage": "https://www.skyguide.ch",
            "countrycode": "ch",
            "state": "Geneva",
            "language": "english,french",
            "tags": "aviation,atc,Airband,atc",
            "codec": "mp3",
            "bitrate": 128,
            "votes": 84,
            "clickcount": 512,
            "geo_lat": 46.2381,
            "geo_long": 6.1089,
            "lastcheckok": 1
        });

        let station = normalize_radio_browser_station(&valid).unwrap();
        assert_eq!(station.id, "9617a958-0601-11e8-ae97-52543be04c81");
        assert_eq!(station.name, "Geneva Tower Airband");
        assert_eq!(station.stream_url, "https://streams.example.org/lsgg-atc.mp3");
        assert_eq!(station.homepage.as_deref(), Some("https://www.skyguide.ch/"));
        assert_eq!(station.country_code, "CH");
        assert_eq!(station.state.as_deref(), Some("Geneva"));
        assert_eq!(station.category, RadioCategory::Aviation);
        assert_eq!(station.codec, "MP3");
        assert_eq!(station.bitrate_kbps, 128);
        assert_eq!(station.votes, 84);
        assert_eq!(station.click_count, 512);
        assert_eq!(station.tags, vec!["aviation", "atc", "airband"]);

        // Reject HTTP (non-HTTPS) stream
        let mut http_stream = valid.clone();
        http_stream["url_resolved"] = json!("http://streams.example.org/lsgg.mp3");
        assert!(normalize_radio_browser_station(&http_stream).is_none());

        // Reject stream with credentials
        let mut cred_stream = valid.clone();
        cred_stream["url_resolved"] = json!("https://user:pass@streams.example.org/lsgg.mp3");
        assert!(normalize_radio_browser_station(&cred_stream).is_none());

        // Reject private/local stream targets
        let mut private_stream = valid.clone();
        private_stream["url_resolved"] = json!("https://192.168.1.5/stream.mp3");
        assert!(normalize_radio_browser_station(&private_stream).is_none());

        // Reject unsupported codec (WMA / HLS)
        let mut bad_codec = valid.clone();
        bad_codec["codec"] = json!("WMA");
        assert!(normalize_radio_browser_station(&bad_codec).is_none());

        // Reject null-island (0, 0)
        let mut null_island = valid.clone();
        null_island["geo_lat"] = json!(0.0);
        null_island["geo_long"] = json!(0.0);
        assert!(normalize_radio_browser_station(&null_island).is_none());

        // Reject offline station (lastcheckok != 1)
        let mut offline = valid;
        offline["lastcheckok"] = json!(0);
        assert!(normalize_radio_browser_station(&offline).is_none());
    }

    #[test]
    fn catalog_engine_starts_seeded_and_supports_filtering_and_ingestion() {
        let engine = RadioCatalogEngine::new();
        let initial = engine.snapshot(None, None, 100);
        assert!(initial.total_stations >= 10);
        assert_eq!(initial.generation, 1);
        assert!(!initial.stale);

        // Filter by Aviation category
        let aviation = engine.snapshot(None, Some(RadioCategory::Aviation), 100);
        assert!(aviation.stations.len() >= 2);
        assert!(aviation.stations.iter().all(|s| s.category == RadioCategory::Aviation));

        // Ingest new station payload and verify generation advances
        let now: Timestamp = "2026-10-26T10:15:00Z".parse().unwrap();
        let payload = serde_json::to_vec(&json!([
            {
                "stationuuid": "11111111-2222-3333-4444-555555555555",
                "name": "Tokyo Maritime VHF Watch",
                "url_resolved": "https://streams.example.jp/tokyo-vhf.opus",
                "countrycode": "JP",
                "tags": "marine,vhf",
                "codec": "OPUS",
                "bitrate": 64,
                "votes": 100,
                "clickcount": 99999,
                "geo_lat": 35.6197,
                "geo_long": 139.7798,
                "lastcheckok": 1
            }
        ]))
        .unwrap();

        let ingested = engine.ingest_radio_browser_json(&payload, now).unwrap();
        assert_eq!(ingested, 1);

        let japan_bbox = BBox::new(138.0, 34.0, 141.0, 37.0).unwrap();
        let snap = engine.snapshot(Some(&japan_bbox), Some(RadioCategory::Marine), 10);
        assert_eq!(snap.generation, 2);
        assert_eq!(snap.stations.len(), 1);
        assert_eq!(snap.stations[0].name, "Tokyo Maritime VHF Watch");
    }
}
