//! Normalised entities. Providers convert upstream payloads into these types; the
//! frontend renders only these.

mod aircraft;
mod cyclone;
mod earthquake;
mod satellite;

pub use aircraft::Aircraft;
pub use cyclone::{
    Cyclone, CycloneConeGeometry, CycloneForecastPoint, CycloneGeometryStatus, CycloneMovement,
    CycloneTrackGeometry,
};
pub use earthquake::Earthquake;
pub use satellite::SatelliteElements;
