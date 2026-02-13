use crate::core::client::MessageParser;
use crate::core::error::{BraidError, Result};
use crate::core::protocol;
use crate::core::traits::BraidNetwork;
use crate::core::types::{BraidRequest, BraidResponse, Patch, Update};
use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use reqwest::Client;

/// Serialize patches for HTTP request body according to Braid protocol.
/// 
/// - Single patch: Returns the patch content as-is with Content-Range header
/// - Multiple patches: Returns custom Braid wire format with Patches: N header
/// 
/// Wire format for multiple patches:
///   Content-Length: <bytes>
///   Content-Range: <unit> <range>
///   \r
///   {patch data}
///   \r\n\r\n
///   (next patch...)
fn serialize_patches(patches: &[Patch]) -> (Bytes, Option<String>) {
    if patches.is_empty() {
        return (Bytes::new(), None);
    }

    if patches.len() == 1 {
        // Single patch: use content directly with Content-Range header
        let patch = &patches[0];
        return (patch.content.clone(), Some(patch.content_range_header()));
    }

    // Multiple patches: create Braid wire format body
    // Format per patch:
    //   Content-Length: <len>\r\n
    //   Content-Range: <unit> <range>\r\n
    //   [extra headers...]\r\n
    //   \r\n
    //   <content>\r\n
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

pub struct NativeNetwork {
    client: Client,
}

impl NativeNetwork {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub fn client(&self) -> &Client {
        &self.client
    }
}

#[async_trait]
impl BraidNetwork for NativeNetwork {
    async fn fetch(&self, url: &str, request: BraidRequest) -> Result<BraidResponse> {
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

        // Handle body and patches
        if let Some(patches) = &request.patches {
            if !patches.is_empty() {
                let (body, content_range) = serialize_patches(patches);
                
                if patches.len() == 1 {
                    // Single patch: Content-Range header with patch content as body
                    if let Some(content_range) = content_range {
                        req_builder = req_builder.header("Content-Range", content_range);
                    }
                    req_builder = req_builder.header(reqwest::header::CONTENT_TYPE, "application/json");
                    req_builder = req_builder.body(body);
                } else {
                    // Multiple patches: Patches: N header with Braid wire format body
                    req_builder = req_builder.header("Patches", patches.len().to_string());
                    req_builder = req_builder.body(body);
                }
            } else if !request.body.is_empty() {
                req_builder = req_builder.header(reqwest::header::CONTENT_TYPE, "application/json");
                req_builder = req_builder.body(request.body.clone());
            }
        } else if !request.body.is_empty() {
            req_builder = req_builder.header(reqwest::header::CONTENT_TYPE, "application/json");
            req_builder = req_builder.body(request.body.clone());
        }

        if let Some(versions) = &request.version {
            req_builder = req_builder.header("version", protocol::format_version_header(versions));
        }
        if let Some(parents) = &request.parents {
            req_builder = req_builder.header("parents", protocol::format_version_header(parents));
        }
        if request.subscribe {
            req_builder = req_builder.header("subscribe", "true");
        }
        if let Some(peer) = &request.peer {
            req_builder = req_builder.header("peer", peer);
        }
        if let Some(merge_type) = &request.merge_type {
            req_builder = req_builder.header("merge-type", merge_type);
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

    async fn subscribe(
        &self,
        url: &str,
        mut request: BraidRequest,
    ) -> Result<async_channel::Receiver<Result<Update>>> {
        request.subscribe = true;
        let mut req_builder = self.client.get(url).header("subscribe", "true");

        if let Some(versions) = &request.version {
            req_builder = req_builder.header("version", protocol::format_version_header(versions));
        }

        let response = req_builder
            .send()
            .await
            .map_err(|e| BraidError::Http(e.to_string()))?;

        let (tx, rx) = async_channel::bounded(100);
        let mut stream = response.bytes_stream();

        tokio::spawn(async move {
            let mut parser = MessageParser::new();
            while let Some(chunk_res) = stream.next().await {
                match chunk_res {
                    Ok(chunk) => {
                        if let Ok(messages) = parser.feed(&chunk) {
                            for msg in messages {
                                let update = crate::core::client::utils::message_to_update(msg);
                                let _ = tx.send(Ok(update)).await;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(BraidError::Http(e.to_string()))).await;
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }
}
