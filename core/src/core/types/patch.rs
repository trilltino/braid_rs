//! Patch representing a partial update to a resource.
//!
//! Patches enable efficient incremental updates by specifying only the parts of a resource
//! that have changed. Instead of sending the entire resource state, patches identify a
//! specific range and provide the new content for that range.
//!
//! # Overview
//!
//! A patch consists of:
//!
//! - **Unit**: The addressing scheme (e.g., `"json"`, `"bytes"`, `"text"`)
//! - **Range**: The location within the resource using the unit's syntax
//! - **Content**: The new content to apply at that range
//! - **Content-Length**: Byte length (required for multi-patch responses)
//!
//! # Patch Units
//!
//! Different units support different addressing schemes:
//!
//! | Unit | Range Syntax | Example |
//! |------|--------------|---------|
//! | `json` | JSONPath-like | `.users[0].name` |
//! | `bytes` | Byte range | `100:200` or `100-200` |
//! | `text` | Text path | `.title` |
//! | `lines` | Line range | `10:20` |
//!
//! # Multi-Patch Format (Section 3.3)
//!
//! When sending multiple patches in a single response, each patch must declare its
//! `Content-Length` header to mark boundaries in the stream:
//!
//! ```text
//! Patches: 2
//!
//! Content-Length: 15
//! Content-Range: json .name
//!
//! "Alice"
//!
//! Content-Length: 12
//! Content-Range: json .age
//!
//! 30
//! ```
//!
//! # Examples
//!
//! ```
//! use crate::core::Patch;
//!
//! // JSON patch - update a specific field
//! let json_patch = Patch::json(".users[0].name", r#""Alice""#);
//!
//! // Byte range patch - replace bytes 100-200
//! let byte_patch = Patch::bytes("100:200", &b"new content"[..]);
//!
//! // Text patch - update a text field
//! let text_patch = Patch::text(".title", "New Title");
//!
//! // Custom unit patch
//! let custom = Patch::new("lines", "10:20", "replacement text");
//! ```
//!
//! # Specification
//!
//! See Section 3 of [draft-toomim-httpbis-braid-http-04] for patch specifications:
//!
//! - **Section 3.1**: Content-Range header format
//! - **Section 3.2**: Single patch updates
//! - **Section 3.3**: Multi-patch updates with `Patches: N` header
//!
//! [draft-toomim-httpbis-braid-http-04]: https://datatracker.ietf.org/doc/html/draft-toomim-httpbis-braid-http

use bytes::Bytes;

use serde::{Deserialize, Serialize};

/// A patch representing a partial update to a resource.
///
/// Patches specify incremental changes to specific ranges within a resource,
/// enabling efficient synchronization without transferring the entire state.
///
/// # Fields
///
/// - `unit`: The addressing scheme (e.g., `"json"`, `"bytes"`, `"text"`)
/// - `range`: The location within the resource
/// - `content`: The new content for that range
/// - `content_length`: Byte length (auto-calculated, required for multi-patch)
///
/// # Creating Patches
///
/// Use the convenience constructors for common patch types:
///
/// ```
/// use crate::core::Patch;
///
/// let json = Patch::json(".field", "value");
/// let bytes = Patch::bytes("0:100", &b"data"[..]);
/// let text = Patch::text(".title", "Title");
/// ```
///
/// Or use [`Patch::new`] for custom units:
///
/// ```
/// use crate::core::Patch;
///
/// let custom = Patch::new("lines", "1:10", "new lines");
/// ```
///
/// # Specification
///
/// See Section 3 of draft-toomim-httpbis-braid-http for patch semantics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Patch {
    /// The addressing unit type.
    ///
    /// Common values:
    /// - `"json"`: JSONPath-like addressing (e.g., `.users[0].name`)
    /// - `"bytes"`: Byte range addressing (e.g., `100:200`)
    /// - `"text"`: Text path addressing
    /// - `"lines"`: Line range addressing
    pub unit: String,

    /// The range specification within the resource.
    ///
    /// Format depends on the unit:
    /// - JSON: `.field`, `.array[0]`, `.nested.path`
    /// - Bytes: `start:end` or `start-end`
    /// - Lines: `start:end`
    pub range: String,

    /// The patch content to apply at the specified range.
    pub content: Bytes,

    /// Content length in bytes.
    ///
    /// Required for multi-patch responses (Section 3.3) to mark patch boundaries.
    /// Automatically calculated by constructors.
    pub content_length: Option<usize>,
}

impl Patch {
    /// Create a new patch with a custom unit.
    ///
    /// This is the general constructor for patches with any addressing unit.
    /// For common units, prefer the convenience constructors:
    /// [`Patch::json`], [`Patch::bytes`], [`Patch::text`].
    ///
    /// # Arguments
    ///
    /// * `unit` - The addressing scheme (e.g., `"json"`, `"bytes"`, `"lines"`)
    /// * `range` - The range specification within the resource
    /// * `content` - The content to apply at that range
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Patch;
    ///
    /// let patch = Patch::new("lines", "10:20", "replacement lines");
    /// assert_eq!(patch.unit, "lines");
    /// assert_eq!(patch.range, "10:20");
    /// ```
    #[must_use]
    pub fn new(
        unit: impl Into<String>,
        range: impl Into<String>,
        content: impl Into<Bytes>,
    ) -> Self {
        let content_bytes = content.into();
        let content_length = content_bytes.len();
        Patch {
            unit: unit.into(),
            range: range.into(),
            content: content_bytes,
            content_length: Some(content_length),
        }
    }

    /// Create a JSON patch.
    ///
    /// JSON patches use JSONPath-like syntax to address specific fields
    /// within a JSON document.
    ///
    /// # Arguments
    ///
    /// * `range` - JSONPath-like range (e.g., `.field`, `.array[0]`, `.nested.path`)
    /// * `content` - The JSON content to apply
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Patch;
    ///
    /// // Update a simple field
    /// let patch = Patch::json(".name", r#""Alice""#);
    ///
    /// // Update an array element
    /// let patch = Patch::json(".users[0]", r#"{"id": 1, "name": "Bob"}"#);
    ///
    /// // Update a nested field
    /// let patch = Patch::json(".config.settings.theme", r#""dark""#);
    /// ```
    #[inline]
    #[must_use]
    pub fn json(range: impl Into<String>, content: impl Into<Bytes>) -> Self {
        Self::new("json", range, content)
    }

    /// Create a byte range patch.
    ///
    /// Byte patches address specific byte ranges within a binary resource.
    ///
    /// # Arguments
    ///
    /// * `range` - Byte range in `start:end` or `start-end` format
    /// * `content` - The bytes to apply at that range
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Patch;
    ///
    /// // Replace bytes 100-200
    /// let patch = Patch::bytes("100:200", &b"new content"[..]);
    ///
    /// // Insert at position 0
    /// let patch = Patch::bytes("0:0", &b"prefix"[..]);
    /// ```
    #[inline]
    #[must_use]
    pub fn bytes(range: impl Into<String>, content: impl Into<Bytes>) -> Self {
        Self::new("bytes", range, content)
    }

    /// Create a text patch.
    ///
    /// Text patches are similar to JSON patches but for plain text content.
    ///
    /// # Arguments
    ///
    /// * `range` - Text path specification
    /// * `content` - The text content to apply
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Patch;
    ///
    /// let patch = Patch::text(".title", "New Document Title");
    /// assert_eq!(patch.unit, "text");
    /// ```
    #[inline]
    #[must_use]
    pub fn text(range: impl Into<String>, content: impl Into<String>) -> Self {
        let content_str = content.into();
        Self::new("text", range, Bytes::from(content_str))
    }

    /// Create a line range patch.
    ///
    /// Line patches address specific line ranges within a text resource.
    ///
    /// # Arguments
    ///
    /// * `range` - Line range in `start:end` format (1-indexed)
    /// * `content` - The lines to apply at that range
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Patch;
    ///
    /// // Replace lines 10-20
    /// let patch = Patch::lines("10:20", "new line content\n");
    /// assert_eq!(patch.unit, "lines");
    /// ```
    #[inline]
    #[must_use]
    pub fn lines(range: impl Into<String>, content: impl Into<String>) -> Self {
        let content_str = content.into();
        Self::new("lines", range, Bytes::from(content_str))
    }

    /// Create a patch with explicit content length.
    ///
    /// Use this when the content length differs from the actual byte length,
    /// or when constructing patches from parsed data.
    ///
    /// # Arguments
    ///
    /// * `unit` - The addressing scheme
    /// * `range` - The range specification
    /// * `content` - The content to apply
    /// * `length` - The explicit content length
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Patch;
    ///
    /// let patch = Patch::with_length("json", ".field", "value", 5);
    /// assert_eq!(patch.content_length, Some(5));
    /// ```
    #[must_use]
    pub fn with_length(
        unit: impl Into<String>,
        range: impl Into<String>,
        content: impl Into<Bytes>,
        length: usize,
    ) -> Self {
        Patch {
            unit: unit.into(),
            range: range.into(),
            content: content.into(),
            content_length: Some(length),
        }
    }

    /// Check if this is a JSON patch.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Patch;
    ///
    /// assert!(Patch::json(".field", "value").is_json());
    /// assert!(!Patch::bytes("0:10", &b"data"[..]).is_json());
    /// ```
    #[inline]
    #[must_use]
    pub fn is_json(&self) -> bool {
        self.unit == "json"
    }

    /// Check if this is a bytes patch.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Patch;
    ///
    /// assert!(Patch::bytes("0:10", &b"data"[..]).is_bytes());
    /// assert!(!Patch::json(".field", "value").is_bytes());
    /// ```
    #[inline]
    #[must_use]
    pub fn is_bytes(&self) -> bool {
        self.unit == "bytes"
    }

    /// Check if this is a text patch.
    #[inline]
    #[must_use]
    pub fn is_text(&self) -> bool {
        self.unit == "text"
    }

    /// Check if this is a lines patch.
    #[inline]
    #[must_use]
    pub fn is_lines(&self) -> bool {
        self.unit == "lines"
    }

    /// Get the content as a UTF-8 string.
    ///
    /// # Returns
    ///
    /// `Some(&str)` if the content is valid UTF-8, `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Patch;
    ///
    /// let patch = Patch::json(".field", "value");
    /// assert_eq!(patch.content_str(), Some("value"));
    /// ```
    #[inline]
    #[must_use]
    pub fn content_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.content).ok()
    }

    /// Get the content as UTF-8 text (alias for `content_str`).
    ///
    /// Matches the JavaScript reference implementation's `content_text` property.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Patch;
    ///
    /// let patch = Patch::json(".field", "value");
    /// assert_eq!(patch.content_text(), Some("value"));
    /// ```
    #[inline]
    #[must_use]
    pub fn content_text(&self) -> Option<&str> {
        self.content_str()
    }

    /// Get the content length, calculating from content if not set.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Patch;
    ///
    /// let patch = Patch::json(".field", "value");
    /// assert_eq!(patch.len(), 5);
    /// ```
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.content_length.unwrap_or_else(|| self.content.len())
    }

    /// Check if the patch content is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Patch;
    ///
    /// let patch = Patch::json(".field", "");
    /// assert!(patch.is_empty());
    /// ```
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Format as a Content-Range header value.
    ///
    /// Returns the header value in the format: `unit range`
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Patch;
    ///
    /// let patch = Patch::json(".users[0]", "{}");
    /// assert_eq!(patch.content_range_header(), "json .users[0]");
    /// ```
    #[must_use]
    pub fn content_range_header(&self) -> String {
        format!("{} {}", self.unit, self.range)
    }

    /// Validate patch according to Braid-HTTP specification.
    ///
    /// Ensures the patch conforms to Section 3 requirements:
    /// - Patch MUST have non-empty unit (addressing scheme)
    /// - Patch MUST have non-empty range (location specification)
    ///
    /// # Returns
    ///
    /// `Ok(())` if valid, `Err(BraidError)` if invalid.
    ///
    /// # Specification
    ///
    /// Per Section 3 of draft-toomim-httpbis-braid-http-04:
    /// > Every patch MUST include either Content-Type OR Content-Range.
    ///
    /// The `unit` and `range` fields together constitute the Content-Range
    /// requirement.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Patch;
    ///
    /// let valid = Patch::json(".field", "value");
    /// assert!(valid.validate().is_ok());
    ///
    /// let invalid = Patch {
    ///     unit: String::new(),  // Empty unit
    ///     range: ".field".to_string(),
    ///     content: bytes::Bytes::from("value"),
    ///     content_length: Some(5),
    /// };
    /// assert!(invalid.validate().is_err());
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `BraidError::InvalidPatch` if:
    /// - Unit is empty
    /// - Range is empty
    pub fn validate(&self) -> crate::core::error::Result<()> {
        if self.unit.is_empty() {
            return Err(crate::core::error::BraidError::Protocol(
                "Patch unit cannot be empty (spec Section 3 requires Content-Range)".into(),
            ));
        }
        if self.range.is_empty() {
            return Err(crate::core::error::BraidError::Protocol(
                "Patch range cannot be empty (spec Section 3 requires Content-Range)".into(),
            ));
        }
        Ok(())
    }
}

impl Default for Patch {
    fn default() -> Self {
        Patch {
            unit: "bytes".to_string(),
            range: String::new(),
            content: Bytes::new(),
            content_length: Some(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patch_new() {
        let patch = Patch::new("custom", "range", "content");
        assert_eq!(patch.unit, "custom");
        assert_eq!(patch.range, "range");
        assert_eq!(patch.content_length, Some(7));
    }

    #[test]
    fn test_patch_json() {
        let patch = Patch::json(".field", "value");
        assert_eq!(patch.unit, "json");
        assert_eq!(patch.range, ".field");
        assert!(patch.is_json());
    }

    #[test]
    fn test_patch_bytes() {
        let patch = Patch::bytes("0:100", &b"content"[..]);
        assert_eq!(patch.unit, "bytes");
        assert_eq!(patch.range, "0:100");
        assert!(patch.is_bytes());
    }

    #[test]
    fn test_patch_text() {
        let patch = Patch::text(".title", "New Title");
        assert_eq!(patch.unit, "text");
        assert_eq!(patch.range, ".title");
        assert!(patch.is_text());
    }

    #[test]
    fn test_patch_lines() {
        let patch = Patch::lines("10:20", "new lines\n");
        assert_eq!(patch.unit, "lines");
        assert_eq!(patch.range, "10:20");
        assert!(patch.is_lines());
    }

    #[test]
    fn test_content_str() {
        let patch = Patch::json(".field", "value");
        assert_eq!(patch.content_str(), Some("value"));
    }

    #[test]
    fn test_len_and_is_empty() {
        let patch = Patch::json(".field", "value");
        assert_eq!(patch.len(), 5);
        assert!(!patch.is_empty());

        let empty = Patch::json(".field", "");
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_content_range_header() {
        let patch = Patch::json(".users[0]", "{}");
        assert_eq!(patch.content_range_header(), "json .users[0]");
    }

    #[test]
    fn test_default() {
        let patch = Patch::default();
        assert_eq!(patch.unit, "bytes");
        assert!(patch.range.is_empty());
        assert!(patch.is_empty());
    }

    #[test]
    fn test_validate_valid_patch() {
        let patch = Patch::json(".field", "value");
        assert!(patch.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_unit() {
        let patch = Patch {
            unit: String::new(),
            range: ".field".to_string(),
            content: Bytes::from("value"),
            content_length: Some(5),
        };
        assert!(patch.validate().is_err());
    }

    #[test]
    fn test_validate_empty_range() {
        let patch = Patch {
            unit: "json".to_string(),
            range: String::new(),
            content: Bytes::from("value"),
            content_length: Some(5),
        };
        assert!(patch.validate().is_err());
    }

    #[test]
    fn test_validate_all_types() {
        assert!(Patch::json(".f", "v").validate().is_ok());
        assert!(Patch::bytes("0:10", &b"data"[..]).validate().is_ok());
        assert!(Patch::text(".t", "text").validate().is_ok());
        assert!(Patch::lines("1:5", "lines").validate().is_ok());
    }
}
