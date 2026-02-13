//! WASM network implementation for Braid HTTP client.
//!
//! This module provides a browser-compatible network implementation using
//! `web-sys` and `gloo-net` for making HTTP requests.

use crate::core::client::MessageParser;
use crate::core::error::{BraidError, Result};
use crate::core::protocol;
use crate::core::traits::BraidNetwork;
use crate::core::types::{BraidRequest, BraidResponse, Patch, Update};
use async_trait::async_trait;
use bytes::Bytes;
use gloo_net::http::{Method, RequestBuilder};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortController, ReadableStreamDefaultReader};

/// WASM-compatible network implementation.
pub struct WasmNetwork;

impl WasmNetwork {
    pub fn new() -> Self {
        Self
    }

    /// Serialize patches for HTTP request body.
    /// 
    /// - Single patch: Returns the patch content as-is with Content-Range header
    /// - Multiple patches: Returns Braid wire format with Patches: N header
    fn serialize_patches(patches: &[Patch]) -> (Bytes, Option<String>) {
        if patches.is_empty() {
            return (Bytes::new(), None);
        }

        if patches.len() == 1 {
            let patch = &patches[0];
            return (patch.content.clone(), Some(patch.content_range_header()));
        }

        // Multiple patches: create Braid wire format body
        let mut body = Vec::new();

        for (i, patch) in patches.iter().enumerate() {
            // Separate patches with double CRLF (except before first patch)
            if i > 0 {
                body.extend_from_slice(b"\r\n\r\n");
            }

            // Write Content-Length header
            body.extend_from_slice(format!("Content-Length: {}\r\n", patch.content.len()).as_bytes());
            
            // Write Content-Range header
            body.extend_from_slice(format!("Content-Range: {}\r\n", patch.content_range_header()).as_bytes());
            
            // Write extra headers from patch
            for (key, value) in &patch.extra_headers {
                body.extend_from_slice(format!("{}: {}\r\n", key, value).as_bytes());
            }
            
            // Blank line separates headers from body
            body.extend_from_slice(b"\r\n");
            
            // Write patch content
            body.extend_from_slice(&patch.content);
        }

        (Bytes::from(body), None)
    }

    /// Build a RequestBuilder from BraidRequest.
    fn build_request(&self, url: &str, request: &BraidRequest) -> Result<RequestBuilder> {
        let method = match request.method.to_uppercase().as_str() {
            "POST" => Method::POST,
            "PUT" => Method::PUT,
            "DELETE" => Method::DELETE,
            "PATCH" => Method::PATCH,
            _ => Method::GET,
        };

        let mut builder = RequestBuilder::new(url).method(method);

        // Add extra headers
        for (k, v) in &request.extra_headers {
            builder = builder.header(k, v);
        }

        // Set cache: no-cache (browser requirement for Braid)
        if !request.extra_headers.contains_key("cache-control") {
            builder = builder.header("cache-control", "no-cache, no-store");
        }

        // Handle patches or body
        if let Some(patches) = &request.patches {
            if !patches.is_empty() {
                let (body, content_range) = Self::serialize_patches(patches);

                if patches.len() == 1 {
                    // Single patch: Content-Range header
                    if let Some(content_range) = content_range {
                        builder = builder.header("Content-Range", &content_range);
                    }
                    builder = builder.header("Content-Type", "application/json");
                    builder = builder.body(js_sys::Uint8Array::from(&body[..]));
                } else {
                    // Multiple patches: Patches: N header
                    builder = builder.header("Patches", &patches.len().to_string());
                    builder = builder.body(js_sys::Uint8Array::from(&body[..]));
                }
            } else if !request.body.is_empty() {
                builder = builder.header("Content-Type", "application/json");
                builder = builder.body(js_sys::Uint8Array::from(&request.body[..]));
            }
        } else if !request.body.is_empty() {
            builder = builder.header("Content-Type", "application/json");
            builder = builder.body(js_sys::Uint8Array::from(&request.body[..]));
        }

        // Add Braid headers
        if let Some(versions) = &request.version {
            builder = builder.header("version", &protocol::format_version_header(versions));
        }
        if let Some(parents) = &request.parents {
            builder = builder.header("parents", &protocol::format_version_header(parents));
        }
        if request.subscribe {
            builder = builder.header("subscribe", "true");
        }
        if let Some(peer) = &request.peer {
            builder = builder.header("peer", peer);
        }
        if let Some(merge_type) = &request.merge_type {
            builder = builder.header("merge-type", merge_type);
        }

        Ok(builder)
    }
}

impl Default for WasmNetwork {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl BraidNetwork for WasmNetwork {
    async fn fetch(&self, url: &str, request: BraidRequest) -> Result<BraidResponse> {
        // Call on_fetch callback if set
        request.notify_on_fetch(url);

        let builder = self.build_request(url, &request)?;

        let response = builder
            .send()
            .await
            .map_err(|e| BraidError::Http(format!("WASM fetch failed: {:?}", e)))?;

        let status = response.status();
        let headers = response.headers();

        // Collect headers
        let mut header_map = std::collections::BTreeMap::new();
        let header_keys = js_sys::Object::keys(headers.as_ref());
        for i in 0..header_keys.length() {
            if let Ok(key) = header_keys.get(i).as_string() {
                if let Ok(value) = headers.get(&key) {
                    header_map.insert(key.to_lowercase(), value);
                }
            }
        }

        // Get body
        let body_bytes = response
            .binary()
            .await
            .map_err(|e| BraidError::Http(format!("Failed to read response body: {:?}", e)))?;

        // Call on_bytes callback if set
        request.notify_on_bytes(&body_bytes);

        Ok(BraidResponse {
            status,
            headers: header_map,
            body: Bytes::from(body_bytes),
            is_subscription: status == 209,
        })
    }

    async fn subscribe(
        &self,
        url: &str,
        mut request: BraidRequest,
    ) -> Result<async_channel::Receiver<Result<Update>>> {
        request.subscribe = true;

        // Call on_fetch callback
        request.notify_on_fetch(url);

        let builder = self.build_request(url, &request)?;

        // Use web_sys directly for streaming response
        let window = web_sys::window().ok_or_else(|| BraidError::Http("No window available".to_string()))?;
        
        let abort_controller = AbortController::new().map_err(|e| {
            BraidError::Http(format!("Failed to create AbortController: {:?}", e))
        })?;
        let signal = abort_controller.signal();

        let fetch_request = web_sys::Request::new_with_str_and_init(
            url,
            web_sys::RequestInit::new().signal(&signal),
        ).map_err(|e| BraidError::Http(format!("Failed to create request: {:?}", e)))?;

        // Add headers
        let headers = fetch_request.headers();
        for (k, v) in &request.extra_headers {
            let _ = headers.set(k, v);
        }
        let _ = headers.set("subscribe", "true");
        if let Some(versions) = &request.version {
            let _ = headers.set("version", &protocol::format_version_header(versions));
        }

        let response_promise = window.fetch_with_request(&fetch_request);
        let response = JsFuture::from(response_promise)
            .await
            .map_err(|e| BraidError::Http(format!("Fetch failed: {:?}", e)))?;

        let response: web_sys::Response = response.dyn_into().map_err(|e| {
            BraidError::Http(format!("Invalid response type: {:?}", e))
        })?;

        let body = response.body().ok_or_else(|| {
            BraidError::Http("Response has no body".to_string())
        })?;

        let reader: ReadableStreamDefaultReader = body
            .get_reader()
            .dyn_into()
            .map_err(|e| BraidError::Http(format!("Failed to get reader: {:?}", e)))?;

        let (tx, rx) = async_channel::bounded(100);

        // Spawn task to read stream
        wasm_bindgen_futures::spawn_local(async move {
            let mut parser = MessageParser::new();
            
            loop {
                let promise = reader.read();
                let chunk = match JsFuture::from(promise).await {
                    Ok(chunk) => chunk,
                    Err(e) => {
                        let _ = tx.send(Err(BraidError::Http(format!("Stream read error: {:?}", e)))).await;
                        break;
                    }
                };

                let done = js_sys::Reflect::get(&chunk, &"done".into())
                    .ok()
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);

                if done {
                    break;
                }

                if let Ok(value) = js_sys::Reflect::get(&chunk, &"value".into()) {
                    if let Ok(array) = value.dyn_into::<js_sys::Uint8Array>() {
                        let mut bytes = vec![0u8; array.length() as usize];
                        array.copy_to(&mut bytes);
                        
                        // Call on_bytes callback
                        request.notify_on_bytes(&bytes);
                        
                        if let Ok(messages) = parser.feed(&bytes) {
                            for msg in messages {
                                let update = crate::core::client::utils::message_to_update(msg);
                                let _ = tx.send(Ok(update)).await;
                            }
                        }
                    }
                }
            }
        });

        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::Patch;

    #[test]
    fn test_serialize_single_patch() {
        let patches = vec![Patch::json(".name", "\"Alice\"")];
        let (body, content_range) = WasmNetwork::serialize_patches(&patches);
        
        assert_eq!(body, Bytes::from("\"Alice\""));
        assert_eq!(content_range, Some("json .name".to_string()));
    }

    #[test]
    fn test_serialize_multi_patch() {
        let patches = vec![
            Patch::json(".name", "\"Alice\""),
            Patch::json(".age", "30"),
        ];
        let (body, content_range) = WasmNetwork::serialize_patches(&patches);
        
        assert!(content_range.is_none());
        let body_str = String::from_utf8_lossy(&body);
        assert!(body_str.contains("Content-Length:"));
        assert!(body_str.contains("Content-Range: json .name"));
        assert!(body_str.contains("Content-Range: json .age"));
    }
}
