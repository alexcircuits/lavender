//! GTFS-Realtime Protocol Buffer decoder for live vehicle positions.

use std::collections::HashMap;

use lavender_domain::{GeoPoint, TransitFeed, TransitVehicle};
use prost::Message;

use super::error::TransitError;
use super::proto::{
    FeedMessage, GTFS_INCREMENTALITY_DIFFERENTIAL, GTFS_MAX_ENTITIES, GTFS_MAX_STRING_CHARS,
    format_occupancy_status, format_stop_status,
};
use super::registry::resolve_route_mode;

/// Decoded GTFS-RT snapshot containing metadata and vehicle positions.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedTransitSnapshot {
    /// Feed identifier.
    pub feed_id: String,
    /// Human-readable feed name.
    pub name: String,
    /// Timestamp of feed generation reported by operator.
    pub feed_timestamp: Option<i64>,
    /// Protocol version string.
    pub version: Option<String>,
    /// Total entity count reported in feed.
    pub entity_count: usize,
    /// Whether entity decoding was truncated at the entity ceiling.
    pub truncated: bool,
    /// Normalized vehicle positions.
    pub vehicles: Vec<TransitVehicle>,
}

/// Returns whether coordinates represent a valid and plausible geographic surface position.
#[must_use]
pub fn is_plausible_vehicle_position(lat: f64, lon: f64) -> bool {
    if !lat.is_finite() || !lon.is_finite() {
        return false;
    }
    if lat.abs() > 90.0 || lon.abs() > 180.0 {
        return false;
    }
    // Reject (0.0, 0.0) null island typically produced by uninitialised GPS units
    if lat.abs() < 1e-6 && lon.abs() < 1e-6 {
        return false;
    }
    true
}

fn check_bounded_string(opt: Option<&str>) -> Result<Option<String>, ()> {
    match opt {
        None => Ok(None),
        Some(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else if trimmed.chars().count() > GTFS_MAX_STRING_CHARS {
                Err(())
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
    }
}

/// Decode raw GTFS-Realtime wire bytes into normalized `TransitVehicle` records.
///
/// # Errors
/// Returns [`TransitError::Decode`] if the protobuf wire bytes cannot be parsed,
/// or [`TransitError::DifferentialFeed`] if the feed publishes differential updates.
#[allow(clippy::too_many_lines)]
pub fn decode_vehicle_positions(
    bytes: &[u8],
    feed: &TransitFeed,
    now_s: i64,
) -> Result<DecodedTransitSnapshot, TransitError> {
    let message = FeedMessage::decode(bytes).map_err(|e| TransitError::Decode(e.to_string()))?;

    let header = message.header.unwrap_or_default();
    if header.incrementality == Some(GTFS_INCREMENTALITY_DIFFERENTIAL) {
        return Err(TransitError::DifferentialFeed(GTFS_INCREMENTALITY_DIFFERENTIAL));
    }

    let header_timestamp = header.timestamp.and_then(|ts| i64::try_from(ts).ok());
    let entity_count = message.entity.len();
    let truncated = entity_count > GTFS_MAX_ENTITIES;
    let entities_to_process =
        if truncated { &message.entity[..GTFS_MAX_ENTITIES] } else { &message.entity[..] };

    let mut by_id: HashMap<String, TransitVehicle> =
        HashMap::with_capacity(entities_to_process.len());

    for entity in entities_to_process {
        if entity.is_deleted == Some(true) {
            continue;
        }

        let Some(vehicle_pos) = &entity.vehicle else {
            continue;
        };

        let Some(pos) = &vehicle_pos.position else {
            continue;
        };

        let raw_lat = f64::from(pos.latitude);
        let raw_lon = f64::from(pos.longitude);
        if !is_plausible_vehicle_position(raw_lat, raw_lon) {
            continue;
        }
        let lat = (raw_lat * 1_000_000.0).round() / 1_000_000.0;
        let lon = (raw_lon * 1_000_000.0).round() / 1_000_000.0;

        // Bounded string checks for all identifiers
        let Ok(entity_id) = check_bounded_string(Some(entity.id.as_str())) else {
            continue;
        };

        let desc_id = vehicle_pos.vehicle.as_ref().and_then(|v| v.id.as_deref());
        let Ok(vehicle_id_opt) = check_bounded_string(desc_id) else {
            continue;
        };

        let Some(vehicle_id) = vehicle_id_opt.or(entity_id) else {
            continue;
        };

        let label_opt = vehicle_pos.vehicle.as_ref().and_then(|v| v.label.as_deref());
        let Ok(label) = check_bounded_string(label_opt) else {
            continue;
        };

        let Ok(stop_id) = check_bounded_string(vehicle_pos.stop_id.as_deref()) else {
            continue;
        };

        let (trip_id, route_id, direction_id) = if let Some(trip) = &vehicle_pos.trip {
            let Ok(trip_id) = check_bounded_string(trip.trip_id.as_deref()) else {
                continue;
            };
            let Ok(route_id) = check_bounded_string(trip.route_id.as_deref()) else {
                continue;
            };
            (trip_id, route_id, trip.direction_id)
        } else {
            (None, None, None)
        };

        let bearing = pos.bearing.map(|b| ((b % 360.0) + 360.0) % 360.0);
        let speed_mps = pos.speed.filter(|&s| s >= 0.0);

        // Timestamp repair ladder
        let (timestamp, timestamp_source) =
            match vehicle_pos.timestamp.and_then(|ts| i64::try_from(ts).ok()) {
                Some(ts) if ts > 0 => (Some(ts), "vehicle"),
                _ => match header_timestamp {
                    Some(header_ts) if header_ts > 0 => (Some(header_ts), "header"),
                    _ => (Some(now_s), "fetch"),
                },
            };

        let status = vehicle_pos.current_status.and_then(format_stop_status).map(str::to_string);

        let occupancy =
            vehicle_pos.occupancy_status.and_then(format_occupancy_status).map(str::to_string);

        let mode = resolve_route_mode(&feed.id, route_id.as_deref(), &feed.default_mode);

        let Ok(position) = GeoPoint::new(lat, lon) else {
            continue;
        };

        let record = TransitVehicle {
            id: format!("{}:{}", feed.id, vehicle_id),
            vehicle_id: vehicle_id.clone(),
            feed_id: feed.id.clone(),
            position,
            bearing,
            speed_mps,
            timestamp,
            timestamp_source: timestamp_source.to_string(),
            route_id,
            trip_id,
            direction_id,
            label,
            stop_id,
            status,
            occupancy,
            mode,
        };

        // Deduplicate: vehicle ID collisions keep the newest timestamp
        if let Some(existing) = by_id.get(&vehicle_id) {
            if record.timestamp.unwrap_or(0) >= existing.timestamp.unwrap_or(0) {
                by_id.insert(vehicle_id, record);
            }
        } else {
            by_id.insert(vehicle_id, record);
        }
    }

    Ok(DecodedTransitSnapshot {
        feed_id: feed.id.clone(),
        name: feed.name.clone(),
        feed_timestamp: header_timestamp,
        version: header.gtfs_realtime_version,
        entity_count,
        truncated,
        vehicles: by_id.into_values().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::transit::proto::{
        FeedEntity, FeedHeader, FeedMessage, Position, TripDescriptor, VehicleDescriptor,
        VehiclePosition,
    };
    use lavender_domain::GeoPoint;

    fn test_feed() -> TransitFeed {
        TransitFeed {
            id: "mbta".to_string(),
            name: "MBTA".to_string(),
            operator: "Massachusetts Bay Transportation Authority".to_string(),
            region: "Boston, MA".to_string(),
            center: GeoPoint::new(42.3601, -71.0589).unwrap(),
            load_radius_km: 70.0,
            url: "https://cdn.mbta.com/realtime/VehiclePositions.pb".to_string(),
            license: "MassDOT Developers License Agreement".to_string(),
            attribution: "MBTA / MassDOT".to_string(),
            default_enabled: true,
            default_mode: "bus".to_string(),
        }
    }

    #[test]
    fn decode_valid_gtfs_rt_feed_and_normalize() {
        let feed = test_feed();
        let message = FeedMessage {
            header: Some(FeedHeader {
                gtfs_realtime_version: Some("2.0".to_string()),
                incrementality: Some(0),
                timestamp: Some(1_700_000_000),
            }),
            entity: vec![FeedEntity {
                id: "ent-1".to_string(),
                is_deleted: Some(false),
                vehicle: Some(VehiclePosition {
                    trip: Some(TripDescriptor {
                        trip_id: Some("trip-101".to_string()),
                        route_id: Some("1".to_string()),
                        direction_id: Some(0),
                        start_time: None,
                        start_date: None,
                        schedule_relationship: None,
                    }),
                    position: Some(Position {
                        latitude: 42.355,
                        longitude: -71.065,
                        bearing: Some(180.0),
                        odometer: None,
                        speed: Some(12.5),
                    }),
                    current_stop_sequence: Some(5),
                    current_status: Some(2), // IN_TRANSIT_TO
                    timestamp: Some(1_700_000_010),
                    congestion_level: None,
                    stop_id: Some("stop-99".to_string()),
                    vehicle: Some(VehicleDescriptor {
                        id: Some("bus-123".to_string()),
                        label: Some("123".to_string()),
                        license_plate: None,
                    }),
                    occupancy_status: Some(2), // FEW_SEATS_AVAILABLE
                }),
            }],
        };

        let mut bytes = Vec::new();
        message.encode(&mut bytes).unwrap();

        let snapshot = decode_vehicle_positions(&bytes, &feed, 1_700_000_020).unwrap();
        assert_eq!(snapshot.feed_id, "mbta");
        assert_eq!(snapshot.name, "MBTA");
        assert_eq!(snapshot.feed_timestamp, Some(1_700_000_000));
        assert_eq!(snapshot.version.as_deref(), Some("2.0"));
        assert_eq!(snapshot.vehicles.len(), 1);

        let v = &snapshot.vehicles[0];
        assert_eq!(v.id, "mbta:bus-123");
        assert_eq!(v.vehicle_id, "bus-123");
        assert!((v.position.lat - 42.355).abs() < 0.001);
        assert!((v.position.lon - -71.065).abs() < 0.001);
        assert_eq!(v.bearing, Some(180.0));
        assert_eq!(v.speed_mps, Some(12.5));
        assert_eq!(v.timestamp, Some(1_700_000_010));
        assert_eq!(v.timestamp_source, "vehicle");
        assert_eq!(v.route_id.as_deref(), Some("1"));
        assert_eq!(v.trip_id.as_deref(), Some("trip-101"));
        assert_eq!(v.direction_id, Some(0));
        assert_eq!(v.label.as_deref(), Some("123"));
        assert_eq!(v.stop_id.as_deref(), Some("stop-99"));
        assert_eq!(v.status.as_deref(), Some("IN_TRANSIT_TO"));
        assert_eq!(v.occupancy.as_deref(), Some("FEW_SEATS_AVAILABLE"));
        assert_eq!(v.mode, "bus");
    }

    #[test]
    fn reject_differential_feed() {
        let feed = test_feed();
        let message = FeedMessage {
            header: Some(FeedHeader {
                gtfs_realtime_version: Some("2.0".to_string()),
                incrementality: Some(1), // DIFFERENTIAL
                timestamp: Some(1_700_000_000),
            }),
            entity: Vec::new(),
        };

        let mut bytes = Vec::new();
        message.encode(&mut bytes).unwrap();

        let err = decode_vehicle_positions(&bytes, &feed, 1_700_000_000).unwrap_err();
        match err {
            TransitError::DifferentialFeed(1) => {}
            other => panic!("expected DifferentialFeed(1), got {other:?}"),
        }
    }

    #[test]
    fn timestamp_repair_ladder() {
        let feed = test_feed();

        // 1. Has vehicle timestamp
        let msg_v = FeedMessage {
            header: Some(FeedHeader { timestamp: Some(1_700_000_000), ..Default::default() }),
            entity: vec![FeedEntity {
                id: "1".into(),
                is_deleted: None,
                vehicle: Some(VehiclePosition {
                    position: Some(Position {
                        latitude: 42.0,
                        longitude: -71.0,
                        bearing: None,
                        odometer: None,
                        speed: None,
                    }),
                    timestamp: Some(1_700_000_050),
                    vehicle: Some(VehicleDescriptor {
                        id: Some("v1".into()),
                        label: None,
                        license_plate: None,
                    }),
                    ..Default::default()
                }),
            }],
        };
        let mut b1 = Vec::new();
        msg_v.encode(&mut b1).unwrap();
        let s1 = decode_vehicle_positions(&b1, &feed, 1_700_000_100).unwrap();
        assert_eq!(s1.vehicles[0].timestamp, Some(1_700_000_050));
        assert_eq!(s1.vehicles[0].timestamp_source, "vehicle");

        // 2. Uses header timestamp
        let msg_h = FeedMessage {
            header: Some(FeedHeader { timestamp: Some(1_700_000_000), ..Default::default() }),
            entity: vec![FeedEntity {
                id: "1".into(),
                is_deleted: None,
                vehicle: Some(VehiclePosition {
                    position: Some(Position {
                        latitude: 42.0,
                        longitude: -71.0,
                        bearing: None,
                        odometer: None,
                        speed: None,
                    }),
                    timestamp: None,
                    vehicle: Some(VehicleDescriptor {
                        id: Some("v1".into()),
                        label: None,
                        license_plate: None,
                    }),
                    ..Default::default()
                }),
            }],
        };
        let mut b2 = Vec::new();
        msg_h.encode(&mut b2).unwrap();
        let s2 = decode_vehicle_positions(&b2, &feed, 1_700_000_100).unwrap();
        assert_eq!(s2.vehicles[0].timestamp, Some(1_700_000_000));
        assert_eq!(s2.vehicles[0].timestamp_source, "header");

        // 3. Falls back to fetch timestamp
        let msg_f = FeedMessage {
            header: None,
            entity: vec![FeedEntity {
                id: "1".into(),
                is_deleted: None,
                vehicle: Some(VehiclePosition {
                    position: Some(Position {
                        latitude: 42.0,
                        longitude: -71.0,
                        bearing: None,
                        odometer: None,
                        speed: None,
                    }),
                    timestamp: None,
                    vehicle: Some(VehicleDescriptor {
                        id: Some("v1".into()),
                        label: None,
                        license_plate: None,
                    }),
                    ..Default::default()
                }),
            }],
        };
        let mut b3 = Vec::new();
        msg_f.encode(&mut b3).unwrap();
        let s3 = decode_vehicle_positions(&b3, &feed, 1_700_000_100).unwrap();
        assert_eq!(s3.vehicles[0].timestamp, Some(1_700_000_100));
        assert_eq!(s3.vehicles[0].timestamp_source, "fetch");
    }

    #[test]
    fn null_island_and_invalid_coordinates_rejected() {
        assert!(!is_plausible_vehicle_position(0.0, 0.0));
        assert!(!is_plausible_vehicle_position(0.000_000_1, -0.000_000_1));
        assert!(!is_plausible_vehicle_position(95.0, 10.0));
        assert!(!is_plausible_vehicle_position(45.0, 200.0));
        assert!(!is_plausible_vehicle_position(f64::NAN, 10.0));
        assert!(is_plausible_vehicle_position(42.36, -71.05));
    }

    #[test]
    fn deduplication_keeps_newest_timestamp() {
        let feed = test_feed();
        let message = FeedMessage {
            header: Some(FeedHeader { timestamp: Some(1_700_000_000), ..Default::default() }),
            entity: vec![
                FeedEntity {
                    id: "ent-1".into(),
                    is_deleted: None,
                    vehicle: Some(VehiclePosition {
                        position: Some(Position {
                            latitude: 42.0,
                            longitude: -71.0,
                            bearing: None,
                            odometer: None,
                            speed: None,
                        }),
                        timestamp: Some(1_700_000_010),
                        vehicle: Some(VehicleDescriptor {
                            id: Some("bus-1".into()),
                            label: None,
                            license_plate: None,
                        }),
                        ..Default::default()
                    }),
                },
                FeedEntity {
                    id: "ent-2".into(),
                    is_deleted: None,
                    vehicle: Some(VehiclePosition {
                        position: Some(Position {
                            latitude: 42.1,
                            longitude: -71.1,
                            bearing: None,
                            odometer: None,
                            speed: None,
                        }),
                        timestamp: Some(1_700_000_020),
                        vehicle: Some(VehicleDescriptor {
                            id: Some("bus-1".into()),
                            label: None,
                            license_plate: None,
                        }),
                        ..Default::default()
                    }),
                },
            ],
        };

        let mut bytes = Vec::new();
        message.encode(&mut bytes).unwrap();

        let snapshot = decode_vehicle_positions(&bytes, &feed, 1_700_000_030).unwrap();
        assert_eq!(snapshot.vehicles.len(), 1);
        assert!((snapshot.vehicles[0].position.lat - 42.1).abs() < 0.001);
        assert_eq!(snapshot.vehicles[0].timestamp, Some(1_700_000_020));
    }
}
