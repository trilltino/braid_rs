//! Blob sync client for BraidFS.
//!
//! Implements client-side blob synchronization.
//! Matches JS `braid_blob.sync()` from braid-blob/index.js.

use crate::core::{BraidClient, BraidError, Result};
use tokio::sync::mpsc;

/// Blob sync configuration.
#[derive(Debug, Clone)]
pub struct BlobSyncConfig {
    /// Remote URL to sync with.
    pub url: String,
    /// Local peer ID.
    pub peer: String,
    /// Additional headers (e.g., cookies).
    pub headers: std::collections::HashMap<String, String>,
    /// Reconnect delay in milliseconds.
    pub reconnect_delay_ms: u64,
}

/// Blob sync client.
pub struct BlobSyncClient {
    config: BlobSyncConfig,
    client: BraidClient,
    /// Channel to receive blob updates.
    update_tx: mpsc::Sender<BlobUpdate>,
}

/// A blob update received from the server.
#[derive(Debug, Clone)]
pub struct BlobUpdate {
    pub key: String,
    pub version: Vec<String>,
    pub content_type: Option<String>,
    pub data: Vec<u8>,
}

impl BlobSyncClient {
    /// Create a new blob sync client.
    pub fn new(config: BlobSyncConfig) -> Result<(Self, mpsc::Receiver<BlobUpdate>)> {
        let (update_tx, update_rx) = mpsc::channel(100);

        Ok((
            Self {
                config,
                client: BraidClient::new()?,
                update_tx,
            },
            update_rx,
        ))
    }

    /// Start syncing with the remote blob server.
    pub async fn sync<F, W, D>(&self, _on_read: F, on_write: W, _on_delete: D) -> Result<()>
    where
        F: Fn(
                &str,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<Option<Vec<u8>>>> + Send>,
            > + Send
            + Sync,
        W: Fn(
                &str,
                Vec<u8>,
            )
                -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>
            + Send
            + Sync,
        D: Fn(&str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>
            + Send
            + Sync,
    {
        tracing::info!("Starting blob sync with {}", self.config.url);

        // Build request with headers
        let mut req = self
            .client
            .client()
            .get(&self.config.url)
            .header("Subscribe", "true")
            .header("Peer", &self.config.peer);

        for (key, value) in &self.config.headers {
            req = req.header(key, value);
        }

        // Send request
        let response = req
            .send()
            .await
            .map_err(|e| BraidError::Http(e.to_string()))?;
        if !response.status().is_success() {
            return Err(BraidError::Http(format!(
                "Blob sync failed: {}",
                response.status()
            )));
        }

        // Handle subscription stream
        let mut stream = response.bytes_stream();
        use futures::StreamExt;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk
                .map_err(|e| BraidError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

            // Parse chunk as blob update
            // In real implementation, this would parse Braid-HTTP updates
            let update = BlobUpdate {
                key: self.config.url.clone(),
                version: vec![],
                content_type: None,
                data: chunk.to_vec(),
            };

            // Write to local storage
            on_write(&update.key, update.data.clone()).await?;

            // Notify listeners
            let _ = self.update_tx.send(update).await;
        }

        Ok(())
    }

    /// Upload local changes to the remote server.
    pub async fn put(
        &self,
        key: &str,
        data: Vec<u8>,
        version: Option<Vec<String>>,
    ) -> Result<Vec<String>> {
        let mut req = self
            .client
            .client()
            .put(&format!("{}/{}", self.config.url, key))
            .header("Peer", &self.config.peer)
            .body(data);

        if let Some(v) = &version {
            let parents_str = v
                .iter()
                .map(|p| format!("\"{}\"", p))
                .collect::<Vec<_>>()
                .join(", ");
            req = req.header("Parents", parents_str);
        }

        for (k, v) in &self.config.headers {
            req = req.header(k, v);
        }

        let response = req
            .send()
            .await
            .map_err(|e| BraidError::Http(e.to_string()))?;
        if !response.status().is_success() {
            return Err(BraidError::Http(format!(
                "Blob put failed: {}",
                response.status()
            )));
        }

        // Extract version from response
        let new_version = response
            .headers()
            .get("Version")
            .and_then(|v| v.to_str().ok())
            .map(|s| vec![s.to_string()])
            .unwrap_or_default();

        Ok(new_version)
    }

    /// Delete a blob from the remote server.
    pub async fn delete(&self, key: &str) -> Result<()> {
        let mut req = self
            .client
            .client()
            .delete(&format!("{}/{}", self.config.url, key))
            .header("Peer", &self.config.peer);

        for (k, v) in &self.config.headers {
            req = req.header(k, v);
        }

        let response = req
            .send()
            .await
            .map_err(|e| BraidError::Http(e.to_string()))?;
        if !response.status().is_success() && response.status().as_u16() != 404 {
            return Err(BraidError::Http(format!(
                "Blob delete failed: {}",
                response.status()
            )));
        }

        Ok(())
    }
}

/// Reconnector that handles automatic reconnection with backoff.
/// Matches JS `reconnector()` from braid-blob/index.js.
pub struct Reconnector<F> {
    get_delay: Box<dyn Fn(Option<&str>, u32) -> u64 + Send + Sync>,
    func: F,
    retry_count: u32,
}

impl<F, Fut> Reconnector<F>
where
    F: Fn() -> Fut + Send + Sync,
    Fut: std::future::Future<Output = Result<()>> + Send,
{
    /// Create a new reconnector.
    pub fn new<D>(get_delay: D, func: F) -> Self
    where
        D: Fn(Option<&str>, u32) -> u64 + Send + Sync + 'static,
    {
        Self {
            get_delay: Box::new(get_delay),
            func,
            retry_count: 0,
        }
    }

    /// Run with automatic reconnection.
    pub async fn run(&mut self) -> Result<()> {
        loop {
            match (self.func)().await {
                Ok(()) => {
                    self.retry_count = 0;
                    return Ok(());
                }
                Err(e) => {
                    self.retry_count += 1;
                    let delay = (self.get_delay)(Some(&e.to_string()), self.retry_count);
                    tracing::warn!(
                        "Reconnecting in {}ms after error: {} (attempt {})",
                        delay,
                        e,
                        self.retry_count
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blob_sync_config() {
        let config = BlobSyncConfig {
            url: "http://example.com/blob".to_string(),
            peer: "test-peer".to_string(),
            headers: Default::default(),
            reconnect_delay_ms: 3000,
        };
        assert_eq!(config.url, "http://example.com/blob");
    }
}
