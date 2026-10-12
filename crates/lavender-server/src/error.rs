//! `ApiError` → HTTP. Status codes are chosen so generic HTTP tooling behaves sensibly;
//! the `code` field is what clients act on.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use lavender_core::CoreError;
use lavender_domain::{ApiError, ErrorCode};

/// Wrapper so handlers can `?` core errors into responses.
#[derive(Debug)]
pub struct HttpError(pub ApiError);

#[must_use]
pub fn status_for(code: ErrorCode) -> StatusCode {
    match code {
        ErrorCode::InvalidRequest => StatusCode::BAD_REQUEST,
        ErrorCode::NotFound => StatusCode::NOT_FOUND,
        ErrorCode::Unconfigured => StatusCode::SERVICE_UNAVAILABLE,
        ErrorCode::DisabledByLicence => StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS,
        ErrorCode::UpstreamUnavailable => StatusCode::BAD_GATEWAY,
        ErrorCode::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        ErrorCode::Forbidden => StatusCode::FORBIDDEN,
        ErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        (status_for(self.0.code), Json(self.0)).into_response()
    }
}

impl From<CoreError> for HttpError {
    fn from(err: CoreError) -> Self {
        Self(ApiError::from(&err))
    }
}

impl HttpError {
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self(ApiError::new(code, message))
    }
}

/// An `ApiError` envelope with an explicit status, for the few cases where the status is
/// not the code's usual one (for example 503 for a full server, 504 for a timeout).
#[must_use]
pub fn json_error(status: StatusCode, code: ErrorCode, message: impl Into<String>) -> Response {
    (status, Json(ApiError::new(code, message))).into_response()
}

/// The error code for a bare HTTP error status produced outside Lavender's handlers
/// (method mismatches, body limits, caught panics).
#[must_use]
pub fn code_for_status(status: StatusCode) -> ErrorCode {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ErrorCode::Forbidden,
        StatusCode::NOT_FOUND | StatusCode::GONE => ErrorCode::NotFound,
        StatusCode::TOO_MANY_REQUESTS => ErrorCode::RateLimited,
        StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS => ErrorCode::DisabledByLicence,
        StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT => {
            ErrorCode::UpstreamUnavailable
        }
        s if s.is_server_error() => ErrorCode::Internal,
        _ => ErrorCode::InvalidRequest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_map_to_codes() {
        assert_eq!(code_for_status(StatusCode::METHOD_NOT_ALLOWED), ErrorCode::InvalidRequest);
        assert_eq!(code_for_status(StatusCode::PAYLOAD_TOO_LARGE), ErrorCode::InvalidRequest);
        assert_eq!(code_for_status(StatusCode::INTERNAL_SERVER_ERROR), ErrorCode::Internal);
        assert_eq!(code_for_status(StatusCode::GATEWAY_TIMEOUT), ErrorCode::UpstreamUnavailable);
        for code in [ErrorCode::InvalidRequest, ErrorCode::NotFound, ErrorCode::RateLimited] {
            assert_eq!(code_for_status(status_for(code)), code);
        }
    }
}
