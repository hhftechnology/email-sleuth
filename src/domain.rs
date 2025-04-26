//! Utility functions for handling domain names and URLs.

use crate::error::{AppError, Result};
use url::Url;

/// Extracts the base domain name (e.g., "example.com") from a given URL string.
/// Handles missing schemes, "www." prefixes, and ports.
///
/// # Arguments
/// * `website_url_str` - The input URL string.
///
/// # Returns
/// * `Ok(String)` containing the lowercase domain name if successful.
/// * `Err(AppError::DomainExtraction)` if the URL is empty or cannot be parsed.
pub(crate) fn get_domain_from_url(website_url_str: &str) -> Result<String> {
    tracing::debug!("Attempting to extract domain from URL: {}", website_url_str);
    if website_url_str.is_empty() {
        tracing::warn!("Received empty website URL for domain extraction.");
        return Err(AppError::DomainExtraction(
            "Input URL string is empty".to_string(),
        ));
    }

    let url_str_with_scheme =
        if !website_url_str.starts_with("http://") && !website_url_str.starts_with("https://") {
            format!("https://{}", website_url_str)
        } else {
            website_url_str.to_string()
        };

    let url = Url::parse(&url_str_with_scheme).map_err(|e| {
        tracing::error!(
            "Failed to parse URL '{}' (original: {}): {}",
            url_str_with_scheme,
            website_url_str,
            e
        );
        AppError::UrlParse(e)
    })?;

    let host = url.host_str().ok_or_else(|| {
        tracing::warn!(
            "Could not extract host from parsed URL: {}",
            url_str_with_scheme
        );
        AppError::DomainExtraction(format!(
            "Could not extract host from parsed URL: {}",
            url_str_with_scheme
        ))
    })?;

    let domain = host.strip_prefix("www.").unwrap_or(host);

    let final_domain = domain.to_lowercase();

    tracing::debug!(
        "Extracted domain '{}' from '{}'",
        final_domain,
        website_url_str
    );
    Ok(final_domain)
}

/// Parses the input website string into a valid Url object, adding a scheme if necessary.
pub(crate) fn normalize_url(website_url_str: &str) -> Result<Url> {
    if website_url_str.is_empty() {
        return Err(AppError::InsufficientInput(
            "Website URL is empty".to_string(),
        ));
    }
    let url_str_with_scheme =
        if !website_url_str.starts_with("http://") && !website_url_str.starts_with("https://") {
            format!("https://{}", website_url_str)
        } else {
            website_url_str.to_string()
        };
    Url::parse(&url_str_with_scheme).map_err(|e| AppError::UrlParse(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_domain_from_url_simple() {
        assert_eq!(
            get_domain_from_url("https://www.example.com").unwrap(),
            "example.com"
        );
        assert_eq!(
            get_domain_from_url("http://example.com").unwrap(),
            "example.com"
        );
        assert_eq!(get_domain_from_url("example.com").unwrap(), "example.com");
    }

    #[test]
    fn test_get_domain_from_url_edge_cases() {
        assert_eq!(
            get_domain_from_url("www.example.com").unwrap(),
            "example.com"
        );
        assert_eq!(
            get_domain_from_url("https://EXAMPLE.com/path?query=1").unwrap(),
            "example.com"
        );
        assert_eq!(
            get_domain_from_url("http://example.com:8080").unwrap(),
            "example.com"
        );
        assert_eq!(
            get_domain_from_url("https://sub.domain.example.co.uk").unwrap(),
            "sub.domain.example.co.uk"
        );
    }

    #[test]
    fn test_get_domain_from_url_invalid() {
        assert!(get_domain_from_url("").is_err());
        assert!(get_domain_from_url("http://").is_err());
    }
}
