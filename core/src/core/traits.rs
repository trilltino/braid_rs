use crate::core::error::Result;
use crate::core::types::{BraidRequest, BraidResponse, Update};
use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

/// Abstraction for asynchronous runtime operations.
pub trait BraidRuntime: Send + Sync + 'static {
    /// Spawn a background task.
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>);

    /// Yield execution for a duration.
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

    /// Get current Unix timestamp in milliseconds.
    fn now_ms(&self) -> u64;
}

/// Abstraction for network operations.
#[async_trait]
pub trait BraidNetwork: Send + Sync + 'static {
    /// Perform a standard Braid-HTTP request.
    async fn fetch(&self, url: &str, req: BraidRequest) -> Result<BraidResponse>;

    /// Subscribe to a Braid-HTTP 209 stream.
    async fn subscribe(
        &self,
        url: &str,
        req: BraidRequest,
    ) -> Result<async_channel::Receiver<Result<Update>>>;
}

/// Abstraction for persistent storage.
#[async_trait]
pub trait BraidStorage: Send + Sync + 'static {
    /// Store a blob with its metadata.
    async fn put(&self, key: &str, data: crate::core::types::Bytes, meta: String) -> Result<()>;

    /// Retrieve a blob and its metadata.
    async fn get(&self, key: &str) -> Result<Option<(crate::core::types::Bytes, String)>>;

    /// Delete a blob.
    async fn delete(&self, key: &str) -> Result<()>;

    /// List all keys in storage.
    async fn list_keys(&self) -> Result<Vec<String>>;
}

/// Helper struct to provide default native runtime implementation.
#[cfg(not(target_arch = "wasm32"))]
pub struct NativeRuntime;

#[cfg(not(target_arch = "wasm32"))]
impl BraidRuntime for NativeRuntime {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
        tokio::spawn(future);
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(tokio::time::sleep(duration))
    }

    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}
