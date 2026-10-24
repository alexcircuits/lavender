//! Overpass QL security sanitiser.
//!
//! Ported directly from `legacy/server/providers/overpass/query.js`:
//! Defends against planet-scale abuse: requires exactly one `data` query in which
//! EVERY element selector is individually spatially bounded (around / bbox / `is_in` /
//! area-set / pivot), rejects oversized radii and world-sized bboxes, and clamps
//! every `[timeout:]` directive.

#![allow(clippy::expect_used)] // LazyLock regex statics: patterns are verified at construction

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use url::form_urlencoded;

use super::constants::{
    OVERPASS_ELEMENT_TYPES, OVERPASS_MAX_AROUND_M, OVERPASS_MAX_BBOX_DEG, OVERPASS_MAX_QL_TIMEOUT,
};
use super::lexer::strip_overpass_noise;

static RE_AROUND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)around(?:\.\w+)?:[\s]*([+-]?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)")
        .expect("valid regex")
});

static RE_BBOX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\(\s*(-?\d+(?:\.\d+)?)\s*,\s*(-?\d+(?:\.\d+)?)\s*,\s*(-?\d+(?:\.\d+)?)\s*,\s*(-?\d+(?:\.\d+)?)\s*\)")
        .expect("valid regex")
});

static RE_UNSUPPORTED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:foreach|complete|retro|compare|convert|make)\b").expect("valid regex")
});

static RE_POLY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bpoly\s*:").expect("valid regex"));

static RE_SELECTOR: LazyLock<Regex> = LazyLock::new(|| {
    let pat = format!(r"\b(?:{OVERPASS_ELEMENT_TYPES}|area)\b");
    Regex::new(&pat).expect("valid regex")
});

static RE_AREA_ELEMENT: LazyLock<Regex> = LazyLock::new(|| {
    let pat = format!(r"(?i)\b(?:{OVERPASS_ELEMENT_TYPES})\s*\(\s*area\b");
    Regex::new(&pat).expect("valid regex")
});

static RE_OUT_SET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"->\s*\.([a-zA-Z_]\w*)").expect("valid regex"));

static RE_TAG_FILTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[[^\]]*\]").expect("valid regex"));

static RE_INPUT_SET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|[^\d.])\.([a-zA-Z_]\w*)").expect("valid regex"));

static RE_DIRECT_AROUND: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"around:\s*\d").expect("valid regex"));

static RE_IS_IN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)is_in\s*\(").expect("valid regex"));

static RE_AREA_FN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\barea\s*\(").expect("valid regex"));

static RE_TIMEOUT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[timeout:\s*(\d+)\s*\]").expect("valid regex"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizedQuery {
    pub body: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OverpassError {
    #[error("Malformed query body")]
    MalformedBody,
    #[error("Exactly one data query is required")]
    MissingData,
    #[error("Missing Overpass data query")]
    EmptyQuery,
    #[error("Overpass around radius too large")]
    RadiusTooLarge,
    #[error("Overpass bbox too large")]
    BboxTooLarge,
    #[error("Unsupported Overpass construct")]
    UnsupportedConstruct,
    #[error("Overpass poly filter not allowed")]
    PolyFilterNotAllowed,
    #[error("Overpass area-bounded element selector not allowed")]
    AreaElementSelector,
    #[error("Overpass query has an unbounded selector")]
    UnboundedSelector,
}

/// Validates and clamps an Overpass form body.
///
/// # Errors
/// Returns [`OverpassError`] if the query contains unbounded selectors, oversized bounds,
/// unsupported constructs, or is malformed.
pub fn sanitize_overpass_body(raw_body: &str) -> Result<SanitizedQuery, OverpassError> {
    let mut data_queries = Vec::new();
    for (key, value) in form_urlencoded::parse(raw_body.as_bytes()) {
        if key == "data" {
            data_queries.push(value.into_owned());
        }
    }

    if data_queries.len() != 1 {
        return Err(OverpassError::MissingData);
    }

    let data = data_queries.remove(0);
    if data.trim().is_empty() {
        return Err(OverpassError::EmptyQuery);
    }

    let stripped = strip_overpass_noise(&data);

    // 1. Check around radius limits
    for cap in RE_AROUND.captures_iter(&stripped) {
        if let Some(m) = cap.get(1) {
            let radius: f64 = m.as_str().parse().unwrap_or(f64::NAN);
            if !radius.is_finite() || radius > OVERPASS_MAX_AROUND_M {
                return Err(OverpassError::RadiusTooLarge);
            }
        }
    }

    // 2. Check bbox limits
    for cap in RE_BBOX.captures_iter(&stripped) {
        let s: f64 = cap[1].parse().unwrap_or(f64::NAN);
        let w: f64 = cap[2].parse().unwrap_or(f64::NAN);
        let n: f64 = cap[3].parse().unwrap_or(f64::NAN);
        let e: f64 = cap[4].parse().unwrap_or(f64::NAN);
        if !s.is_finite() || !w.is_finite() || !n.is_finite() || !e.is_finite() {
            return Err(OverpassError::BboxTooLarge);
        }
        if (n - s).abs() > OVERPASS_MAX_BBOX_DEG || (e - w).abs() > OVERPASS_MAX_BBOX_DEG {
            return Err(OverpassError::BboxTooLarge);
        }
    }

    // 3. Reject forbidden control constructs
    if RE_UNSUPPORTED.is_match(&stripped) {
        return Err(OverpassError::UnsupportedConstruct);
    }

    // 4. Reject poly: filter
    if RE_POLY.is_match(&stripped) {
        return Err(OverpassError::PolyFilterNotAllowed);
    }

    // 5. Check each statement for bounds and set provenance
    let mut bounded_sets: HashSet<String> = HashSet::new();

    for raw_stmt in stripped.split(';') {
        let stmt = raw_stmt.trim();
        if stmt.is_empty() || stmt.starts_with('[') || stmt.starts_with("out") {
            continue;
        }

        let mut out_sets = Vec::new();
        for cap in RE_OUT_SET.captures_iter(stmt) {
            out_sets.push(cap[1].to_string());
        }

        let without_out = RE_OUT_SET.replace_all(stmt, " ");
        let probe = RE_TAG_FILTER.replace_all(&without_out, " ");

        if RE_AREA_ELEMENT.is_match(&probe) {
            return Err(OverpassError::AreaElementSelector);
        }

        let has_selector = RE_SELECTOR.is_match(&probe);

        let mut input_sets = Vec::new();
        for cap in RE_INPUT_SET.captures_iter(&probe) {
            input_sets.push(cap[1].to_string());
        }

        let direct_bound = RE_DIRECT_AROUND.is_match(&probe)
            || RE_BBOX.is_match(&probe)
            || RE_IS_IN.is_match(&probe)
            || RE_AREA_FN.is_match(&probe);

        let set_bound = input_sets.iter().any(|s| bounded_sets.contains(s));
        let bounded = direct_bound || set_bound;

        if has_selector && !bounded {
            return Err(OverpassError::UnboundedSelector);
        }

        if bounded {
            for name in out_sets {
                bounded_sets.insert(name);
            }
        }
    }

    // 6. Clamp timeout directive
    let clamped = RE_TIMEOUT.replace_all(&data, |caps: &regex::Captures| {
        let raw_n: u32 = caps[1].parse().unwrap_or(OVERPASS_MAX_QL_TIMEOUT);
        let clamped_n = raw_n.min(OVERPASS_MAX_QL_TIMEOUT);
        format!("[timeout:{clamped_n}]")
    });

    let encoded: String = form_urlencoded::byte_serialize(clamped.as_bytes()).collect();
    Ok(SanitizedQuery { body: format!("data={encoded}") })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_bounded_queries() {
        let q = "data=[out:json][timeout:25];node(around:500,30.27,-97.74);out;";
        let res = sanitize_overpass_body(q).unwrap();
        assert!(res.body.starts_with("data="));

        let q_bbox = "data=[out:json];way[\"highway\"](30.1,-97.9,30.5,-97.5);out geom;";
        assert!(sanitize_overpass_body(q_bbox).is_ok());

        let q_isin = "data=[out:json];is_in(37.5,14.2)->.a;area.a[\"admin_level\"];out tags;";
        assert!(sanitize_overpass_body(q_isin).is_ok());

        let q_pivot = "data=[out:json];area(3600039152)->.x;rel(pivot.x);out geom;";
        assert!(sanitize_overpass_body(q_pivot).is_ok());
    }

    #[test]
    fn rejects_unbounded_queries() {
        let q = "data=[out:json];node[\"name\"=\"Central Park\"];out;";
        assert_eq!(sanitize_overpass_body(q).unwrap_err(), OverpassError::UnboundedSelector);

        let q_global_way = "data=[out:json];way[\"highway\"=\"primary\"];out;";
        assert_eq!(
            sanitize_overpass_body(q_global_way).unwrap_err(),
            OverpassError::UnboundedSelector
        );
    }

    #[test]
    fn rejects_radius_too_large() {
        let q = "data=node(around:60000,30.27,-97.74);out;";
        assert_eq!(sanitize_overpass_body(q).unwrap_err(), OverpassError::RadiusTooLarge);

        let q_sci = "data=node(around:5e7,30.27,-97.74);out;";
        assert_eq!(sanitize_overpass_body(q_sci).unwrap_err(), OverpassError::RadiusTooLarge);
    }

    #[test]
    fn rejects_bbox_too_large() {
        let q = "data=way(0,0,15,15);out;";
        assert_eq!(sanitize_overpass_body(q).unwrap_err(), OverpassError::BboxTooLarge);
    }

    #[test]
    fn rejects_unsupported_constructs() {
        for construct in ["foreach", "complete", "retro", "compare", "convert", "make"] {
            let q = format!("data=[out:json];{construct};node(around:10,0,0);out;");
            assert_eq!(
                sanitize_overpass_body(&q).unwrap_err(),
                OverpassError::UnsupportedConstruct
            );
        }
    }

    #[test]
    fn rejects_poly_filter() {
        let q = "data=node(poly:\"30 -97 31 -97 31 -96 30 -96\");out;";
        assert_eq!(sanitize_overpass_body(q).unwrap_err(), OverpassError::PolyFilterNotAllowed);
    }

    #[test]
    fn rejects_area_bounded_element_selectors() {
        let q = "data=area(1)->.a;way(area.a)[\"highway\"];out;";
        assert_eq!(sanitize_overpass_body(q).unwrap_err(), OverpassError::AreaElementSelector);
    }

    #[test]
    fn clamps_timeout() {
        let q = "data=[out:json][timeout:120];node(around:10,0,0);out;";
        let res = sanitize_overpass_body(q).unwrap();
        let decoded = url::form_urlencoded::parse(res.body.as_bytes()).next().unwrap().1;
        assert!(decoded.contains("[timeout:30]"));
    }
}
