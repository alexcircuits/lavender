//! GTFS-Realtime Protocol Buffer wire contracts for `VehiclePositions` feeds.

/// Hard ceiling on decoded entities per snapshot.
pub const GTFS_MAX_ENTITIES: usize = 50_000;

/// Maximum accepted character length for individual string fields.
pub const GTFS_MAX_STRING_CHARS: usize = 256;

/// `FeedHeader.Incrementality`: Full dataset snapshot.
pub const GTFS_INCREMENTALITY_FULL_DATASET: i32 = 0;

/// `FeedHeader.Incrementality`: Differential update (unsupported).
pub const GTFS_INCREMENTALITY_DIFFERENTIAL: i32 = 1;

/// Top-level container for a GTFS-Realtime feed.
#[derive(Clone, PartialEq, prost::Message)]
pub struct FeedMessage {
    /// Feed metadata header.
    #[prost(message, optional, tag = "1")]
    pub header: Option<FeedHeader>,

    /// Feed entities contained in this snapshot.
    #[prost(message, repeated, tag = "2")]
    pub entity: Vec<FeedEntity>,
}

/// Metadata header for a GTFS-Realtime feed message.
#[derive(Clone, PartialEq, prost::Message)]
pub struct FeedHeader {
    /// Version of the GTFS-RT specification (e.g. "2.0").
    #[prost(string, optional, tag = "1")]
    pub gtfs_realtime_version: Option<String>,

    /// Incrementality indicator: 0 = `FULL_DATASET`, 1 = `DIFFERENTIAL`.
    #[prost(int32, optional, tag = "2")]
    pub incrementality: Option<i32>,

    /// Timestamp of feed generation in epoch seconds.
    #[prost(uint64, optional, tag = "3")]
    pub timestamp: Option<u64>,
}

/// A single entity within a GTFS-Realtime feed message.
#[derive(Clone, PartialEq, prost::Message)]
pub struct FeedEntity {
    /// Unique identifier for this entity within the feed.
    #[prost(string, tag = "1")]
    pub id: String,

    /// Whether this entity has been deleted.
    #[prost(bool, optional, tag = "2")]
    pub is_deleted: Option<bool>,

    /// Vehicle position observation payload.
    #[prost(message, optional, tag = "4")]
    pub vehicle: Option<VehiclePosition>,
}

/// Realtime positioning information for a transit vehicle.
#[derive(Clone, PartialEq, prost::Message)]
pub struct VehiclePosition {
    /// Trip descriptor for the assigned trip.
    #[prost(message, optional, tag = "1")]
    pub trip: Option<TripDescriptor>,

    /// Geographic position and kinematics.
    #[prost(message, optional, tag = "2")]
    pub position: Option<Position>,

    /// Current sequence index along the scheduled trip.
    #[prost(uint32, optional, tag = "3")]
    pub current_stop_sequence: Option<u32>,

    /// Vehicle relationship to the stop (`0=INCOMING_AT`, `1=STOPPED_AT`, `2=IN_TRANSIT_TO`).
    #[prost(int32, optional, tag = "4")]
    pub current_status: Option<i32>,

    /// Observation timestamp in epoch seconds.
    #[prost(uint64, optional, tag = "5")]
    pub timestamp: Option<u64>,

    /// Congestion level indicator.
    #[prost(uint32, optional, tag = "6")]
    pub congestion_level: Option<u32>,

    /// Current stop identifier.
    #[prost(string, optional, tag = "7")]
    pub stop_id: Option<String>,

    /// Vehicle descriptive labels and identifiers.
    #[prost(message, optional, tag = "8")]
    pub vehicle: Option<VehicleDescriptor>,

    /// Passenger occupancy status enum index.
    #[prost(int32, optional, tag = "9")]
    pub occupancy_status: Option<i32>,
}

/// Trip descriptor associating a vehicle with scheduled routes and trips.
#[derive(Clone, PartialEq, prost::Message)]
pub struct TripDescriptor {
    /// Scheduled trip identifier.
    #[prost(string, optional, tag = "1")]
    pub trip_id: Option<String>,

    /// Initial scheduled departure time.
    #[prost(string, optional, tag = "2")]
    pub start_time: Option<String>,

    /// Initial scheduled departure date (YYYYMMDD).
    #[prost(string, optional, tag = "3")]
    pub start_date: Option<String>,

    /// Relationship to schedule (e.g. SCHEDULED, ADDED, UNSCHEDULED, CANCELED).
    #[prost(uint32, optional, tag = "4")]
    pub schedule_relationship: Option<u32>,

    /// Route identifier.
    #[prost(string, optional, tag = "5")]
    pub route_id: Option<String>,

    /// Direction of travel (typically 0 or 1).
    #[prost(uint32, optional, tag = "6")]
    pub direction_id: Option<u32>,
}

/// Geographic coordinates and motion vector.
#[derive(Clone, PartialEq, prost::Message)]
pub struct Position {
    /// Latitude in WGS84 degrees.
    #[prost(float, tag = "1")]
    pub latitude: f32,

    /// Longitude in WGS84 degrees.
    #[prost(float, tag = "2")]
    pub longitude: f32,

    /// Compass heading / bearing in degrees (0..360) clockwise from north.
    #[prost(float, optional, tag = "3")]
    pub bearing: Option<f32>,

    /// Cumulative odometer reading in metres.
    #[prost(double, optional, tag = "4")]
    pub odometer: Option<f64>,

    /// Instantaneous speed in metres per second.
    #[prost(float, optional, tag = "5")]
    pub speed: Option<f32>,
}

/// Vehicle descriptive identification.
#[derive(Clone, PartialEq, prost::Message)]
pub struct VehicleDescriptor {
    /// Unique internal vehicle ID.
    #[prost(string, optional, tag = "1")]
    pub id: Option<String>,

    /// Publicly visible vehicle label.
    #[prost(string, optional, tag = "2")]
    pub label: Option<String>,

    /// Vehicle license plate string.
    #[prost(string, optional, tag = "3")]
    pub license_plate: Option<String>,
}

/// Convert integer `current_status` to standard string label.
#[must_use]
pub fn format_stop_status(status: i32) -> Option<&'static str> {
    match status {
        0 => Some("INCOMING_AT"),
        1 => Some("STOPPED_AT"),
        2 => Some("IN_TRANSIT_TO"),
        _ => None,
    }
}

/// Convert integer `occupancy_status` to standard string label.
#[must_use]
pub fn format_occupancy_status(status: i32) -> Option<&'static str> {
    match status {
        0 => Some("EMPTY"),
        1 => Some("MANY_SEATS_AVAILABLE"),
        2 => Some("FEW_SEATS_AVAILABLE"),
        3 => Some("STANDING_ROOM_ONLY"),
        4 => Some("CRUSHED_STANDING_ROOM_ONLY"),
        5 => Some("FULL"),
        6 => Some("NOT_ACCEPTING_PASSENGERS"),
        7 => Some("NO_DATA_AVAILABLE"),
        8 => Some("NOT_BOARDABLE"),
        _ => None,
    }
}
