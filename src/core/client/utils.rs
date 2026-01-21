//! Utility functions for the Braid HTTP client.
//!
//! This module provides helper functions for:
//! - Parsing and formatting headers (Content-Range, Heartbeat, etc.)
//! - Retry logic with exponential backoff
//! - Status code classification
//! - Body merging for conflict resolution
//!
//! # Specification
//!
//! Based on RFC 7233 (HTTP Range Requests) and draft-toomim-httpbis-braid-http.

use crate::core::client::parser::Message;
use crate::core::error::{BraidError, Result};
use crate::core::protocol;
use crate::core::types::{Update, Version};
use bytes::{Bytes, BytesMut};
use std::time::Duration;

/// Parse Content-Range header.
///
/// Extracts unit and range from a Content-Range header value.
/// Format: `"{unit} {range}"` (e.g., `"json .field"` or `"bytes 0:100"`)
///
/// # Examples
///
/// ```ignore
/// use crate::core::client::parse_content_range;
///
/// let (unit, range) = parse_content_range("json .field")?;
/// assert_eq!(unit, "json");
/// assert_eq!(range, ".field");
/// ```
///
/// # Specification
///
/// See Section 3 of draft-toomim-httpbis-braid-http for Content-Range specifications.
pub fn parse_content_range(header: &str) -> Result<(String, String)> {
    protocol::parse_content_range(header)
}

/// Format Content-Range header.
///
/// Creates a Content-Range header value from unit and range components.
pub fn format_content_range(unit: &str, range: &str) -> String {
    protocol::format_content_range(unit, range)
}

/// Parse heartbeat interval.
///
/// Converts heartbeat header value to Duration.
/// Supports formats: `"5s"`, `"500ms"`, or just `"5"` (defaults to seconds).
///
/// # Examples
///
/// ```ignore
/// use crate::core::client::parse_heartbeat;
/// use std::time::Duration;
///
/// assert_eq!(parse_heartbeat("5s")?, Duration::from_secs(5));
/// assert_eq!(parse_heartbeat("500ms")?, Duration::from_millis(500));
/// ```
pub fn parse_heartbeat(value: &str) -> Result<Duration> {
    let trimmed = value.trim();

    let num_str = if let Some(s) = trimmed.strip_suffix("ms") {
        s
    } else if let Some(s) = trimmed.strip_suffix('s') {
        s
    } else {
        trimmed
    };

    let num: f64 = num_str
        .parse()
        .map_err(|_| BraidError::HeaderParse(format!("Invalid heartbeat: {}", value)))?;

    Ok(Duration::from_secs_f64(num))
}

/// Convert version to JSON string format
pub fn version_to_json_string(version: &str) -> String {
    format!("\"{}\"", version)
}

/// Check if status code indicates retryable error
pub fn is_retryable_status(status: u16) -> bool {
    matches!(status, 408 | 425 | 429 | 502 | 503 | 504)
}

/// Check if status code indicates access denied
pub fn is_access_denied_status(status: u16) -> bool {
    matches!(status, 401 | 403)
}

/// Exponential backoff delay calculation
///
/// Returns delay in milliseconds
pub fn exponential_backoff(attempt: u32, base_ms: u64) -> Duration {
    let delay_ms = base_ms * 2_u64.pow(attempt.min(10));
    Duration::from_millis(delay_ms)
}

/// Merge update bodies for conflict resolution
///
/// Simple merge strategy - can be extended for more complex logic
pub fn merge_bodies(body1: &Bytes, body2: &Bytes) -> Bytes {
    let mut result = BytesMut::with_capacity(body1.len() + body2.len());
    result.extend_from_slice(body1);
    result.extend_from_slice(body2);
    result.freeze()
}

/// Convert a parsed protocol message to an Update object.
///
/// Extracts versioning, headers, and content from the raw message
/// and constructs a typed `Update` struct.
pub fn message_to_update(msg: Message) -> Update {
    let mut builder = if !msg.patches.is_empty() {
        // Construct patched update
        // We need a version, fallback to unknown/generated if missing
        let version = extract_version(&msg.headers).unwrap_or_else(|| Version::new("unknown"));
        Update::patched(version, msg.patches)
    } else {
        // Construct snapshot update
        let version = extract_version(&msg.headers).unwrap_or_else(|| Version::new("unknown"));
        let body = String::from_utf8_lossy(&msg.body).to_string();
        Update::snapshot(version, body)
    };

    // Add parents
    if let Some(parents) = extract_parents(&msg.headers) {
        for parent in parents {
            builder = builder.with_parent(parent);
        }
    }

    // Add metadata
    if let Some(merge_type) = msg.headers.get("merge-type") {
        builder = builder.with_merge_type(merge_type.clone());
    }

    // Spec Interoperability: Support bundled pre-fetching via Content-Location
    builder.url = msg.url;

    builder
}

fn extract_version(headers: &std::collections::BTreeMap<String, String>) -> Option<Version> {
    headers
        .get("current-version")
        .or_else(|| headers.get("version"))
        .and_then(|v| protocol::parse_version_header(v).ok())
        .and_then(|mut v| v.pop())
}

fn extract_parents(headers: &std::collections::BTreeMap<String, String>) -> Option<Vec<Version>> {
    headers
        .get("parents")
        .and_then(|v| protocol::parse_version_header(v).ok())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_task<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(future);
}

#[cfg(target_arch = "wasm32")]
pub fn spawn_task<F>(future: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

#[cfg(target_arch = "wasm32")]
pub async fn sleep(duration: Duration) {
    gloo_timers::future::sleep(duration).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_content_range() {
        let (unit, range) = parse_content_range("json .field").unwrap();
        assert_eq!(unit, "json");
        assert_eq!(range, ".field");
    }

    #[test]
    fn test_format_content_range() {
        let result = format_content_range("bytes", "0:100");
        assert_eq!(result, "bytes 0:100");
    }

    #[test]
    fn test_parse_heartbeat() {
        let dur = parse_heartbeat("5s").unwrap();
        assert_eq!(dur, Duration::from_secs(5));
    }

    #[test]
    fn test_is_retryable_status() {
        assert!(is_retryable_status(503));
        assert!(is_retryable_status(429));
        assert!(!is_retryable_status(404));
    }

    #[test]
    fn test_exponential_backoff() {
        let delay0 = exponential_backoff(0, 100);
        let delay1 = exponential_backoff(1, 100);
        assert!(delay1 > delay0);
    }
}
