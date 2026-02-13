//! Axum middleware for Braid protocol support.

use super::parse_update::ParsedUpdate;
use super::resource_state::ResourceStateManager;
use crate::core::protocol::{self, constants::headers};
use crate::core::types::{Patch, Version};
use axum::{extract::Request, middleware::Next, response::Response};
use bytes::Bytes;
use futures::StreamExt;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Newtype wrapper indicating Firefox browser detection.
#[derive(Clone, Copy, Debug)]
pub struct IsFirefox(pub bool);

async fn braid_middleware_handler(
    axum::extract::State(state): axum::extract::State<BraidLayer>,
    req: Request,
    next: Next,
) -> Response {
    state.handle_middleware(req, next).await
}

/// Braid protocol state extracted from HTTP request headers and body.
#[derive(Clone, Debug)]
pub struct BraidState {
    pub subscribe: bool,
    pub version: Option<Vec<Version>>,
    pub parents: Option<Vec<Version>>,
    pub peer: Option<String>,
    pub heartbeat: Option<u64>,
    pub merge_type: Option<String>,
    pub content_range: Option<String>,
    pub multiplex_through: Option<String>,
    pub ack: Option<Vec<crate::core::types::Version>>,
    /// Number of patches expected (from Patches header)
    pub patches_count: Option<usize>,
    /// Parsed patches from request body (populated after body parsing)
    pub patches: Vec<Patch>,
    /// Full body if snapshot mode
    pub body: Option<Bytes>,
    pub headers: BTreeMap<String, String>,
}

impl BraidState {
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
            patches_count: None,
            patches: Vec::new(),
            body: None,
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
                    braid_state.ack = protocol::parse_version_header(value_str).ok();
                } else if name_lower == "patches" {
                    braid_state.patches_count = value_str.parse().ok();
                }
            }
        }
        braid_state
    }

    /// Parse request body and populate patches or body field.
    ///
    /// This should be called after `from_headers` to complete the parsing.
    /// It detects the request mode (snapshot, single patch, or multi-patch)
    /// and parses the body accordingly.
    ///
    /// # Errors
    ///
    /// Returns an error if the body cannot be parsed as patches when patches are expected.
    pub fn parse_body(&mut self, body: Bytes) -> crate::core::error::Result<()> {
        let headers = extract_headers_from_btreemap(&self.headers);
        let parsed = ParsedUpdate::from_parts(&headers, body)?;
        
        self.patches = parsed.patches;
        self.body = parsed.body;
        
        Ok(())
    }

    /// Returns true if this request contains patches.
    #[inline]
    #[must_use]
    pub fn is_patched(&self) -> bool {
        !self.patches.is_empty()
    }

    /// Returns true if this request is a snapshot (full body).
    #[inline]
    #[must_use]
    pub fn is_snapshot(&self) -> bool {
        self.body.is_some()
    }
}

/// Helper to convert BTreeMap to HashMap for header extraction.
fn extract_headers_from_btreemap(headers: &BTreeMap<String, String>) -> std::collections::HashMap<String, String> {
    headers.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// Axum middleware layer for Braid protocol support.
#[derive(Clone)]
pub struct BraidLayer {
    config: super::config::ServerConfig,
    pub resource_manager: Arc<ResourceStateManager>,
    pub multiplexer_registry: Arc<super::multiplex::MultiplexerRegistry>,
}

impl BraidLayer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: super::config::ServerConfig::default(),
            resource_manager: Arc::new(ResourceStateManager::new()),
            multiplexer_registry: Arc::new(super::multiplex::MultiplexerRegistry::new()),
        }
    }

    #[must_use]
    pub fn with_config(config: super::config::ServerConfig) -> Self {
        Self {
            config,
            resource_manager: Arc::new(ResourceStateManager::new()),
            multiplexer_registry: Arc::new(super::multiplex::MultiplexerRegistry::new()),
        }
    }

    #[inline]
    #[must_use]
    pub fn config(&self) -> &super::config::ServerConfig {
        &self.config
    }

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
                    while let Some(data) = rx.recv().await { yield Ok::<_, std::io::Error>(axum::body::Bytes::from(data)); }
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
            let parts: Vec<&str> = through.split('/').collect();
            if parts.len() >= 5 && parts[1] == ".well-known" && parts[2] == "multiplexer" {
                let m_id = parts[3];
                let r_id = parts[4];

                if let Some(conn) = m_registry.get(m_id).await {
                    let sender = conn.sender.clone();
                    let r_id = r_id.to_string();
                    let mut cors_headers = axum::http::HeaderMap::new();
                    for (k, v) in response.headers() {
                        if k.as_str().starts_with("access-control-") {
                            cors_headers.insert(k.clone(), v.clone());
                        }
                    }

                    tokio::spawn(async move {
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
                        let _ = sender
                            .send(
                                crate::core::protocol::multiplex::MultiplexEvent::Data(
                                    r_id.clone(),
                                    header_block.clone().into_bytes(),
                                )
                                .to_string()
                                .into_bytes(),
                            )
                            .await;
                        let _ = sender.send(header_block.into_bytes()).await;
                        let mut body_stream = response.into_body().into_data_stream();
                        while let Some(Ok(chunk)) = body_stream.next().await {
                            let _ = sender
                                .send(
                                    crate::core::protocol::multiplex::MultiplexEvent::Data(
                                        r_id.clone(),
                                        chunk.to_vec(),
                                    )
                                    .to_string()
                                    .into_bytes(),
                                )
                                .await;
                            let _ = sender.send(chunk.to_vec()).await;
                        }
                        let _ = sender
                            .send(
                                crate::core::protocol::multiplex::MultiplexEvent::CloseResponse(
                                    r_id,
                                )
                                .to_string()
                                .into_bytes(),
                            )
                            .await;
                    });

                    let mut builder = Response::builder().status(293).header(
                        crate::core::protocol::constants::headers::MULTIPLEX_VERSION,
                        "1.0",
                    );
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
    use axum::http::HeaderMap;

    // ========== BraidState Tests ==========

    #[test]
    fn test_braid_state_parses_all_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("version", "\"v1\"".parse().unwrap());
        headers.insert("parents", "\"v0\"".parse().unwrap());
        headers.insert("subscribe", "true".parse().unwrap());
        headers.insert("peer", "alice".parse().unwrap());
        headers.insert("heartbeats", "5s".parse().unwrap());
        headers.insert("merge-type", "diamond".parse().unwrap());
        headers.insert("patches", "3".parse().unwrap());
        headers.insert("content-range", "json .field".parse().unwrap());

        let state = BraidState::from_headers(&headers);
        
        assert!(state.subscribe);
        assert_eq!(state.version, Some(vec![Version::new("v1")]));
        assert_eq!(state.parents, Some(vec![Version::new("v0")]));
        assert_eq!(state.peer, Some("alice".to_string()));
        assert_eq!(state.heartbeat, Some(5));
        assert_eq!(state.merge_type, Some("diamond".to_string()));
        assert_eq!(state.patches_count, Some(3));
        assert_eq!(state.content_range, Some("json .field".to_string()));
    }

    #[test]
    fn test_braid_state_empty_headers() {
        let headers = HeaderMap::new();
        let state = BraidState::from_headers(&headers);
        
        assert!(!state.subscribe);
        assert!(state.version.is_none());
        assert!(state.parents.is_none());
        assert!(state.peer.is_none());
        assert!(state.heartbeat.is_none());
        assert!(state.merge_type.is_none());
        assert!(state.patches_count.is_none());
        assert!(state.content_range.is_none());
        assert!(state.patches.is_empty());
        assert!(state.body.is_none());
    }

    #[test]
    fn test_braid_state_subscribe_true_variations() {
        for value in ["true", "TRUE", "True"] {
            let mut headers = HeaderMap::new();
            headers.insert("subscribe", value.parse().unwrap());
            let state = BraidState::from_headers(&headers);
            assert!(state.subscribe, "Failed for value: {}", value);
        }
    }

    #[test]
    fn test_braid_state_subscribe_false_variations() {
        for value in ["false", "FALSE", "False", "no", "yes", "1"] {
            let mut headers = HeaderMap::new();
            headers.insert("subscribe", value.parse().unwrap());
            let state = BraidState::from_headers(&headers);
            assert!(!state.subscribe, "Failed for value: {}", value);
        }
    }

    #[test]
    fn test_braid_state_is_patched() {
        let mut state = BraidState::from_headers(&HeaderMap::new());
        assert!(!state.is_patched());
        
        state.patches = vec![Patch::json(".field", "value")];
        assert!(state.is_patched());
    }

    #[test]
    fn test_braid_state_is_snapshot() {
        let mut state = BraidState::from_headers(&HeaderMap::new());
        assert!(!state.is_snapshot());
        
        state.body = Some(Bytes::from("content"));
        assert!(state.is_snapshot());
    }

    #[test]
    fn test_braid_state_parse_body_snapshot() {
        let mut state = BraidState::from_headers(&HeaderMap::new());
        state.parse_body(Bytes::from("test content")).unwrap();
        
        assert!(state.is_snapshot());
        assert_eq!(state.body, Some(Bytes::from("test content")));
        assert!(state.patches.is_empty());
    }

    #[test]
    fn test_braid_state_parse_body_single_patch() {
        // Single patch mode uses Content-Range header
        let mut headers = HeaderMap::new();
        headers.insert("content-range", "json .field".parse().unwrap());
        
        let mut state = BraidState::from_headers(&headers);
        state.parse_body(Bytes::from(r#"{"value": 123}"#)).unwrap();
        
        assert!(state.is_patched());
        assert_eq!(state.patches.len(), 1);
        assert_eq!(state.patches[0].unit, "json");
        assert_eq!(state.patches[0].range, ".field");
    }

    #[test]
    fn test_braid_state_parse_body_snapshot_with_content() {
        // Snapshot mode - no patches or content-range header
        let mut state = BraidState::from_headers(&HeaderMap::new());
        state.parse_body(Bytes::from(r#"{"key": "value"}"#)).unwrap();
        
        assert!(state.is_snapshot());
        assert_eq!(state.body, Some(Bytes::from(r#"{"key": "value"}"#)));
        assert!(state.patches.is_empty());
    }

    #[test]
    fn test_braid_state_parse_body_empty() {
        let mut state = BraidState::from_headers(&HeaderMap::new());
        state.parse_body(Bytes::new()).unwrap();
        
        // Empty body with no patches header results in empty patches, not a snapshot
        assert!(!state.is_snapshot());
        assert!(state.body.is_none());
        assert!(state.patches.is_empty());
    }

    #[test]
    fn test_is_firefox_detection_true() {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/119.0".parse().unwrap());
        
        let is_firefox = headers.get("user-agent")
            .and_then(|v| v.to_str().ok())
            .map(|ua| ua.to_lowercase().contains("firefox"))
            .unwrap_or(false);
        
        assert!(is_firefox);
    }

    #[test]
    fn test_is_firefox_detection_false() {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/119.0.0.0 Safari/537.36".parse().unwrap());
        
        let is_firefox = headers.get("user-agent")
            .and_then(|v| v.to_str().ok())
            .map(|ua| ua.to_lowercase().contains("firefox"))
            .unwrap_or(false);
        
        assert!(!is_firefox);
    }

    // ========== BraidLayer Tests ==========

    #[test]
    fn test_braid_layer_new() {
        let layer = BraidLayer::new();
        assert!(Arc::strong_count(&layer.resource_manager) >= 1);
    }

    #[test]
    fn test_braid_layer_with_config() {
        let config = crate::core::server::ServerConfig::default();
        let layer = BraidLayer::with_config(config);
        assert!(Arc::strong_count(&layer.resource_manager) >= 1);
    }

    #[test]
    fn test_braid_layer_default() {
        let layer: BraidLayer = Default::default();
        assert!(Arc::strong_count(&layer.resource_manager) >= 1);
    }

    #[test]
    fn test_braid_layer_config() {
        let layer = BraidLayer::new();
        let _config = layer.config();
    }

    // ========== Helper Tests ==========

    #[test]
    fn test_extract_headers_from_btreemap() {
        let mut btree = BTreeMap::new();
        btree.insert("version".to_string(), "v1".to_string());
        btree.insert("content-type".to_string(), "application/json".to_string());
        
        let hashmap = super::extract_headers_from_btreemap(&btree);
        assert_eq!(hashmap.get("version"), Some(&"v1".to_string()));
        assert_eq!(hashmap.get("content-type"), Some(&"application/json".to_string()));
    }

    #[test]
    fn test_parse_headers_basic() {
        let result = protocol::parse_version_header("\"v1\", \"v2\"");
        assert_eq!(result.unwrap().len(), 2);
    }
}
