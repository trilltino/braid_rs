//! Complete update in the Braid protocol.
//!
//! An [`Update`] represents a single mutation that transforms a resource from one state
//! to another. It is the fundamental unit of state synchronization in Braid-HTTP.
//!
//! # Overview
//!
//! Updates contain:
//!
//! - **Version information**: Unique ID(s) and parent version(s) forming a DAG
//! - **Content**: Either a complete snapshot or incremental patches
//! - **Metadata**: Merge type, content type, status code
//!
//! # Update Types
//!
//! | Type | Description | Constructor |
//! |------|-------------|-------------|
//! | Snapshot | Complete state representation | [`Update::snapshot`] |
//! | Patched | Incremental changes via patches | [`Update::patched`] |
//!
//! # Version DAG
//!
//! Updates form a Directed Acyclic Graph (DAG) through version and parent relationships:
//!
//! ```text
//!     v1 (initial)
//!      │
//!     v2 (parent: v1)
//!    /  \
//!   v3   v4 (concurrent edits, both parent: v2)
//!    \  /
//!     v5 (merge, parents: v3, v4)
//! ```
//!
//! # Examples
//!
//! ## Snapshot Update
//!
//! ```
//! use crate::core::{Update, Version};
//!
//! let update = Update::snapshot(Version::new("v2"), r#"{"name": "Alice"}"#)
//!     .with_parent(Version::new("v1"))
//!     .with_content_type("application/json");
//! ```
//!
//! ## Patch Update
//!
//! ```
//! use crate::core::{Update, Version, Patch};
//!
//! let update = Update::patched(Version::new("v3"), vec![
//!     Patch::json(".name", r#""Bob""#),
//!     Patch::json(".age", "30"),
//! ]).with_parent(Version::new("v2"));
//! ```
//!
//! ## Merge Update
//!
//! ```
//! use crate::core::{Update, Version};
//!
//! let merge = Update::snapshot(Version::new("v5"), r#"{"merged": true}"#)
//!     .with_parent(Version::new("v3"))
//!     .with_parent(Version::new("v4"))
//!     .with_merge_type("diamond");
//! ```
//!
//! # HTTP Status Codes
//!
//! Updates are sent with these status codes:
//!
//! | Code | Meaning | Use Case |
//! |------|---------|----------|
//! | 200 | OK | Standard update response |
//! | 206 | Partial Content | Range-based patches (RFC 7233) |
//! | 209 | Subscription | Streaming subscription update |
//! | 293 | Merge Conflict | Conflict detected during merge |
//! | 410 | Gone | Resource deleted |
//! | 416 | Range Not Satisfiable | Invalid patch range |
//!
//! # Specification
//!
//! See [draft-toomim-httpbis-braid-http-04]:
//!
//! - **Section 2**: Versioning - Version IDs, Parents, DAG structure
//! - **Section 2.2**: Merge-Types - Conflict resolution
//! - **Section 3**: Updates as Patches - Patch format
//! - **Section 4.4**: Current-Version - Subscription catch-up
//!
//! [draft-toomim-httpbis-braid-http-04]: https://datatracker.ietf.org/doc/html/draft-toomim-httpbis-braid-http

use crate::core::types::{ContentRange, Patch, Version};
use bytes::Bytes;
use std::collections::BTreeMap;

/// A complete update in the Braid protocol.
///
/// Represents a single mutation that transforms a resource from one state to another.
/// Updates can be either snapshots (complete state) or patches (incremental changes).
///
/// # Creating Updates
///
/// Use the constructors for the two update types:
///
/// ```
/// use crate::core::{Update, Version, Patch};
///
/// // Snapshot: complete state
/// let snapshot = Update::snapshot(Version::new("v1"), "complete state");
///
/// // Patched: incremental changes
/// let patched = Update::patched(Version::new("v2"), vec![
///     Patch::json(".field", "value")
/// ]);
/// ```
///
/// # Builder Pattern
///
/// Use builder methods to add metadata:
///
/// ```
/// use crate::core::{Update, Version};
///
/// let update = Update::snapshot(Version::new("v2"), "data")
///     .with_parent(Version::new("v1"))
///     .with_merge_type("diamond")
///     .with_content_type("application/json")
///     .with_status(200);
/// ```
///
/// # Fields
///
/// - `version`: Version ID(s) uniquely identifying this update
/// - `parents`: Parent version(s) this update builds upon
/// - `body`: Complete content for snapshot updates
/// - `patches`: Incremental changes for patch updates
/// - `merge_type`: Conflict resolution strategy
/// - `status`: HTTP status code for the response
#[derive(Clone, Debug)]
pub struct Update {
    /// Version ID(s) of this update.
    ///
    /// Most updates have a single version, but merges may have multiple.
    pub version: Vec<Version>,

    /// Parent version(s) in the version DAG.
    ///
    /// - Empty for initial versions
    /// - Single parent for linear history
    /// - Multiple parents for merges
    pub parents: Vec<Version>,

    /// Current version(s) for subscription catch-up signaling.
    ///
    /// Used in subscriptions to indicate the latest version the server has,
    /// allowing clients to detect if they need to catch up.
    pub current_version: Option<Vec<Version>>,

    /// Merge type for conflict resolution.
    ///
    /// Common values: `"sync9"`, `"diamond"`, or application-defined.
    pub merge_type: Option<String>,

    /// Patches for incremental updates.
    ///
    /// Present for patch updates, `None` for snapshots.
    pub patches: Option<Vec<Patch>>,

    /// Complete body for snapshot updates.
    ///
    /// Present for snapshots, `None` for patch updates.
    pub body: Option<Bytes>,

    /// Content range specification for single-patch updates.
    ///
    /// Used when the update is a single patch with a specific range.
    pub content_range: Option<ContentRange>,

    /// Content type of the body/patches.
    ///
    /// For snapshot updates, this is typically the resource's media type
    /// (e.g., `"application/json"`, `"text/plain"`).
    ///
    /// For patch updates, this MUST be `"application/braid-patch"` according
    /// to Section 3 of the specification.
    pub content_type: Option<String>,

    /// HTTP status code for the response.
    ///
    /// Default: 200. Common values: 200, 206, 209, 293, 410, 416.
    pub status: u16,

    /// Additional headers to include in the response.
    pub extra_headers: BTreeMap<String, String>,

    /// Target URL for this update (for multiplexed streams)
    pub url: Option<String>,
}

impl Update {
    /// Create a snapshot update with complete state.
    ///
    /// A snapshot update contains the complete representation of the resource state.
    /// Use this when sending the full state rather than incremental changes.
    ///
    /// # Arguments
    ///
    /// * `version` - The version ID for this update
    /// * `body` - The complete state content
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{Update, Version};
    ///
    /// let update = Update::snapshot(
    ///     Version::new("v1"),
    ///     r#"{"name": "Alice", "age": 30}"#
    /// );
    ///
    /// assert!(update.is_snapshot());
    /// assert!(!update.is_patched());
    /// ```
    #[must_use]
    pub fn snapshot(version: Version, body: impl Into<Bytes>) -> Self {
        Update {
            version: vec![version],
            parents: vec![],
            current_version: None,
            merge_type: None,
            patches: None,
            body: Some(body.into()),
            content_range: None,
            content_type: None,
            status: 200,
            extra_headers: BTreeMap::new(),
            url: None,
        }
    }

    /// Create a patch update with incremental changes.
    ///
    /// A patch update contains one or more patches that modify specific ranges
    /// of the resource. Use this for efficient incremental updates.
    ///
    /// # Arguments
    ///
    /// * `version` - The version ID for this update
    /// * `patches` - The list of patches to apply
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{Update, Version, Patch};
    ///
    /// let update = Update::patched(
    ///     Version::new("v2"),
    ///     vec![
    ///         Patch::json(".name", r#""Bob""#),
    ///         Patch::json(".age", "31"),
    ///     ]
    /// );
    ///
    /// assert!(update.is_patched());
    /// assert!(!update.is_snapshot());
    /// ```
    #[must_use]
    pub fn patched(version: Version, patches: Vec<Patch>) -> Self {
        Update {
            version: vec![version],
            parents: vec![],
            current_version: None,
            merge_type: None,
            patches: Some(patches),
            body: None,
            content_range: None,
            content_type: None,
            status: 200,
            url: None,
            extra_headers: BTreeMap::new(),
        }
    }

    /// Check if this is a snapshot update.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{Update, Version};
    ///
    /// let update = Update::snapshot(Version::new("v1"), "data");
    /// assert!(update.is_snapshot());
    /// ```
    #[inline]
    #[must_use]
    pub fn is_snapshot(&self) -> bool {
        self.body.is_some()
    }

    /// Check if this is a patch update.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{Update, Version, Patch};
    ///
    /// let update = Update::patched(Version::new("v1"), vec![Patch::json(".f", "v")]);
    /// assert!(update.is_patched());
    /// ```
    #[inline]
    #[must_use]
    pub fn is_patched(&self) -> bool {
        self.patches.is_some()
    }

    /// Get the primary version ID.
    ///
    /// Returns the first version ID, or `None` if no versions are set.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{Update, Version};
    ///
    /// let update = Update::snapshot(Version::new("v1"), "data");
    /// assert_eq!(update.primary_version(), Some(&Version::new("v1")));
    /// ```
    #[inline]
    #[must_use]
    pub fn primary_version(&self) -> Option<&Version> {
        self.version.first()
    }

    /// Get the body content as a UTF-8 string.
    ///
    /// # Returns
    ///
    /// `Some(&str)` if the body exists and is valid UTF-8, `None` otherwise.
    #[inline]
    #[must_use]
    pub fn body_str(&self) -> Option<&str> {
        self.body.as_ref().and_then(|b| std::str::from_utf8(b).ok())
    }

    /// Create a subscription snapshot update with HTTP 209 status.
    ///
    /// This is a convenience method for creating snapshot updates that will be sent
    /// as part of a subscription stream. It automatically sets the status code to 209
    /// as required by the Braid-HTTP specification (Section 4).
    ///
    /// # Arguments
    ///
    /// * `version` - The version ID for this update
    /// * `body` - The complete state content
    ///
    /// # Specification
    ///
    /// Per Section 4 of draft-toomim-httpbis-braid-http, subscription responses
    /// MUST use HTTP status code 209 to indicate an active subscription.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{Update, Version};
    ///
    /// let update = Update::subscription_snapshot(
    ///     Version::new("v1"),
    ///     r#"{"name": "Alice"}"#
    /// );
    ///
    /// assert_eq!(update.status, 209);
    /// assert!(update.is_snapshot());
    /// ```
    #[must_use]
    pub fn subscription_snapshot(version: Version, body: impl Into<Bytes>) -> Self {
        Update::snapshot(version, body).with_status(209)
    }

    /// Create a subscription patch update with HTTP 209 status.
    ///
    /// This is a convenience method for creating patch updates that will be sent
    /// as part of a subscription stream. It automatically sets the status code to 209
    /// as required by the Braid-HTTP specification (Section 4).
    ///
    /// # Arguments
    ///
    /// * `version` - The version ID for this update
    /// * `patches` - The list of patches to apply
    ///
    /// # Specification
    ///
    /// Per Section 4 of draft-toomim-httpbis-braid-http, subscription responses
    /// MUST use HTTP status code 209 to indicate an active subscription.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{Update, Version, Patch};
    ///
    /// let update = Update::subscription_patched(
    ///     Version::new("v2"),
    ///     vec![Patch::json(".name", r#""Bob""#)]
    /// );
    ///
    /// assert_eq!(update.status, 209);
    /// assert!(update.is_patched());
    /// ```
    #[must_use]
    pub fn subscription_patched(version: Version, patches: Vec<Patch>) -> Self {
        Update::patched(version, patches).with_status(209)
    }

    /// Add a parent version.
    ///
    /// Parent versions indicate which version(s) this update builds upon.
    /// Most updates have a single parent; merges have multiple.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{Update, Version};
    ///
    /// let update = Update::snapshot(Version::new("v2"), "data")
    ///     .with_parent(Version::new("v1"));
    ///
    /// assert_eq!(update.parents.len(), 1);
    /// ```
    #[must_use]
    pub fn with_parent(mut self, parent: Version) -> Self {
        self.parents.push(parent);
        self
    }

    /// Add multiple parent versions.
    ///
    /// Use this for merge updates that combine multiple branches.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{Update, Version};
    ///
    /// let update = Update::snapshot(Version::new("v5"), "merged")
    ///     .with_parents(vec![Version::new("v3"), Version::new("v4")]);
    ///
    /// assert_eq!(update.parents.len(), 2);
    /// ```
    #[must_use]
    pub fn with_parents(mut self, parents: Vec<Version>) -> Self {
        self.parents.extend(parents);
        self
    }

    /// Set current version for subscription catch-up signaling.
    ///
    /// Used in subscriptions to indicate the latest version the server has,
    /// allowing clients to detect if they need to catch up.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{Update, Version};
    ///
    /// let update = Update::snapshot(Version::new("v1"), "data")
    ///     .with_current_version(Version::new("v5"));
    /// ```
    #[must_use]
    pub fn with_current_version(mut self, version: Version) -> Self {
        if self.current_version.is_none() {
            self.current_version = Some(Vec::new());
        }
        if let Some(ref mut versions) = self.current_version {
            versions.push(version);
        }
        self
    }

    /// Set merge type for conflict resolution.
    ///
    /// The merge type specifies how conflicts should be resolved when
    /// multiple clients update the same resource concurrently.
    ///
    /// # Common Values
    ///
    /// - `"sync9"`: Simple synchronization
    /// - `"diamond"`: Diamond-types CRDT
    /// - Application-defined algorithms
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{Update, Version};
    ///
    /// let update = Update::snapshot(Version::new("v1"), "data")
    ///     .with_merge_type("diamond");
    ///
    /// assert_eq!(update.merge_type, Some("diamond".to_string()));
    /// ```
    #[must_use]
    pub fn with_merge_type(mut self, merge_type: impl Into<String>) -> Self {
        self.merge_type = Some(merge_type.into());
        self
    }

    /// Set content range for single-patch updates.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{Update, Version, ContentRange};
    ///
    /// let update = Update::snapshot(Version::new("v1"), "data")
    ///     .with_content_range(ContentRange::json(".field"));
    /// ```
    #[must_use]
    pub fn with_content_range(mut self, content_range: ContentRange) -> Self {
        self.content_range = Some(content_range);
        self
    }

    /// Set content type.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{Update, Version};
    ///
    /// let update = Update::snapshot(Version::new("v1"), "{}")
    ///     .with_content_type("application/json");
    /// ```
    #[must_use]
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// Set HTTP status code.
    ///
    /// # Common Status Codes
    ///
    /// - `200`: OK (default)
    /// - `206`: Partial Content (range-based patches)
    /// - `209`: Subscription update
    /// - `293`: Merge conflict
    /// - `410`: Gone (resource deleted)
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{Update, Version};
    ///
    /// let update = Update::snapshot(Version::new("v1"), "data")
    ///     .with_status(209);
    /// ```
    #[must_use]
    pub fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    /// Add a custom header.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{Update, Version};
    ///
    /// let update = Update::snapshot(Version::new("v1"), "data")
    ///     .with_header("X-Custom", "value");
    /// ```
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_headers.insert(name.into(), value.into());
        self
    }

    /// Convert to a JSON value.
    ///
    /// Serializes the update to a JSON object with version, parents, and body.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{Update, Version};
    ///
    /// let update = Update::snapshot(Version::new("v1"), "data")
    ///     .with_parent(Version::new("v0"));
    ///
    /// let json = update.to_json();
    /// assert!(json.get("version").is_some());
    /// assert!(json.get("parents").is_some());
    /// ```
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        let mut obj = serde_json::Map::new();

        obj.insert(
            "version".to_string(),
            serde_json::Value::Array(self.version.iter().map(|v| v.to_json()).collect()),
        );

        obj.insert(
            "parents".to_string(),
            serde_json::Value::Array(self.parents.iter().map(|v| v.to_json()).collect()),
        );

        if let Some(body) = &self.body {
            obj.insert(
                "body".to_string(),
                serde_json::Value::String(String::from_utf8_lossy(body).into_owned()),
            );
        }

        if let Some(merge_type) = &self.merge_type {
            obj.insert(
                "merge_type".to_string(),
                serde_json::Value::String(merge_type.clone()),
            );
        }

        serde_json::Value::Object(obj)
    }
}

impl Default for Update {
    fn default() -> Self {
        Update {
            version: vec![],
            parents: vec![],
            current_version: None,
            merge_type: None,
            patches: None,
            body: None,
            content_range: None,
            content_type: None,
            status: 200,
            extra_headers: BTreeMap::new(),
            url: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_snapshot() {
        let update = Update::snapshot(Version::new("v1"), "body");
        assert_eq!(update.version.len(), 1);
        assert!(update.body.is_some());
        assert!(update.patches.is_none());
        assert!(update.is_snapshot());
        assert!(!update.is_patched());
    }

    #[test]
    fn test_update_patched() {
        let update = Update::patched(Version::new("v1"), vec![Patch::json(".field", "value")]);
        assert_eq!(update.version.len(), 1);
        assert!(update.patches.is_some());
        assert!(update.body.is_none());
        assert!(update.is_patched());
        assert!(!update.is_snapshot());
    }

    #[test]
    fn test_update_builder() {
        let update = Update::snapshot(Version::new("v1"), "body")
            .with_parent(Version::new("v0"))
            .with_merge_type("sync9");
        assert_eq!(update.parents.len(), 1);
        assert_eq!(update.merge_type, Some("sync9".to_string()));
    }

    #[test]
    fn test_primary_version() {
        let update = Update::snapshot(Version::new("v1"), "body");
        assert_eq!(update.primary_version(), Some(&Version::new("v1")));
    }

    #[test]
    fn test_body_str() {
        let update = Update::snapshot(Version::new("v1"), "hello");
        assert_eq!(update.body_str(), Some("hello"));
    }

    #[test]
    fn test_with_parents() {
        let update = Update::snapshot(Version::new("v3"), "merged")
            .with_parents(vec![Version::new("v1"), Version::new("v2")]);
        assert_eq!(update.parents.len(), 2);
    }

    #[test]
    fn test_with_header() {
        let update = Update::snapshot(Version::new("v1"), "data").with_header("X-Custom", "value");
        assert_eq!(
            update.extra_headers.get("X-Custom"),
            Some(&"value".to_string())
        );
    }

    #[test]
    fn test_to_json() {
        let update = Update::snapshot(Version::new("v1"), "data")
            .with_parent(Version::new("v0"))
            .with_merge_type("diamond");
        let json = update.to_json();
        assert!(json.get("version").is_some());
        assert!(json.get("parents").is_some());
        assert!(json.get("body").is_some());
        assert!(json.get("merge_type").is_some());
    }

    #[test]
    fn test_default() {
        let update = Update::default();
        assert!(update.version.is_empty());
        assert!(update.parents.is_empty());
        assert_eq!(update.status, 200);
    }

    #[test]
    fn test_subscription_snapshot() {
        let update = Update::subscription_snapshot(Version::new("v1"), "data");
        assert_eq!(update.status, 209);
        assert!(update.is_snapshot());
        assert!(!update.is_patched());
    }

    #[test]
    fn test_subscription_patched() {
        let update =
            Update::subscription_patched(Version::new("v2"), vec![Patch::json(".field", "value")]);
        assert_eq!(update.status, 209);
        assert!(update.is_patched());
        assert!(!update.is_snapshot());
    }

    #[test]
    fn test_subscription_with_parents() {
        let update = Update::subscription_snapshot(Version::new("v2"), "data")
            .with_parent(Version::new("v1"));
        assert_eq!(update.status, 209);
        assert_eq!(update.parents.len(), 1);
    }

    #[test]
    fn test_zero_length_body() {
        // Zero-length bodies are valid per spec Section 4
        let update = Update::snapshot(Version::new("v1"), "");
        assert!(update.body.is_some());
        assert_eq!(update.body.as_ref().unwrap().len(), 0);
    }

    #[test]
    fn test_zero_length_subscription() {
        // Zero-length subscription updates are valid (e.g., deletion marker)
        let update = Update::subscription_snapshot(Version::new("v1"), Bytes::new());
        assert_eq!(update.status, 209);
        assert!(update.body.is_some());
        assert_eq!(update.body.as_ref().unwrap().len(), 0);
    }
}
