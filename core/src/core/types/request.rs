//! Braid-specific request parameters.

use crate::core::types::{Patch, Version};
use std::sync::Arc;

/// Callback type for dynamic parent resolution.
/// 
/// This is called before each retry to get the latest parent versions.
/// It's used for retry safety - ensuring that even if the request fails,
/// we retry with the most up-to-date parent information.
pub type ParentsCallback = Arc<dyn Fn() -> Vec<Version> + Send + Sync>;

/// Braid-specific request parameters.
#[derive(Clone)]
pub struct BraidRequest {
    pub version: Option<Vec<Version>>,
    /// Parents can be either a static list or a callback function.
    /// Use `with_parents()` for static list, `with_parents_fn()` for callback.
    pub parents: Option<Vec<Version>>,
    /// Callback for dynamic parent resolution (JS: parents as function)
    pub parents_fn: Option<ParentsCallback>,
    pub subscribe: bool,
    pub patches: Option<Vec<Patch>>,
    pub heartbeat_interval: Option<u64>,
    pub peer: Option<String>,
    pub ack: Option<Vec<Version>>,
    pub enable_multiplex: bool,
    pub merge_type: Option<String>,
    pub content_type: Option<String>,
    pub method: String,
    pub body: bytes::Bytes,
    pub extra_headers: std::collections::BTreeMap<String, String>,
    pub retry: Option<crate::core::client::retry::RetryConfig>,
    /// Pre-fetch callback (JS: onFetch)
    pub on_fetch: Option<Arc<dyn Fn(&str, &BraidRequest) + Send + Sync>>,
    /// Bytes received callback for progress tracking (JS: onBytes)
    pub on_bytes: Option<Arc<dyn Fn(&[u8]) + Send + Sync>>,
}

impl std::fmt::Debug for BraidRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BraidRequest")
            .field("version", &self.version)
            .field("parents", &self.parents)
            .field("has_parents_fn", &self.parents_fn.is_some())
            .field("subscribe", &self.subscribe)
            .field("patches", &self.patches)
            .field("heartbeat_interval", &self.heartbeat_interval)
            .field("peer", &self.peer)
            .field("ack", &self.ack)
            .field("enable_multiplex", &self.enable_multiplex)
            .field("merge_type", &self.merge_type)
            .field("content_type", &self.content_type)
            .field("method", &self.method)
            .field("body_len", &self.body.len())
            .field("extra_headers", &self.extra_headers)
            .field("retry", &self.retry)
            .field("has_on_fetch", &self.on_fetch.is_some())
            .field("has_on_bytes", &self.on_bytes.is_some())
            .finish()
    }
}

impl Default for BraidRequest {
    fn default() -> Self {
        Self {
            version: None,
            parents: None,
            parents_fn: None,
            subscribe: false,
            patches: None,
            heartbeat_interval: None,
            peer: None,
            ack: None,
            enable_multiplex: false,
            merge_type: None,
            content_type: None,
            method: "GET".to_string(),
            body: bytes::Bytes::new(),
            extra_headers: std::collections::BTreeMap::new(),
            retry: None,
            on_fetch: None,
            on_bytes: None,
        }
    }
}

impl BraidRequest {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(mut self) -> Self {
        self.subscribe = true;
        self
    }

    #[inline]
    pub fn is_subscription(&self) -> bool {
        self.subscribe
    }

    pub fn with_version(mut self, version: Version) -> Self {
        self.version.get_or_insert_with(Vec::new).push(version);
        self
    }

    pub fn with_versions(mut self, versions: Vec<Version>) -> Self {
        self.version = Some(versions);
        self
    }

    pub fn with_parent(mut self, version: Version) -> Self {
        self.parents.get_or_insert_with(Vec::new).push(version);
        self
    }

    pub fn with_peer(mut self, peer: impl Into<String>) -> Self {
        self.peer = Some(peer.into());
        self
    }

    pub fn with_ack(mut self, version: Version) -> Self {
        self.ack.get_or_insert_with(Vec::new).push(version);
        self
    }

    pub fn with_parents(mut self, parents: Vec<Version>) -> Self {
        self.parents = Some(parents);
        self
    }

    /// Set parents as a callback function (JS equivalent: parents as function).
    /// 
    /// This is called before each retry to get the latest parent versions.
    /// It's used for retry safety - ensuring that even if the request fails,
    /// we retry with the most up-to-date parent information.
    /// 
    /// # Example
    /// 
    /// ```ignore
    /// let request = BraidRequest::new()
    ///     .with_parents_fn(|| {
    ///         // Get latest parents from local state
    ///         vec![Version::new("latest")]
    ///     });
    /// ```
    pub fn with_parents_fn<F>(mut self, callback: F) -> Self
    where
        F: Fn() -> Vec<Version> + Send + Sync + 'static,
    {
        self.parents_fn = Some(Arc::new(callback));
        self
    }

    /// Get the current parents, either from static list or by calling the function.
    /// 
    /// This should be called before each request attempt to get the latest parents.
    pub fn resolve_parents(&self) -> Option<Vec<Version>> {
        if let Some(ref callback) = self.parents_fn {
            Some(callback())
        } else {
            self.parents.clone()
        }
    }

    /// Returns true if parents are provided (either static or function).
    pub fn has_parents(&self) -> bool {
        self.parents.is_some() || self.parents_fn.is_some()
    }

    pub fn with_patches(mut self, patches: Vec<Patch>) -> Self {
        self.patches = Some(patches);
        self
    }

    #[inline]
    pub fn has_patches(&self) -> bool {
        self.patches.as_ref().is_some_and(|p| !p.is_empty())
    }

    pub fn with_heartbeat(mut self, seconds: u64) -> Self {
        self.heartbeat_interval = Some(seconds);
        self
    }

    pub fn with_multiplex(mut self, enable: bool) -> Self {
        self.enable_multiplex = enable;
        self
    }

    pub fn with_merge_type(mut self, merge_type: impl Into<String>) -> Self {
        self.merge_type = Some(merge_type.into());
        self
    }

    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    pub fn with_method(mut self, method: impl Into<String>) -> Self {
        self.method = method.into();
        self
    }

    pub fn with_body(mut self, body: impl Into<bytes::Bytes>) -> Self {
        self.body = body.into();
        self
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_headers.insert(key.into(), value.into());
        self
    }

    pub fn with_retry(mut self, config: crate::core::client::retry::RetryConfig) -> Self {
        self.retry = Some(config);
        self
    }

    pub fn retry(self) -> Self {
        self.with_retry(crate::core::client::retry::RetryConfig::default())
    }

    /// Set pre-fetch callback (JS equivalent: onFetch).
    /// 
    /// Called before each fetch with the URL and request parameters.
    /// Useful for logging, debugging, or request modification.
    pub fn with_on_fetch<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str, &BraidRequest) + Send + Sync + 'static,
    {
        self.on_fetch = Some(Arc::new(callback));
        self
    }

    /// Set bytes received callback (JS equivalent: onBytes).
    /// 
    /// Called when bytes are received during streaming responses.
    /// Useful for progress tracking.
    pub fn with_on_bytes<F>(mut self, callback: F) -> Self
    where
        F: Fn(&[u8]) + Send + Sync + 'static,
    {
        self.on_bytes = Some(Arc::new(callback));
        self
    }

    /// Notify the on_fetch callback if set.
    pub fn notify_on_fetch(&self, url: &str) {
        if let Some(ref callback) = self.on_fetch {
            callback(url, self);
        }
    }

    /// Notify the on_bytes callback if set.
    pub fn notify_on_bytes(&self, bytes: &[u8]) {
        if let Some(ref callback) = self.on_bytes {
            callback(bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_parents_as_function() {
        let req = BraidRequest::new()
            .with_parents_fn(|| vec![Version::new("dynamic")]);

        assert!(req.has_parents());
        assert!(req.parents_fn.is_some());
        
        let parents = req.resolve_parents();
        assert_eq!(parents.unwrap()[0].to_string(), "dynamic");
    }

    #[test]
    fn test_parents_static_vs_function() {
        // Static parents
        let req1 = BraidRequest::new()
            .with_parents(vec![Version::new("static")]);
        assert!(req1.has_parents());
        assert_eq!(req1.resolve_parents().unwrap()[0].to_string(), "static");

        // Function parents
        let req2 = BraidRequest::new()
            .with_parents_fn(|| vec![Version::new("function")]);
        assert!(req2.has_parents());
        assert_eq!(req2.resolve_parents().unwrap()[0].to_string(), "function");
    }

    #[test]
    fn test_callbacks() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let req = BraidRequest::new()
            .with_on_fetch(move |_, _| {
                call_count_clone.fetch_add(1, Ordering::SeqCst);
            });

        req.notify_on_fetch("http://example.com");
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }
}
