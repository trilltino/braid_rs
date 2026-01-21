//! Protocol parsing utilities.
//!
//! This module provides shared parsing utilities for Braid protocol messages
//! and structures. It includes a `HeaderParser` type that provides a convenient
//! API for parsing and formatting protocol headers.
//!
//! # HeaderParser
//!
//! The `HeaderParser` is a zero-sized type (no runtime cost) that provides
//! static methods for parsing and formatting Braid protocol headers. It serves
//! as a namespace for header-related operations.
//!
//! # Examples
//!
//! ```ignore
//! use crate::core::protocol::HeaderParser;
//! use crate::core::Version;
//!
//! // Parse headers
//! let versions = HeaderParser::parse_version("\"v1\", \"v2\"")?;
//! let (unit, range) = HeaderParser::parse_content_range("json .field")?;
//!
//! // Format headers
//! let header = HeaderParser::format_version(&versions);
//! let range_header = HeaderParser::format_content_range("bytes", "0:100");
//! ```
//!
//! # Alternative API
//!
//! You can also use the free functions directly from the `protocol` module:
//!
//! ```ignore
//! use crate::core::protocol;
//!
//! let versions = protocol::parse_version_header("\"v1\"")?;
//! let header = protocol::format_version_header(&versions);
//! ```

use crate::core::error::Result;
use crate::core::types::Version;

/// Header parser for protocol messages.
///
/// Provides utility methods for parsing and formatting Braid protocol headers
/// in RFC 8941 Structured Headers format. This is a zero-sized type that serves
/// as a namespace for header operations.
///
/// # Zero-Sized Type
///
/// `HeaderParser` has no fields and takes no space at runtime. It exists solely
/// to provide a convenient namespace for header parsing operations.
///
/// # Examples
///
/// ```ignore
/// use crate::core::protocol::HeaderParser;
///
/// let versions = HeaderParser::parse_version("\"v1\", \"v2\"")?;
/// ```
pub struct HeaderParser;

impl HeaderParser {
    /// Parse version header value.
    ///
    /// Handles comma-separated quoted version strings per RFC 8941.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use crate::core::protocol::HeaderParser;
    ///
    /// let versions = HeaderParser::parse_version("\"v1\", \"v2\"")?;
    /// assert_eq!(versions.len(), 2);
    /// ```
    pub fn parse_version(value: &str) -> Result<Vec<Version>> {
        crate::core::protocol::parse_version_header(value)
    }

    /// Parse Content-Range header.
    ///
    /// Splits header into (unit, range) tuple.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use crate::core::protocol::HeaderParser;
    ///
    /// let (unit, range) = HeaderParser::parse_content_range("json .field")?;
    /// assert_eq!(unit, "json");
    /// assert_eq!(range, ".field");
    /// ```
    pub fn parse_content_range(value: &str) -> Result<(String, String)> {
        crate::core::protocol::parse_content_range(value)
    }

    /// Format version header with RFC 8941 Structured Headers.
    ///
    /// Converts version IDs to quoted, comma-separated format.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use crate::core::{Version, protocol::HeaderParser};
    ///
    /// let header = HeaderParser::format_version(&[
    ///     Version::new("v1"),
    ///     Version::new("v2"),
    /// ]);
    /// assert_eq!(header, "\"v1\", \"v2\"");
    /// ```
    pub fn format_version(versions: &[Version]) -> String {
        crate::core::protocol::format_version_header(versions)
    }

    /// Format Content-Range header.
    ///
    /// Combines unit and range into header format.
    pub fn format_content_range(unit: &str, range: &str) -> String {
        crate::core::protocol::format_content_range(unit, range)
    }
}

