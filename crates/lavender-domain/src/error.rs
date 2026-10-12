use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A value that failed domain validation.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DomainError {
    #[error("latitude {0} is outside -90..=90")]
    Latitude(f64),
    #[error("longitude {0} is outside -180..=180")]
    Longitude(f64),
    #[error("{field} must be a finite number")]
    NotFinite { field: &'static str },
    #[error("bounding box south {south} is north of north {north}")]
    InvertedBBox { south: f64, north: f64 },
    #[error("cannot parse {what}: {reason}")]
    Parse { what: &'static str, reason: String },
    #[error("{field} is out of range: {reason}")]
    OutOfRange { field: &'static str, reason: String },
}

/// Machine-readable error categories shared by every transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ErrorCode {
    /// The request was malformed or out of range.
    InvalidRequest,
    /// The requested feed, entity or route does not exist.
    NotFound,
    /// The source needs a key or configuration that is missing.
    Unconfigured,
    /// The licence policy (commercial mode) disables this source.
    DisabledByLicence,
    /// The upstream provider failed or is unreachable.
    UpstreamUnavailable,
    /// The caller exceeded a rate or subscription limit.
    RateLimited,
    /// The server refused the request for policy reasons (origin, host).
    Forbidden,
    /// An unexpected internal failure.
    Internal,
}

/// The error envelope for HTTP responses, stream frames and Tauri command errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApiError {
    pub code: ErrorCode,
    /// A sentence a person can act on. Never contains secrets or upstream bodies.
    pub message: String,
    /// Whether retrying the same request later may succeed.
    pub retryable: bool,
}

impl ApiError {
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        let retryable = matches!(code, ErrorCode::UpstreamUnavailable | ErrorCode::RateLimited);
        Self { code, message: message.into(), retryable }
    }
}

impl From<DomainError> for ApiError {
    fn from(err: DomainError) -> Self {
        Self::new(ErrorCode::InvalidRequest, err.to_string())
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}
