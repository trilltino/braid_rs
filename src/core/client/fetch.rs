//! Main Braid HTTP client implementation.
//!
//! Provides the primary `BraidClient` for making requests with Braid protocol support.

use crate::core::client::{config::ClientConfig, MessageParser};
use crate::core::error::{BraidError, Result};
use crate::core::protocol;
use crate::core::types::{BraidRequest, BraidResponse, Version};
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
// use tokio::time::sleep; // Replaced by utils::sleep

/// The main Braid HTTP client
#[derive(Clone)]
pub struct BraidClient {
    client: reqwest::Client,
    config: Arc<ClientConfig>,
    /// Active multiplexers by origin.
    #[cfg(not(target_arch = "wasm32"))]
    multiplexers: Arc<
        tokio::sync::Mutex<
            std::collections::HashMap<String, Arc<crate::core::client::multiplex::Multiplexer>>,
        >,
    >,
}

impl BraidClient {
    /// Get the underlying reqwest client
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Create a new Braid client with default configuration
    pub fn new() -> Self {
        Self::with_config(ClientConfig::default())
    }

    /// Create a new Braid client with custom configuration
    pub fn with_config(config: ClientConfig) -> Self {
        #[allow(unused_mut)]
        let mut builder = reqwest::Client::builder();

        #[cfg(not(target_arch = "wasm32"))]
        {
            builder = builder
                .timeout(std::time::Duration::from_millis(config.request_timeout_ms))
                .pool_idle_timeout(std::time::Duration::from_secs(90))
                .pool_max_idle_per_host(config.max_total_connections as usize);
        }

        #[cfg(not(target_arch = "wasm32"))]
        if !config.proxy_url.is_empty() {
            if let Ok(proxy) = reqwest::Proxy::all(&config.proxy_url) {
                builder = builder.proxy(proxy);
            }
        }

        let client = builder.build().unwrap_or_default();

        BraidClient {
            client,
            config: Arc::new(config),
            #[cfg(not(target_arch = "wasm32"))]
            multiplexers: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Create a new Braid client wrapping an existing reqwest Client
    pub fn with_client(client: reqwest::Client) -> Self {
        BraidClient {
            client,
            config: Arc::new(ClientConfig::default()),
            #[cfg(not(target_arch = "wasm32"))]
            multiplexers: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Make a simple GET request
    pub async fn get(&self, url: &str) -> Result<BraidResponse> {
        self.fetch(url, BraidRequest::new()).await
    }

    /// Make a Braid PUT request to create or update a versioned resource.
    ///
    /// This sends a PUT request with proper Braid protocol headers:
    /// - `Version`: The version being created (auto-generated if not provided)
    /// - `Parents`: The parent version(s) (optional)
    /// - `Content-Type`: Set to "application/json"
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::{BraidClient, BraidRequest};
    ///
    /// let client = BraidClient::new();
    /// let request = BraidRequest::new();
    /// let body = r#"{"title": "Hello"}"#;
    /// // let response = client.put("http://example.com/post/123", body, request).await?;
    /// ```
    pub async fn put(
        &self,
        url: &str,
        body: &str,
        mut request: BraidRequest,
    ) -> Result<BraidResponse> {
        request = request.with_method("PUT").with_body(body.to_string());

        // Ensure Content-Type is set for PUT requests
        if request.content_type.is_none() {
            request = request.with_content_type("application/json");
        }

        // Auto-generate version if not provided
        if request.version.is_none() {
            let random_version = uuid::Uuid::new_v4().to_string();
            request.version = Some(vec![crate::core::types::Version::new(&random_version)]);
        }

        self.fetch(url, request).await
    }

    /// Make a Braid POST request.
    pub async fn post(
        &self,
        url: &str,
        body: &str,
        mut request: BraidRequest,
    ) -> Result<BraidResponse> {
        request = request.with_method("POST").with_body(body.to_string());
        self.fetch(url, request).await
    }

    /// Send a Braid "poke" notification.
    ///
    /// Notifies a recipient (or their server) that a new post exists at the given URL.
    /// Based on the Braid Mail spec: POST /poke {url}
    pub async fn poke(&self, recipient_endpoint: &str, post_url: &str) -> Result<BraidResponse> {
        let request = BraidRequest::new()
            .with_method("POST")
            .with_body(post_url.to_string())
            .with_content_type("text/plain");

        self.fetch(recipient_endpoint, request).await
    }

    /// Make a Braid protocol request
    pub async fn fetch(&self, url: &str, request: BraidRequest) -> Result<BraidResponse> {
        self.fetch_with_retries(url, request, 0).await
    }

    /// Subscribe to streaming updates
    pub async fn subscribe(
        &self,
        url: &str,
        mut request: BraidRequest,
    ) -> Result<crate::core::client::Subscription> {
        request.subscribe = true;

        let mut req_builder = self.client.get(url);
        req_builder = req_builder.header(
            crate::core::protocol::constants::headers::SUBSCRIBE.as_str(),
            "true",
        );

        // Disable compression for streaming to avoid decoding errors
        req_builder = req_builder.header(reqwest::header::ACCEPT_ENCODING, "identity");

        // Disable timeout for long-lived subscriptions (24 hours)
        #[cfg(not(target_arch = "wasm32"))]
        {
            req_builder = req_builder.timeout(std::time::Duration::from_secs(86400));
        }

        // Apply headers from request object (fix for missing Authorization)
        for (k, v) in &request.extra_headers {
            req_builder = req_builder.header(k, v);
        }

        if let Some(versions) = &request.version {
            req_builder = req_builder.header(
                crate::core::protocol::constants::headers::VERSION.as_str(),
                crate::core::protocol::format_version_header(versions),
            );
        }
        if let Some(parents) = &request.parents {
            req_builder = req_builder.header(
                crate::core::protocol::constants::headers::PARENTS.as_str(),
                crate::core::protocol::format_version_header(parents),
            );
        }
        if let Some(peer) = &request.peer {
            req_builder = req_builder.header(
                crate::core::protocol::constants::headers::PEER.as_str(),
                peer,
            );
        }
        if let Some(merge_type) = &request.merge_type {
            req_builder = req_builder.header(
                crate::core::protocol::constants::headers::MERGE_TYPE.as_str(),
                merge_type,
            );
        }

        let origin = url.split('/').take(3).collect::<Vec<_>>().join("/");

        #[cfg(not(target_arch = "wasm32"))]
        {
            let mplexers = self.multiplexers.lock().await;
            if let Some(mplex) = mplexers.get(&origin) {
                // Use multiplexer for this request
                let (raw_tx, raw_rx) = async_channel::bounded(100);
                let request_id = uuid::Uuid::new_v4().to_string();
                mplex.add_request(request_id.clone(), raw_tx).await;

                // Construct a virtual subscription
                let (tx, rx) = async_channel::bounded(100);
                crate::core::client::utils::spawn_task(async move {
                    while let Ok(data) = raw_rx.recv().await {
                        let mut update = crate::core::types::Update::default();
                        update.body = Some(data.into());
                        let _ = tx.send(Ok(update)).await;
                    }
                });
                return Ok(crate::core::client::Subscription::new(rx));
            }
        }

        let response = req_builder
            .send()
            .await
            .map_err(|e| BraidError::Http(e.to_string()))?;

        // Accept both HTTP 200 (regular response) and HTTP 209 (Braid subscription)
        let status = response.status().as_u16();
        if status == 410 {
            return Err(BraidError::HistoryDropped);
        }
        if status != 200 && status != 209 {
            return Err(BraidError::InvalidSubscriptionStatus(status));
        }

        fn extract_version(
            headers: &std::collections::BTreeMap<String, String>,
        ) -> Option<Version> {
            headers
                .get("current-version")
                .or_else(|| headers.get("version"))
                .and_then(|v| protocol::parse_version_header(v).ok())
                .and_then(|mut v| v.pop())
        }
        // Parse heartbeats header if present (JS: res.headers.get('heartbeats'))
        let heartbeat_config = response
            .headers()
            .get("heartbeats")
            .and_then(|v| v.to_str().ok())
            .and_then(crate::core::client::HeartbeatConfig::from_header);

        let (tx, rx) = async_channel::bounded(100);
        if status == 209 {
            // HTTP 209: Streaming subscription
            crate::core::client::utils::spawn_task(async move {
                let mut parser = MessageParser::new();
                let mut stream = response.bytes_stream();

                while let Some(chunk_res) = stream.next().await {
                    match chunk_res {
                        Ok(chunk) => {
                            match parser.feed(&chunk) {
                                Ok(messages) => {
                                    for msg in messages {
                                        let update =
                                            crate::core::client::utils::message_to_update(msg);
                                        if tx.send(Ok(update)).await.is_err() {
                                            return; // Receiver dropped
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = tx.send(Err(e)).await;
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(BraidError::Http(e.to_string()))).await;
                            return;
                        }
                    }
                }
            });
        } else {
            // HTTP 200: Regular response (treat as single snapshot update)
            // Extract version from headers (prefer Current-Version, fallback to Version)
            let version_str = response
                .headers()
                .get("current-version")
                .or_else(|| response.headers().get("version"))
                .and_then(|v| v.to_str().ok())
                .map(|v| v.trim_matches('"').to_string())
                .unwrap_or_else(|| "latest".to_string());

            tracing::debug!("[BRAID-CLIENT] 200 response with version: {}", version_str);

            crate::core::client::utils::spawn_task(async move {
                match response.bytes().await {
                    Ok(body) => {
                        tracing::debug!("[BRAID-CLIENT] Received body: {} bytes", body.len());
                        let update = crate::core::types::Update::snapshot(
                            crate::core::types::Version::new(&version_str),
                            body,
                        );

                        let _ = tx.send(Ok(update)).await;
                    }
                    Err(e) => {
                        let _ = tx.send(Err(BraidError::Http(e.to_string()))).await;
                    }
                }
            });
        }

        // Create subscription with or without heartbeat config
        Ok(if let Some(config) = heartbeat_config {
            crate::core::client::Subscription::with_heartbeat(rx, config)
        } else {
            crate::core::client::Subscription::new(rx)
        })
    }

    /// Internal fetch with retry logic using the new RetryState system.
    ///
    /// If the request has a `retry` config, uses that. Otherwise falls back
    /// to the client's default retry behavior based on `max_retries`.
    async fn fetch_with_retries(
        &self,
        url: &str,
        request: BraidRequest,
        _attempt: u32,
    ) -> Result<BraidResponse> {
        // Determine retry config: use request's config if provided, else create from client config
        let retry_config = request.retry.clone().unwrap_or_else(|| {
            if self.config.max_retries == 0 {
                crate::core::client::retry::RetryConfig::no_retry()
            } else {
                crate::core::client::retry::RetryConfig::default()
                    .with_max_retries(self.config.max_retries)
                    .with_initial_backoff(std::time::Duration::from_millis(
                        self.config.retry_delay_ms,
                    ))
            }
        });

        let mut retry_state = crate::core::client::retry::RetryState::new(retry_config);

        loop {
            match self.fetch_internal(url, &request).await {
                Ok(response) => {
                    // Check if server returned a retryable status code
                    let status = response.status;
                    if (400..600).contains(&status) {
                        // Parse Retry-After header if present
                        let retry_after = response
                            .headers
                            .get("retry-after")
                            .and_then(|v| crate::core::client::retry::parse_retry_after(v));

                        // Check if this status should be retried
                        match retry_state.should_retry_status(status, retry_after) {
                            crate::core::client::retry::RetryDecision::Retry(delay) => {
                                if self.config.enable_logging {
                                    tracing::warn!(
                                        "Request returned {} (attempt {}), retrying after {:?}",
                                        status,
                                        retry_state.attempts,
                                        delay
                                    );
                                }
                                crate::core::client::utils::sleep(delay).await;
                                continue;
                            }
                            crate::core::client::retry::RetryDecision::DontRetry => {
                                return Ok(response);
                            }
                        }
                    }
                    // Success - reset retry state and return
                    retry_state.reset();
                    return Ok(response);
                }
                Err(e) => {
                    // Check if this is an abort error (don't retry)
                    let is_abort = matches!(&e, BraidError::Aborted);

                    match retry_state.should_retry_error(is_abort) {
                        crate::core::client::retry::RetryDecision::Retry(delay) => {
                            if self.config.enable_logging {
                                tracing::warn!(
                                    "Request failed (attempt {}), retrying after {:?}: {}",
                                    retry_state.attempts,
                                    delay,
                                    e
                                );
                            }
                            crate::core::client::utils::sleep(delay).await;
                            continue;
                        }
                        crate::core::client::retry::RetryDecision::DontRetry => {
                            return Err(e);
                        }
                    }
                }
            }
        }
    }

    /// Internal fetch implementation
    async fn fetch_internal(&self, url: &str, request: &BraidRequest) -> Result<BraidResponse> {
        let method = match request.method.to_uppercase().as_str() {
            "POST" => reqwest::Method::POST,
            "PUT" => reqwest::Method::PUT,
            "DELETE" => reqwest::Method::DELETE,
            "PATCH" => reqwest::Method::PATCH,
            _ => reqwest::Method::GET,
        };

        let mut req_builder = self.client.request(method, url);

        for (k, v) in &request.extra_headers {
            req_builder = req_builder.header(k, v);
        }

        // Always add application/json for Braid-HTTP requests with bodies
        if !request.body.is_empty() {
            req_builder = req_builder.header(reqwest::header::CONTENT_TYPE, "application/json");
        }

        if let Some(versions) = &request.version {
            req_builder = req_builder.header(
                crate::core::protocol::constants::headers::VERSION.as_str(),
                crate::core::protocol::format_version_header(versions),
            );
        }
        if let Some(parents) = &request.parents {
            req_builder = req_builder.header(
                crate::core::protocol::constants::headers::PARENTS.as_str(),
                crate::core::protocol::format_version_header(parents),
            );
        }
        if request.subscribe {
            req_builder = req_builder.header(
                crate::core::protocol::constants::headers::SUBSCRIBE.as_str(),
                "true",
            );
        }
        if let Some(peer) = &request.peer {
            req_builder = req_builder.header(
                crate::core::protocol::constants::headers::PEER.as_str(),
                peer,
            );
        }
        if let Some(ack) = &request.ack {
            req_builder = req_builder.header(
                "ack", // TODO: Use constant
                crate::core::protocol::format_version_header(ack),
            );
        }
        if let Some(merge_type) = &request.merge_type {
            req_builder = req_builder.header(
                crate::core::protocol::constants::headers::MERGE_TYPE.as_str(),
                merge_type,
            );
        }

        if !request.body.is_empty() {
            req_builder = req_builder.body(request.body.clone());
        }

        let response = req_builder
            .send()
            .await
            .map_err(|e| BraidError::Http(e.to_string()))?;

        let status = response.status().as_u16();

        let mut headers = std::collections::BTreeMap::new();
        for (k, v) in response.headers() {
            if let Ok(val) = v.to_str() {
                headers.insert(k.as_str().to_string(), val.to_string());
            }
        }

        let body = response
            .bytes()
            .await
            .map_err(|e| BraidError::Http(e.to_string()))?;

        Ok(BraidResponse {
            status,
            headers,
            body,
            is_subscription: status == 209,
        })
    }

    /// Make a multiplexed Braid protocol request.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn fetch_multiplexed(
        &self,
        url: &str,
        mut request: BraidRequest,
    ) -> Result<BraidResponse> {
        let parsed_url = url::Url::parse(url).map_err(|e| BraidError::Config(e.to_string()))?;
        let origin = format!(
            "{}://{}",
            parsed_url.scheme(),
            parsed_url.host_str().unwrap_or("")
        );

        let mut multiplexers = self.multiplexers.lock().await;
        let multiplexer = if let Some(m) = multiplexers.get(&origin) {
            m.clone()
        } else {
            let multiplex_url = format!("{}/.multiplex", origin);
            let m_id = format!("{:x}", rand::random::<u64>());
            let m = Arc::new(crate::core::client::multiplex::Multiplexer::new(
                origin.clone(),
                m_id,
            ));

            let client = self.clone();
            let m_inner = m.clone();
            crate::core::client::utils::spawn_task(async move {
                let req = client
                    .client
                    .request(
                        reqwest::Method::from_bytes(b"MULTIPLEX").unwrap(),
                        &multiplex_url,
                    )
                    .header(
                        reqwest::header::HeaderName::from_bytes(
                            crate::core::protocol::constants::headers::MULTIPLEX_VERSION
                                .as_str()
                                .as_bytes(),
                        )
                        .unwrap(),
                        "1.0",
                    )
                    .send()
                    .await;

                if let Ok(resp) = req {
                    let _ = m_inner.run_stream(resp).await;
                }
            });

            multiplexers.insert(origin.clone(), m.clone());
            m
        };
        drop(multiplexers);

        let r_id = format!("{:x}", rand::random::<u32>());
        let (tx, rx) = async_channel::bounded(100);
        multiplexer.add_request(r_id.clone(), tx).await;

        request.extra_headers.insert(
            crate::core::protocol::constants::headers::MULTIPLEX_THROUGH.to_string(),
            format!("/.well-known/multiplexer/{}/{}", multiplexer.id, r_id),
        );

        let initial_response = self.fetch_internal(url, &request).await?;

        if initial_response.status == 293 {
            // Buffer to collect raw response from multiplexer
            let mut response_buffer = Vec::new();
            let mut headers_parsed = None;

            while let Ok(chunk) = rx.recv().await {
                response_buffer.extend_from_slice(&chunk);

                if headers_parsed.is_none() {
                    if let Ok((status, headers, body_start)) =
                        crate::core::protocol::parse_tunneled_response(&response_buffer)
                    {
                        headers_parsed = Some((status, headers, body_start));
                    }
                }
            }

            if let Some((status, headers, body_start)) = headers_parsed {
                let body = bytes::Bytes::copy_from_slice(&response_buffer[body_start..]);
                return Ok(BraidResponse {
                    status,
                    headers,
                    body,
                    is_subscription: false, // We'd need more logic here to detect subscription
                });
            } else {
                return Err(crate::core::error::BraidError::Protocol(
                    "Multiplexed response ended before headers received".to_string(),
                ));
            }
        }

        Ok(initial_response)
    }

    /// Get the client configuration
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }
}

impl Default for BraidClient {
    fn default() -> Self {
        Self::new()
    }
}
