//! Turn-by-turn steps from an OSRM route, formatted in plain English.
//!
//! OSRM returns each step as a maneuver `type` + `modifier` (+ roundabout `exit`)
//! and road name; it does not phrase natural instructions. This module generates
//! plain English instructions matching standard OSRM navigation vocabulary.

use lavender_domain::RouteStep;
use serde::Deserialize;

/// Maximum number of maneuver steps returned for one route.
pub const ROUTE_STEPS_MAX: usize = 200;

/// Road label for a step: "Name (Ref)", "Name", "Ref", or "".
#[must_use]
pub fn road_label(name: &str, r#ref: Option<&str>) -> String {
    let clean_name = name.trim();
    let clean_ref = r#ref.map_or("", str::trim);
    if !clean_name.is_empty() && !clean_ref.is_empty() {
        if clean_name.contains(clean_ref) {
            clean_name.to_owned()
        } else {
            format!("{clean_name} ({clean_ref})")
        }
    } else if !clean_name.is_empty() {
        clean_name.to_owned()
    } else if !clean_ref.is_empty() {
        clean_ref.to_owned()
    } else {
        String::new()
    }
}

fn onto_road(name: &str, r#ref: Option<&str>) -> String {
    let road = road_label(name, r#ref);
    if road.is_empty() { String::new() } else { format!(" onto {road}") }
}

fn direction(modifier: &str) -> &str {
    let m = modifier.trim();
    if m.eq_ignore_ascii_case("left") {
        "left"
    } else if m.eq_ignore_ascii_case("right") {
        "right"
    } else if m.eq_ignore_ascii_case("slight left") {
        "slightly left"
    } else if m.eq_ignore_ascii_case("slight right") {
        "slightly right"
    } else if m.eq_ignore_ascii_case("sharp left") {
        "sharply left"
    } else if m.eq_ignore_ascii_case("sharp right") {
        "sharply right"
    } else if m.eq_ignore_ascii_case("straight") {
        "straight"
    } else if m.eq_ignore_ascii_case("uturn") {
        "around"
    } else {
        m
    }
}

fn ordinal(n: u32) -> Option<String> {
    if n == 0 {
        return None;
    }
    let mod100 = n % 100;
    let suffix = if (11..=13).contains(&mod100) {
        "th"
    } else {
        match n % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    };
    Some(format!("{n}{suffix}"))
}

/// One English sentence describing an OSRM step maneuver.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn instruction_for(
    maneuver_type: &str,
    modifier: Option<&str>,
    exit: Option<u32>,
    name: &str,
    r#ref: Option<&str>,
) -> String {
    let t = maneuver_type.trim().to_lowercase();
    let mod_raw = modifier.map(|m| m.trim().to_lowercase()).unwrap_or_default();
    let dir = direction(&mod_raw);
    let onto = onto_road(name, r#ref);
    let road = road_label(name, r#ref);

    match t.as_str() {
        "depart" => {
            if road.is_empty() {
                "Head out".to_owned()
            } else {
                format!("Head out on {road}")
            }
        }
        "arrive" => {
            if mod_raw == "left" || mod_raw == "right" {
                format!("Arrive at your destination, on the {mod_raw}")
            } else {
                "Arrive at your destination".to_owned()
            }
        }
        "turn" => {
            if mod_raw == "uturn" {
                format!("Make a U-turn{onto}")
            } else if mod_raw == "straight" {
                format!("Continue straight{onto}")
            } else if dir.is_empty() {
                format!("Turn{onto}")
            } else {
                format!("Turn {dir}{onto}")
            }
        }
        "continue" => {
            if mod_raw == "uturn" {
                format!("Make a U-turn{onto}")
            } else if dir.is_empty() || dir == "straight" {
                format!("Continue straight{onto}")
            } else {
                format!("Continue {dir}{onto}")
            }
        }
        "end of road" => {
            if dir.is_empty() {
                format!("At the end of the road, continue{onto}")
            } else {
                format!("At the end of the road, turn {dir}{onto}")
            }
        }
        "fork" => {
            if dir.is_empty() {
                format!("Continue at the fork{onto}")
            } else {
                format!("Keep {dir} at the fork{onto}")
            }
        }
        "merge" => {
            if dir.is_empty() {
                format!("Merge{onto}")
            } else {
                format!("Merge {dir}{onto}")
            }
        }
        "on ramp" => {
            if dir.is_empty() {
                format!("Take the ramp{onto}")
            } else {
                format!("Take the ramp on the {dir}{onto}")
            }
        }
        "off ramp" => {
            if dir.is_empty() {
                format!("Take the exit{onto}")
            } else {
                format!("Take the exit on the {dir}{onto}")
            }
        }
        "roundabout" | "rotary" => {
            let exit_str = exit.and_then(ordinal);
            if let Some(ord) = exit_str {
                format!("At the roundabout, take the {ord} exit{onto}")
            } else {
                format!("Enter the roundabout{onto}")
            }
        }
        "roundabout turn" => {
            if dir.is_empty() {
                format!("At the roundabout, continue{onto}")
            } else {
                format!("At the roundabout, turn {dir}{onto}")
            }
        }
        "exit roundabout" | "exit rotary" => format!("Exit the roundabout{onto}"),
        "use lane" => {
            if dir.is_empty() {
                format!("Continue{onto}")
            } else {
                format!("Use the {dir} lane{onto}")
            }
        }
        _ => format!("Continue{onto}"),
    }
}

/// Raw OSRM response types for deserialization.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct RawOsrmResponse {
    pub code: Option<String>,
    #[serde(default)]
    pub routes: Vec<RawOsrmRoute>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RawOsrmRoute {
    #[serde(default)]
    pub distance: f64,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub geometry: RawGeometry,
    #[serde(default)]
    pub legs: Vec<RawOsrmLeg>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RawGeometry {
    #[serde(default)]
    pub coordinates: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RawOsrmLeg {
    #[serde(default)]
    pub steps: Vec<RawOsrmStep>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RawOsrmStep {
    pub maneuver: Option<RawManeuver>,
    pub name: Option<String>,
    pub r#ref: Option<String>,
    pub distance: Option<f64>,
    pub duration: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RawManeuver {
    pub location: Option<[f64; 2]>,
    pub r#type: Option<String>,
    pub modifier: Option<String>,
    pub exit: Option<u32>,
}

/// Flatten an OSRM route's legs into the compact step list the proxy serves.
///
/// "exit roundabout" / "exit rotary" steps are folded into the roundabout step
/// before them, so a reader gets one instruction per decision.
///
/// Routes exceeding [`ROUTE_STEPS_MAX`] are capped, returning `truncated: true`.
#[must_use]
pub fn normalize_osrm_steps(legs: &[RawOsrmLeg]) -> (Vec<RouteStep>, bool) {
    let mut out: Vec<RouteStep> = Vec::new();

    for leg in legs {
        for raw in &leg.steps {
            let Some(location) = raw.maneuver.as_ref().and_then(|m| m.location) else {
                continue;
            };
            let lon = location[0];
            let lat = location[1];
            if !lon.is_finite() || !lat.is_finite() {
                continue;
            }

            let m_type = raw
                .maneuver
                .as_ref()
                .and_then(|m| m.r#type.as_deref())
                .unwrap_or("continue")
                .trim()
                .to_lowercase();
            let modifier = raw
                .maneuver
                .as_ref()
                .and_then(|m| m.modifier.as_deref())
                .map(|m| m.trim().to_lowercase())
                .filter(|m| !m.is_empty());
            let exit = raw.maneuver.as_ref().and_then(|m| m.exit);
            let name = raw.name.as_deref().unwrap_or("").trim().to_owned();
            let r#ref = raw
                .r#ref
                .as_deref()
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .map(ToOwned::to_owned);

            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let distance_m = raw.distance.unwrap_or(0.0).max(0.0).round() as u64;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let duration_s = raw.duration.unwrap_or(0.0).max(0.0).round() as u64;

            // Round coordinates to 6 decimal places (approx. 11 cm precision)
            let rounded_lon = (lon * 1_000_000.0).round() / 1_000_000.0;
            let rounded_lat = (lat * 1_000_000.0).round() / 1_000_000.0;

            // Check if this step should fold into the previous roundabout step
            if (m_type == "exit roundabout" || m_type == "exit rotary") && !out.is_empty() {
                let last_idx = out.len() - 1;
                let prev_type = out[last_idx].r#type.as_str();
                if prev_type == "roundabout"
                    || prev_type == "rotary"
                    || prev_type == "roundabout turn"
                {
                    out[last_idx].distance_m += distance_m;
                    out[last_idx].duration_s += duration_s;
                    if road_label(&out[last_idx].name, out[last_idx].r#ref.as_deref()).is_empty()
                        && !road_label(&name, r#ref.as_deref()).is_empty()
                    {
                        out[last_idx].name = name;
                        out[last_idx].r#ref = r#ref;
                        out[last_idx].instruction = instruction_for(
                            &out[last_idx].r#type,
                            out[last_idx].modifier.as_deref(),
                            out[last_idx].exit,
                            &out[last_idx].name,
                            out[last_idx].r#ref.as_deref(),
                        );
                    }
                    continue;
                }
            }

            let instruction =
                instruction_for(&m_type, modifier.as_deref(), exit, &name, r#ref.as_deref());

            let step = RouteStep {
                index: out.len(),
                r#type: m_type,
                modifier,
                exit,
                name,
                r#ref,
                distance_m,
                duration_s,
                lon: rounded_lon,
                lat: rounded_lat,
                instruction,
            };

            out.push(step);
            if out.len() >= ROUTE_STEPS_MAX {
                return (out, true);
            }
        }
    }

    (out, false)
}

/// Format a distance in meters: "8 m", "850 m", "1.2 km", "21 km".
#[must_use]
pub fn format_route_distance(meters: f64) -> String {
    if !meters.is_finite() || meters < 0.0 {
        return String::new();
    }
    if meters < 100.0 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let m = meters.round() as u64;
        return format!("{m} m");
    }
    if meters < 1000.0 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let m = ((meters / 10.0).round() * 10.0) as u64;
        return format!("{m} m");
    }
    let km = meters / 1000.0;
    if km < 10.0 {
        format!("{km:.1} km")
    } else {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let km_rounded = km.round() as u64;
        format!("{km_rounded} km")
    }
}

/// Format a duration in seconds: "40 s", "25 min", "1 h 5 min", "2 h".
#[must_use]
pub fn format_route_duration(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return String::new();
    }
    if seconds < 60.0 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let s = seconds.round() as u64;
        return format!("{s} s");
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let minutes = (seconds / 60.0).round() as u64;
    if minutes < 60 {
        return format!("{minutes} min");
    }
    let hours = minutes / 60;
    let rest = minutes % 60;
    if rest > 0 { format!("{hours} h {rest} min") } else { format!("{hours} h") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructions_read_like_a_navigator() {
        assert_eq!(
            instruction_for("depart", None, None, "Kaivokatu", None),
            "Head out on Kaivokatu"
        );
        assert_eq!(instruction_for("depart", None, None, "", None), "Head out");
        assert_eq!(
            instruction_for("new name", Some("straight"), None, "Simonkatu", None),
            "Continue onto Simonkatu"
        );
        assert_eq!(
            instruction_for("end of road", Some("left"), None, "Annankatu", None),
            "At the end of the road, turn left onto Annankatu"
        );
        assert_eq!(
            instruction_for("turn", Some("right"), None, "Kansakoulukatu", None),
            "Turn right onto Kansakoulukatu"
        );
        assert_eq!(
            instruction_for("turn", Some("slight left"), None, "Ring I", Some("Kt 50")),
            "Turn slightly left onto Ring I (Kt 50)"
        );
        assert_eq!(instruction_for("turn", Some("uturn"), None, "", None), "Make a U-turn");
        assert_eq!(
            instruction_for("roundabout", Some("right"), Some(2), "Malminrinne", None),
            "At the roundabout, take the 2nd exit onto Malminrinne"
        );
        assert_eq!(
            instruction_for("rotary", None, Some(1), "", None),
            "At the roundabout, take the 1st exit"
        );
        assert_eq!(
            instruction_for("roundabout", None, Some(11), "", None),
            "At the roundabout, take the 11th exit"
        );
        assert_eq!(
            instruction_for("fork", Some("slight right"), None, "", Some("E12")),
            "Keep slightly right at the fork onto E12"
        );
        assert_eq!(
            instruction_for("off ramp", Some("right"), None, "Turunväylä", None),
            "Take the exit on the right onto Turunväylä"
        );
        assert_eq!(
            instruction_for("on ramp", Some("left"), None, "", None),
            "Take the ramp on the left"
        );
        assert_eq!(
            instruction_for("merge", Some("slight left"), None, "Länsiväylä", None),
            "Merge slightly left onto Länsiväylä"
        );
        assert_eq!(
            instruction_for("arrive", Some("right"), None, "", None),
            "Arrive at your destination, on the right"
        );
        assert_eq!(instruction_for("arrive", None, None, "", None), "Arrive at your destination");
        assert_eq!(instruction_for("made-up", None, None, "X", None), "Continue onto X");
    }

    #[test]
    fn road_label_prefers_name_and_ref() {
        assert_eq!(road_label("Ring I", Some("Kt 50")), "Ring I (Kt 50)");
        assert_eq!(road_label("E12 Turunväylä", Some("E12")), "E12 Turunväylä");
        assert_eq!(road_label("", Some("E12")), "E12");
        assert_eq!(road_label("", None), "");
    }

    #[test]
    fn test_normalize_osrm_steps_folding_and_filtering() {
        let legs = vec![
            RawOsrmLeg {
                steps: vec![
                    RawOsrmStep {
                        maneuver: Some(RawManeuver {
                            location: Some([24.9384, 60.1699]),
                            r#type: Some("depart".into()),
                            modifier: None,
                            exit: None,
                        }),
                        name: Some("Kaivokatu".into()),
                        r#ref: None,
                        distance: Some(8.3),
                        duration: Some(1.3),
                    },
                    RawOsrmStep {
                        maneuver: Some(RawManeuver {
                            location: Some([24.9313, 60.1675]),
                            r#type: Some("roundabout".into()),
                            modifier: Some("right".into()),
                            exit: Some(2),
                        }),
                        name: Some(String::new()),
                        r#ref: None,
                        distance: Some(34.7),
                        duration: Some(7.3),
                    },
                    RawOsrmStep {
                        maneuver: Some(RawManeuver {
                            location: Some([24.931, 60.1674]),
                            r#type: Some("exit roundabout".into()),
                            modifier: Some("slight right".into()),
                            exit: Some(2),
                        }),
                        name: Some("Malminrinne".into()),
                        r#ref: None,
                        distance: Some(100.8),
                        duration: Some(17.1),
                    },
                    RawOsrmStep {
                        maneuver: Some(RawManeuver {
                            location: None, // No location -> dropped!
                            r#type: Some("turn".into()),
                            modifier: Some("left".into()),
                            exit: None,
                        }),
                        name: Some("Nowhere".into()),
                        r#ref: None,
                        distance: Some(5.0),
                        duration: Some(1.0),
                    },
                ],
            },
            RawOsrmLeg {
                steps: vec![RawOsrmStep {
                    maneuver: Some(RawManeuver {
                        location: Some([24.6559, 60.2055]),
                        r#type: Some("arrive".into()),
                        modifier: Some("left".into()),
                        exit: None,
                    }),
                    name: Some("Kirkkojärventie".into()),
                    r#ref: None,
                    distance: Some(0.0),
                    duration: Some(0.0),
                }],
            },
        ];

        let (steps, truncated) = normalize_osrm_steps(&legs);
        assert!(!truncated);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].r#type, "depart");
        assert_eq!(steps[1].r#type, "roundabout");
        assert_eq!(steps[2].r#type, "arrive");
        assert_eq!(steps[1].distance_m, 136); // 35 + 101 rounded
        assert_eq!(steps[1].duration_s, 24); // 7 + 17 rounded
        assert_eq!(steps[1].instruction, "At the roundabout, take the 2nd exit onto Malminrinne");
        assert_eq!(steps[2].instruction, "Arrive at your destination, on the left");
        assert_eq!(steps[0].lon, 24.9384);
        assert_eq!(steps[0].lat, 60.1699);
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn test_step_list_capped() {
        let mut steps = Vec::new();
        for i in 0..(ROUTE_STEPS_MAX + 50) {
            steps.push(RawOsrmStep {
                maneuver: Some(RawManeuver {
                    location: Some([0.001 * (i as f64), 0.5]),
                    r#type: Some("turn".into()),
                    modifier: Some("left".into()),
                    exit: None,
                }),
                name: Some(format!("Street {i}")),
                r#ref: None,
                distance: Some(10.0),
                duration: Some(2.0),
            });
        }

        let (cut_steps, truncated) = normalize_osrm_steps(&[RawOsrmLeg { steps: steps.clone() }]);
        assert_eq!(cut_steps.len(), ROUTE_STEPS_MAX);
        assert!(truncated);

        let (exact_steps, exact_truncated) =
            normalize_osrm_steps(&[RawOsrmLeg { steps: steps[..ROUTE_STEPS_MAX - 1].to_vec() }]);
        assert_eq!(exact_steps.len(), ROUTE_STEPS_MAX - 1);
        assert!(!exact_truncated);
    }

    #[test]
    fn distance_and_duration_formatting() {
        assert_eq!(format_route_distance(8.0), "8 m");
        assert_eq!(format_route_distance(846.0), "850 m");
        assert_eq!(format_route_distance(1234.0), "1.2 km");
        assert_eq!(format_route_distance(21479.0), "21 km");
        assert_eq!(format_route_distance(-1.0), "");
        assert_eq!(format_route_duration(40.0), "40 s");
        assert_eq!(format_route_duration(1478.0), "25 min");
        assert_eq!(format_route_duration(3900.0), "1 h 5 min");
        assert_eq!(format_route_duration(7200.0), "2 h");
        assert_eq!(format_route_duration(f64::NAN), "");
    }
}
