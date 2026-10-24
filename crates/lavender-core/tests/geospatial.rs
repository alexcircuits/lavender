#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::assert_is_empty)]

use lavender_core::config::CoreConfig;
use lavender_core::{InfrastructureKind, LavenderCore};
use lavender_domain::BBox;

#[test]
fn core_regions_point_in_polygon_known_coordinates() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();

    // London: lon -0.1276, lat 51.5074
    let london = core.region_at(-0.1276, 51.5074).unwrap();
    assert!(london.is_some(), "London coordinates must match a region");
    let london_reg = london.unwrap();
    assert!(
        london_reg.name == "Great Britain" || london_reg.name == "British Isles",
        "Expected Great Britain or British Isles, got {}",
        london_reg.name
    );
    assert_eq!(london_reg.iso_a2.as_deref(), Some("GB"));

    // Tokyo: lon 139.6917, lat 35.6895
    let tokyo = core.region_at(139.6917, 35.6895).unwrap();
    assert!(tokyo.is_some(), "Tokyo coordinates must match a region");
    let tokyo_reg = tokyo.unwrap();
    assert!(
        tokyo_reg.name == "Honshü" || tokyo_reg.name == "Japan",
        "Expected Honshü or Japan, got {}",
        tokyo_reg.name
    );

    // New York (Queens / Brooklyn: lon -73.9, lat 40.72)
    let ny = core.region_at(-73.9, 40.72).unwrap();
    assert!(ny.is_some(), "New York coordinates must match a region");
    let ny_reg = ny.unwrap();
    assert!(
        ny_reg.name == "Long I." || ny_reg.name == "North America",
        "Expected Long I. or North America, got {}",
        ny_reg.name
    );

    // Manhattan coordinates (-74.006, 40.7128) via coastal tolerance
    let manhattan = core.region_at(-74.006, 40.7128).unwrap();
    assert!(manhattan.is_some(), "Manhattan coordinates must match a region");

    // Invalid coordinates return BadRequest
    assert!(core.region_at(200.0, 0.0).is_err());
    assert!(core.region_at(0.0, 100.0).is_err());
    assert!(core.region_at(f64::NAN, 0.0).is_err());
}

#[test]
fn core_regions_search() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();

    let results = core.search_regions("Great Britain").unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].name, "Great Britain");
    assert_eq!(results[0].iso_a2.as_deref(), Some("GB"));

    let japan = core.search_regions("Japan").unwrap();
    assert!(!japan.is_empty());
    assert!(japan.iter().any(|r| r.name == "Japan"));

    let alps = core.search_regions("Alps").unwrap();
    assert!(!alps.is_empty());
    assert!(alps.iter().any(|r| r.name.contains("Alps")));

    let empty = core.search_regions("NonexistentLandXYZ").unwrap();
    assert!(empty.is_empty());
}

#[test]
fn core_infrastructure_query_and_zoom_thinning() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();

    let world = BBox::WORLD;

    // Zoom < 6: only major facilities
    let low_zoom = core.infrastructure_query(&world, 3, 500).unwrap();
    assert!(!low_zoom.is_empty());
    for item in &low_zoom {
        assert!(item.is_major(), "All items at zoom < 6 must be major facilities");
    }

    // Zoom >= 10: all facilities (high limit returns non-major facilities too)
    let high_zoom = core.infrastructure_query(&world, 10, 2500).unwrap();
    assert!(!high_zoom.is_empty());
    // At high zoom with sufficient limit, there should be items that are not major
    let has_non_major = high_zoom.iter().any(|item| !item.is_major());
    assert!(has_non_major, "High zoom should return non-major items too");

    // Limit respected
    let capped = core.infrastructure_query(&world, 12, 5).unwrap();
    assert_eq!(capped.len(), 5);

    // Specific regional box (Europe: lon -10..30, lat 35..60)
    let europe = BBox::new(-10.0, 35.0, 30.0, 60.0).unwrap();
    let europe_items = core.infrastructure_query(&europe, 10, 200).unwrap();
    assert!(!europe_items.is_empty());
    for item in &europe_items {
        assert!(europe.contains(&item.position));
    }

    // Both kinds are present (dams and datacenters)
    let has_dams = high_zoom.iter().any(|i| i.kind == InfrastructureKind::Dam);
    let has_datacenters = high_zoom.iter().any(|i| i.kind == InfrastructureKind::Datacenter);
    assert!(has_dams, "Dams must be present");
    assert!(has_datacenters, "Datacenters must be present");
}
