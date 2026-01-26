//! Content-Range specification for patches.
//!
//! The `Content-Range` header specifies which portion of a resource a patch applies to.
//! It follows the format `"{unit} {range}"` where:
//!
//! - **unit**: The addressing scheme (e.g., `"json"`, `"bytes"`)
//! - **range**: The location within the resource using the unit's syntax
//!
//! # Header Format
//!
//! ```text
//! Content-Range: json .users[0].name
//! Content-Range: bytes 100:200
//! Content-Range: lines 10:20
//! ```
//!
//! # Supported Units
//!
//! | Unit | Range Syntax | Example |
//! |------|--------------|---------|
//! | `json` | JSONPath-like | `.field`, `.array[0]`, `.nested.path` |
//! | `bytes` | Byte range | `0:100`, `100-200` |
//! | `text` | Text path | `.title` |
//! | `lines` | Line range | `10:20` |
//!
//! Custom application-defined units are also supported.
//!
//! # Examples
//!
//! ```
//! use crate::core::ContentRange;
//!
//! // Create from components
//! let json_range = ContentRange::new("json", ".messages[1:1]");
//! let byte_range = ContentRange::new("bytes", "0:100");
//!
//! // Format as header value
//! assert_eq!(json_range.to_header_value(), "json .messages[1:1]");
//!
//! // Parse from header value
//! let parsed = ContentRange::from_header_value("json .field").unwrap();
//! assert_eq!(parsed.unit, "json");
//! assert_eq!(parsed.range, ".field");
//! ```
//!
//! # Relationship to Patches
//!
//! [`ContentRange`] is closely related to [`Patch`](crate::core::Patch). A patch contains
//! a content range (unit + range) plus the actual content to apply:
//!
//! ```
//! use crate::core::{ContentRange, Patch};
//!
//! let range = ContentRange::new("json", ".name");
//! let patch = Patch::json(".name", r#""Alice""#);
//!
//! // Both represent the same range
//! assert_eq!(range.unit, patch.unit);
//! assert_eq!(range.range, patch.range);
//! ```
//!
//! # Specification
//!
//! See Section 3.1 of [draft-toomim-httpbis-braid-http-04] for Content-Range format.
//! Also related to [RFC 7233 HTTP Range Requests](https://datatracker.ietf.org/doc/html/rfc7233).
//!
//! [draft-toomim-httpbis-braid-http-04]: https://datatracker.ietf.org/doc/html/draft-toomim-httpbis-braid-http

use std::fmt;
use std::str::FromStr;

/// Content-Range specification for patches.
///
/// Specifies the range of a resource that a patch applies to.
/// Format: `"{unit} {range}"` (e.g., `"json .field"` or `"bytes 0:100"`)
///
/// # Fields
///
/// - `unit`: The addressing scheme (e.g., `"json"`, `"bytes"`, `"text"`)
/// - `range`: The location within the resource
///
/// # Creating Content Ranges
///
/// ```
/// use crate::core::ContentRange;
///
/// // Using constructor
/// let range = ContentRange::new("json", ".users[0]");
///
/// // Using convenience methods
/// let json = ContentRange::json(".field");
/// let bytes = ContentRange::bytes("0:100");
/// ```
///
/// # Parsing and Formatting
///
/// ```
/// use crate::core::ContentRange;
///
/// // Parse from header value
/// let range = ContentRange::from_header_value("json .field").unwrap();
///
/// // Format as header value
/// assert_eq!(range.to_header_value(), "json .field");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ContentRange {
    /// The addressing unit type.
    ///
    /// Common values: `"json"`, `"bytes"`, `"text"`, `"lines"`
    pub unit: String,

    /// The range specification within the resource.
    ///
    /// Format depends on the unit type.
    pub range: String,
}

impl ContentRange {
    /// Create a new content range.
    ///
    /// # Arguments
    ///
    /// * `unit` - The addressing scheme (e.g., `"json"`, `"bytes"`)
    /// * `range` - The range specification
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::ContentRange;
    ///
    /// let range = ContentRange::new("json", ".users[0].name");
    /// assert_eq!(range.unit, "json");
    /// assert_eq!(range.range, ".users[0].name");
    /// ```
    #[inline]
    #[must_use]
    pub fn new(unit: impl Into<String>, range: impl Into<String>) -> Self {
        ContentRange {
            unit: unit.into(),
            range: range.into(),
        }
    }

    /// Create a JSON content range.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::ContentRange;
    ///
    /// let range = ContentRange::json(".users[0]");
    /// assert_eq!(range.unit, "json");
    /// ```
    #[inline]
    #[must_use]
    pub fn json(range: impl Into<String>) -> Self {
        Self::new("json", range)
    }

    /// Create a bytes content range.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::ContentRange;
    ///
    /// let range = ContentRange::bytes("0:100");
    /// assert_eq!(range.unit, "bytes");
    /// ```
    #[inline]
    #[must_use]
    pub fn bytes(range: impl Into<String>) -> Self {
        Self::new("bytes", range)
    }

    /// Create a text content range.
    #[inline]
    #[must_use]
    pub fn text(range: impl Into<String>) -> Self {
        Self::new("text", range)
    }

    /// Create a lines content range.
    #[inline]
    #[must_use]
    pub fn lines(range: impl Into<String>) -> Self {
        Self::new("lines", range)
    }

    /// Check if this is a JSON range.
    #[inline]
    #[must_use]
    pub fn is_json(&self) -> bool {
        self.unit == "json"
    }

    /// Check if this is a bytes range.
    #[inline]
    #[must_use]
    pub fn is_bytes(&self) -> bool {
        self.unit == "bytes"
    }

    /// Format as a Content-Range header value.
    ///
    /// Returns the header value in the format: `"{unit} {range}"`
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::ContentRange;
    ///
    /// let range = ContentRange::new("json", ".field");
    /// assert_eq!(range.to_header_value(), "json .field");
    /// ```
    #[must_use]
    pub fn to_header_value(&self) -> String {
        format!("{} {}", self.unit, self.range)
    }

    /// Parse from a Content-Range header value.
    ///
    /// Expects format: `"{unit} {range}"`
    ///
    /// # Arguments
    ///
    /// * `value` - The header value to parse
    ///
    /// # Returns
    ///
    /// `Ok(ContentRange)` if parsing succeeds, `Err(String)` with error message otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::ContentRange;
    ///
    /// let range = ContentRange::from_header_value("json .field").unwrap();
    /// assert_eq!(range.unit, "json");
    /// assert_eq!(range.range, ".field");
    ///
    /// // Complex ranges are preserved
    /// let range = ContentRange::from_header_value("json .users[0].name").unwrap();
    /// assert_eq!(range.range, ".users[0].name");
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the value doesn't contain a space separator:
    ///
    /// ```
    /// use crate::core::ContentRange;
    ///
    /// let result = ContentRange::from_header_value("invalid");
    /// assert!(result.is_err());
    /// ```
    pub fn from_header_value(value: &str) -> Result<Self, String> {
        let parts: Vec<&str> = value.splitn(2, ' ').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid Content-Range: expected 'unit range', got '{}'", value));
        }
        Ok(ContentRange {
            unit: parts[0].to_string(),
            range: parts[1].to_string(),
        })
    }
}

impl Default for ContentRange {
    fn default() -> Self {
        ContentRange {
            unit: "bytes".to_string(),
            range: "0:0".to_string(),
        }
    }
}

impl fmt::Display for ContentRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.unit, self.range)
    }
}

impl FromStr for ContentRange {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_header_value(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_range_new() {
        let range = ContentRange::new("json", ".field");
        assert_eq!(range.unit, "json");
        assert_eq!(range.range, ".field");
    }

    #[test]
    fn test_content_range_json() {
        let range = ContentRange::json(".users[0]");
        assert_eq!(range.unit, "json");
        assert!(range.is_json());
    }

    #[test]
    fn test_content_range_bytes() {
        let range = ContentRange::bytes("0:100");
        assert_eq!(range.unit, "bytes");
        assert!(range.is_bytes());
    }

    #[test]
    fn test_content_range_to_header_value() {
        let range = ContentRange::new("bytes", "0:100");
        assert_eq!(range.to_header_value(), "bytes 0:100");
    }

    #[test]
    fn test_content_range_from_header_value() {
        let range = ContentRange::from_header_value("json .field").unwrap();
        assert_eq!(range.unit, "json");
        assert_eq!(range.range, ".field");
    }

    #[test]
    fn test_content_range_from_header_value_complex() {
        let range = ContentRange::from_header_value("json .users[0].name").unwrap();
        assert_eq!(range.unit, "json");
        assert_eq!(range.range, ".users[0].name");
    }

    #[test]
    fn test_content_range_from_header_value_invalid() {
        let result = ContentRange::from_header_value("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_content_range_display() {
        let range = ContentRange::new("json", ".field");
        assert_eq!(format!("{}", range), "json .field");
    }

    #[test]
    fn test_content_range_from_str() {
        let range: ContentRange = "json .field".parse().unwrap();
        assert_eq!(range.unit, "json");
        assert_eq!(range.range, ".field");
    }

    #[test]
    fn test_content_range_default() {
        let range = ContentRange::default();
        assert_eq!(range.unit, "bytes");
        assert_eq!(range.range, "0:0");
    }

    #[test]
    fn test_content_range_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(ContentRange::json(".a"));
        set.insert(ContentRange::json(".b"));
        set.insert(ContentRange::bytes("0:10"));
        assert_eq!(set.len(), 3);
    }
}
