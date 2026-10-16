//! The fire perimeter contract survives a JSON round trip and keeps its wire shape.

#![allow(clippy::unwrap_used)]

use lavender_domain::*;
use serde_json::json;

fn perimeter() -> FirePerimeter {
    FirePerimeter {
        id: "2026-CALPF-002271".into(),
        name: Some("Timber".into()),
        acres: Some(25_350.0),
        contained_pct: Some(85.0),
        state: Some("US-CA".into()),
        category: Some("WF".into()),
        discovered_at: Some("2026-08-09T20:11:00Z".parse().unwrap()),
        updated_at: Some("2026-10-14T03:00:00Z".parse().unwrap()),
        cause: Some("Natural".into()),
        behavior: Some("Minimal".into()),
        personnel: Some(41),
        county: Some("Fresno".into()),
        cost_to_date_usd: Some(1_200_000.0),
        complexity: Some("Type 3 Incident".into()),
        complex_name: None,
        anchor: GeoPoint::new(37.0, -119.0).unwrap(),
        extent: BBox::new(-119.1, 36.9, -118.9, 37.1).unwrap(),
        inciweb_url: Some(
            "https://inciweb.wildfire.gov/incident-information/calpf-timber-fire".into(),
        ),
        detail: PerimeterDetail { level: 2, tolerance_m: 500.0 },
        polygons: vec![vec![vec![[-119.1, 36.9], [-118.9, 36.9], [-118.9, 37.1], [-119.1, 36.9]]]],
    }
}

#[test]
fn perimeters_round_trip_with_camel_case_fields() {
    let items = FeedItems::Perimeters(vec![perimeter()]);
    let value = serde_json::to_value(&items).unwrap();
    assert_eq!(serde_json::from_value::<FeedItems>(value.clone()).unwrap(), items);
    assert_eq!(value["kind"], "perimeters");
    let item = &value["items"][0];
    assert_eq!(item["containedPct"], 85.0);
    assert_eq!(item["costToDateUsd"], 1_200_000.0);
    assert_eq!(item["detail"], json!({"level": 2, "toleranceM": 500.0}));
    assert_eq!(
        item["inciwebUrl"],
        "https://inciweb.wildfire.gov/incident-information/calpf-timber-fire"
    );
    assert!(item.get("complexName").is_none(), "absent facts are omitted");
    assert_eq!(item["polygons"][0][0][0], json!([-119.1, 36.9]));
}

#[test]
fn perimeters_feed_has_a_stable_name() {
    assert_eq!(FeedId::Perimeters.as_str(), "perimeters");
    assert_eq!("perimeters".parse::<FeedId>().unwrap(), FeedId::Perimeters);
    assert!(FeedItems::empty(FeedId::Perimeters).is_empty());
}

#[test]
fn invalid_anchor_or_extent_is_rejected() {
    let mut value = serde_json::to_value(perimeter()).unwrap();
    value["anchor"]["lat"] = json!(95.0);
    assert!(serde_json::from_value::<FirePerimeter>(value).is_err());
    let mut value = serde_json::to_value(perimeter()).unwrap();
    value["extent"]["south"] = json!(40.0);
    assert!(serde_json::from_value::<FirePerimeter>(value).is_err());
}
