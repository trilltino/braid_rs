use crate::core::client::MessageParser;
use crate::core::error::{BraidError, Result};
use crate::core::protocol;
use crate::core::traits::BraidNetwork;
use crate::core::types::{BraidRequest, BraidResponse, Update};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;

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

        if !request.body.is_empty() {
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

        // Add other headers as in fetch... (simplified for brevity here, but should be complete)
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
