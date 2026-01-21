//! Shared header parsing and formatting for Braid-HTTP.
//!
//! This module provides utilities for parsing and formatting Braid protocol headers
//! according to [RFC 8941 Structured Headers] format.
//!
//! # Header Formats
//!
//! | Header | Format | Example |
//! |--------|--------|---------|
//! | Version | Quoted, comma-separated | `"v1", "v2"` |
//! | Parents | Quoted, comma-separated | `"v0"` |
//! | Content-Range | `{unit} {range}` | `json .field` |
//! | Heartbeats | Number with optional unit | `30s`, `500ms`, `30` |
//!
//! # Examples
//!
//! ```
//! use crate::core::protocol::{
//!     parse_version_header, format_version_header,
//!     parse_content_range, format_content_range,
//!     parse_heartbeat,
//! };
//! use crate::core::Version;
//!
//! // Version headers
//! let versions = parse_version_header(r#""v1", "v2""#).unwrap();
//! let header = format_version_header(&versions);
//!
//! // Content-Range headers
//! let (unit, range) = parse_content_range("json .field").unwrap();
//! let header = format_content_range("bytes", "0:100");
//!
//! // Heartbeat headers
//! let seconds = parse_heartbeat("30s").unwrap();
//! ```
//!
//! [RFC 8941 Structured Headers]: https://datatracker.ietf.org/doc/html/rfc8941

use crate::core::error::{BraidError, Result};
use crate::core::types::Version;

/// Parse version header value (comma-separated quoted strings).
///
/// Handles [RFC 8941 Structured Headers] format for version identifiers.
/// Version headers are parsed as a List of Items (Strings or Integers).
///
/// # Arguments
///
/// * `value` - The header value to parse
///
/// # Returns
///
/// A vector of [`Version`] values, or an empty vector if the input is empty.
///
/// # Examples
///
/// ```
/// use crate::core::protocol::parse_version_header;
///
/// // Quoted versions
/// let versions = parse_version_header(r#""v1", "v2""#).unwrap();
/// assert_eq!(versions.len(), 2);
///
/// // Integer versions
/// let versions = parse_version_header("1, 2").unwrap();
/// assert_eq!(versions.len(), 2);
/// ```
///
/// [RFC 8941 Structured Headers]: https://datatracker.ietf.org/doc/html/rfc8941
pub fn parse_version_header(value: &str) -> Result<Vec<Version>> {
    use sfv::{BareItem, FieldType, List, ListEntry, Parser};

    let list: List = Parser::new(value)
        .parse()
        .map_err(|e| BraidError::HeaderParse(format!("Invalid structured header: {:?}", e)))?;

    let mut versions = Vec::new();
    for member in list {
        match member {
            ListEntry::Item(item) => match item.bare_item {
                BareItem::String(s) => versions.push(Version::String(s.into())),
                BareItem::Integer(i) => versions.push(Version::Integer(i.into())),
                BareItem::Token(t) => versions.push(Version::String(t.into())),
                _ => {
                    return Err(BraidError::HeaderParse(
                        "Unsupported item type in version list".to_string(),
                    ))
                }
            },
            ListEntry::InnerList(_) => {
                return Err(BraidError::HeaderParse(
                    "Inner lists not supported for versions".to_string(),
                ))
            }
        }
    }

    Ok(versions)
}

/// Format version header value (comma-separated quoted strings).
///
/// Converts a vector of versions to [RFC 8941 Structured Headers] format.
///
/// # Arguments
///
/// * `versions` - Slice of versions to format
///
/// # Examples
///
/// ```
/// use crate::core::{Version, protocol::format_version_header};
///
/// let versions = vec![Version::new("v1"), Version::new("v2")];
/// let header = format_version_header(&versions);
/// assert_eq!(header, r#""v1", "v2""#);
/// ```
///
/// [RFC 8941 Structured Headers]: https://datatracker.ietf.org/doc/html/rfc8941
pub fn format_version_header(versions: &[Version]) -> String {
    use sfv::{BareItem, FieldType, Item, List, ListEntry};

    let mut list = List::new();
    for version in versions {
        let bare_item = match version {
            Version::String(s) => Bare_item_to_sfv_member(s),
            Version::Integer(i) => BareItem::Integer((*i).try_into().unwrap()),
        };
        list.push(ListEntry::Item(Item::new(bare_item)));
    }

    list.serialize().unwrap_or_default()
}

/// Helper to convert a string to a BareItem (preferring Token if valid, otherwise String).
fn Bare_item_to_sfv_member(s: &str) -> sfv::BareItem {
    // Try to parse as token first, fallback to string if invalid token chars
    if let Ok(token) = sfv::Token::try_from(s.to_string()) {
        sfv::BareItem::Token(token)
    } else {
        sfv::BareItem::String(sfv::String::try_from(s.to_string()).unwrap())
    }
}

/// Parse Current-Version header value.
///
/// Identical to Version header parsing but semantically represents the latest version.
pub fn parse_current_version_header(value: &str) -> Result<Vec<Version>> {
    parse_version_header(value)
}

/// Parse Content-Range header.
///
/// Extracts unit and range from a Content-Range header value.
/// Format: `"{unit} {range}"` (e.g., `"json .field"` or `"bytes 0:100"`).
///
/// # Arguments
///
/// * `value` - The header value to parse
///
/// # Returns
///
/// A tuple of `(unit, range)` strings.
///
/// # Errors
///
/// Returns an error if the value doesn't contain a space separator.
///
/// # Examples
///
/// ```
/// use crate::core::protocol::parse_content_range;
///
/// let (unit, range) = parse_content_range("json .field").unwrap();
/// assert_eq!(unit, "json");
/// assert_eq!(range, ".field");
///
/// let (unit, range) = parse_content_range("bytes 0:100").unwrap();
/// assert_eq!(unit, "bytes");
/// assert_eq!(range, "0:100");
/// ```
pub fn parse_content_range(value: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = value.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return Err(BraidError::HeaderParse(format!(
            "Invalid Content-Range: expected 'unit range', got '{}'",
            value
        )));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Format Content-Range header.
///
/// Creates a Content-Range header value from unit and range components.
///
/// # Arguments
///
/// * `unit` - The addressing unit (e.g., `"json"`, `"bytes"`)
/// * `range` - The range specification
///
/// # Examples
///
/// ```
/// use crate::core::protocol::format_content_range;
///
/// let header = format_content_range("json", ".field");
/// assert_eq!(header, "json .field");
///
/// let header = format_content_range("bytes", "0:100");
/// assert_eq!(header, "bytes 0:100");
/// ```
#[inline]
pub fn format_content_range(unit: &str, range: &str) -> String {
    format!("{} {}", unit, range)
}

/// Parse heartbeat interval.
///
/// Converts heartbeat header value to seconds.
///
/// # Supported Formats
///
/// | Format | Example | Result |
/// |--------|---------|--------|
/// | Seconds with suffix | `"5s"` | 5 |
/// | Milliseconds | `"500ms"` | 0 (rounded down) |
/// | Plain number | `"30"` | 30 |
///
/// # Arguments
///
/// * `value` - The header value to parse
///
/// # Returns
///
/// The interval in seconds.
///
/// # Errors
///
/// Returns an error if the value cannot be parsed as a number.
///
/// # Examples
///
/// ```
/// use crate::core::protocol::parse_heartbeat;
///
/// assert_eq!(parse_heartbeat("5s").unwrap(), 5);
/// assert_eq!(parse_heartbeat("30").unwrap(), 30);
/// assert_eq!(parse_heartbeat("1000ms").unwrap(), 1);
/// ```
pub fn parse_heartbeat(value: &str) -> Result<u64> {
    let trimmed = value.trim();

    // Handle milliseconds
    if let Some(ms_str) = trimmed.strip_suffix("ms") {
        return ms_str
            .parse::<u64>()
            .map(|n| n / 1000)
            .map_err(|_| BraidError::HeaderParse(format!("Invalid heartbeat: {}", value)));
    }

    // Handle seconds with 's' suffix
    if let Some(s_str) = trimmed.strip_suffix('s') {
        return s_str
            .parse()
            .map_err(|_| BraidError::HeaderParse(format!("Invalid heartbeat: {}", value)));
    }

    // Plain number (assume seconds)
    trimmed
        .parse()
        .map_err(|_| BraidError::HeaderParse(format!("Invalid heartbeat: {}", value)))
}

/// Parse and validate Merge-Type header.
///
/// Ensures the merge type is one of the supported values (e.g., "sync9", "diamond").
///
/// # Arguments
///
/// * `value` - The header value to parse
///
/// # Returns
///
/// The validated merge type string.
///
/// # Errors
///
/// Returns an error if the merge type is unknown or invalid.
///
/// # Examples
///
/// ```
/// use crate::core::protocol::parse_merge_type;
///
/// assert!(parse_merge_type("diamond").is_ok());
/// assert!(parse_merge_type("invalid").is_err());
/// ```
pub fn parse_merge_type(value: &str) -> Result<String> {
    let trimmed = value.trim();
    match trimmed {
        crate::core::protocol::constants::merge_types::SYNC9
        | crate::core::protocol::constants::merge_types::DIAMOND => Ok(trimmed.to_string()),
        _ => Err(BraidError::HeaderParse(format!(
            "Unsupported merge-type: {}",
            value
        ))),
    }
}

/// Parse a tunneled response (status line and headers).
/// Format: ":status: {code}\r\n{header}: {value}\r\n\r\n"
pub fn parse_tunneled_response(
    bytes: &[u8],
) -> Result<(u16, std::collections::BTreeMap<String, String>, usize)> {
    let s = String::from_utf8_lossy(bytes);
    if let Some(end_idx) = s.find("\r\n\r\n") {
        let headers_part = &s[..end_idx];
        let mut status = 200;
        let mut headers = std::collections::BTreeMap::new();

        for line in headers_part.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Handle ":status: 200"
            if let Some(val) = line.strip_prefix(":status:") {
                let val = val.trim();
                status = val.parse().unwrap_or(200);
                continue;
            }

            if let Some((name, value)) = line.split_once(':') {
                let name = name.trim().to_lowercase();
                let value = value.trim();
                headers.insert(name, value.to_string());
            }
        }
        Ok((status, headers, end_idx + 4))
    } else {
        Err(BraidError::HeaderParse("Incomplete headers".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_header() {
        let result = parse_version_header(r#""v1", "v2", "v3""#).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], Version::new("v1"));
    }

    #[test]
    fn test_parse_version_header_integers() {
        let result = parse_version_header("11, 12, \"v1\"").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], Version::Integer(11));
        assert_eq!(result[1], Version::Integer(12));
        assert_eq!(result[2], Version::String("v1".to_string()));
    }

    #[test]
    fn test_parse_version_header_unquoted() {
        let result = parse_version_header("v1, v2").unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_parse_version_header_empty() {
        let result = parse_version_header("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_format_version_header() {
        let versions = vec![Version::new("v1"), Version::new("v2")];
        let header = format_version_header(&versions);
        assert_eq!(header, r#""v1", "v2""#);
    }

    #[test]
    fn test_parse_content_range() {
        let (unit, range) = parse_content_range("json .field").unwrap();
        assert_eq!(unit, "json");
        assert_eq!(range, ".field");
    }

    #[test]
    fn test_parse_content_range_complex() {
        let (unit, range) = parse_content_range("json .users[0].name").unwrap();
        assert_eq!(unit, "json");
        assert_eq!(range, ".users[0].name");
    }

    #[test]
    fn test_parse_content_range_invalid() {
        let result = parse_content_range("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_format_content_range() {
        let result = format_content_range("bytes", "0:100");
        assert_eq!(result, "bytes 0:100");
    }

    #[test]
    fn test_parse_heartbeat_seconds() {
        assert_eq!(parse_heartbeat("5s").unwrap(), 5);
    }

    #[test]
    fn test_parse_heartbeat_plain() {
        assert_eq!(parse_heartbeat("30").unwrap(), 30);
    }

    #[test]
    fn test_parse_heartbeat_milliseconds() {
        assert_eq!(parse_heartbeat("1000ms").unwrap(), 1);
        assert_eq!(parse_heartbeat("500ms").unwrap(), 0);
    }

    #[test]
    fn test_parse_heartbeat_invalid() {
        assert!(parse_heartbeat("abc").is_err());
    }

    #[test]
    fn test_parse_merge_type() {
        assert!(parse_merge_type("diamond").is_ok());
        assert!(parse_merge_type("sync9").is_ok());
        assert!(parse_merge_type("unknown").is_err());
    }
}
