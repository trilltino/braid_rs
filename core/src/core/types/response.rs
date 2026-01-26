//! HTTP response with Braid protocol information.
//!
//! The [`BraidResponse`] type wraps an HTTP response with Braid-specific metadata
//! and provides helper methods for extracting protocol headers.
//!
//! # Overview
//!
//! Braid responses extend standard HTTP responses with:
//!
//! | Header | Description |
//! |--------|-------------|
//! | `Version` | Version ID(s) of this response |
//! | `Parents` | Parent version(s) in the DAG |
//! | `Current-Version` | Latest version (catch-up signaling) |
//! | `Merge-Type` | Conflict resolution strategy |
//! | `Content-Range` | Range specification for patches |
//!
//! # Status Codes
//!
//! | Code | Name | Description |
//! |------|------|-------------|
//! | 200 | OK | Standard response with version info |
//! | 206 | Partial Content | Range-based patches (RFC 7233) |
//! | 209 | Subscription | Streaming subscription update |
//! | 293 | Merge Conflict | Conflict detected in merge |
//! | 410 | Gone | History dropped, client must restart |
//! | 416 | Range Not Satisfiable | Invalid range request |
//!
//! # Examples
//!
//! ```
//! use crate::core::BraidResponse;
//!
//! let response = BraidResponse::new(200, r#"{"data": "value"}"#)
//!     .with_header("Version", r#""v1""#)
//!     .with_header("Merge-Type", "diamond");
//!
//! assert_eq!(response.status, 200);
//! assert!(!response.is_subscription);
//! ```
//!
//! # Specification
//!
//! See [draft-toomim-httpbis-braid-http-04]:
//!
//! - **Section 2**: Versioning headers
//! - **Section 3**: Patch responses
//! - **Section 4**: Subscription responses (209)
//!
//! [draft-toomim-httpbis-braid-http-04]: https://datatracker.ietf.org/doc/html/draft-toomim-httpbis-braid-http

use crate::core::protocol;
use crate::core::types::{ContentRange, Version};
use bytes::Bytes;
use std::collections::BTreeMap;

/// HTTP response with Braid protocol information.
///
/// Wraps an HTTP response with Braid-specific metadata and provides
/// helper methods for extracting protocol headers.
///
/// # Creating Responses
///
/// ```
/// use crate::core::BraidResponse;
///
/// let response = BraidResponse::new(200, "body content")
///     .with_header("Version", r#""v1""#);
/// ```
///
/// # Extracting Headers
///
/// ```
/// use crate::core::BraidResponse;
///
/// let mut response = BraidResponse::new(200, "data");
/// response.headers.insert("Version".to_string(), r#""v1""#.to_string());
///
/// if let Some(versions) = response.get_version() {
///     println!("Versions: {:?}", versions);
/// }
/// ```
///
/// # Fields
///
/// - `status`: HTTP status code
/// - `headers`: Response headers
/// - `body`: Response body content
/// - `is_subscription`: Whether this is a subscription response (209)
#[derive(Clone, Debug)]
pub struct BraidResponse {
    /// HTTP status code.
    ///
    /// Common values: 200, 206, 209, 293, 410, 416.
    pub status: u16,

    /// Response headers.
    ///
    /// Contains Braid-specific headers like Version, Parents, Merge-Type.
    pub headers: BTreeMap<String, String>,

    /// Response body content.
    ///
    /// Contains either a complete snapshot or patch data.
    pub body: Bytes,

    /// Whether this is a subscription response.
    ///
    /// `true` when status is 209 (Subscription).
    pub is_subscription: bool,
}

impl BraidResponse {
    /// Create a new response with status and body.
    ///
    /// # Arguments
    ///
    /// * `status` - HTTP status code
    /// * `body` - Response body content
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::BraidResponse;
    ///
    /// let response = BraidResponse::new(200, "data");
    /// assert_eq!(response.status, 200);
    /// ```
    pub fn new(status: u16, body: impl Into<Bytes>) -> Self {
        BraidResponse {
            status,
            headers: BTreeMap::new(),
            body: body.into(),
            is_subscription: status == 209,
        }
    }

    /// Create a subscription response (status 209).
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::BraidResponse;
    ///
    /// let response = BraidResponse::subscription("update data");
    /// assert_eq!(response.status, 209);
    /// assert!(response.is_subscription);
    /// ```
    pub fn subscription(body: impl Into<Bytes>) -> Self {
        Self::new(209, body)
    }

    /// Add a header to the response.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::BraidResponse;
    ///
    /// let response = BraidResponse::new(200, "data")
    ///     .with_header("Version", r#""v1""#)
    ///     .with_header("Merge-Type", "diamond");
    /// ```
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    /// Get a header value by name (case-insensitive).
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::BraidResponse;
    ///
    /// let response = BraidResponse::new(200, "data")
    ///     .with_header("Content-Type", "application/json");
    ///
    /// assert_eq!(response.header("content-type"), Some("application/json"));
    /// ```
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Get version(s) from the Version header.
    ///
    /// Parses the Version header according to RFC 8941 structured headers format.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::BraidResponse;
    ///
    /// let response = BraidResponse::new(200, "data")
    ///     .with_header("Version", r#""v1", "v2""#);
    ///
    /// let versions = response.get_version().unwrap();
    /// assert_eq!(versions.len(), 2);
    /// ```
    pub fn get_version(&self) -> Option<Vec<Version>> {
        self.header("version")
            .and_then(|v| protocol::parse_version_header(v).ok())
    }

    /// Get parent version(s) from the Parents header.
    pub fn get_parents(&self) -> Option<Vec<Version>> {
        self.header("parents")
            .and_then(|v| protocol::parse_version_header(v).ok())
    }

    /// Get current version(s) from the Current-Version header.
    ///
    /// Used for subscription catch-up signaling.
    pub fn get_current_version(&self) -> Option<Vec<Version>> {
        self.header("current-version")
            .and_then(|v| protocol::parse_version_header(v).ok())
    }

    /// Get merge type from the Merge-Type header.
    pub fn get_merge_type(&self) -> Option<String> {
        self.header("merge-type").map(|s| s.to_string())
    }

    /// Get content range from the Content-Range header.
    pub fn get_content_range(&self) -> Option<ContentRange> {
        self.header("content-range")
            .and_then(|v| ContentRange::from_header_value(v).ok())
    }

    /// Get the body as a UTF-8 string.
    ///
    /// # Returns
    ///
    /// `Some(&str)` if the body is valid UTF-8, `None` otherwise.
    pub fn body_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.body).ok()
    }

    /// Check if this is a successful response (2xx status).
    #[inline]
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Check if this is a partial content response (206).
    #[inline]
    pub fn is_partial(&self) -> bool {
        self.status == 206
    }
}

impl Default for BraidResponse {
    fn default() -> Self {
        BraidResponse {
            status: 200,
            headers: BTreeMap::new(),
            body: Bytes::new(),
            is_subscription: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let response = BraidResponse::new(200, "data");
        assert_eq!(response.status, 200);
        assert!(!response.is_subscription);
    }

    #[test]
    fn test_subscription() {
        let response = BraidResponse::subscription("data");
        assert_eq!(response.status, 209);
        assert!(response.is_subscription);
    }

    #[test]
    fn test_with_header() {
        let response = BraidResponse::new(200, "data")
            .with_header("Version", r#""v1""#);
        assert_eq!(response.header("Version"), Some(r#""v1""#));
    }

    #[test]
    fn test_header_case_insensitive() {
        let response = BraidResponse::new(200, "data")
            .with_header("Content-Type", "application/json");
        assert_eq!(response.header("content-type"), Some("application/json"));
    }

    #[test]
    fn test_body_str() {
        let response = BraidResponse::new(200, "hello");
        assert_eq!(response.body_str(), Some("hello"));
    }

    #[test]
    fn test_is_success() {
        assert!(BraidResponse::new(200, "").is_success());
        assert!(BraidResponse::new(209, "").is_success());
        assert!(!BraidResponse::new(404, "").is_success());
    }

    #[test]
    fn test_is_partial() {
        assert!(BraidResponse::new(206, "").is_partial());
        assert!(!BraidResponse::new(200, "").is_partial());
    }

    #[test]
    fn test_default() {
        let response = BraidResponse::default();
        assert_eq!(response.status, 200);
        assert!(response.body.is_empty());
    }
}
