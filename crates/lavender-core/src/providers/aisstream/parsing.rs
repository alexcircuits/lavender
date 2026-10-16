//! AISStream frames into store reports. Schema: the aisstream.io `AisStreamMessage`
//! `OpenAPI` models (github.com/aisstream/ais-message-models, `type-definition.yaml`).
//!
//! Every field is read leniently and then validated: a frame with a bad MMSI, a
//! decoder-invalid flag or an out-of-range position is dropped on its own. AIS
//! "not available" codes (latitude 91, longitude 181, speed 102.3, course 360,
//! heading 511, navigational status 15, ETA month 0, draught 0, dimensions 0) become
//! absent values, never data.

use jiff::Timestamp;
use lavender_domain::entities::{normalize_course, normalize_heading, normalize_speed};
use lavender_domain::text::clean;
use lavender_domain::{GeoPoint, VesselDimensions, VesselEta};
use serde::Deserialize;

use crate::providers::{MAX_ERROR_CHARS, lenient};
use crate::vessel_store::{Mmsi, VesselDetails, VesselPosition};

/// The message types Lavender subscribes to (`FilterMessageTypes`).
pub const MESSAGE_TYPES: [&str; 3] =
    ["PositionReport", "StandardClassBPositionReport", "ShipStaticData"];

/// AIS names and destinations are at most 20 characters.
const MAX_TEXT_CHARS: usize = 20;
/// AIS call signs are at most seven characters.
const MAX_CALL_SIGN_CHARS: usize = 7;
/// The largest AIS static draught, in metres (255 tenths: "25.5 m or more").
const MAX_DRAUGHT_M: f64 = 25.5;
/// The largest bow/stern (A, B) and port/starboard (C, D) offsets AIS can carry, in metres.
const MAX_LENGTH_OFFSET_M: u64 = 511;
const MAX_BEAM_OFFSET_M: u64 = 63;
/// The AIS code for "latitude not available".
const LAT_UNAVAILABLE: f64 = 91.0;
/// The AIS code for "longitude not available".
const LON_UNAVAILABLE: f64 = 181.0;

/// One frame from the stream, classified.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Position(VesselPosition),
    Details(VesselDetails),
    /// A valid report whose position is the AIS "not available" code.
    PositionUnavailable(Mmsi),
    /// AISStream accepted the subscription.
    Confirmation,
    /// AISStream reported an error (cleaned and length-capped; it may still echo
    /// subscription content, so callers must not forward it verbatim).
    UpstreamError(String),
    /// A well-formed message of a type Lavender does not use.
    Ignored,
    /// Not JSON, not an AISStream envelope, or a record that fails validation.
    Malformed,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(rename = "MessageType", default, deserialize_with = "lenient")]
    message_type: Option<String>,
    #[serde(rename = "MetaData", default, deserialize_with = "lenient")]
    meta: Option<Meta>,
    #[serde(rename = "Message", default, deserialize_with = "lenient")]
    message: Option<Bodies>,
    #[serde(default, deserialize_with = "lenient")]
    error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Meta {
    #[serde(rename = "MMSI", default, deserialize_with = "lenient")]
    mmsi: Option<u64>,
    #[serde(rename = "ShipName", default, deserialize_with = "lenient")]
    ship_name: Option<String>,
    #[serde(rename = "time_utc", default, deserialize_with = "lenient")]
    time_utc: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Bodies {
    #[serde(rename = "PositionReport", default, deserialize_with = "lenient")]
    position_report: Option<RawPosition>,
    #[serde(rename = "StandardClassBPositionReport", default, deserialize_with = "lenient")]
    class_b_position: Option<RawPosition>,
    #[serde(rename = "ShipStaticData", default, deserialize_with = "lenient")]
    ship_static: Option<RawStatic>,
}

/// The fields Lavender uses from `PositionReport` and `StandardClassBPositionReport`.
#[derive(Debug, Deserialize)]
struct RawPosition {
    #[serde(rename = "UserID", default, deserialize_with = "lenient")]
    user_id: Option<u64>,
    #[serde(rename = "Valid", default, deserialize_with = "lenient")]
    valid: Option<bool>,
    #[serde(rename = "Latitude", default, deserialize_with = "lenient")]
    latitude: Option<f64>,
    #[serde(rename = "Longitude", default, deserialize_with = "lenient")]
    longitude: Option<f64>,
    #[serde(rename = "Sog", default, deserialize_with = "lenient")]
    sog: Option<f64>,
    #[serde(rename = "Cog", default, deserialize_with = "lenient")]
    cog: Option<f64>,
    #[serde(rename = "TrueHeading", default, deserialize_with = "lenient")]
    true_heading: Option<f64>,
    /// Class A reports only; Class B reports have no such field.
    #[serde(rename = "NavigationalStatus", default, deserialize_with = "lenient")]
    navigational_status: Option<u64>,
}

/// The fields Lavender uses from `ShipStaticData`.
#[derive(Debug, Deserialize)]
struct RawStatic {
    #[serde(rename = "UserID", default, deserialize_with = "lenient")]
    user_id: Option<u64>,
    #[serde(rename = "Valid", default, deserialize_with = "lenient")]
    valid: Option<bool>,
    #[serde(rename = "Name", default, deserialize_with = "lenient")]
    name: Option<String>,
    #[serde(rename = "ImoNumber", default, deserialize_with = "lenient")]
    imo_number: Option<u64>,
    #[serde(rename = "Type", default, deserialize_with = "lenient")]
    ship_type: Option<u64>,
    #[serde(rename = "Destination", default, deserialize_with = "lenient")]
    destination: Option<String>,
    #[serde(rename = "CallSign", default, deserialize_with = "lenient")]
    call_sign: Option<String>,
    #[serde(rename = "Eta", default, deserialize_with = "lenient")]
    eta: Option<RawEta>,
    /// Metres, as AISStream decodes it (0 = not available).
    #[serde(rename = "MaximumStaticDraught", default, deserialize_with = "lenient")]
    maximum_static_draught: Option<f64>,
    #[serde(rename = "Dimension", default, deserialize_with = "lenient")]
    dimension: Option<RawDimension>,
}

/// `ShipStaticData.Eta`: month 0, day 0, hour 24 and minute 60 mean "not available".
#[derive(Debug, Deserialize)]
struct RawEta {
    #[serde(rename = "Month", default, deserialize_with = "lenient")]
    month: Option<u64>,
    #[serde(rename = "Day", default, deserialize_with = "lenient")]
    day: Option<u64>,
    #[serde(rename = "Hour", default, deserialize_with = "lenient")]
    hour: Option<u64>,
    #[serde(rename = "Minute", default, deserialize_with = "lenient")]
    minute: Option<u64>,
}

/// `ShipStaticData.Dimension`: offsets from the position reference point to bow (A),
/// stern (B), port (C) and starboard (D), in metres; 0 means "not available".
#[derive(Debug, Deserialize)]
struct RawDimension {
    #[serde(rename = "A", default, deserialize_with = "lenient")]
    a: Option<u64>,
    #[serde(rename = "B", default, deserialize_with = "lenient")]
    b: Option<u64>,
    #[serde(rename = "C", default, deserialize_with = "lenient")]
    c: Option<u64>,
    #[serde(rename = "D", default, deserialize_with = "lenient")]
    d: Option<u64>,
}

/// Classifies one frame's bytes (AISStream sends UTF-8 JSON in binary frames).
#[must_use]
pub fn parse_frame(bytes: &[u8]) -> Frame {
    let Ok(envelope) = serde_json::from_slice::<Envelope>(bytes) else { return Frame::Malformed };
    if let Some(error) = envelope.error {
        return Frame::UpstreamError(clean(&error, MAX_ERROR_CHARS));
    }
    let meta = envelope.meta.unwrap_or_default();
    let bodies = envelope.message.unwrap_or_default();
    match envelope.message_type.as_deref() {
        Some("SubscriptionConfirmation") => Frame::Confirmation,
        Some("PositionReport") => {
            bodies.position_report.as_ref().map_or(Frame::Malformed, |b| position(b, &meta))
        }
        Some("StandardClassBPositionReport") => {
            bodies.class_b_position.as_ref().map_or(Frame::Malformed, |b| position(b, &meta))
        }
        Some("ShipStaticData") => {
            bodies.ship_static.as_ref().map_or(Frame::Malformed, |b| details(b, &meta))
        }
        Some(_) => Frame::Ignored,
        None => Frame::Malformed,
    }
}

/// The record's MMSI: the message's `UserID`, which must agree with the metadata's when
/// both are present.
fn mmsi(user_id: Option<u64>, meta: &Meta) -> Option<Mmsi> {
    match (user_id, meta.mmsi) {
        (Some(a), Some(b)) if a != b => None,
        (id, fallback) => id.or(fallback).and_then(Mmsi::from_number),
    }
}

fn position(raw: &RawPosition, meta: &Meta) -> Frame {
    let Some(mmsi) = mmsi(raw.user_id, meta) else { return Frame::Malformed };
    if raw.valid == Some(false) {
        return Frame::Malformed;
    }
    let (Some(lat), Some(lon)) = (raw.latitude, raw.longitude) else { return Frame::Malformed };
    if is_code(lat, LAT_UNAVAILABLE) || is_code(lon, LON_UNAVAILABLE) {
        return Frame::PositionUnavailable(mmsi);
    }
    let Ok(position) = GeoPoint::new(lat, lon) else { return Frame::Malformed };
    let finite = |v: Option<f64>| v.filter(|x| x.is_finite());
    Frame::Position(VesselPosition {
        mmsi,
        position,
        speed_knots: normalize_speed(finite(raw.sog)),
        course_deg: normalize_course(finite(raw.cog)),
        heading_deg: normalize_heading(finite(raw.true_heading)),
        observed_at: observed_at(meta),
        name_hint: text(meta.ship_name.as_deref()),
        navigational_status: raw.navigational_status.and_then(navigational_status),
    })
}

fn details(raw: &RawStatic, meta: &Meta) -> Frame {
    let Some(mmsi) = mmsi(raw.user_id, meta) else { return Frame::Malformed };
    if raw.valid == Some(false) {
        return Frame::Malformed;
    }
    Frame::Details(VesselDetails {
        name: text(raw.name.as_deref()).or_else(|| text(meta.ship_name.as_deref())),
        imo: raw.imo_number.and_then(imo),
        vessel_type: raw.ship_type.and_then(ship_type),
        destination: text(raw.destination.as_deref()),
        call_sign: call_sign(raw.call_sign.as_deref()),
        eta: raw.eta.as_ref().and_then(eta),
        draught_m: raw.maximum_static_draught.and_then(draught),
        dimensions: raw.dimension.as_ref().and_then(dimensions),
        observed_at: observed_at(meta),
        ..VesselDetails::new(mmsi)
    })
}

fn is_code(value: f64, code: f64) -> bool {
    (value - code).abs() < 1e-9
}

/// AIS six-bit text pads with `@`; everything from the first `@` is padding.
fn text(raw: Option<&str>) -> Option<String> {
    let value = raw?.split('@').next().unwrap_or_default();
    Some(clean(value, MAX_TEXT_CHARS)).filter(|s| !s.is_empty())
}

/// A call sign: AIS six-bit text of up to seven characters, without padding.
fn call_sign(raw: Option<&str>) -> Option<String> {
    let value = raw?.split('@').next().unwrap_or_default();
    Some(clean(value, MAX_CALL_SIGN_CHARS)).filter(|s| !s.is_empty())
}

/// The ETA as the crew entered it. A missing month or day makes the whole ETA unavailable;
/// a missing hour drops the time of day, a missing minute keeps the hour.
fn eta(raw: &RawEta) -> Option<VesselEta> {
    let in_range = |value: Option<u64>, max: u64| {
        value.filter(|v| (1..=max).contains(v)).and_then(|v| u8::try_from(v).ok())
    };
    let month = in_range(raw.month, 12)?;
    let day = in_range(raw.day, 31)?;
    let hour = raw.hour.filter(|h| *h < 24).and_then(|h| u8::try_from(h).ok());
    let minute = hour.and(raw.minute.filter(|m| *m < 60).and_then(|m| u8::try_from(m).ok()));
    Some(VesselEta { month, day, hour, minute })
}

/// Static draught in metres: 0 is "not available"; AIS cannot say more than 25.5 m.
fn draught(metres: f64) -> Option<f64> {
    (metres.is_finite() && metres > 0.0 && metres <= MAX_DRAUGHT_M)
        .then(|| (metres * 10.0).round() / 10.0)
}

/// Length (A + B) and beam (C + D) in metres. An out-of-range offset makes that measure
/// unavailable; zeros on both sides mean AIS did not report it.
fn dimensions(raw: &RawDimension) -> Option<VesselDimensions> {
    let span = |near: Option<u64>, far: Option<u64>, max: u64| {
        let (near, far) = (near.unwrap_or(0), far.unwrap_or(0));
        let total = (near <= max && far <= max).then_some(near + far).filter(|t| *t > 0)?;
        u16::try_from(total).ok()
    };
    let length_m = span(raw.a, raw.b, MAX_LENGTH_OFFSET_M);
    let beam_m = span(raw.c, raw.d, MAX_BEAM_OFFSET_M);
    (length_m.is_some() || beam_m.is_some()).then_some(VesselDimensions { length_m, beam_m })
}

/// The ITU-R M.1371 navigational status, as words. "Not defined" (15) and the codes
/// reserved for future use are absent.
fn navigational_status(code: u64) -> Option<&'static str> {
    Some(match code {
        0 => "Under way using engine",
        1 => "At anchor",
        2 => "Not under command",
        3 => "Restricted manoeuvrability",
        4 => "Constrained by her draught",
        5 => "Moored",
        6 => "Aground",
        7 => "Engaged in fishing",
        8 => "Under way sailing",
        11 => "Towing astern",
        12 => "Pushing ahead or towing alongside",
        14 => "AIS-SART, MOB or EPIRB active",
        _ => return None,
    })
}

/// A seven-digit IMO ship number whose check digit is right; 0 and anything else are
/// "not available".
fn imo(number: u64) -> Option<String> {
    if !(1_000_000..=9_999_999).contains(&number) {
        return None;
    }
    let digits: Vec<u64> = number.to_string().bytes().map(|b| u64::from(b - b'0')).collect();
    let weighted: u64 = digits[..6].iter().zip((2..=7).rev()).map(|(d, w)| d * w).sum();
    (weighted % 10 == digits[6]).then(|| number.to_string())
}

/// The ITU-R M.1371 ship-and-cargo type, as words (Table 53). Not-available, reserved
/// and out-of-range codes are absent.
fn ship_type(code: u64) -> Option<String> {
    let label = match code {
        20..=29 => "Wing in ground craft",
        30 => "Fishing",
        31 | 32 => "Towing",
        33 => "Dredging or underwater operations",
        34 => "Diving operations",
        35 => "Military operations",
        36 => "Sailing",
        37 => "Pleasure craft",
        40..=49 => "High-speed craft",
        50 => "Pilot vessel",
        51 => "Search and rescue vessel",
        52 => "Tug",
        53 => "Port tender",
        54 => "Anti-pollution vessel",
        55 => "Law enforcement",
        58 => "Medical transport",
        59 => "Noncombatant ship",
        56 | 57 | 90..=99 => "Other",
        60..=69 => "Passenger",
        70..=79 => "Cargo",
        80..=89 => "Tanker",
        _ => return None,
    };
    Some(label.to_owned())
}

/// `MetaData.time_utc`, for example `2026-10-15 11:58:12.318353 +0000 UTC`. Missing or
/// unreadable times are absent; the store then uses the receive time.
fn observed_at(meta: &Meta) -> Option<Timestamp> {
    let raw = meta.time_utc.as_deref()?.trim();
    let raw = raw.strip_suffix(" UTC").unwrap_or(raw);
    jiff::fmt::strtime::parse("%Y-%m-%d %H:%M:%S%.f %z", raw)
        .and_then(|parsed| parsed.to_timestamp())
        .or_else(|_| raw.parse::<Timestamp>())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position_frame(body: &str, meta: &str) -> String {
        format!(
            r#"{{"MessageType":"PositionReport","MetaData":{meta},"Message":{{"PositionReport":{body}}}}}"#
        )
    }

    const META: &str = r#"{"MMSI":230123456,"ShipName":"TEST TRADER@@@","latitude":60.1,"longitude":24.9,"time_utc":"2026-10-15 11:58:12.318353 +0000 UTC"}"#;

    #[test]
    fn reads_a_position_report_with_its_receiver_time() {
        let frame = position_frame(
            r#"{"UserID":230123456,"Valid":true,"Latitude":60.1,"Longitude":24.9,"Sog":12.4,"Cog":86.7,"TrueHeading":87,"NavigationalStatus":0}"#,
            META,
        );
        let Frame::Position(p) = parse_frame(frame.as_bytes()) else { panic!("position") };
        assert_eq!(p.mmsi.to_string(), "230123456");
        assert_eq!((p.position.lat, p.position.lon), (60.1, 24.9));
        assert_eq!(
            (p.speed_knots, p.course_deg, p.heading_deg),
            (Some(12.4), Some(86.7), Some(87.0))
        );
        assert_eq!(p.observed_at.unwrap().to_string(), "2026-10-15T11:58:12.318353Z");
        assert_eq!(p.name_hint.as_deref(), Some("TEST TRADER"));
        assert_eq!(p.navigational_status, Some("Under way using engine"));
    }

    #[test]
    fn navigational_status_is_words_and_undefined_codes_are_absent() {
        for (code, expected) in [
            ("1", Some("At anchor")),
            ("5", Some("Moored")),
            ("8", Some("Under way sailing")),
            ("9", None),
            ("15", None),
            ("16", None),
            ("\"moored\"", None),
        ] {
            let frame = position_frame(
                &format!(
                    r#"{{"UserID":230123456,"Valid":true,"Latitude":60.1,"Longitude":24.9,"NavigationalStatus":{code}}}"#
                ),
                META,
            );
            let Frame::Position(p) = parse_frame(frame.as_bytes()) else { panic!("position") };
            assert_eq!(p.navigational_status, expected, "{code}");
        }
    }

    #[test]
    fn not_available_codes_are_absent_values() {
        let frame = position_frame(
            r#"{"UserID":230123456,"Valid":true,"Latitude":60.1,"Longitude":24.9,"Sog":102.3,"Cog":360,"TrueHeading":511}"#,
            META,
        );
        let Frame::Position(p) = parse_frame(frame.as_bytes()) else { panic!("position") };
        assert_eq!((p.speed_knots, p.course_deg, p.heading_deg), (None, None, None));
        for (lat, lon) in [(91.0, 24.9), (60.1, 181.0), (91.0, 181.0)] {
            let body = format!(
                r#"{{"UserID":230123456,"Valid":true,"Latitude":{lat},"Longitude":{lon}}}"#
            );
            assert_eq!(
                parse_frame(position_frame(&body, META).as_bytes()),
                Frame::PositionUnavailable(Mmsi::parse("230123456").unwrap())
            );
        }
    }

    #[test]
    fn class_b_reports_are_positions_too() {
        let frame = r#"{"MessageType":"StandardClassBPositionReport","MetaData":{"MMSI":244000123},"Message":{"StandardClassBPositionReport":{"UserID":244000123,"Valid":true,"Latitude":52.37,"Longitude":4.89,"Sog":0,"Cog":0,"TrueHeading":511}}}"#;
        let Frame::Position(p) = parse_frame(frame.as_bytes()) else { panic!("class B") };
        assert_eq!((p.speed_knots, p.heading_deg, p.observed_at), (Some(0.0), None, None));
    }

    #[test]
    fn malformed_records_are_dropped_one_by_one() {
        let bad = [
            "not json".to_owned(),
            "[1,2,3]".to_owned(),
            r#"{"MetaData":{}}"#.to_owned(),
            r#"{"MessageType":"PositionReport","Message":{}}"#.to_owned(),
            position_frame(r#"{"UserID":0,"Latitude":1,"Longitude":1}"#, "{}"),
            position_frame(r#"{"UserID":1234567890,"Latitude":1,"Longitude":1}"#, "{}"),
            position_frame(r#"{"UserID":230123457,"Latitude":1,"Longitude":1}"#, META),
            position_frame(
                r#"{"UserID":230123456,"Valid":false,"Latitude":1,"Longitude":1}"#,
                META,
            ),
            position_frame(r#"{"UserID":230123456,"Latitude":95,"Longitude":1}"#, META),
            position_frame(r#"{"UserID":230123456,"Latitude":"north","Longitude":1}"#, META),
            position_frame(r#"{"UserID":230123456,"Longitude":1}"#, META),
            position_frame(r#""a string""#, META),
        ];
        for frame in &bad {
            assert_eq!(parse_frame(frame.as_bytes()), Frame::Malformed, "{frame}");
        }
        // A wrong-typed optional field drops only that field.
        let frame = position_frame(
            r#"{"UserID":230123456,"Latitude":60,"Longitude":25,"Sog":"fast","TrueHeading":null}"#,
            r#"{"time_utc":"yesterday"}"#,
        );
        let Frame::Position(p) = parse_frame(frame.as_bytes()) else { panic!("position") };
        assert_eq!((p.speed_knots, p.heading_deg, p.observed_at), (None, None, None));
    }

    #[test]
    fn reads_static_data_and_validates_the_imo_number() {
        let frame = r#"{"MessageType":"ShipStaticData","MetaData":{"MMSI":230123456,"time_utc":"2026-10-15 11:00:00 +0000 UTC"},"Message":{"ShipStaticData":{"UserID":230123456,"Valid":true,"Name":"TEST TRADER@@@@@@@@@","ImoNumber":9074729,"Type":71,"Destination":"FIHEL@@@@","CallSign":"OJAB"}}}"#;
        let Frame::Details(d) = parse_frame(frame.as_bytes()) else { panic!("details") };
        assert_eq!(d.name.as_deref(), Some("TEST TRADER"));
        assert_eq!(d.imo.as_deref(), Some("9074729"));
        assert_eq!(d.vessel_type.as_deref(), Some("Cargo"));
        assert_eq!(d.destination.as_deref(), Some("FIHEL"));
        assert_eq!(d.observed_at.unwrap().to_string(), "2026-10-15T11:00:00Z");
        assert_eq!(d.call_sign.as_deref(), Some("OJAB"));
        assert_eq!(imo(9_074_728), None, "wrong check digit");
        assert_eq!(imo(0), None);
        assert_eq!(ship_type(0), None);
        assert_eq!(ship_type(15), None);
        assert_eq!(ship_type(100), None);
        assert_eq!(ship_type(52).as_deref(), Some("Tug"));
        assert_eq!(text(Some("@@@@")), None);
    }

    #[test]
    fn reads_voyage_eta_draught_and_dimensions_with_their_not_available_codes() {
        let frame = |fields: &str| {
            format!(
                r#"{{"MessageType":"ShipStaticData","MetaData":{{"MMSI":230123456}},"Message":{{"ShipStaticData":{{"UserID":230123456,"Valid":true,{fields}}}}}}}"#
            )
        };
        let full = frame(
            r#""CallSign":"XTEST@@","Eta":{"Month":10,"Day":17,"Hour":8,"Minute":30},"MaximumStaticDraught":6.2,"Dimension":{"A":50,"B":20,"C":6,"D":7}"#,
        );
        let Frame::Details(d) = parse_frame(full.as_bytes()) else { panic!("details") };
        assert_eq!(d.call_sign.as_deref(), Some("XTEST"));
        assert_eq!(d.eta, Some(VesselEta { month: 10, day: 17, hour: Some(8), minute: Some(30) }));
        assert_eq!(d.draught_m, Some(6.2));
        assert_eq!(d.dimensions, Some(VesselDimensions { length_m: Some(70), beam_m: Some(13) }));

        let unavailable = frame(
            r#""CallSign":"@@@@@@@","Eta":{"Month":0,"Day":0,"Hour":24,"Minute":60},"MaximumStaticDraught":0,"Dimension":{"A":0,"B":0,"C":0,"D":0}"#,
        );
        let Frame::Details(d) = parse_frame(unavailable.as_bytes()) else { panic!("details") };
        assert_eq!((d.call_sign, d.eta, d.draught_m, d.dimensions), (None, None, None, None));

        // Partly known: a date without a time, a length without a beam, no draught over 25.5.
        let partial = frame(
            r#""Eta":{"Month":12,"Day":31,"Hour":24,"Minute":0},"MaximumStaticDraught":30,"Dimension":{"A":0,"B":120,"C":0,"D":64}"#,
        );
        let Frame::Details(d) = parse_frame(partial.as_bytes()) else { panic!("details") };
        assert_eq!(d.eta, Some(VesselEta { month: 12, day: 31, hour: None, minute: None }));
        assert_eq!(d.draught_m, None);
        assert_eq!(d.dimensions, Some(VesselDimensions { length_m: Some(120), beam_m: None }));

        // Wrong types and impossible dates drop only that field.
        let odd = frame(
            r#""CallSign":42,"Eta":{"Month":13,"Day":1},"MaximumStaticDraught":"deep","Dimension":"big","Name":"STILL NAMED""#,
        );
        let Frame::Details(d) = parse_frame(odd.as_bytes()) else { panic!("details") };
        assert_eq!((d.call_sign, d.eta, d.draught_m, d.dimensions), (None, None, None, None));
        assert_eq!(d.name.as_deref(), Some("STILL NAMED"));
    }

    #[test]
    fn errors_confirmations_and_other_types_are_classified() {
        assert_eq!(
            parse_frame(br#"{"error":"Api Key Is Not Valid"}"#),
            Frame::UpstreamError("Api Key Is Not Valid".to_owned())
        );
        assert_eq!(
            parse_frame(br#"{"MessageType":"SubscriptionConfirmation","Message":{"CompressionEnabled":false}}"#),
            Frame::Confirmation
        );
        assert_eq!(
            parse_frame(br#"{"MessageType":"BaseStationReport","MetaData":{"MMSI":2300000},"Message":{"BaseStationReport":{}}}"#),
            Frame::Ignored
        );
        let long = format!(r#"{{"error":"{}"}}"#, "x".repeat(5_000));
        let Frame::UpstreamError(message) = parse_frame(long.as_bytes()) else { panic!("error") };
        assert!(message.chars().count() <= MAX_ERROR_CHARS);
    }
}
