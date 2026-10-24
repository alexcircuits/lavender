//! Error types for the terrain point-heights provider.

/// Errors that can occur when querying terrain heights.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum TerrainError {
    /// Points array is empty.
    #[error("empty points array; at least 1 point required")]
    EmptyPoints,

    /// Number of points exceeds the allowed limit.
    #[error("too many points ({count}); max {max} per request")]
    TooManyPoints {
        /// Number of points in the request.
        count: usize,
        /// Maximum allowed points.
        max: usize,
    },

    /// A coordinate is not a valid geographic coordinate.
    #[error("invalid coordinate at index {index}: {reason}")]
    InvalidCoordinate {
        /// 0-based index of the invalid coordinate.
        index: usize,
        /// Detailed reason why the coordinate is invalid.
        reason: String,
    },

    /// Upstream fetch failed or omitted points and no cache is available.
    #[error("terrain heights fetch failed and no cache available for every point")]
    MissingHeight {
        /// Number of requested points that could not be resolved.
        missing_count: usize,
    },

    /// The terrain provider is disabled in configuration.
    #[error("terrain heights provider is disabled")]
    Disabled,

    /// Network error while communicating with upstream Re:Earth service.
    #[error("network error: {0}")]
    Network(String),

    /// Upstream HTTP error response.
    #[error("upstream error: HTTP {status} - {message}")]
    Upstream {
        /// HTTP status code from upstream.
        status: u16,
        /// Upstream error message or description.
        message: String,
    },
}
