use lavender_domain::{ApiError, DomainError, ErrorCode, FeedId};

use crate::fetch::FetchError;

/// Errors surfaced by [`crate::LavenderCore`]. Adapters convert them into [`ApiError`]
/// (HTTP status or Tauri command error) without adding detail.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error(transparent)]
    Invalid(#[from] DomainError),
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("the {feed} layer is not configured: {missing}")]
    Unconfigured { feed: FeedId, missing: String },
    #[error("the {feed} layer is disabled by the commercial licence policy")]
    DisabledByLicence { feed: FeedId },
    #[error("{0}")]
    Upstream(String),
    #[error("{0}")]
    Limit(String),
    #[error("the core is shutting down")]
    ShuttingDown,
    #[error("timed out waiting for {0}")]
    Timeout(String),
    /// An error that already carries its category (for example from a subscription).
    #[error("{}", .0.message)]
    Api(ApiError),
}

impl From<FetchError> for CoreError {
    fn from(err: FetchError) -> Self {
        Self::Upstream(err.to_string())
    }
}

impl From<&CoreError> for ApiError {
    fn from(err: &CoreError) -> Self {
        if let CoreError::Api(api) = err {
            return api.clone();
        }
        let code = match err {
            CoreError::Invalid(_) | CoreError::BadRequest(_) => ErrorCode::InvalidRequest,
            CoreError::NotFound(_) => ErrorCode::NotFound,
            CoreError::Unconfigured { .. } => ErrorCode::Unconfigured,
            CoreError::DisabledByLicence { .. } => ErrorCode::DisabledByLicence,
            CoreError::Upstream(_) | CoreError::Timeout(_) => ErrorCode::UpstreamUnavailable,
            CoreError::Limit(_) => ErrorCode::RateLimited,
            CoreError::ShuttingDown | CoreError::Api(_) => ErrorCode::Internal,
        };
        ApiError::new(code, err.to_string())
    }
}

impl From<CoreError> for ApiError {
    fn from(err: CoreError) -> Self {
        Self::from(&err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_errors_keep_their_code() {
        let limit = ApiError::new(ErrorCode::RateLimited, "busy");
        let back = ApiError::from(CoreError::Api(limit.clone()));
        assert_eq!(back, limit);
        assert_eq!(CoreError::Api(limit).to_string(), "busy");
    }
}
