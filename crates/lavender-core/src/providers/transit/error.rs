//! Errors that can occur when fetching, decoding, or serving GTFS-Realtime transit data.

use thiserror::Error;

/// Transit provider error.
#[derive(Debug, Error)]
pub enum TransitError {
    /// The requested transit feed is not in the approved registry.
    #[error("unknown transit feed '{0}'")]
    UnknownFeed(String),

    /// The requested vehicle identifier contains invalid characters or exceeds the length limit.
    #[error("invalid transit vehicle identifier '{0}'")]
    InvalidIdentifier(String),

    /// Upstream feed is published in differential mode, which is not supported.
    #[error("upstream feed is differential (incrementality {0})")]
    DifferentialFeed(i32),

    /// A redirect hop violated egress security policies.
    #[error("upstream redirect refused: {reason}")]
    RedirectRefused {
        /// Reason for redirect refusal.
        reason: String,
    },

    /// Too many redirect hops were encountered.
    #[error("upstream redirected too many times")]
    TooManyRedirects,

    /// Request rate limit reached for the feed.
    #[error(
        "transit request rate limit reached for feed '{feed_id}', retry after {retry_after_s}s"
    )]
    RateLimited {
        /// Feed identifier.
        feed_id: String,
        /// Seconds until permitted.
        retry_after_s: u64,
    },

    /// Feed is currently in cooldown following consecutive upstream failures.
    #[error("transit feed '{feed_id}' is in backoff cooldown, retry after {retry_after_s}s")]
    Cooldown {
        /// Feed identifier.
        feed_id: String,
        /// Seconds until next attempt.
        retry_after_s: u64,
    },

    /// Response body exceeded maximum size limit.
    #[error("upstream response exceeded {limit} byte limit")]
    BodyTooLarge {
        /// Maximum allowed bytes.
        limit: usize,
    },

    /// Upstream network or HTTP failure.
    #[error("upstream network error: {0}")]
    Network(String),

    /// Protobuf decoding failure.
    #[error("failed to decode GTFS-RT protobuf: {0}")]
    Decode(String),

    /// No data or history trail available for vehicle.
    #[error("no trail available for vehicle '{vehicle_id}' in feed '{feed_id}'")]
    TrailNotFound {
        /// Feed identifier.
        feed_id: String,
        /// Vehicle identifier.
        vehicle_id: String,
    },
}
