//! Protocol message formatter.
//!
//! Converts `Update` objects into Braid protocol message bytes.
//! supporting standard headers, body content, and multi-patch formats.

use crate::core::error::Result;
use crate::core::protocol::constants::headers;
use crate::core::protocol;
use crate::core::types::{Update, Patch};
use bytes::{Bytes, BytesMut};

/// Format an Update into Braid protocol message bytes.
///
/// Serializes the update including all headers and body/patches
/// into a byte buffer ready for transmission.
///
/// # Arguments
///
/// * `update` - The update to format
///
/// # Returns
///
/// Bytes containing the formatted message.
pub fn format_update(update: &Update) -> Result<Bytes> {
    let mut buffer = BytesMut::new();

    // Format headers
    if !update.version.is_empty() {
        write_header(&mut buffer, headers::VERSION.as_str(), &protocol::format_version_header(&update.version));
    }

    if !update.parents.is_empty() {
        write_header(&mut buffer, headers::PARENTS.as_str(), &protocol::format_version_header(&update.parents));
    }

    if let Some(merge_type) = &update.merge_type {
        write_header(&mut buffer, headers::MERGE_TYPE.as_str(), merge_type);
    }

    for (k, v) in &update.extra_headers {
        write_header(&mut buffer, k, v);
    }

    // Body or Patches
    if let Some(body) = &update.body {
        write_header(&mut buffer, headers::CONTENT_LENGTH.as_str(), &body.len().to_string());
        if let Some(ct) = &update.content_type {
            write_header(&mut buffer, headers::CONTENT_TYPE.as_str(), ct);
        }
        buffer.extend_from_slice(b"\r\n"); // End of headers
        buffer.extend_from_slice(body);
    } else if let Some(patches) = &update.patches {
        if !patches.is_empty() {
             write_header(&mut buffer, headers::PATCHES.as_str(), &patches.len().to_string());
             buffer.extend_from_slice(b"\r\n"); // End of message headers
             
             // Format each patch
             for patch in patches {
                 format_patch(&mut buffer, patch)?;
             }
        } else {
             write_header(&mut buffer, headers::CONTENT_LENGTH.as_str(), "0");
             buffer.extend_from_slice(b"\r\n");
        }
    } else {
        // Empty body
        write_header(&mut buffer, headers::CONTENT_LENGTH.as_str(), "0");
        buffer.extend_from_slice(b"\r\n");
    }

    Ok(buffer.freeze())
}

fn write_header(buffer: &mut BytesMut, key: &str, value: &str) {
    buffer.extend_from_slice(key.as_bytes());
    buffer.extend_from_slice(b": ");
    buffer.extend_from_slice(value.as_bytes());
    buffer.extend_from_slice(b"\r\n");
}

fn format_patch(buffer: &mut BytesMut, patch: &Patch) -> Result<()> {
    write_header(buffer, headers::CONTENT_LENGTH.as_str(), &patch.content.len().to_string());
    
    let content_range = format!("{} {}", patch.unit, patch.range);
    write_header(buffer, headers::CONTENT_RANGE.as_str(), &content_range);
    
    buffer.extend_from_slice(b"\r\n"); // End of patch headers
    buffer.extend_from_slice(&patch.content);
    
    Ok(())
}

/// Escape non-ASCII characters in a string for use in HTTP headers.
///
/// This function replaces any character outside the printable ASCII range
/// (0x20-0x7E) with a Unicode escape sequence (\uXXXX).
///
/// # Arguments
///
/// * `s` - The input string to process
///
/// # Returns
///
/// A string with all non-ASCII characters escaped.
///
/// # Example
///
/// ```
/// use braid_http_rs::core::protocol::formatter::ascii_ify;
///
/// let result = ascii_ify("Hello 🎉");
/// // Note: Rust uses Unicode scalar values (U+1F389), not UTF-16 surrogate pairs
/// assert_eq!(result, "Hello \\u1f389");
/// ```
pub fn ascii_ify(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii() && c >= '\x20' && c <= '\x7E' {
                // Printable ASCII - keep as-is
                c.to_string()
            } else {
                // Non-ASCII or control character - escape as \uXXXX
                format!("\\u{:04x}", c as u32)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::Version;

    #[test]
    fn test_format_snapshot() {
        let update = Update::snapshot(Version::new("v1"), "data");
        let bytes = format_update(&update).unwrap();
        let s = std::str::from_utf8(&bytes).unwrap();
        
        assert!(s.contains("version: \"v1\""));
        assert!(s.contains("content-length: 4"));
        assert!(s.ends_with("\r\ndata"));
    }

    #[test]
    fn test_ascii_ify_basic() {
        assert_eq!(ascii_ify("hello"), "hello");
    }

    #[test]
    fn test_ascii_ify_unicode() {
        // Note: Rust uses Unicode scalar values, not UTF-16 surrogate pairs
        // So the party popper emoji (U+1F389) becomes \u1f389, not \ud83c\udf89
        assert_eq!(ascii_ify("🎉"), "\\u1f389");
        assert_eq!(ascii_ify("Hello 🎉"), "Hello \\u1f389");
    }

    #[test]
    fn test_ascii_ify_mixed() {
        assert_eq!(ascii_ify("Test: 你好"), "Test: \\u4f60\\u597d");
    }

    #[test]
    fn test_ascii_ify_control_chars() {
        assert_eq!(ascii_ify("Hello\nWorld"), "Hello\\u000aWorld");
    }
}
