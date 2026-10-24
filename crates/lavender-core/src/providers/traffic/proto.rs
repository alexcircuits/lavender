//! Mapbox Vector Tile (MVT) Protocol Buffer wire definitions (spec v2.1).

/// Top-level container for a Mapbox Vector Tile.
#[derive(Clone, PartialEq, prost::Message)]
pub struct Tile {
    /// Vector layers contained in this tile.
    #[prost(message, repeated, tag = "3")]
    pub layers: Vec<Layer>,
}

/// A single named layer within a vector tile.
#[derive(Clone, PartialEq, prost::Message)]
pub struct Layer {
    /// Layer specification version (typically 2).
    #[prost(uint32, tag = "15")]
    pub version: u32,

    /// Unique name of the layer (e.g. "Traffic flow", "transportation").
    #[prost(string, tag = "1")]
    pub name: String,

    /// Geographic features in this layer.
    #[prost(message, repeated, tag = "2")]
    pub features: Vec<Feature>,

    /// Dictionary of property keys referenced by feature tags.
    #[prost(string, repeated, tag = "3")]
    pub keys: Vec<String>,

    /// Dictionary of property values referenced by feature tags.
    #[prost(message, repeated, tag = "4")]
    pub values: Vec<Value>,

    /// Coordinate extent grid dimension (default 4096).
    #[prost(uint32, optional, tag = "5")]
    pub extent: Option<u32>,
}

/// A geographic feature with attributes and encoded geometry commands.
#[derive(Clone, PartialEq, prost::Message)]
pub struct Feature {
    /// Optional integer feature identifier.
    #[prost(uint64, optional, tag = "1")]
    pub id: Option<u64>,

    /// Attribute key/value index pairs into `layer.keys` and `layer.values`.
    #[prost(uint32, repeated, tag = "2")]
    pub tags: Vec<u32>,

    /// Geometry type: 0=Unknown, 1=Point, 2=LineString, 3=Polygon.
    #[prost(int32, optional, tag = "3")]
    pub r#type: Option<i32>,

    /// Packed geometry command and parameter integers.
    #[prost(uint32, repeated, tag = "4")]
    pub geometry: Vec<u32>,
}

/// A polymorphic attribute value.
#[derive(Clone, PartialEq, prost::Message)]
pub struct Value {
    #[prost(string, optional, tag = "1")]
    pub string_value: Option<String>,

    #[prost(float, optional, tag = "2")]
    pub float_value: Option<f32>,

    #[prost(double, optional, tag = "3")]
    pub double_value: Option<f64>,

    #[prost(int64, optional, tag = "4")]
    pub int_value: Option<i64>,

    #[prost(uint64, optional, tag = "5")]
    pub uint_value: Option<u64>,

    #[prost(sint64, optional, tag = "6")]
    pub sint_value: Option<i64>,

    #[prost(bool, optional, tag = "7")]
    pub bool_value: Option<bool>,
}
