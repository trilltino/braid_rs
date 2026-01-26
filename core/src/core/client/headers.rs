//! Braid-specific HTTP header handling.
//!
//! This module handles encoding and parsing of Braid protocol headers according to
//! draft-toomim-httpbis-braid-http specifications.
//!
//! # Supported Headers
//!
//! ## Request Headers (sent by client)
//!
//! - **Version**: Requested version ID(s) (Section 2.3)
//! - **Parents**: Parent version(s) for history range (Section 2.4)
//! - **Subscribe**: Enable subscription mode (Section 4.1)
//! - **Heartbeats**: Keep-alive interval (Section 4.1)
//! - **Peer**: Client identifier for idempotence
//! - **Merge-Type**: Conflict resolution strategy (Section 2.2)
//! - **Content-Range**: Range specification for patches (Section 3)
//! - **Patches**: Number of patches in request
//!
//! ## Response Headers (sent by server)
//!
//! - **Version**: Current version ID(s) (Section 2)
//! - **Parents**: Parent version(s) in DAG (Section 2)
//! - **Current-Version**: Latest version for catch-up (Section 4.4)
//! - **Merge-Type**: Conflict resolution strategy used (Section 2.2)
//! - **Content-Range**: Range specification for patches (Section 3)
//!
//! # Format Specification
//!
//! All version headers use RFC 8941 Structured Headers format:
//! - Version IDs are quoted strings: `"v1"`, `"v2"`, etc.
//! - Multiple values are comma-separated: `"v1", "v2", "v3"`
//!
//! Content-Range format: `"{unit} {range}"`
//! - Example: `"json .field"` or `"bytes 0:100"`
//!
//! # Examples
//!
//! ```ignore
//! use crate::core::client::BraidHeaders;
//! use crate::core::Version;
//!
//! let headers = BraidHeaders::new()
//!     .with_version(Version::new("v1"))
//!     .with_subscribe()
//!     .with_merge_type("sync9")
//!     .with_heartbeat("30s".to_string());
//!
//! let map = headers.to_header_map()?;
//! ```

use crate::core::error::{BraidError, Result};
use crate::core::protocol;
use crate::core::types::Version;
use http::header::{HeaderMap, HeaderValue};

/// Braid-specific HTTP headers.
///
/// Encapsulates all Braid protocol headers that can be sent in HTTP requests or responses.
/// Use the builder pattern to construct headers.
///
/// # Usage
///
/// ```ignore
/// use crate::core::client::BraidHeaders;
/// use crate::core::Version;
///
/// let headers = BraidHeaders::new()
///     .with_version(Version::new("v1"))
///     .with_parent(Version::new("v0"))
///     .with_subscribe()
///     .with_merge_type("sync9");
///
/// let header_map = headers.to_header_map()?;
/// ```
///
/// # Specification
///
/// See draft-toomim-httpbis-braid-http sections on versioning (2), patches (3),
/// and subscriptions (4).
#[derive(Clone, Debug, Default)]
pub struct BraidHeaders {
    /// Version identifier(s) from Version header (Section 2)
    pub version: Option<Vec<Version>>,
    /// Parent version(s) from Parents header (Section 2)
    pub parents: Option<Vec<Version>>,
    /// Current version(s) from Current-Version header (Section 4.4)
    pub current_version: Option<Vec<Version>>,
    /// Subscribe header indicating subscription mode (Section 4.1)
    pub subscribe: bool,
    /// Peer identifier from Peer header
    pub peer: Option<String>,
    /// Heartbeat interval from Heartbeats header (Section 4.1)
    pub heartbeat: Option<String>,
    /// Merge type from Merge-Type header (Section 2.2)
    pub merge_type: Option<String>,
    /// Number of patches from Patches header
    pub patches_count: Option<usize>,
    /// Content range from Content-Range header (Section 3)
    pub content_range: Option<String>,
    /// Retry-After header for backoff guidance
    pub retry_after: Option<String>,
    /// Additional non-Braid headers
    pub extra: std::collections::BTreeMap<String, String>,
}

impl BraidHeaders {
    /// Create new empty Braid headers
    pub fn new() -> Self {
        Self::default()
    }

    /// Set version
    pub fn with_version(mut self, version: Version) -> Self {
        let mut versions = self.version.unwrap_or_default();
        versions.push(version);
        self.version = Some(versions);
        self
    }

    /// Set versions
    pub fn with_versions(mut self, versions: Vec<Version>) -> Self {
        self.version = Some(versions);
        self
    }

    /// Set parent version
    pub fn with_parent(mut self, parent: Version) -> Self {
        let mut parents = self.parents.unwrap_or_default();
        parents.push(parent);
        self.parents = Some(parents);
        self
    }

    /// Set parent versions
    pub fn with_parents(mut self, parents: Vec<Version>) -> Self {
        self.parents = Some(parents);
        self
    }

    /// Set current version
    pub fn with_current_version(mut self, version: Version) -> Self {
        let mut versions = self.current_version.unwrap_or_default();
        versions.push(version);
        self.current_version = Some(versions);
        self
    }

    /// Set current versions
    pub fn with_current_versions(mut self, versions: Vec<Version>) -> Self {
        self.current_version = Some(versions);
        self
    }

    /// Enable subscription
    pub fn with_subscribe(mut self) -> Self {
        self.subscribe = true;
        self
    }

    /// Set merge type
    pub fn with_merge_type(mut self, merge_type: impl Into<String>) -> Self {
        self.merge_type = Some(merge_type.into());
        self
    }

    /// Set content range
    pub fn with_content_range(mut self, content_range: impl Into<String>) -> Self {
        self.content_range = Some(content_range.into());
        self
    }

    /// Set heartbeat interval
    pub fn with_heartbeat(mut self, interval: String) -> Self {
        self.heartbeat = Some(interval);
        self
    }

    /// Set peer identifier
    pub fn with_peer(mut self, peer: String) -> Self {
        self.peer = Some(peer);
        self
    }

    /// Convert to HTTP HeaderMap
    pub fn to_header_map(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();

        if let Some(ref versions) = self.version {
            let version_str = protocol::format_version_header(versions);
            headers.insert(
                "Version",
                HeaderValue::from_str(&version_str)
                    .map_err(|e| BraidError::Config(e.to_string()))?,
            );
        }

        if let Some(ref parents) = self.parents {
            let parents_str = protocol::format_version_header(parents);
            headers.insert(
                "Parents",
                HeaderValue::from_str(&parents_str)
                    .map_err(|e| BraidError::Config(e.to_string()))?,
            );
        }

        if let Some(ref current_versions) = self.current_version {
            let current_version_str = protocol::format_version_header(current_versions);
            headers.insert(
                "Current-Version",
                HeaderValue::from_str(&current_version_str)
                    .map_err(|e| BraidError::Config(e.to_string()))?,
            );
        }

        if self.subscribe {
            headers.insert("Subscribe", HeaderValue::from_static("true"));
        }

        if let Some(ref peer) = self.peer {
            headers.insert(
                "Peer",
                HeaderValue::from_str(peer)
                    .map_err(|e| BraidError::Config(e.to_string()))?,
            );
        }

        if let Some(ref heartbeat) = self.heartbeat {
            headers.insert(
                "Heartbeats",
                HeaderValue::from_str(heartbeat)
                    .map_err(|e| BraidError::Config(e.to_string()))?,
            );
        }

        if let Some(ref merge_type) = self.merge_type {
            headers.insert(
                "Merge-Type",
                HeaderValue::from_str(merge_type)
                    .map_err(|e| BraidError::Config(e.to_string()))?,
            );
        }

        if let Some(count) = self.patches_count {
            headers.insert(
                "Patches",
                HeaderValue::from_str(&count.to_string())
                    .map_err(|e| BraidError::Config(e.to_string()))?,
            );
        }

        if let Some(ref content_range) = self.content_range {
            headers.insert(
                "Content-Range",
                HeaderValue::from_str(content_range)
                    .map_err(|e| BraidError::Config(e.to_string()))?,
            );
        }

        Ok(headers)
    }

    /// Parse from HTTP HeaderMap
    pub fn from_header_map(headers: &HeaderMap) -> Result<Self> {
        let mut braid_headers = BraidHeaders::new();

        for (name, value) in headers.iter() {
            let name_lower = name.as_str().to_lowercase();
            let value_str = value
                .to_str()
                .map_err(|_| BraidError::HeaderParse("Invalid header value".to_string()))?;

            match name_lower.as_str() {
                "version" => {
                    braid_headers.version = Some(protocol::parse_version_header(value_str)?);
                }
                "parents" => {
                    braid_headers.parents = Some(protocol::parse_version_header(value_str)?);
                }
                "current-version" => {
                    braid_headers.current_version = Some(protocol::parse_version_header(value_str)?);
                }
                "subscribe" => {
                    braid_headers.subscribe = value_str.to_lowercase() == "true";
                }
                "peer" => {
                    braid_headers.peer = Some(value_str.to_string());
                }
                "heartbeats" => {
                    braid_headers.heartbeat = Some(value_str.to_string());
                }
                "merge-type" => {
                    braid_headers.merge_type = Some(value_str.to_string());
                }
                "patches" => {
                    braid_headers.patches_count = value_str.parse().ok();
                }
                "content-range" => {
                    braid_headers.content_range = Some(value_str.to_string());
                }
                "retry-after" => {
                    braid_headers.retry_after = Some(value_str.to_string());
                }
                _ => {
                    braid_headers
                        .extra
                        .insert(name_lower, value_str.to_string());
                }
            }
        }

        Ok(braid_headers)
    }
}

/// Header parser for protocol messages.
///
/// Provides utility methods for parsing and formatting Braid protocol headers
/// in RFC 8941 Structured Headers format.
///
/// # Header Format
///
/// - **Version/Parents**: Comma-separated quoted strings: `"v1", "v2", "v3"`
/// - **Content-Range**: `"{unit} {range}"` (e.g., `"json .field"`)
pub struct HeaderParser;

impl HeaderParser {
    /// Parse version header value.
    ///
    /// Handles comma-separated quoted version strings per RFC 8941.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let versions = HeaderParser::parse_version("\"v1\", \"v2\"")?;
    /// assert_eq!(versions.len(), 2);
    /// ```
    pub fn parse_version(value: &str) -> Result<Vec<Version>> {
        protocol::parse_version_header(value)
    }

    /// Parse Content-Range header.
    ///
    /// Splits header into (unit, range) tuple.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let (unit, range) = HeaderParser::parse_content_range("json .field")?;
    /// assert_eq!(unit, "json");
    /// assert_eq!(range, ".field");
    /// ```
    pub fn parse_content_range(value: &str) -> Result<(String, String)> {
        protocol::parse_content_range(value)
    }

    /// Format version header with RFC 8941 Structured Headers.
    ///
    /// Converts version IDs to quoted, comma-separated format.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use crate::core::Version;
    ///
    /// let header = HeaderParser::format_version(&[
    ///     Version::new("v1"),
    ///     Version::new("v2"),
    /// ]);
    /// assert_eq!(header, "\"v1\", \"v2\"");
    /// ```
    pub fn format_version(versions: &[Version]) -> String {
        protocol::format_version_header(versions)
    }

    /// Format Content-Range header.
    ///
    /// Combines unit and range into header format.
    pub fn format_content_range(unit: &str, range: &str) -> String {
        protocol::format_content_range(unit, range)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_braid_headers_to_map() {
        let headers = BraidHeaders::new()
            .with_version(Version::String("v1".to_string()))
            .with_subscribe();

        let map = headers.to_header_map().unwrap();
        assert!(map.contains_key("Version"));
        assert!(map.contains_key("Subscribe"));
    }

    #[test]
    fn test_parse_version_header() {
        let result = protocol::parse_version_header("\"v1\", \"v2\", \"v3\"").unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_parse_content_range() {
        let (unit, range) = protocol::parse_content_range("json .field").unwrap();
        assert_eq!(unit, "json");
        assert_eq!(range, ".field");
    }
}
