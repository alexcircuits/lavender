//! Hop-by-hop HTTP redirect decision logic for transit upstream endpoints.

use url::Url;

use super::error::TransitError;

/// Maximum number of redirect hops followed before declaring upstream unreachable.
pub const TRANSIT_MAX_REDIRECTS: usize = 3;

/// Evaluate whether a redirect hop may be followed BEFORE requesting it.
///
/// Checking the destination URL after following redirects is unsafe because the
/// untrusted host has already received the request. This function verifies each
/// hop synchronously against the registered feed origin.
///
/// # Errors
/// Returns [`TransitError::RedirectRefused`] if the target URL is invalid,
/// drops to plain HTTP, leaves the registered HTTPS origin, or omits `Location`.
pub fn transit_redirect_decision(
    origin_url: &str,
    current_url: &str,
    location: Option<&str>,
) -> Result<String, TransitError> {
    let Some(loc) = location.map(str::trim).filter(|s| !s.is_empty()) else {
        return Err(TransitError::RedirectRefused {
            reason: "redirect without a Location header".to_string(),
        });
    };

    let origin = Url::parse(origin_url).map_err(|_| TransitError::RedirectRefused {
        reason: "origin target is not a valid URL".to_string(),
    })?;

    let current = Url::parse(current_url).map_err(|_| TransitError::RedirectRefused {
        reason: "current target is not a valid URL".to_string(),
    })?;

    let next = current.join(loc).map_err(|_| TransitError::RedirectRefused {
        reason: "redirect target is not a valid URL".to_string(),
    })?;

    if origin.scheme() == "https" && next.scheme() != "https" {
        return Err(TransitError::RedirectRefused { reason: "redirect left https".to_string() });
    }

    if next.origin() != origin.origin() {
        return Err(TransitError::RedirectRefused {
            reason: format!("redirect left {}", origin.origin().ascii_serialization()),
        });
    }

    Ok(next.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_origin_relative_and_absolute_are_accepted() {
        let origin = "https://data.example.gov/download/feed.pb";

        let abs =
            transit_redirect_decision(origin, origin, Some("https://data.example.gov/files/a.pb"));
        assert_eq!(abs.unwrap(), "https://data.example.gov/files/a.pb");

        let rel = transit_redirect_decision(origin, origin, Some("/files/b.pb"));
        assert_eq!(rel.unwrap(), "https://data.example.gov/files/b.pb");
    }

    #[test]
    fn off_origin_redirect_is_refused() {
        let origin = "https://data.example.gov/download/feed.pb";
        let res =
            transit_redirect_decision(origin, origin, Some("https://cdn.elsewhere.test/a.pb"));
        assert!(res.is_err());
        match res.unwrap_err() {
            TransitError::RedirectRefused { reason } => {
                assert!(reason.contains("left https://data.example.gov"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn http_downgrade_and_missing_location_are_refused() {
        let origin = "https://data.example.gov/download/feed.pb";

        let http = transit_redirect_decision(origin, origin, Some("http://data.example.gov/a.pb"));
        assert!(http.is_err());

        let missing = transit_redirect_decision(origin, origin, None);
        assert!(missing.is_err());
    }
}
