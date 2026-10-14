//! Aircraft from adsb.lol (`ODbL` 1.0). Regional point/radius queries per grid cell, and the
//! worldwide military feed when a viewport is too large for regional queries. Civil and
//! military contacts share one feed with an `isMilitary` flag (parity matrix: military
//! merged into aircraft).

use std::time::Duration;

use async_trait::async_trait;
use jiff::{SignedDuration, Timestamp};
use lavender_domain::text::clean_opt;
use lavender_domain::{Aircraft, FeedId, FeedItems, GeoPoint, ProviderScope};
use serde::Deserialize;
use url::Url;

use super::{FeedProvider, FetchContext, Fetched, ProviderError, Region, lenient, parse_rows};
use crate::config::AdsbLolConfig;
use crate::fetch::{FetchOptions, HostPolicy};
use crate::sources;

const MAX_BODY: usize = 12 * 1024 * 1024;
const FEET_PER_METRE: f64 = 3.280_84;
/// `dbFlags` bit marking a military registration in the adsb.lol database.
const DB_FLAG_MILITARY: u32 = 1;

#[derive(Debug)]
pub struct AdsbLolProvider {
    config: AdsbLolConfig,
    policy: HostPolicy,
}

impl AdsbLolProvider {
    #[must_use]
    pub fn new(config: AdsbLolConfig) -> Self {
        // adsb.lol rejects bursts with 420/429: space requests across every cell.
        let policy = HostPolicy::for_url(&config.base_url)
            .with_min_interval(Duration::from_millis(config.min_request_interval_ms));
        Self { config, policy }
    }
}

/// The document must be an object whose `ac` (when present and not null) is an array;
/// each aircraft is then parsed on its own.
#[derive(Debug, Deserialize)]
struct Response {
    #[serde(default, deserialize_with = "lenient")]
    now: Option<serde_json::Number>,
    #[serde(default)]
    ac: Option<Vec<serde_json::Value>>,
}

#[derive(Debug)]
struct ParsedDocument {
    aircraft: Vec<Aircraft>,
    dropped: usize,
    now: Option<serde_json::Number>,
    reliable_position_ages: bool,
}

/// adsb.lol forwards readsb's `jv2`, which uses milliseconds; compatible services
/// also publish epoch seconds with millisecond resolution (ADS-B Exchange V2).
/// Disjoint 2000–2100 ranges avoid guessing units for malformed/ambiguous values.
fn document_time(value: Option<&serde_json::Number>, received_at: Timestamp) -> Option<Timestamp> {
    const START_2000: f64 = 946_684_800.0;
    const START_2100: f64 = 4_102_444_800.0;
    let value = value?;
    let number = value.as_f64().filter(|number| number.is_finite())?;
    let time = if (START_2000..START_2100).contains(&number) {
        let duration = SignedDuration::try_from_secs_f64(number).ok()?;
        Timestamp::UNIX_EPOCH.checked_add(duration).ok()?
    } else if (START_2000 * 1000.0..START_2100 * 1000.0).contains(&number) {
        if let Some(milliseconds) = value.as_i64() {
            Timestamp::from_millisecond(milliseconds).ok()?
        } else {
            let duration = SignedDuration::try_from_secs_f64(number / 1000.0).ok()?;
            Timestamp::UNIX_EPOCH.checked_add(duration).ok()?
        }
    } else {
        return None;
    };
    (time <= received_at).then_some(time)
}

// Every field is lenient: a wrong type reads as missing, and `normalize_aircraft` drops the record
// only when a required field (hex, lat, lon) is missing or invalid.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct RawAircraft {
    #[serde(default, deserialize_with = "lenient")]
    hex: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    flight: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    r: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    t: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    category: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    lat: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    lon: Option<f64>,
    #[serde(default)]
    alt_baro: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "lenient")]
    alt_geom: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    gs: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    track: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    baro_rate: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    geom_rate: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    squawk: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    emergency: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    seen_pos: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    seen: Option<f64>,
    #[serde(rename = "dbFlags", default, deserialize_with = "lenient")]
    db_flags: Option<u32>,
}

fn finite(v: Option<f64>) -> Option<f64> {
    v.filter(|x| x.is_finite())
}

fn valid_hex(hex: &str) -> bool {
    let body = hex.strip_prefix('~').unwrap_or(hex);
    body.len() == 6 && body.chars().all(|c| c.is_ascii_hexdigit())
}

fn normalize_aircraft(raw: RawAircraft, military_feed: bool) -> Option<Aircraft> {
    let hex = raw.hex?.trim().to_ascii_lowercase();
    if !valid_hex(&hex) {
        return None;
    }
    let position = GeoPoint::new(raw.lat?, raw.lon?).ok()?;
    let (on_ground, altitude_baro_ft) = match raw.alt_baro {
        Some(serde_json::Value::String(s)) if s == "ground" => (true, None),
        Some(serde_json::Value::Number(n)) => (false, finite(n.as_f64())),
        _ => (false, None),
    };
    let altitude_geom_ft = finite(raw.alt_geom);
    let position = position
        .with_altitude(altitude_baro_ft.or(altitude_geom_ft).map(|ft| ft / FEET_PER_METRE))
        .ok()?;
    let is_military = military_feed || raw.db_flags.is_some_and(|f| f & DB_FLAG_MILITARY != 0);
    Some(Aircraft {
        hex,
        callsign: clean_opt(raw.flight.as_deref(), 12),
        registration: clean_opt(raw.r.as_deref(), 16),
        type_code: clean_opt(raw.t.as_deref(), 8),
        category: clean_opt(raw.category.as_deref(), 4),
        position,
        altitude_baro_ft,
        altitude_geom_ft,
        on_ground,
        ground_speed_kt: finite(raw.gs).filter(|v| (0.0..2_500.0).contains(v)),
        track_deg: finite(raw.track).filter(|v| (0.0..=360.0).contains(v)),
        vertical_rate_fpm: finite(raw.baro_rate).or(finite(raw.geom_rate)),
        squawk: clean_opt(raw.squawk.as_deref(), 4)
            .filter(|s| s.chars().all(|c| c.is_ascii_digit())),
        emergency: clean_opt(raw.emergency.as_deref(), 16).filter(|e| e != "none"),
        is_military,
        seen_pos_s: finite(raw.seen_pos).or(finite(raw.seen)).unwrap_or(0.0).max(0.0),
        source: sources::ADSB_LOL.to_owned(),
    })
}

/// Parses an adsb.lol v2 response. Returns the valid aircraft and how many records were
/// skipped.
///
/// # Errors
/// Returns an error when the document is not an adsb.lol aircraft list.
pub fn parse(body: &[u8], military_feed: bool) -> Result<(Vec<Aircraft>, usize), ProviderError> {
    let document = parse_document(body, military_feed)?;
    Ok((document.aircraft, document.dropped))
}

fn parse_document(body: &[u8], military_feed: bool) -> Result<ParsedDocument, ProviderError> {
    let response: Response =
        serde_json::from_slice(body).map_err(|e| ProviderError::parse("adsb.lol", &e))?;
    let rows = response.ac.unwrap_or_default();
    let total = rows.len();
    let (raw, _) = parse_rows::<RawAircraft>(rows);
    let mut reliable_position_ages = true;
    let aircraft: Vec<Aircraft> = raw
        .into_iter()
        .filter_map(|raw| {
            let reliable_age = raw.seen_pos.is_some_and(|age| age.is_finite() && age >= 0.0);
            let aircraft = normalize_aircraft(raw, military_feed)?;
            // `seen` measures any message, not a position. Keep display compatibility,
            // but never turn that fallback into a reliable historical observation.
            reliable_position_ages &= reliable_age;
            Some(aircraft)
        })
        .collect();
    let dropped = total - aircraft.len();
    Ok(ParsedDocument { aircraft, dropped, now: response.now, reliable_position_ages })
}

impl AdsbLolProvider {
    fn url_for(&self, region: Region) -> Result<(Url, bool), ProviderError> {
        let path = match region {
            Region::Global => "v2/mil".to_owned(),
            Region::Cell(cell) => {
                let c = cell.center();
                let radius = cell.covering_radius_nm().ceil().clamp(1.0, 250.0);
                format!("v2/point/{:.4}/{:.4}/{radius:.0}", c.lat, c.lon)
            }
        };
        let url = self
            .config
            .base_url
            .join(&path)
            .map_err(|e| ProviderError::Unconfigured(format!("adsb.lol URL: {e}")))?;
        Ok((url, region == Region::Global))
    }
}

#[async_trait]
impl FeedProvider for AdsbLolProvider {
    fn feed(&self) -> FeedId {
        FeedId::Aircraft
    }
    fn source_id(&self) -> &'static str {
        sources::ADSB_LOL
    }
    fn cadence(&self) -> Duration {
        Duration::from_secs(self.config.cadence_s)
    }
    fn scope(&self) -> ProviderScope {
        ProviderScope::Regional { cell_deg: self.config.cell_deg, max_cells: self.config.max_cells }
    }
    fn enabled(&self) -> bool {
        self.config.enabled
    }
    fn global_fallback_notice(&self) -> Option<&'static str> {
        Some("Showing military aircraft worldwide. Zoom in to see all traffic.")
    }

    async fn fetch(
        &self,
        ctx: &FetchContext<'_>,
        region: Region,
    ) -> Result<Fetched, ProviderError> {
        let (url, military) = self.url_for(region)?;
        let opts = FetchOptions {
            max_body_bytes: Some(MAX_BODY),
            cancel: Some(ctx.cancel.clone()),
            ..FetchOptions::default()
        };
        let response = ctx.fetcher.get(&url, &self.policy, &opts).await?;
        let document = parse_document(&response.body, military)?;
        let as_of = document
            .reliable_position_ages
            .then(|| document_time(document.now.as_ref(), Timestamp::now()))
            .flatten();
        let dropped = document.dropped;
        if dropped > 0 {
            tracing::debug!(dropped, "adsb.lol records skipped during validation");
        }
        let mut fetched = Fetched::fresh(FeedItems::Aircraft(document.aircraft))
            .with_dropped("adsb.lol", dropped);
        fetched.as_of = as_of;
        if as_of.is_none() {
            let reason = if document.reliable_position_ages {
                "adsb.lol document time is missing or invalid; aircraft history was not updated."
            } else {
                "adsb.lol position ages are missing or invalid; aircraft history was not updated."
            };
            fetched.degraded = Some(reason.to_owned());
        }
        Ok(fetched)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_and_flags_military() {
        let body = br#"{"ac":[
            {"hex":"4CA1B2","flight":"RYR12AB ","r":"EI-ABC","t":"B738","lat":53.4,"lon":-6.2,"alt_baro":35000,"gs":450.2,"track":91.5,"baro_rate":-64,"squawk":"7700","emergency":"general","seen_pos":1.2},
            {"hex":"ae1234","lat":38.9,"lon":-77.0,"alt_baro":"ground","dbFlags":1},
            {"hex":"zzzzzz","lat":1,"lon":1},
            {"hex":"abcdef"},
            {"hex":"~2a1b3c","lat":91,"lon":0}
        ]}"#;
        let (ac, dropped) = parse(body, false).unwrap();
        assert_eq!((ac.len(), dropped), (2, 3));
        assert_eq!(ac[0].hex, "4ca1b2");
        assert_eq!(ac[0].callsign.as_deref(), Some("RYR12AB"));
        assert_eq!(ac[0].emergency.as_deref(), Some("general"));
        assert!((ac[0].position.alt_m.unwrap() - 35_000.0 / FEET_PER_METRE).abs() < 1e-6);
        assert!(!ac[0].is_military);
        assert!(ac[1].on_ground && ac[1].is_military && ac[1].altitude_baro_ft.is_none());
    }

    #[test]
    fn military_feed_marks_everything_military() {
        let body = br#"{"ac":[{"hex":"ae0001","lat":10,"lon":10,"alt_baro":20000}]}"#;
        assert!(parse(body, true).unwrap().0[0].is_military);
    }

    #[test]
    fn rejects_non_lists() {
        assert!(parse(b"<html>rate limited</html>", false).is_err());
        assert!(parse(br#"{"ac":"nope"}"#, false).is_err());
        assert_eq!(parse(br#"{"ac":null}"#, false).unwrap().0.len(), 0);
    }

    #[test]
    fn wrong_typed_fields_drop_only_their_row() {
        // Review finding 10: `"lat":"n/a"` used to reject every aircraft in the response.
        let body = br#"{"ac":[
            {"hex":"4ca1b2","lat":"n/a","lon":-6.2},
            {"hex":"4ca1b3","lat":53.4,"lon":-6.2,"gs":"fast","dbFlags":"x","squawk":7700},
            [1,2,3]
        ]}"#;
        let (ac, dropped) = parse(body, false).unwrap();
        assert_eq!(ac.len(), 1);
        assert_eq!(ac[0].hex, "4ca1b3");
        assert_eq!(ac[0].ground_speed_kt, None);
        assert_eq!(ac[0].squawk, None, "a numeric squawk is the wrong type and reads as missing");
        assert_eq!(dropped, 2);
    }

    #[test]
    fn timestamp_units_preserve_milliseconds_and_seen_pos_is_in_seconds() {
        let received_at: Timestamp = "2026-10-13T12:00:00Z".parse().unwrap();
        for now in [serde_json::json!(1_760_000_000.125), serde_json::json!(1_760_000_000_125_i64)]
        {
            let body = serde_json::to_vec(&serde_json::json!({"now": now, "ac":[
                {"hex":"abcdef","lat":60,"lon":24,"seen_pos":2.125,"seen":0.25}
            ]}))
            .unwrap();
            let document = parse_document(&body, false).unwrap();
            let as_of = document_time(document.now.as_ref(), received_at).unwrap();
            assert_eq!(as_of, Timestamp::from_millisecond(1_760_000_000_125).unwrap());
            assert!(document.reliable_position_ages);
            assert!((document.aircraft[0].seen_pos_s - 2.125).abs() < f64::EPSILON);
            let observation = as_of
                .checked_sub(
                    SignedDuration::try_from_secs_f64(document.aircraft[0].seen_pos_s).unwrap(),
                )
                .unwrap();
            assert_eq!(observation, Timestamp::from_second(1_759_999_998).unwrap());
        }
    }

    #[test]
    fn invalid_or_future_document_times_do_not_make_the_aircraft_list_unreadable() {
        let received_at: Timestamp = "2026-10-13T12:00:00Z".parse().unwrap();
        for now in [
            serde_json::Value::Null,
            serde_json::json!("private timestamp"),
            serde_json::json!(-1),
            serde_json::json!(0),
            serde_json::json!(1e30),
            serde_json::json!(2_000_000_000),
            serde_json::json!(2_000_000_000_000_i64),
        ] {
            let body = serde_json::to_vec(&serde_json::json!({"now": now, "ac":[
                {"hex":"abcdef","lat":60,"lon":24,"seen_pos":1}
            ]}))
            .unwrap();
            let document = parse_document(&body, false).unwrap();
            assert_eq!((document.aircraft.len(), document.dropped), (1, 0));
            assert_eq!(document_time(document.now.as_ref(), received_at), None);
            assert_eq!(parse(&body, false).unwrap().0.len(), 1);
        }
    }

    #[test]
    fn point_query_url_uses_cell_centre_and_bounded_radius() {
        let p = AdsbLolProvider::new(AdsbLolConfig::default());
        let cell = lavender_geo::plan_cells(
            &lavender_domain::BBox::new(24.0, 60.0, 25.0, 60.5).unwrap(),
            5.0,
            1,
        )
        .cells[0];
        let (url, mil) = p.url_for(Region::Cell(cell)).unwrap();
        assert!(!mil);
        assert!(
            url.as_str().starts_with("https://api.adsb.lol/v2/point/62.5000/22.5000/"),
            "{url}"
        );
        let radius: f64 = url.path_segments().unwrap().next_back().unwrap().parse().unwrap();
        assert!(radius <= 250.0);
        assert_eq!(p.url_for(Region::Global).unwrap().0.as_str(), "https://api.adsb.lol/v2/mil");
    }
}
