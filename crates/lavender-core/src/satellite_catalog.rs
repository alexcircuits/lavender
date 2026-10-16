//! Satellite catalogue storage and launch payload matching.
//!
//! A launch is matched to the tracked satellites it carried. Once a launch reaches orbit,
//! Launch Library 2 gives it a COSPAR launch designator (`2026-230`) and every object from
//! that launch carries it in its international designator (`2026-230A`, `2026-230B` …),
//! so those objects, minus rocket bodies and debris, are its payloads. Only a launch that
//! Launch Library 2 reports as having reached orbit (successful, or its payload deployed)
//! but that has no designator yet falls back to name scoring, which rejects namesakes from
//! other launch years. Any other launch (upcoming, on hold, in flight, failed) can only
//! match a namesake, so it matches nothing.

use std::collections::HashMap;
use std::sync::{PoisonError, RwLock};

use jiff::Timestamp;
use lavender_domain::{Launch, SatelliteElements};

/// Minimum score for a satellite name match to be accepted.
pub const MATCH_THRESHOLD: u32 = 12;

/// Ignored generic words in satellite name scoring.
const IGNORED_TOKENS: [&str; 6] = ["group", "block", "mission", "launch", "falcon", "rocket"];

/// Launch Library 2 statuses (names, and the abbreviations older answers used) saying the
/// launch reached orbit.
const REACHED_ORBIT_STATUSES: [&str; 4] =
    ["launch successful", "payload deployed", "success", "deployed"];

/// Scores a mission-to-catalogue name match. Compact identifier containment handles
/// names such as "Sirius SXM-11" → "SXM-11" (score ≥ 200), while weak generic matches
/// such as "Starlink Group 17-40" → an arbitrary "STARLINK-1008" remain below the
/// acceptance threshold (score < 12).
#[must_use]
pub fn score_satellite_name_match(query: &str, catalog_name: &str) -> u32 {
    let q = query.trim().to_lowercase();
    let name = catalog_name.trim().to_lowercase();
    if q.is_empty() || name.is_empty() {
        return 0;
    }

    let q_compact = compact(&q);
    let name_compact = compact(&name);
    if q_compact == name_compact {
        return 1000;
    }

    let mut score = 0;
    let min_len = q_compact.len().min(name_compact.len());
    if min_len >= 5 && (q_compact.contains(&name_compact) || name_compact.contains(&q_compact)) {
        score += 200 + u32::try_from(min_len).unwrap_or(0);
    }
    for token in q.split(|c: char| !c.is_ascii_alphanumeric()) {
        if token.len() >= 4 && !IGNORED_TOKENS.contains(&token) && name.contains(token) {
            score += u32::try_from(token.len()).unwrap_or(0);
        }
    }
    score
}

/// Lower-case ASCII letters and digits only.
fn compact(text: &str) -> String {
    text.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()).collect()
}

/// Extracts the 4-digit launch year from an international designator (COSPAR ID),
/// e.g. "2026-148A" -> 2026, "26148A" -> 2026, "98067A" -> 1998.
#[must_use]
pub fn extract_designator_year(intl_designator: &str) -> Option<i32> {
    let trimmed = intl_designator.trim();
    if let Some((year, _)) = trimmed.split_once('-')
        && year.len() == 4
        && year.bytes().all(|b| b.is_ascii_digit())
    {
        return year.parse().ok();
    }
    // Two-digit short year: "YY...". `get` refuses to split a multi-byte character.
    let short = trimmed.get(..2).filter(|yy| yy.bytes().all(|b| b.is_ascii_digit()))?;
    let short: i32 = short.parse().ok()?;
    Some(if short >= 57 { 1900 + short } else { 2000 + short })
}

/// Extracts the UTC year from a timestamp.
#[must_use]
pub fn timestamp_year(ts: Timestamp) -> i32 {
    i32::from(ts.to_zoned(jiff::tz::TimeZone::UTC).year())
}

/// Whether `satellite` came from the launch with COSPAR designator `launch_designator`:
/// its own designator is the launch's followed by a piece code of one to three letters.
fn launched_by(satellite: &SatelliteElements, launch_designator: &str) -> bool {
    satellite
        .intl_designator
        .as_deref()
        .and_then(|own| own.trim().strip_prefix(launch_designator))
        .is_some_and(|piece| {
            (1..=3).contains(&piece.len()) && piece.bytes().all(|b| b.is_ascii_uppercase())
        })
}

/// Rocket bodies ("CZ-12 R/B") and debris ("CZ-6A DEB") come from a launch but are not
/// what it carried.
fn is_payload(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    !upper.contains("R/B") && !upper.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| w == "DEB")
}

/// The best name match for `query`, skipping satellites from another launch year.
fn best_name_match(
    query: &str,
    launch_year: Option<i32>,
    satellites: &[&SatelliteElements],
) -> Option<u32> {
    let query = query.trim();
    if query.is_empty() {
        return None;
    }
    let query_compact = compact(query);
    let mut best: Option<(u32, u32)> = None;
    for sat in satellites {
        let sat_year = sat.intl_designator.as_deref().and_then(extract_designator_year);
        if launch_year.zip(sat_year).is_some_and(|(launch, sat)| launch != sat) {
            continue;
        }
        if sat
            .intl_designator
            .as_deref()
            .map(compact)
            .is_some_and(|d| !d.is_empty() && d == query_compact)
        {
            return Some(sat.norad_id);
        }
        let score = score_satellite_name_match(query, &sat.name);
        if best.is_none_or(|(best_score, _)| score > best_score) {
            best = Some((score, sat.norad_id));
        }
    }
    best.filter(|(score, _)| *score >= MATCH_THRESHOLD).map(|(_, id)| id)
}

/// Name matching for a launch without a designator: each payload name, else the mission
/// name, else the launch name.
fn match_by_name(launch: &Launch, satellites: &[&SatelliteElements]) -> Vec<u32> {
    let year = launch.net.or(launch.window_start).map(timestamp_year);
    let by_payload: Vec<u32> = launch
        .payload_names
        .iter()
        .filter_map(|payload| best_name_match(payload, year, satellites))
        .collect();
    if !by_payload.is_empty() {
        return by_payload;
    }
    launch
        .mission
        .iter()
        .chain(std::iter::once(&launch.name))
        .find_map(|query| best_name_match(query, year, satellites))
        .into_iter()
        .collect()
}

/// Whether Launch Library 2 reports that `launch` reached orbit by `now`.
fn reached_orbit(launch: &Launch, now: Timestamp) -> bool {
    let status = launch.status.trim().to_lowercase();
    launch.net.is_some_and(|net| net <= now) && REACHED_ORBIT_STATUSES.contains(&status.as_str())
}

/// Sets `launch.matched_norad_ids` to the catalogue numbers of the satellites it carried,
/// as seen at `now` (see the module documentation for the rules).
pub fn match_launch_satellites<'a>(
    launch: &mut Launch,
    satellites: impl IntoIterator<Item = &'a SatelliteElements>,
    now: Timestamp,
) {
    let mut ids: Vec<u32> = match launch.launch_designator.as_deref() {
        Some(designator) => satellites
            .into_iter()
            .filter(|sat| launched_by(sat, designator) && is_payload(&sat.name))
            .map(|sat| sat.norad_id)
            .collect(),
        None if reached_orbit(launch, now) => {
            let satellites: Vec<&SatelliteElements> = satellites.into_iter().collect();
            match_by_name(launch, &satellites)
        }
        None => Vec::new(),
    };
    ids.sort_unstable();
    ids.dedup();
    launch.matched_norad_ids = ids;
}

/// Shared thread-safe catalogue of satellite elements used for launch payload matching.
#[derive(Debug, Default)]
pub struct SatelliteCatalog {
    satellites: RwLock<HashMap<u32, SatelliteElements>>,
}

impl SatelliteCatalog {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Updates or inserts satellite orbital elements into the catalogue.
    pub fn update(&self, elements: &[SatelliteElements]) {
        let mut map = self.satellites.write().unwrap_or_else(PoisonError::into_inner);
        for s in elements {
            map.insert(s.norad_id, s.clone());
        }
    }

    /// Returns the satellite with the given NORAD ID.
    #[must_use]
    pub fn get(&self, norad_id: u32) -> Option<SatelliteElements> {
        self.satellites.read().unwrap_or_else(PoisonError::into_inner).get(&norad_id).cloned()
    }

    /// Returns all satellites currently held in the catalogue.
    #[must_use]
    pub fn all(&self) -> Vec<SatelliteElements> {
        self.satellites.read().unwrap_or_else(PoisonError::into_inner).values().cloned().collect()
    }

    /// Matches each launch against the current catalogue, as seen at `now`.
    pub fn match_launches(&self, launches: &mut [Launch], now: Timestamp) {
        let map = self.satellites.read().unwrap_or_else(PoisonError::into_inner);
        for launch in launches {
            match_launch_satellites(launch, map.values(), now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_satellite_name_match() {
        // Sirius SXM-11 -> SXM-11 match
        assert!(score_satellite_name_match("Sirius SXM-11", "SXM-11") >= 200);

        // Constellation rejection
        assert!(score_satellite_name_match("Starlink Group 17-40", "STARLINK-1008") < 12);

        // Exact match
        assert_eq!(score_satellite_name_match("ISS", "ISS"), 1000);

        // Empty cases
        assert_eq!(score_satellite_name_match("", "ISS"), 0);
        assert_eq!(score_satellite_name_match("ISS", ""), 0);
    }

    #[test]
    fn test_extract_designator_year() {
        assert_eq!(extract_designator_year("2026-148A"), Some(2026));
        assert_eq!(extract_designator_year("26148A"), Some(2026));
        assert_eq!(extract_designator_year("1998-067A"), Some(1998));
        assert_eq!(extract_designator_year("98067A"), Some(1998));
        assert_eq!(extract_designator_year("57001A"), Some(1957));
        assert_eq!(extract_designator_year(""), None);
    }

    fn object(norad_id: u32, name: &str, intl_designator: &str) -> SatelliteElements {
        SatelliteElements {
            norad_id,
            name: name.to_owned(),
            intl_designator: Some(intl_designator.to_owned()),
            group: "test".to_owned(),
            epoch: "2026-10-10T00:00:00Z".parse().unwrap(),
            mean_motion: 15.0,
            eccentricity: 0.001,
            inclination_deg: 51.6,
            raan_deg: 0.0,
            arg_of_pericenter_deg: 0.0,
            mean_anomaly_deg: 0.0,
            bstar: 0.0,
            mean_motion_dot: 0.0,
            mean_motion_ddot: 0.0,
            rev_at_epoch: None,
            classification: None,
        }
    }

    #[test]
    fn a_designator_matches_its_own_pieces_only_and_never_rocket_bodies_or_debris() {
        let catalog = [
            object(1, "SAT A", "2026-230A"),
            object(2, "SAT AB", "2026-230AB"),
            object(3, "CZ-12 R/B", "2026-230K"),
            object(4, "VEGA C DEB [VESPA]", "2026-230L"),
            object(5, "OTHER", "2026-231A"),
            object(6, "NOT A PIECE", "2026-2301"),
            object(7, "LOWER CASE", "2026-230a"),
        ];
        let mut launch = Launch {
            id: "l".to_owned(),
            name: "Rocket | Mission".to_owned(),
            status: "Launch Successful".to_owned(),
            net: Some("2026-10-09T19:27:00Z".parse().unwrap()),
            window_start: None,
            window_end: None,
            launch_designator: Some("2026-230".to_owned()),
            provider: None,
            rocket: None,
            mission: None,
            mission_description: None,
            orbit: None,
            pad: None,
            payload_names: Vec::new(),
            matched_norad_ids: vec![99],
        };
        match_launch_satellites(&mut launch, &catalog, "2026-10-15T00:00:00Z".parse().unwrap());
        assert_eq!(launch.matched_norad_ids, vec![1, 2], "stale matches are replaced");
    }

    fn launch_without_designator(status: &str) -> Launch {
        Launch {
            id: "l".to_owned(),
            name: "Falcon 9 | Sirius SXM-11".to_owned(),
            status: status.to_owned(),
            net: Some("2026-10-09T19:27:00Z".parse().unwrap()),
            window_start: None,
            window_end: None,
            launch_designator: None,
            provider: None,
            rocket: None,
            mission: Some("Sirius SXM-11".to_owned()),
            mission_description: None,
            orbit: None,
            pad: None,
            payload_names: vec!["SXM-11".to_owned()],
            matched_norad_ids: Vec::new(),
        }
    }

    #[test]
    fn names_are_matched_only_once_the_launch_is_reported_to_have_reached_orbit() {
        // The NET has passed in every case, but only a successful launch (or one whose
        // payload is deployed) can have put a satellite into the catalogue; anything else
        // could only match a namesake.
        let catalog = [object(60100, "SXM-11", "2026-050A")];
        let after_the_net = "2026-10-15T00:00:00Z".parse().unwrap();
        let matched = |status: &str| {
            let mut launch = launch_without_designator(status);
            match_launch_satellites(&mut launch, &catalog, after_the_net);
            launch.matched_norad_ids
        };
        for reached_orbit in ["Launch Successful", "Success", "Payload Deployed", " deployed "] {
            assert_eq!(matched(reached_orbit), vec![60100], "{reached_orbit}");
        }
        for not_in_orbit in [
            "Go for Launch",
            "To Be Determined",
            "To Be Confirmed",
            "On Hold",
            "Launch in Flight",
            "Launch Failure",
            "Launch was a Partial Failure",
            "Unknown",
        ] {
            assert!(matched(not_in_orbit).is_empty(), "{not_in_orbit}");
        }
    }

    #[test]
    fn designator_year_rejects_non_ascii_without_panicking() {
        // CelesTrak OBJECT_ID is untrusted text; a multi-byte first character must not
        // be sliced through.
        assert_eq!(extract_designator_year("€26-001A"), None);
        assert_eq!(extract_designator_year("2é"), None);
    }
}
