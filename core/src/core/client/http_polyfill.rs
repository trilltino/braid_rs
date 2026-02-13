//! HTTP polyfill for Node.js-style HTTP client support.
//!
//! This module provides a polyfill for Node.js's `http` module with Braid protocol support.
//! It adds Braid-specific features to standard HTTP requests.
//!
//! # Usage
//!
//! ```ignore
//! use braid_http_rs::core::client::http_polyfill::HttpPolyfill;
//!
//! // Create polyfilled HTTP client
//! let http = HttpPolyfill::new();
//!
//! // Make Braid-enabled GET request
//! let response = http.get("http://example.com/resource", |res| {
//!     res.on_update(|update| {
//!         println!("Update: {:?}", update);
//!     });
//! });
//! ```
//!
//! # JS Equivalent
//!
//! ```javascript
//! const http = require('http')
//! const {http: braidify_http} = require('braid-http')
//! braidify_http(http)
//!
//! http.get(url, {subscribe: true}, (res) => {
//!   res.on('update', (update) => { ... })
//! })
//! ```

use crate::core::error::Result;
use crate::core::types::{BraidRequest, BraidResponse, Update};
use std::sync::Arc;

/// Callback type for update events (JS: res.on('update', cb)).
pub type UpdateCallback = Arc<dyn Fn(Update) + Send + Sync>;

/// Callback type for error events (JS: res.on('error', cb)).
pub type ErrorCallback = Arc<dyn Fn(crate::core::error::BraidError) + Send + Sync>;

/// Polyfilled HTTP response with Braid support.
///
/// This wraps a standard HTTP response and adds Braid-specific event handlers
/// for subscription updates.
#[derive(Clone)]
pub struct PolyfillResponse {
    inner: BraidResponse,
    update_callback: Option<UpdateCallback>,
    error_callback: Option<ErrorCallback>,
}

impl std::fmt::Debug for PolyfillResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PolyfillResponse")
            .field("inner", &self.inner)
            .field("has_update_callback", &self.update_callback.is_some())
            .field("has_error_callback", &self.error_callback.is_some())
            .finish()
    }
}

impl PolyfillResponse {
    pub fn new(inner: BraidResponse) -> Self {
        Self {
            inner,
            update_callback: None,
            error_callback: None,
        }
    }

    /// Register update callback (JS: res.on('update', cb)).
    ///
    /// Called when a new update is received during a subscription.
    pub fn on_update<F>(&mut self, callback: F)
    where
        F: Fn(Update) + Send + Sync + 'static,
    {
        self.update_callback = Some(Arc::new(callback));
    }

    /// Register error callback (JS: res.on('error', cb)).
    ///
    /// Called when an error occurs during streaming.
    pub fn on_error<F>(&mut self, callback: F)
    where
        F: Fn(crate::core::error::BraidError) + Send + Sync + 'static,
    {
        self.error_callback = Some(Arc::new(callback));
    }

    /// Get the inner response.
    pub fn inner(&self) -> &BraidResponse {
        &self.inner
    }

    /// Notify update callback.
    pub fn notify_update(&self, update: Update) {
        if let Some(ref cb) = self.update_callback {
            cb(update);
        }
    }

    /// Notify error callback.
    pub fn notify_error(&self, error: crate::core::error::BraidError) {
        if let Some(ref cb) = self.error_callback {
            cb(error);
        }
    }
}

impl std::ops::Deref for PolyfillResponse {
    type Target = BraidResponse;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// HTTP polyfill with Braid protocol support.
///
/// Wraps a BraidClient and provides a Node.js-style HTTP API.
pub struct HttpPolyfill {
    client: Arc<crate::core::client::BraidClient>,
}

impl HttpPolyfill {
    /// Create a new HTTP polyfill with default client.
    pub fn new() -> Result<Self> {
        let client = Arc::new(crate::core::client::BraidClient::new()?);
        Ok(Self { client })
    }

    /// Create with an existing client.
    pub fn with_client(client: Arc<crate::core::client::BraidClient>) -> Self {
        Self { client }
    }

    /// Make a GET request with Braid support (JS: http.get).
    ///
    /// # Arguments
    ///
    /// * `url` - Request URL
    /// * `options` - Request options (can include `subscribe: true`)
    /// * `callback` - Called with the response
    ///
    /// # Example
    ///
    /// ```ignore
    /// http.get("http://example.com/resource", |res| {
    ///     println!("Status: {}", res.status);
    /// });
    ///
    /// // With subscription
    /// http.get("http://example.com/resource",
    ///     BraidRequest::new().subscribe(),
    ///     |res| {
    ///         res.on_update(|update| {
    ///             println!("Update: {:?}", update);
    ///         });
    ///     }
    /// );
    /// ```
    pub async fn get<F>(
        &self,
        url: &str,
        options: impl Into<Option<BraidRequest>>,
        callback: F,
    ) -> Result<()>
    where
        F: FnOnce(PolyfillResponse) + Send + 'static,
    {
        let request = options.into().unwrap_or_default();

        if request.subscribe {
            // Subscription request - set up streaming
            let mut subscription = self.client.subscribe(url, request).await?;

            // Create initial response (status 209)
            let initial_response = BraidResponse::new(209, bytes::Bytes::new());
            let polyfill_response = Arc::new(std::sync::Mutex::new(PolyfillResponse::new(
                initial_response,
            )));

            // Clone for the task
            let response_for_task = polyfill_response.clone();

            // Spawn task to handle updates
            tokio::spawn(async move {
                while let Some(result) = subscription.next().await {
                    match result {
                        Ok(update) => {
                            if let Ok(res) = response_for_task.lock() {
                                res.notify_update(update);
                            }
                        }
                        Err(e) => {
                            if let Ok(res) = response_for_task.lock() {
                                res.notify_error(e);
                            }
                            break;
                        }
                    }
                }
            });

            // Get the response back from Arc<Mutex<_>>
            let response = polyfill_response.lock().unwrap().clone();
            callback(response);
        } else {
            // Regular request
            let response = self.client.get(url).await?;
            callback(PolyfillResponse::new(response));
        }

        Ok(())
    }

    /// Make a PUT request with Braid support.
    pub async fn put(&self, url: &str, body: &str, request: BraidRequest) -> Result<BraidResponse> {
        self.client.put(url, body, request).await
    }

    /// Make a POST request with Braid support.
    pub async fn post(
        &self,
        url: &str,
        body: &str,
        request: BraidRequest,
    ) -> Result<BraidResponse> {
        self.client.post(url, body, request).await
    }

    /// Make a PATCH request with Braid support.
    pub async fn patch(
        &self,
        url: &str,
        patches: Vec<crate::core::types::Patch>,
        request: BraidRequest,
    ) -> Result<BraidResponse> {
        let request = request.with_patches(patches);
        self.client.fetch(url, request).await
    }
}

impl Default for HttpPolyfill {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| {
            let client = Arc::new(crate::core::client::BraidClient::default());
            Self { client }
        })
    }
}

/// Extension trait for reqwest Response to add Braid event handlers.
///
/// This provides the `on()` method for event-based response handling.
pub trait PolyfillResponseExt {
    /// Register an event handler.
    ///
    /// Supported events:
    /// - "update" - Called when a new update is received
    /// - "error" - Called when an error occurs
    fn on<F>(&mut self, event: &str, callback: F)
    where
        F: Fn(Update) + Send + Sync + 'static;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_polyfill_response() {
        let response = BraidResponse::new(200, "test");
        let mut polyfill = PolyfillResponse::new(response);

        let received = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let received_clone = received.clone();

        polyfill.on_update(move |_| {
            received_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        // Simulate update
        let update =
            crate::core::types::Update::snapshot(crate::core::types::Version::new("v1"), "data");
        polyfill.notify_update(update);

        assert!(received.load(std::sync::atomic::Ordering::SeqCst));
    }
}
