//! Axum middleware for Braid protocol support.
//!
//! Provides an Axum layer that extracts Braid protocol information from incoming
//! requests and makes it available to handlers.
//!
//! # Usage
//!
//! ```ignore
//! use axum::{Router, routing::get};
//! use crate::core::BraidLayer;
//!
//! let app = Router::new()
//!     .route("/resource", get(handler))
//!     .layer(BraidLayer::new().middleware());
//! ```
//!
//! # How It Works
//!
//! The middleware:
//! 1. Extracts Braid headers from incoming requests (Version, Parents, Subscribe, etc.)
//! 2. Parses them into structured Rust types
//! 3. Attaches the `BraidState` to request extensions
//! 4. Handlers can extract `BraidState` to access the information
//!
//! # Specification
//!
//! See draft-toomim-httpbis-braid-http sections 2, 3, and 4 for protocol details.

use super::resource_state::ResourceStateManager;
use crate::core::protocol::{self, constants::headers};
use crate::core::types::Version;
use axum::{extract::Request, middleware::Next, response::Response};
use futures::StreamExt;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Newtype wrapper indicating Firefox browser detection.
///
/// Firefox requires extra newlines in streaming responses to work around
/// network buffering bugs. Handlers can extract this to apply workarounds.
#[derive(Clone, Copy, Debug)]
pub struct IsFirefox(pub bool);

/// Middleware handler function for use with `from_fn_with_state`.
/// This is a free function wrapper around `BraidLayer::handle_middleware`.
async fn braid_middleware_handler(
    axum::extract::State(state): axum::extract::State<BraidLayer>,
    req: Request,
    next: Next,
) -> Response {
    state.handle_middleware(req, next).await
}

/// Braid protocol state extracted from HTTP request headers.
///
/// This immutable state contains all parsed Braid protocol information from the incoming request.
/// It's automatically extracted by the Axum middleware and made available to handlers via
/// request extensions.
///
/// # Fields
///
/// - **subscribe**: Client requested subscription (HTTP 209 streaming)
/// - **version**: Requested version ID(s) from `Version` header (Section 2.3)
/// - **parents**: Parent version(s) from `Parents` header for history (Section 2.4)
/// - **peer**: Peer identifier from `Peer` header (for idempotent updates)
/// - **heartbeat**: Desired heartbeat interval in seconds (Section 4.1)
/// - **merge_type**: Requested conflict resolution strategy (Section 2.2)
/// - **content_range**: Range specification for patch operations (Section 3)
/// - **headers**: Complete set of HTTP headers (keys normalized to lowercase)
///
/// # Examples
///
/// ```ignore
/// use axum::extract::Extension;
/// use crate::core::BraidState;
/// use std::sync::Arc;
///
/// async fn handle_resource(
///     Extension(braid): Extension<Arc<BraidState>>,
/// ) -> String {
///     if let Some(versions) = &braid.version {
///         format!("Client requested version(s): {:?}", versions)
///     } else {
///         "Client requested current version".to_string()
///     }
/// }
/// ```
///
/// # Specification
///
/// All fields correspond to Braid-HTTP draft (draft-toomim-httpbis-braid-http).
/// See the specification for header format and semantics details.
#[derive(Clone, Debug)]
pub struct BraidState {
    /// Whether client explicitly requested subscription via `Subscribe: true`
    pub subscribe: bool,

    /// Parsed `Version` header (list of requested versions or None for current)
    pub version: Option<Vec<Version>>,

    /// Parsed `Parents` header (for retrieving history between versions)
    pub parents: Option<Vec<Version>>,

    /// `Peer` identifier for detecting duplicate requests from the same client
    pub peer: Option<String>,

    /// Heartbeat interval in seconds from `Heartbeats` header
    pub heartbeat: Option<u64>,

    /// Merge type strategy (e.g., "diamond", "sync9", "ot") for conflict resolution
    pub merge_type: Option<String>,

    /// `Content-Range` specification for partial content requests
    pub content_range: Option<String>,

    /// `Multiplex-Through` path for routing requests through a multiplexer
    pub multiplex_through: Option<String>,

    /// Acknowledged versions for reliable delivery
    pub ack: Option<Vec<crate::core::types::Version>>,

    /// Complete HTTP headers map (all keys normalized to lowercase)
    pub headers: BTreeMap<String, String>,
}

impl BraidState {
    /// Parse and create BraidState from HTTP request headers.
    ///
    /// Extracts all recognized Braid protocol headers and stores both parsed
    /// values and the raw header map for inspection.
    #[must_use]
    pub fn from_headers(headers: &axum::http::HeaderMap) -> Self {
        let mut braid_state = BraidState {
            subscribe: false,
            version: None,
            parents: None,
            peer: None,
            heartbeat: None,
            merge_type: None,
            content_range: None,
            multiplex_through: None,
            ack: None,
            headers: BTreeMap::new(),
        };

        for (name, value) in headers.iter() {
            if let Ok(value_str) = value.to_str() {
                let name_lower = name.to_string().to_lowercase();
                braid_state
                    .headers
                    .insert(name_lower.clone(), value_str.to_string());

                if name_lower == headers::SUBSCRIBE.as_str() {
                    braid_state.subscribe = value_str.to_lowercase() == "true";
                } else if name_lower == headers::VERSION.as_str() {
                    braid_state.version = protocol::parse_version_header(value_str).ok();
                } else if name_lower == headers::PARENTS.as_str() {
                    braid_state.parents = protocol::parse_version_header(value_str).ok();
                } else if name_lower == headers::PEER.as_str() {
                    braid_state.peer = Some(value_str.to_string());
                } else if name_lower == headers::HEARTBEATS.as_str() {
                    braid_state.heartbeat = protocol::parse_heartbeat(value_str).ok();
                } else if name_lower == headers::MERGE_TYPE.as_str() {
                    braid_state.merge_type = Some(value_str.to_string());
                } else if name_lower == headers::CONTENT_RANGE.as_str() {
                    braid_state.content_range = Some(value_str.to_string());
                } else if name_lower == protocol::constants::headers::MULTIPLEX_THROUGH.as_str() {
                    braid_state.multiplex_through = Some(value_str.to_string());
                } else if name_lower == "ack" {
                    // TODO: Use constant
                    braid_state.ack = protocol::parse_version_header(value_str).ok();
                }
            }
        }

        braid_state
    }
}

/// Axum middleware layer for Braid protocol support.
///
/// `BraidLayer` processes incoming requests, extracts Braid protocol information,
/// and makes it available to handlers via request extensions. It also manages
/// per-resource CRDT state for collaborative editing features.
///
/// # Features
///
/// - Extracts Braid protocol headers from requests
/// - Manages collaborative document state via Diamond-Types CRDT
/// - Configurable subscription limits and heartbeat intervals
/// - Thread-safe, shareable across async tasks
///
/// # Configuration
///
/// The layer can be customized via `ServerConfig`:
///
/// ```ignore
/// use crate::core::ServerConfig;
///
/// let config = ServerConfig {
///     enable_subscriptions: true,
///     max_subscriptions: 1000,
///     heartbeat_interval: 30,
///     enable_multiplex: false,
/// };
/// ```
///
/// # Usage
///
/// ```ignore
/// use axum::Router;
/// use crate::core::BraidLayer;
///
/// let app = Router::new()
///     .route("/resource", get(handler))
///     .layer(BraidLayer::new().middleware());
/// ```
///
/// # Handlers
///
/// Handlers can extract the Braid state and resource manager:
///
/// ```ignore
/// use axum::extract::Extension;
/// use crate::core::BraidState;
/// use std::sync::Arc;
///
/// async fn handler(
///     Extension(braid): Extension<Arc<BraidState>>,
/// ) -> Response {
///     // Use braid.version, braid.merge_type, etc.
/// }
/// ```
///
/// # Specification
///
/// Implements draft-toomim-httpbis-braid-http sections 2-4.
#[derive(Clone)]
pub struct BraidLayer {
    /// Server configuration for subscriptions, heartbeats, and limits
    config: super::config::ServerConfig,

    /// Shared resource state manager (CRDT instances per resource)
    pub resource_manager: Arc<ResourceStateManager>,

    /// Registry for active multiplexed connections
    pub multiplexer_registry: Arc<super::multiplex::MultiplexerRegistry>,
}

impl BraidLayer {
    /// Create a new Braid layer with default configuration.
    ///
    /// Uses reasonable defaults:
    /// - Subscriptions enabled
    /// - 1000 concurrent subscriptions
    /// - 30-second heartbeat
    /// - No multiplexing
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use crate::core::BraidLayer;
    ///
    /// let layer = BraidLayer::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: super::config::ServerConfig::default(),
            resource_manager: Arc::new(ResourceStateManager::new()),
            multiplexer_registry: Arc::new(super::multiplex::MultiplexerRegistry::new()),
        }
    }

    /// Create a new Braid layer with custom configuration.
    ///
    /// Allows fine-tuning subscription limits, heartbeat intervals,
    /// and other server behavior.
    ///
    /// # Arguments
    ///
    /// * `config` - Custom server configuration
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use crate::core::{BraidLayer, ServerConfig};
    ///
    /// let config = ServerConfig {
    ///     enable_subscriptions: true,
    ///     max_subscriptions: 500,
    ///     ..Default::default()
    /// };
    /// let layer = BraidLayer::with_config(config);
    /// ```
    #[must_use]
    pub fn with_config(config: super::config::ServerConfig) -> Self {
        Self {
            config,
            resource_manager: Arc::new(ResourceStateManager::new()),
            multiplexer_registry: Arc::new(super::multiplex::MultiplexerRegistry::new()),
        }
    }

    /// Get a reference to the layer's configuration.
    ///
    /// # Returns
    ///
    /// A reference to the server configuration used by this layer.
    #[inline]
    #[must_use]
    pub fn config(&self) -> &super::config::ServerConfig {
        &self.config
    }

    /// Create the middleware function for use with Axum.
    ///
    /// Returns a middleware function that extracts Braid protocol information
    /// from request headers and attaches it (and the resource manager) to
    /// request extensions.
    ///
    /// # Returns
    ///
    /// A middleware function compatible with `Router::layer()`.
    #[must_use]
    pub fn middleware(
        &self,
    ) -> impl tower::Layer<
        axum::routing::Route,
        Service = impl tower::Service<
            Request,
            Response = Response,
            Error = std::convert::Infallible,
            Future = impl Send + 'static,
        > + Clone
                      + Send
                      + Sync
                      + 'static,
    > + Clone {
        axum::middleware::from_fn_with_state(self.clone(), braid_middleware_handler)
    }

    async fn handle_middleware(&self, mut req: Request, next: Next) -> Response {
        let resource_manager = self.resource_manager.clone();
        let multiplexer_registry = self.multiplexer_registry.clone();

        // If it's a MULTIPLEX request, we handle it specially
        if req.method().as_str() == "MULTIPLEX" {
            let version = req
                .headers()
                .get(crate::core::protocol::constants::headers::MULTIPLEX_VERSION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("1.0");

            if version == "1.0" {
                let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
                let id = format!("{:x}", rand::random::<u64>());

                multiplexer_registry.add(id.clone(), tx).await;

                let stream = async_stream::stream! {
                    while let Some(data) = rx.recv().await {
                        yield Ok::<_, std::io::Error>(axum::body::Bytes::from(data));
                    }
                };

                let body = axum::body::Body::from_stream(stream);
                return Response::builder()
                    .status(200)
                    .header(
                        crate::core::protocol::constants::headers::MULTIPLEX_VERSION,
                        "1.0",
                    )
                    .body(body)
                    .unwrap();
            }
        }

        let braid_state = BraidState::from_headers(req.headers());
        let multiplex_through = braid_state.multiplex_through.clone();
        let m_registry = multiplexer_registry.clone();

        // Detect Firefox for workaround (extra newlines needed)
        let is_firefox = req
            .headers()
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .map(|ua| ua.to_lowercase().contains("firefox"))
            .unwrap_or(false);

        req.extensions_mut().insert(Arc::new(braid_state));
        req.extensions_mut().insert(resource_manager);
        req.extensions_mut().insert(multiplexer_registry);
        req.extensions_mut().insert(IsFirefox(is_firefox));

        let mut response = next.run(req).await;

        // Add Range-Request-Allow headers (JS braidify line 233-234)
        let headers = response.headers_mut();
        headers.insert(
            axum::http::header::HeaderName::from_static("range-request-allow-methods"),
            axum::http::header::HeaderValue::from_static("PATCH, PUT"),
        );
        headers.insert(
            axum::http::header::HeaderName::from_static("range-request-allow-units"),
            axum::http::header::HeaderValue::from_static("json"),
        );

        if let Some(through) = multiplex_through {
            // "/.well-known/multiplexer/m-id/r-id"
            let parts: Vec<&str> = through.split('/').collect();
            if parts.len() >= 5 && parts[1] == ".well-known" && parts[2] == "multiplexer" {
                let m_id = parts[3];
                let r_id = parts[4];

                if let Some(conn) = m_registry.get(m_id).await {
                    let sender = conn.sender.clone();
                    let r_id = r_id.to_string();

                    // Capture CORS headers to forward in 293 response
                    let mut cors_headers = axum::http::HeaderMap::new();
                    for (k, v) in response.headers() {
                        let k_str = k.as_str();
                        if k_str.starts_with("access-control-") {
                            cors_headers.insert(k.clone(), v.clone());
                        }
                    }

                    tokio::spawn(async move {
                        // 1. Send status and headers as a Data event
                        let mut header_block =
                            format!(":status: {}\r\n", response.status().as_u16());
                        for (name, value) in response.headers() {
                            header_block.push_str(&format!(
                                "{}: {}\r\n",
                                name,
                                value.to_str().unwrap_or("")
                            ));
                        }
                        header_block.push_str("\r\n");

                        let start_evt = crate::core::protocol::multiplex::MultiplexEvent::Data(
                            r_id.clone(),
                            header_block.clone().into_bytes(),
                        );
                        let _ = sender.send(start_evt.to_string().into_bytes()).await;
                        let _ = sender.send(header_block.into_bytes()).await;

                        // 2. Stream body as Data events
                        let mut body_stream = response.into_body().into_data_stream();
                        while let Some(Ok(chunk)) = body_stream.next().await {
                            let data_evt = crate::core::protocol::multiplex::MultiplexEvent::Data(
                                r_id.clone(),
                                chunk.to_vec(),
                            );
                            let _ = sender.send(data_evt.to_string().into_bytes()).await;
                            let _ = sender.send(chunk.to_vec()).await;
                        }

                        // 3. Close response
                        let close_evt =
                            crate::core::protocol::multiplex::MultiplexEvent::CloseResponse(r_id);
                        let _ = sender.send(close_evt.to_string().into_bytes()).await;
                    });

                    let mut builder = Response::builder().status(293).header(
                        crate::core::protocol::constants::headers::MULTIPLEX_VERSION,
                        "1.0",
                    );

                    // Add forwarded CORS headers
                    if let Some(headers) = builder.headers_mut() {
                        headers.extend(cors_headers);
                    }

                    return builder.body(axum::body::Body::empty()).unwrap();
                }
            }
        }

        response
    }
}

impl Default for BraidLayer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_header() {
        let result = protocol::parse_version_header("\"v1\", \"v2\", \"v3\"");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[test]
    fn test_parse_heartbeat() {
        assert!(protocol::parse_heartbeat("30s").is_ok());
        assert!(protocol::parse_heartbeat("30").is_ok());
        if let Ok(val) = protocol::parse_heartbeat("30s") {
            assert_eq!(val, 30);
        }
    }
}
