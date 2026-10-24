//! Error types for traffic flow provider operations.

/// Errors occurring during traffic provider requests, decoding, and budget checks.
#[derive(Debug, thiserror::Error)]
pub enum TrafficError {
    /// Upstream TomTom API key is not configured.
    #[error("TomTom API key is not configured")]
    NoKey,

    /// Daily request budget has been exceeded.
    #[error("Daily request budget exceeded: {0} requests today")]
    BudgetExceeded(u64),

    /// Invalid slippy map tile coordinates.
    #[error("Invalid tile coordinates: z={z}, x={x}, y={y}")]
    InvalidTile { z: u32, x: u32, y: u32 },

    /// Upstream network or HTTP failure.
    #[error("Upstream error: {0}")]
    Upstream(String),

    /// Vector tile protocol buffer decode failure.
    #[error("Vector tile decode error: {0}")]
    Decode(String),

    /// Internal or file system I/O failure.
    #[error("Internal error: {0}")]
    Internal(String),
}
