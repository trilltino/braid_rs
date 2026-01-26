//! Error types for Braid HTTP operations.
//!
//! This module defines all error types that can occur when using the Braid-HTTP protocol.
//! The [`Result`] type alias provides a convenient shorthand for operations that may fail.
//!
//! # Error Categories
//!
//! | Category | Variants | Retryable |
//! |----------|----------|-----------|
//! | Protocol | `HeaderParse`, `BodyParse`, `InvalidVersion` | No |
//! | Network | `Io`, `Timeout` | Yes |
//! | Subscription | `SubscriptionClosed`, `InvalidSubscriptionStatus` | Depends |
//! | Conflict | `MergeConflict`, `HistoryDropped` | No |
//! | Configuration | `Config` | No |
//!
//! # Error Recovery
//!
//! Use the [`BraidError::is_retryable()`] method to determine if an operation should be retried.
//! Most network errors are retryable; protocol errors typically are not.
//!
//! # Examples
//!
//! ## Checking Retryability
//!
//! ```
//! use crate::core::BraidError;
//!
//! let timeout_err = BraidError::Timeout;
//! assert!(timeout_err.is_retryable());
//!
//! let parse_err = BraidError::HeaderParse("invalid".into());
//! assert!(!parse_err.is_retryable());
//! ```
//!
//! ## Error Display
//!
//! ```
//! use crate::core::BraidError;
//!
//! let err = BraidError::InvalidVersion("not-a-version".into());
//! assert!(err.to_string().contains("not-a-version"));
//! ```
//!
//! # Specification
//!
//! See Section 4.5 of [draft-toomim-httpbis-braid-http-04] for error handling requirements.
//!
//! [draft-toomim-httpbis-braid-http-04]: https://datatracker.ietf.org/doc/html/draft-toomim-httpbis-braid-http

use std::io;
use thiserror::Error;

/// Result type for Braid HTTP operations.
///
/// Provides a convenient shorthand for `Result<T, BraidError>`.
pub type Result<T> = std::result::Result<T, BraidError>;

/// Errors that can occur during Braid HTTP operations.
///
/// Each variant represents a different failure mode when working with Braid protocol.
/// Use pattern matching to handle specific errors appropriately.
///
/// # Examples
///
/// ```
/// use crate::core::BraidError;
///
/// fn handle_error(err: BraidError) {
///     match err {
///         BraidError::HistoryDropped => {
///             println!("Server dropped history, restarting sync");
///         },
///         BraidError::Timeout => {
///             println!("Retrying operation...");
///         },
///         _ => eprintln!("Error: {}", err),
///     }
/// }
/// ```
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum BraidError {
    /// HTTP request failed with a given error message.
    ///
    /// This includes cases where the server returns an unexpected status code
    /// or the request cannot be completed.
    #[error("HTTP error: {0}")]
    Http(String),

    /// Network I/O error (connection failed, read/write error, etc.).
    ///
    /// These errors are typically retryable.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Failed to parse Braid protocol headers.
    ///
    /// Indicates the server sent malformed Version, Parents, Merge-Type,
    /// or other Braid-specific headers (Section 2 and 3).
    #[error("Header parse error: {0}")]
    HeaderParse(String),

    /// Failed to parse protocol body.
    ///
    /// The response body is malformed or doesn't match the declared format.
    #[error("Body parse error: {0}")]
    BodyParse(String),

    /// Invalid version format (not a valid version identifier).
    ///
    /// Version IDs must be valid JSON strings or integers (RFC 8941).
    #[error("Invalid version: {0}")]
    InvalidVersion(String),

    /// Subscription-specific error.
    ///
    /// Includes issues with subscription setup, maintenance, or unexpected closure.
    #[error("Subscription error: {0}")]
    Subscription(String),

    /// JSON serialization or deserialization error.
    ///
    /// Indicates a problem encoding or decoding JSON in request/response bodies.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Subscription closed unexpectedly.
    ///
    /// The server closed the connection while a subscription was active.
    /// This is a retryable error - the client should resume the subscription.
    #[error("Subscription closed")]
    SubscriptionClosed,

    /// Invalid subscription status (expected 209)
    #[error("Expected status 209 for subscription, got {0}")]
    InvalidSubscriptionStatus(u16),

    /// Protocol violation error
    #[error("Protocol error: {0}")]
    Protocol(String),

    /// Operation timed out.
    #[error("Operation timed out")]
    Timeout,

    /// Request was aborted.
    #[error("Request aborted")]
    Aborted,

    /// Filesystem error (notify, mapping, etc.)
    #[error("BraidFS Error: {0}")]
    Fs(String),

    /// Invalid UTF-8 sequence in response.
    ///
    /// Response body or headers contain invalid UTF-8 encoding.
    #[error("Invalid UTF-8: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),

    /// Configuration error in Braid setup.
    ///
    /// Invalid parameters were provided when creating a client or server.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Internal error in the library.
    ///
    /// Indicates a bug or unexpected state in the implementation.
    #[error("Internal error: {0}")]
    Internal(String),

    /// Server has dropped version history (HTTP 410 Gone - Section 4.5).
    ///
    /// **Not retryable**. The server has discarded history before the requested version.
    /// The client must restart synchronization from scratch by clearing its local state.
    /// This error signals that the parents requested are no longer available on the server.
    #[error("Server has dropped history - cannot resume synchronization")]
    HistoryDropped,

    /// Conflicting versions detected in merge (HTTP 293 - Section 2.2).
    #[error("Conflicting versions in merge: {0}")]
    MergeConflict(String),

    /// Notify/Watcher error.
    #[cfg(not(target_arch = "wasm32"))]
    #[error("Notify error: {0}")]
    Notify(#[from] notify::Error),

    /// Generic anyhow error (temporary for conversion).
    #[error("Anyhow error: {0}")]
    Anyhow(String),
}

impl From<anyhow::Error> for BraidError {
    fn from(err: anyhow::Error) -> Self {
        BraidError::Anyhow(err.to_string())
    }
}

impl BraidError {
    /// Check if this error is retryable.
    ///
    /// Returns `true` for transient errors that may succeed on retry:
    /// - Network timeouts
    /// - I/O errors
    /// - HTTP 408, 425, 429, 502, 503, 504
    ///
    /// Returns `false` for permanent errors:
    /// - Protocol violations
    /// - History dropped
    /// - Access denied
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::BraidError;
    ///
    /// assert!(BraidError::Timeout.is_retryable());
    /// assert!(!BraidError::HistoryDropped.is_retryable());
    /// ```
    #[inline]
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            BraidError::Http(msg) => {
                msg.contains("408")
                    || msg.contains("425")
                    || msg.contains("429")
                    || msg.contains("502")
                    || msg.contains("503")
                    || msg.contains("504")
            }
            BraidError::Timeout | BraidError::Io(_) => true,
            BraidError::HistoryDropped => false,
            _ => false,
        }
    }

    /// Check if this is an access denied error.
    ///
    /// Returns `true` for HTTP 401 (Unauthorized) or 403 (Forbidden).
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::BraidError;
    ///
    /// let err = BraidError::Http("401 Unauthorized".into());
    /// assert!(err.is_access_denied());
    /// ```
    #[inline]
    #[must_use]
    pub fn is_access_denied(&self) -> bool {
        match self {
            BraidError::Http(msg) => msg.contains("401") || msg.contains("403"),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_is_retryable() {
        assert!(BraidError::Timeout.is_retryable());
    }

    #[test]
    fn test_history_dropped_not_retryable() {
        assert!(!BraidError::HistoryDropped.is_retryable());
    }

    #[test]
    fn test_http_503_is_retryable() {
        let err = BraidError::Http("503 Service Unavailable".into());
        assert!(err.is_retryable());
    }

    #[test]
    fn test_http_404_not_retryable() {
        let err = BraidError::Http("404 Not Found".into());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_access_denied_401() {
        let err = BraidError::Http("401 Unauthorized".into());
        assert!(err.is_access_denied());
    }

    #[test]
    fn test_access_denied_403() {
        let err = BraidError::Http("403 Forbidden".into());
        assert!(err.is_access_denied());
    }

    #[test]
    fn test_not_access_denied() {
        let err = BraidError::Http("500 Internal Server Error".into());
        assert!(!err.is_access_denied());
    }

    #[test]
    fn test_error_display() {
        let err = BraidError::InvalidVersion("bad-version".into());
        assert!(err.to_string().contains("bad-version"));
    }

    #[test]
    fn test_header_parse_error() {
        let err = BraidError::HeaderParse("invalid header".into());
        assert!(err.to_string().contains("invalid header"));
    }

    #[test]
    fn test_subscription_closed() {
        let err = BraidError::SubscriptionClosed;
        assert!(err.to_string().contains("closed"));
    }

    #[test]
    fn test_invalid_subscription_status() {
        let err = BraidError::InvalidSubscriptionStatus(200);
        assert!(err.to_string().contains("200"));
    }

    #[test]
    fn test_merge_conflict() {
        let err = BraidError::MergeConflict("v1 vs v2".into());
        assert!(err.to_string().contains("v1 vs v2"));
    }
}
