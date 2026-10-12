//! Normalised entities. Providers convert upstream payloads into these types; the
//! frontend renders only these.

mod aircraft;
mod earthquake;
mod satellite;

pub use aircraft::Aircraft;
pub use earthquake::Earthquake;
pub use satellite::SatelliteElements;
