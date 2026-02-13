//! Parse incoming update requests for Braid protocol.
//!
//! This module provides utilities for parsing Braid protocol headers and bodies
//! from HTTP requests, extracting version information, patch data, and other
//! protocol parameters.
//!
//! # Request Parsing
//!
//! Handlers can extract protocol information from incoming requests using:
//! - HTTP headers (Version, Parents, Merge-Type, Content-Range, etc.)
//! - Request body (snapshot or patches)
//!
//! # Request Formats Handled
//!
//! ## 1. SNAPSHOT MODE (full state replacement):
//! ```text
//! PUT /resource HTTP/1.1
//! Content-Type: application/json
//! Content-Length: 123
//!
//! {full body data}
//! ```
//!
//! ## 2. SINGLE PATCH MODE:
//! ```text
//! PATCH /resource HTTP/1.1
//! Content-Range: json .path.to.field
//! Content-Length: 50
//!
//! {patch content}
//! ```
//!
//! ## 3. MULTIPLE PATCHES MODE:
//! ```text
//! PATCH /resource HTTP/1.1
//! Patches: 2
//!
//! Content-Length: 30
//! Content-Range: json .field1
//!
//! {patch 1 content}
//!
//! Content-Length: 40
//! Content-Range: json .field2
//!
//! {patch 2 content}
//! ```
//!
//! # Status Codes
//!
//! Servers should respond with:
//! - `200 OK` - Successful update acceptance
//! - `206 Partial Content` - Range-based patches accepted
//! - `208 Already Reported` - Duplicate (idempotent) request
//! - `409 Conflict` - Version conflict in update
//! - `416 Range Not Satisfiable` - Invalid range specified
//!
//! # Specification
//!
//! See Sections 2 and 3 of draft-toomim-httpbis-braid-http for request specifications.

use crate::core::error::{BraidError, Result};
use crate::core::protocol;
use crate::core::types::{Patch, Version};
use bytes::Bytes;
use std::collections::HashMap;

/// Parsed update from request body.
///
/// Contains the structured representation of an incoming update request,
/// including version information and either the full body or patches.
#[derive(Clone, Debug)]
pub struct ParsedUpdate {
    /// Version ID(s) from Version header
    pub version: Vec<Version>,
    /// Parent version ID(s) from Parents header
    pub parents: Vec<Version>,
    /// Patches extracted from request body
    pub patches: Vec<Patch>,
    /// Full body if not patches (snapshot)
    pub body: Option<Bytes>,
}

impl ParsedUpdate {
    /// Create from HTTP request parts (headers + body).
    ///
    /// This is the main entry point for parsing Braid protocol requests.
    /// It detects the request mode (snapshot, single patch, or multi-patch)
    /// and parses accordingly.
    ///
    /// # Arguments
    ///
    /// * `headers` - HTTP headers from the request
    /// * `body` - Request body bytes
    ///
    /// # Returns
    ///
    /// Returns a `ParsedUpdate` with extracted version, parents, and either
    /// body or patches depending on the request type.
    pub fn from_parts(
        headers: &HashMap<String, String>,
        body: Bytes,
    ) -> Result<Self> {
        // Parse version and parents from headers
        let version = headers
            .get("version")
            .map(|v| protocol::parse_version_header(v))
            .transpose()?
            .unwrap_or_default();

        let parents = headers
            .get("parents")
            .map(|v| protocol::parse_version_header(v))
            .transpose()?
            .unwrap_or_default();

        // Check for patches header (multi-patch mode)
        let patches_header = headers.get("patches").and_then(|v| v.parse::<usize>().ok());

        // Check for content-range header (single patch mode)
        let content_range = headers.get("content-range");

        if let Some(num_patches) = patches_header {
            // === MULTIPLE PATCHES MODE ===
            let patches = parse_multi_patches(&body, num_patches)?;
            Ok(ParsedUpdate {
                version,
                parents,
                patches,
                body: None,
            })
        } else if content_range.is_some() {
            // === SINGLE PATCH MODE ===
            let patch = parse_single_patch(&body, content_range.unwrap())?;
            Ok(ParsedUpdate {
                version,
                parents,
                patches: vec![patch],
                body: None,
            })
        } else {
            // === SNAPSHOT MODE ===
            Ok(ParsedUpdate {
                version,
                parents,
                patches: Vec::new(),
                body: if body.is_empty() { None } else { Some(body) },
            })
        }
    }

    /// Returns true if this update contains patches (not a snapshot).
    pub fn is_patched(&self) -> bool {
        !self.patches.is_empty()
    }

    /// Returns true if this update is a snapshot (full body replacement).
    pub fn is_snapshot(&self) -> bool {
        self.body.is_some()
    }
}

/// Parse a single patch from request body.
///
/// Used when Content-Range header is present but Patches header is not.
/// The entire body is the patch content.
fn parse_single_patch(body: &Bytes, content_range: &str) -> Result<Patch> {
    let (unit, range) = parse_content_range(content_range)?;
    Ok(Patch::new(unit, range, body.clone()))
}

/// Parse multiple patches from request body.
///
/// Used when Patches: N header is present. Each patch has the format:
/// ```text
/// Content-Length: <len>
/// Content-Range: <unit> <range>
///
/// <content>
/// ```
///
/// Patches are separated by blank lines (double CRLF).
fn parse_multi_patches(body: &Bytes, num_patches: usize) -> Result<Vec<Patch>> {
    if num_patches == 0 {
        return Ok(Vec::new());
    }

    let mut patches = Vec::with_capacity(num_patches);
    let mut offset = 0;

    for _ in 0..num_patches {
        // Skip leading whitespace (CR/LF) between patches
        while offset < body.len() && (body[offset] == b'\r' || body[offset] == b'\n') {
            offset += 1;
        }

        if offset >= body.len() {
            return Err(BraidError::Protocol(
                format!("Unexpected end of body while parsing patch {} of {}", patches.len() + 1, num_patches)
            ));
        }

        // Find end of headers (double CRLF or LF LF)
        let headers_start = offset;
        let mut headers_end = offset;
        loop {
            if headers_end >= body.len() {
                return Err(BraidError::Protocol(
                    "Incomplete patch headers: expected double CRLF".to_string()
                ));
            }
            // Check for \n\n or \r\n\r\n
            if body[headers_end] == b'\n' {
                if headers_end > headers_start && body[headers_end - 1] == b'\n' {
                    // Found \n\n
                    headers_end += 1;
                    break;
                }
                if headers_end > headers_start + 1 
                    && body[headers_end - 1] == b'\r' 
                    && body[headers_end - 2] == b'\n' {
                    // Found \n\r\n (weird but handle it)
                    headers_end += 1;
                    break;
                }
            }
            headers_end += 1;
        }

        // Parse patch headers
        let headers_str = String::from_utf8_lossy(&body[headers_start..headers_end]);
        let patch_headers = parse_patch_headers(&headers_str)?;

        // Get content length
        let content_length = patch_headers
            .get("content-length")
            .ok_or_else(|| BraidError::Protocol("Patch missing Content-Length header".to_string()))?
            .parse::<usize>()
            .map_err(|_| BraidError::Protocol("Invalid Content-Length in patch".to_string()))?;

        // Get content-range
        let content_range = patch_headers
            .get("content-range")
            .ok_or_else(|| BraidError::Protocol("Patch missing Content-Range header".to_string()))?;

        let (unit, range) = parse_content_range(content_range)?;

        // Extract patch content
        let content_start = headers_end;
        let content_end = content_start + content_length;

        if content_end > body.len() {
            return Err(BraidError::Protocol(
                format!("Patch content exceeds body length: {} > {}", content_end, body.len())
            ));
        }

        let content = body.slice(content_start..content_end);
        patches.push(Patch::new(unit, range, content));

        // Move offset past this patch's content
        offset = content_end;
    }

    Ok(patches)
}

/// Parse headers within a single patch section.
///
/// Returns a map of header names to values (case-insensitive keys).
fn parse_patch_headers(headers_str: &str) -> Result<HashMap<String, String>> {
    let mut headers = HashMap::new();

    for line in headers_str.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some((name, value)) = line.split_once(':') {
            headers.insert(
                name.trim().to_lowercase(),
                value.trim().to_string(),
            );
        }
    }

    Ok(headers)
}

/// Parse Content-Range header value.
///
/// Format: `<unit> <range>` e.g. `json .field` or `bytes 0-100`
pub fn parse_content_range(value: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = value.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return Err(BraidError::HeaderParse(format!(
            "Invalid Content-Range: {}",
            value
        )));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Helper function to extract headers into a HashMap for processing.
#[allow(dead_code)]
pub fn extract_headers(header_map: &axum::http::HeaderMap) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    for (name, value) in header_map.iter() {
        if let Ok(value_str) = value.to_str() {
            headers.insert(name.to_string().to_lowercase(), value_str.to_string());
        }
    }
    headers
}

/// Extension trait for Axum request to parse Braid updates.
///
/// This trait is kept for backward compatibility. The preferred approach
/// is to use `BraidState::parse_body()` after extracting headers.
pub trait ParseUpdateExt {
    /// Parse version from Version header.
    fn get_version(&self) -> Result<Vec<Version>>;

    /// Parse parents from Parents header.
    fn get_parents(&self) -> Result<Vec<Version>>;

    /// Parse patches from request body.
    fn get_patches(&self) -> Result<Vec<Patch>>;

    /// Parse complete update from request.
    fn parse_update(&self) -> Result<ParsedUpdate>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_header() {
        let result = protocol::parse_version_header("\"v1\", \"v2\", \"v3\"").unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_parse_version_header_empty() {
        let result = protocol::parse_version_header("").unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_parse_content_range() {
        let (unit, range) = parse_content_range("json .field").unwrap();
        assert_eq!(unit, "json");
        assert_eq!(range, ".field");
    }

    #[test]
    fn test_parse_single_patch() {
        let body = Bytes::from("{\"name\":\"Alice\"}");
        let patch = parse_single_patch(&body, "json .name").unwrap();
        
        assert_eq!(patch.unit, "json");
        assert_eq!(patch.range, ".name");
        assert_eq!(patch.content, body);
    }

    #[test]
    fn test_parse_multi_patches() {
        // Build multi-patch body
        let patch_body = concat!(
            "Content-Length: 7\r\n",
            "Content-Range: json .name\r\n",
            "\r\n",
            "\"Alice\"\r\n",
            "\r\n",
            "Content-Length: 2\r\n",
            "Content-Range: json .age\r\n",
            "\r\n",
            "30"
        );

        let body = Bytes::from(patch_body);
        let patches = parse_multi_patches(&body, 2).unwrap();

        assert_eq!(patches.len(), 2);
        
        assert_eq!(patches[0].unit, "json");
        assert_eq!(patches[0].range, ".name");
        assert_eq!(patches[0].content, Bytes::from("\"Alice\""));
        
        assert_eq!(patches[1].unit, "json");
        assert_eq!(patches[1].range, ".age");
        assert_eq!(patches[1].content, Bytes::from("30"));
    }

    #[test]
    fn test_parse_patch_headers() {
        let headers = "Content-Length: 100\r\nContent-Range: json .field\r\n";
        let parsed = parse_patch_headers(headers).unwrap();
        
        assert_eq!(parsed.get("content-length"), Some(&"100".to_string()));
        assert_eq!(parsed.get("content-range"), Some(&"json .field".to_string()));
    }

    #[test]
    fn test_parsed_update_from_parts_snapshot() {
        let mut headers = HashMap::new();
        headers.insert("version".to_string(), "\"v1\"".to_string());
        
        let body = Bytes::from("{\"data\":\"test\"}");
        let update = ParsedUpdate::from_parts(&headers, body.clone()).unwrap();

        assert_eq!(update.version.len(), 1);
        assert!(update.is_snapshot());
        assert!(!update.is_patched());
        assert_eq!(update.body, Some(body));
        assert!(update.patches.is_empty());
    }

    #[test]
    fn test_parsed_update_from_parts_single_patch() {
        let mut headers = HashMap::new();
        headers.insert("version".to_string(), "\"v1\"".to_string());
        headers.insert("content-range".to_string(), "json .name".to_string());
        
        let body = Bytes::from("\"Alice\"");
        let update = ParsedUpdate::from_parts(&headers, body.clone()).unwrap();

        assert!(update.is_patched());
        assert!(!update.is_snapshot());
        assert_eq!(update.patches.len(), 1);
        assert_eq!(update.patches[0].unit, "json");
        assert_eq!(update.patches[0].range, ".name");
    }

    #[test]
    fn test_parsed_update_from_parts_multi_patch() {
        let mut headers = HashMap::new();
        headers.insert("version".to_string(), "\"v1\"".to_string());
        headers.insert("patches".to_string(), "2".to_string());
        
        let body = Bytes::from(concat!(
            "Content-Length: 7\r\n",
            "Content-Range: json .name\r\n",
            "\r\n",
            "\"Alice\"\r\n",
            "\r\n",
            "Content-Length: 2\r\n",
            "Content-Range: json .age\r\n",
            "\r\n",
            "30"
        ));
        
        let update = ParsedUpdate::from_parts(&headers, body).unwrap();

        assert!(update.is_patched());
        assert!(!update.is_snapshot());
        assert_eq!(update.patches.len(), 2);
    }
}
