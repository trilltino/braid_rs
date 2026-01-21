//! Braid-specific request parameters.
//!
//! The [`BraidRequest`] type configures Braid-HTTP requests with protocol-specific
//! options using a fluent builder pattern.
//!
//! # Overview
//!
//! Braid requests extend standard HTTP requests with:
//!
//! | Feature | Header | Description |
//! |---------|--------|-------------|
//! | Versioning | `Version` | Request specific version(s) |
//! | Parents | `Parents` | Request version range |
//! | Subscription | `Subscribe` | Stream live updates |
//! | Heartbeats | `Heartbeats` | Keep-alive interval |
//! | Merge Type | `Merge-Type` | Conflict resolution |
//! | Patches | `Patches` | Incremental updates |
//!
//! # Examples
//!
//! ## Simple Subscription
//!
//! ```
//! use crate::core::{BraidRequest, Version};
//!
//! let req = BraidRequest::new()
//!     .subscribe()
//!     .with_heartbeat(30);
//!
//! assert!(req.subscribe);
//! ```
//!
//! ## Version Request
//!
//! ```
//! use crate::core::{BraidRequest, Version};
//!
//! let req = BraidRequest::new()
//!     .with_version(Version::new("v5"))
//!     .with_parent(Version::new("v3"));
//! ```
//!
//! ## Patch Request
//!
//! ```
//! use crate::core::{BraidRequest, Patch};
//!
//! let req = BraidRequest::new()
//!     .with_patches(vec![Patch::json(".name", r#""Alice""#)])
//!     .with_merge_type("diamond");
//! ```
//!
//! # Specification
//!
//! Implements features from [draft-toomim-httpbis-braid-http-04]:
//!
//! - **Section 2**: Versioning (Version, Parents headers)
//! - **Section 2.2**: Merge-Types
//! - **Section 3**: Patches
//! - **Section 4.1**: Subscriptions (Subscribe, Heartbeats headers)
//!
//! [draft-toomim-httpbis-braid-http-04]: https://datatracker.ietf.org/doc/html/draft-toomim-httpbis-braid-http

use crate::core::types::{Patch, Version};

/// Braid-specific request parameters.
///
/// Configures a Braid-HTTP request with protocol-specific options.
/// Use the builder pattern to construct requests.
///
/// # Creating Requests
///
/// ```
/// use crate::core::{BraidRequest, Version};
///
/// let req = BraidRequest::new()
///     .subscribe()
///     .with_version(Version::new("v1"))
///     .with_heartbeat(30);
/// ```
///
/// # Fields
///
/// - `version`: Version(s) to request
/// - `parents`: Parent version(s) for range requests
/// - `subscribe`: Enable subscription mode
/// - `patches`: Patches to send
/// - `heartbeat_interval`: Keep-alive interval in seconds
/// - `peer`: Peer identifier
/// - `merge_type`: Conflict resolution strategy
#[derive(Clone, Debug, Default)]
pub struct BraidRequest {
    /// Version(s) being requested.
    ///
    /// Corresponds to the `Version` header. When set, the server returns
    /// the specified version(s) of the resource.
    pub version: Option<Vec<Version>>,

    /// Parent version(s) for range requests.
    ///
    /// Corresponds to the `Parents` header. Combined with `version`,
    /// requests all updates between parents and version.
    pub parents: Option<Vec<Version>>,

    /// Enable subscription mode.
    ///
    /// When `true`, the server keeps the connection open and streams
    /// updates as they occur. Corresponds to `Subscribe: true` header.
    pub subscribe: bool,

    /// Patches to send instead of a full body.
    ///
    /// For PUT/PATCH requests, send incremental changes rather than
    /// the complete resource state.
    pub patches: Option<Vec<Patch>>,

    /// Heartbeat interval in seconds.
    ///
    /// For subscriptions, the server sends empty updates at this interval
    /// to keep the connection alive. Corresponds to `Heartbeats` header.
    pub heartbeat_interval: Option<u64>,

    /// Peer identifier.
    ///
    /// Identifies this client to the server for tracking purposes.
    /// Corresponds to the `Peer` header.
    pub peer: Option<String>,

    /// Acknowledged version(s).
    ///
    /// For reliable delivery, acknowledges receipt of specific versions.
    /// Corresponds to the `Ack` header.
    pub ack: Option<Vec<Version>>,

    /// Enable multiplexing.
    ///
    /// When enabled, multiple resources can share a single connection.
    pub enable_multiplex: bool,

    /// Merge type for conflict resolution.
    ///
    /// Specifies how the server should resolve conflicts.
    /// Common values: `"sync9"`, `"diamond"`.
    pub merge_type: Option<String>,

    /// Content type of the request body.
    ///
    /// E.g., `"application/json"`, `"text/plain"`.
    pub content_type: Option<String>,

    /// HTTP method (GET, PUT, POST, etc.). Default is "GET".
    pub method: String,

    /// Request body.
    pub body: bytes::Bytes,

    /// Extra headers to include in the request.
    pub extra_headers: std::collections::BTreeMap<String, String>,

    /// Retry configuration for automatic retries on failure.
    ///
    /// When set, the client will automatically retry failed requests
    /// according to this configuration.
    pub retry: Option<crate::core::client::retry::RetryConfig>,
}

impl BraidRequest {
    /// Create a new Braid request with default settings.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::BraidRequest;
    ///
    /// let req = BraidRequest::new();
    /// assert!(!req.subscribe);
    /// ```
    #[inline]
    pub fn new() -> Self {
        Self {
            method: "GET".to_string(),
            body: bytes::Bytes::new(),
            extra_headers: std::collections::BTreeMap::new(),
            ..Default::default()
        }
    }

    /// Enable subscription mode.
    ///
    /// When enabled, the server keeps the connection open and streams
    /// updates as they occur.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::BraidRequest;
    ///
    /// let req = BraidRequest::new().subscribe();
    /// assert!(req.subscribe);
    /// ```
    pub fn subscribe(mut self) -> Self {
        self.subscribe = true;
        self
    }

    /// Check if this is a subscription request.
    #[inline]
    pub fn is_subscription(&self) -> bool {
        self.subscribe
    }

    /// Add a version to request.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{BraidRequest, Version};
    ///
    /// let req = BraidRequest::new()
    ///     .with_version(Version::new("v1"))
    ///     .with_version(Version::new("v2"));
    ///
    /// assert_eq!(req.version.unwrap().len(), 2);
    /// ```
    pub fn with_version(mut self, version: Version) -> Self {
        if self.version.is_none() {
            self.version = Some(Vec::new());
        }
        if let Some(ref mut versions) = self.version {
            versions.push(version);
        }
        self
    }

    /// Set multiple versions at once.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{BraidRequest, Version};
    ///
    /// let req = BraidRequest::new()
    ///     .with_versions(vec![Version::new("v1"), Version::new("v2")]);
    /// ```
    pub fn with_versions(mut self, versions: Vec<Version>) -> Self {
        self.version = Some(versions);
        self
    }

    /// Add a parent version.
    ///
    /// Parent versions define the starting point for range requests.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{BraidRequest, Version};
    ///
    /// let req = BraidRequest::new()
    ///     .with_parent(Version::new("v1"));
    /// ```
    pub fn with_parent(mut self, version: Version) -> Self {
        if self.parents.is_none() {
            self.parents = Some(Vec::new());
        }
        if let Some(ref mut parents) = self.parents {
            parents.push(version);
        }
        self
    }

    /// Set the peer identifier.
    ///
    /// Identifies this client to the server.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::BraidRequest;
    ///
    /// let req = BraidRequest::new().with_peer("client-1");
    /// ```
    pub fn with_peer(mut self, peer: impl Into<String>) -> Self {
        self.peer = Some(peer.into());
        self
    }

    /// Add an acknowledgement.
    ///
    /// Acknowledgements are used for reliable delivery protocols.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{BraidRequest, Version};
    ///
    /// let req = BraidRequest::new().with_ack(Version::new("v1"));
    /// ```
    pub fn with_ack(mut self, version: Version) -> Self {
        if self.ack.is_none() {
            self.ack = Some(Vec::new());
        }
        if let Some(ref mut acks) = self.ack {
            acks.push(version);
        }
        self
    }

    /// Set multiple parent versions at once.
    pub fn with_parents(mut self, parents: Vec<Version>) -> Self {
        self.parents = Some(parents);
        self
    }

    /// Set patches to send.
    ///
    /// For PUT/PATCH requests, send incremental changes.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{BraidRequest, Patch};
    ///
    /// let req = BraidRequest::new()
    ///     .with_patches(vec![Patch::json(".name", r#""Alice""#)]);
    /// ```
    pub fn with_patches(mut self, patches: Vec<Patch>) -> Self {
        self.patches = Some(patches);
        self
    }

    /// Check if this request has patches.
    #[inline]
    pub fn has_patches(&self) -> bool {
        self.patches.as_ref().is_some_and(|p| !p.is_empty())
    }

    /// Set heartbeat interval in seconds.
    ///
    /// For subscriptions, the server sends keep-alive signals at this interval.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::BraidRequest;
    ///
    /// let req = BraidRequest::new()
    ///     .subscribe()
    ///     .with_heartbeat(30);
    /// ```
    pub fn with_heartbeat(mut self, seconds: u64) -> Self {
        self.heartbeat_interval = Some(seconds);
        self
    }

    /// Enable or disable multiplexing.
    pub fn with_multiplex(mut self, enable: bool) -> Self {
        self.enable_multiplex = enable;
        self
    }

    /// Set merge type for conflict resolution.
    ///
    /// # Common Values
    ///
    /// - `"sync9"`: Simple synchronization
    /// - `"diamond"`: Diamond-types CRDT
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::BraidRequest;
    ///
    /// let req = BraidRequest::new()
    ///     .with_merge_type("diamond");
    /// ```
    pub fn with_merge_type(mut self, merge_type: impl Into<String>) -> Self {
        self.merge_type = Some(merge_type.into());
        self
    }

    /// Set content type.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::BraidRequest;
    ///
    /// let req = BraidRequest::new()
    ///     .with_content_type("application/json");
    /// ```
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// Set HTTP method.
    pub fn with_method(mut self, method: impl Into<String>) -> Self {
        self.method = method.into();
        self
    }

    /// Set request body.
    pub fn with_body(mut self, body: impl Into<bytes::Bytes>) -> Self {
        self.body = body.into();
        self
    }

    /// Add a custom header.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_headers.insert(key.into(), value.into());
        self
    }

    /// Enable automatic retries with the given configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::BraidRequest;
    /// use crate::core::client::RetryConfig;
    ///
    /// let req = BraidRequest::new()
    ///     .with_retry(RetryConfig::default().with_max_retries(5));
    /// ```
    pub fn with_retry(mut self, config: crate::core::client::retry::RetryConfig) -> Self {
        self.retry = Some(config);
        self
    }

    /// Enable automatic retries with default configuration.
    pub fn retry(self) -> Self {
        self.with_retry(crate::core::client::retry::RetryConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_braid_request_new() {
        let req = BraidRequest::new();
        assert!(!req.subscribe);
        assert!(req.version.is_none());
        assert!(req.parents.is_none());
    }

    #[test]
    fn test_braid_request_subscribe() {
        let req = BraidRequest::new().subscribe();
        assert!(req.subscribe);
        assert!(req.is_subscription());
    }

    #[test]
    fn test_braid_request_builder() {
        let req = BraidRequest::new()
            .subscribe()
            .with_version(Version::new("v1"))
            .with_heartbeat(5);

        assert!(req.subscribe);
        assert_eq!(req.version.unwrap().len(), 1);
        assert_eq!(req.heartbeat_interval, Some(5));
    }

    #[test]
    fn test_with_versions() {
        let req = BraidRequest::new().with_versions(vec![Version::new("v1"), Version::new("v2")]);
        assert_eq!(req.version.unwrap().len(), 2);
    }

    #[test]
    fn test_with_patches() {
        let req = BraidRequest::new().with_patches(vec![Patch::json(".f", "v")]);
        assert!(req.has_patches());
    }

    #[test]
    fn test_with_parent() {
        let req = BraidRequest::new()
            .with_parent(Version::new("v1"))
            .with_parent(Version::new("v2"));
        assert_eq!(req.parents.unwrap().len(), 2);
    }

    #[test]
    fn test_with_merge_type() {
        let req = BraidRequest::new().with_merge_type("diamond");
        assert_eq!(req.merge_type, Some("diamond".to_string()));
    }

    #[test]
    fn test_with_peer() {
        let req = BraidRequest::new().with_peer("client-1".to_string());
        assert_eq!(req.peer, Some("client-1".to_string()));
    }
}
