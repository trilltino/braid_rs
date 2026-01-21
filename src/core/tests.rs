//! Integration tests for Braid-HTTP.
//!
//! This module contains integration tests that verify the behavior of the
//! Braid-HTTP client and server implementations. These tests validate:
//!
//! - Type creation and manipulation
//! - Request building
//! - Client configuration
//! - Protocol type conversions
//!
//! # Test Organization
//!
//! Tests are organized by functionality:
//! - Version creation and conversion
//! - Patch creation
//! - Update creation
//! - Request builder patterns
//! - Client initialization
//!
//! # Running Tests
//!
//! Run all tests with:
//! ```bash
//! cargo test
//! ```
//!
//! Run with output:
//! ```bash
//! cargo test -- --nocapture
//! ```

#[cfg(test)]
mod integration_tests {
    use crate::core::{BraidClient, BraidRequest, Version, Patch, Update};

    #[test]
    fn test_version_creation() {
        let v1 = Version::from("v1");
        let v2 = Version::new("v2");
        assert_eq!(v1.to_string(), "v1");
        assert_eq!(v2.to_string(), "v2");
    }

    #[test]
    fn test_patch_creation() {
        let patch = Patch::json(".field", "value");
        assert_eq!(patch.unit, "json");
        assert_eq!(patch.range, ".field");
    }

    #[test]
    fn test_update_creation() {
        let update = Update::snapshot(
            Version::from("v1"),
            "test body",
        );
        assert_eq!(update.status, 200);
        assert!(update.body.is_some());
        assert!(update.patches.is_none());
    }

    #[test]
    fn test_braid_request_builder() {
        let request = BraidRequest::new()
            .subscribe()
            .with_version(Version::from("v1"))
            .with_heartbeat(5);

        assert!(request.subscribe);
        assert!(request.version.is_some());
        assert_eq!(request.heartbeat_interval, Some(5));
    }

    #[test]
    fn test_client_creation() {
        let client = BraidClient::new();
        assert_eq!(client.config().max_retries, 3);
    }
}
