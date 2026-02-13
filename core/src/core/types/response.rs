//! HTTP response with Braid protocol information.

use crate::core::protocol;
use crate::core::types::{ContentRange, Version};
use bytes::Bytes;
use std::collections::BTreeMap;

/// HTTP response with Braid protocol information.
#[derive(Clone, Debug)]
pub struct BraidResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Bytes,
    pub is_subscription: bool,
}

impl BraidResponse {
    pub fn new(status: u16, body: impl Into<Bytes>) -> Self {
        BraidResponse {
            status,
            headers: BTreeMap::new(),
            body: body.into(),
            is_subscription: status == 209,
        }
    }

    pub fn subscription(body: impl Into<Bytes>) -> Self {
        Self::new(209, body)
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn get_version(&self) -> Option<Vec<Version>> {
        self.header("version")
            .and_then(|v| protocol::parse_version_header(v).ok())
    }

    pub fn get_parents(&self) -> Option<Vec<Version>> {
        self.header("parents")
            .and_then(|v| protocol::parse_version_header(v).ok())
    }

    pub fn get_current_version(&self) -> Option<Vec<Version>> {
        self.header("current-version")
            .and_then(|v| protocol::parse_version_header(v).ok())
    }

    pub fn get_merge_type(&self) -> Option<String> {
        self.header("merge-type").map(|s| s.to_string())
    }

    pub fn get_content_range(&self) -> Option<ContentRange> {
        self.header("content-range")
            .and_then(|v| ContentRange::from_header_value(v).ok())
    }

    pub fn body_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.body).ok()
    }

    #[inline]
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
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

#[cfg(feature = "fuzzing")]
impl<'a> arbitrary::Arbitrary<'a> for BraidResponse {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let status: u16 = u.arbitrary()?;
        Ok(BraidResponse {
            status,
            headers: u.arbitrary()?,
            body: bytes::Bytes::from(u.arbitrary::<Vec<u8>>()?),
            is_subscription: status == 209,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== Constructor Tests ==========

    #[test]
    fn test_braid_response_new() {
        let response = BraidResponse::new(200, "test body");
        assert_eq!(response.status, 200);
        assert_eq!(response.body_str(), Some("test body"));
        assert!(!response.is_subscription);
    }

    #[test]
    fn test_braid_response_new_empty_body() {
        let response = BraidResponse::new(200, "");
        assert_eq!(response.status, 200);
        assert!(response.body_str().unwrap().is_empty());
    }

    #[test]
    fn test_braid_response_subscription() {
        let response = BraidResponse::subscription("content");
        assert_eq!(response.status, 209);
        assert!(response.is_subscription);
        assert_eq!(response.body_str(), Some("content"));
    }

    #[test]
    fn test_braid_response_default() {
        let response: BraidResponse = Default::default();
        assert_eq!(response.status, 200);
        assert!(response.headers.is_empty());
        assert!(response.body.is_empty());
        assert!(!response.is_subscription);
    }

    // ========== Header Tests ==========

    #[test]
    fn test_with_header_basic() {
        let response = BraidResponse::new(200, "test")
            .with_header("version", "\"v1\"");
        
        assert_eq!(response.header("version"), Some("\"v1\""));
    }

    #[test]
    fn test_with_header_multiple() {
        let response = BraidResponse::new(200, "test")
            .with_header("version", "\"v1\"")
            .with_header("parents", "\"v0\"");
        
        assert_eq!(response.header("version"), Some("\"v1\""));
        assert_eq!(response.header("parents"), Some("\"v0\""));
    }

    #[test]
    fn test_header_case_insensitive() {
        let response = BraidResponse::new(200, "test")
            .with_header("Version", "\"v1\"");
        
        assert_eq!(response.header("version"), Some("\"v1\""));
        assert_eq!(response.header("VERSION"), Some("\"v1\""));
    }

    #[test]
    fn test_header_not_found() {
        let response = BraidResponse::new(200, "test");
        assert_eq!(response.header("nonexistent"), None);
    }

    // ========== Version Header Parsing Tests ==========

    #[test]
    fn test_get_version_single() {
        let response = BraidResponse::new(200, "test")
            .with_header("version", "\"v1\"");
        
        let versions = response.get_version().unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0], Version::new("v1"));
    }

    #[test]
    fn test_get_version_multiple() {
        let response = BraidResponse::new(200, "test")
            .with_header("version", "\"v1\", \"v2\"");
        
        let versions = response.get_version().unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0], Version::new("v1"));
        assert_eq!(versions[1], Version::new("v2"));
    }

    #[test]
    fn test_get_version_integers() {
        let response = BraidResponse::new(200, "test")
            .with_header("version", "1, 2, 3");
        
        let versions = response.get_version().unwrap();
        assert_eq!(versions, vec![Version::Integer(1), Version::Integer(2), Version::Integer(3)]);
    }

    #[test]
    fn test_get_version_none() {
        let response = BraidResponse::new(200, "test");
        assert!(response.get_version().is_none());
    }

    #[test]
    fn test_get_version_invalid() {
        let response = BraidResponse::new(200, "test")
            .with_header("version", "invalid[");
        
        assert!(response.get_version().is_none());
    }

    // ========== Parents Header Parsing Tests ==========

    #[test]
    fn test_get_parents_single() {
        let response = BraidResponse::new(200, "test")
            .with_header("parents", "\"v0\"");
        
        let parents = response.get_parents().unwrap();
        assert_eq!(parents.len(), 1);
        assert_eq!(parents[0], Version::new("v0"));
    }

    #[test]
    fn test_get_parents_multiple() {
        let response = BraidResponse::new(200, "test")
            .with_header("parents", "\"v0\", \"v1\"");
        
        let parents = response.get_parents().unwrap();
        assert_eq!(parents.len(), 2);
    }

    #[test]
    fn test_get_parents_none() {
        let response = BraidResponse::new(200, "test");
        assert!(response.get_parents().is_none());
    }

    // ========== Current Version Tests ==========

    #[test]
    fn test_get_current_version() {
        let response = BraidResponse::new(200, "test")
            .with_header("current-version", "\"v5\"");
        
        let version = response.get_current_version().unwrap();
        assert_eq!(version[0], Version::new("v5"));
    }

    // ========== Merge Type Tests ==========

    #[test]
    fn test_get_merge_type() {
        let response = BraidResponse::new(200, "test")
            .with_header("merge-type", "diamond");
        
        assert_eq!(response.get_merge_type(), Some("diamond".to_string()));
    }

    #[test]
    fn test_get_merge_type_none() {
        let response = BraidResponse::new(200, "test");
        assert!(response.get_merge_type().is_none());
    }

    // ========== Content Range Tests ==========

    #[test]
    fn test_get_content_range_json() {
        let response = BraidResponse::new(206, "test")
            .with_header("content-range", "json .field");
        
        let range = response.get_content_range().unwrap();
        assert_eq!(range.unit, "json");
        assert_eq!(range.range, ".field");
    }

    #[test]
    fn test_get_content_range_bytes() {
        let response = BraidResponse::new(206, "test")
            .with_header("content-range", "bytes 0-100");
        
        let range = response.get_content_range().unwrap();
        assert_eq!(range.unit, "bytes");
    }

    #[test]
    fn test_get_content_range_none() {
        let response = BraidResponse::new(200, "test");
        assert!(response.get_content_range().is_none());
    }

    // ========== Body Tests ==========

    #[test]
    fn test_body_str_utf8() {
        let response = BraidResponse::new(200, "hello world");
        assert_eq!(response.body_str(), Some("hello world"));
    }

    #[test]
    fn test_body_str_invalid_utf8() {
        let response = BraidResponse::new(200, vec![0x80, 0x81, 0x82]);
        assert_eq!(response.body_str(), None); // Invalid UTF-8
    }

    #[test]
    fn test_body_str_empty() {
        let response = BraidResponse::new(200, "");
        assert_eq!(response.body_str(), Some(""));
    }

    // ========== Status Code Tests ==========

    #[test]
    fn test_is_success_200() {
        assert!(BraidResponse::new(200, "").is_success());
    }

    #[test]
    fn test_is_success_201() {
        assert!(BraidResponse::new(201, "").is_success());
    }

    #[test]
    fn test_is_success_209() {
        assert!(BraidResponse::new(209, "").is_success()); // Subscription
    }

    #[test]
    fn test_is_success_400() {
        assert!(!BraidResponse::new(400, "").is_success());
    }

    #[test]
    fn test_is_success_404() {
        assert!(!BraidResponse::new(404, "").is_success());
    }

    #[test]
    fn test_is_success_500() {
        assert!(!BraidResponse::new(500, "").is_success());
    }

    #[test]
    fn test_is_partial_206() {
        assert!(BraidResponse::new(206, "").is_partial());
    }

    #[test]
    fn test_is_partial_200() {
        assert!(!BraidResponse::new(200, "").is_partial());
    }

    #[test]
    fn test_is_partial_209() {
        assert!(!BraidResponse::new(209, "").is_partial());
    }
}
