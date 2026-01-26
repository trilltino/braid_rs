//! Subscription handling for Braid protocol.
//!
//! This module provides types and implementations for managing long-lived
//! subscriptions that receive streamed updates from Braid-HTTP servers.
//!
//! # Overview
//!
//! Subscriptions enable real-time updates using HTTP 209 status code responses.
//! The server keeps the connection open and streams updates as they occur,
//! allowing clients to receive changes immediately without polling.
//!
//! # Types
//!
//! - **Subscription**: Simple subscription type with async `next()` method
//! - **SubscriptionStream**: Implements `Stream` trait for use with `StreamExt`
//! - **HeartbeatConfig**: Configuration for heartbeat timeout detection
//!
//! # Examples
//!
//! ## Using Subscription
//!
//! ```ignore
//! use crate::core::{BraidClient, BraidRequest};
//!
//! let client = BraidClient::new();
//! let request = BraidRequest::new().subscribe();
//! let mut subscription = client.subscribe("http://example.com/resource", request).await?;
//!
//! while let Some(result) = subscription.next().await {
//!     match result {
//!         Ok(update) => println!("Received update: {:?}", update.version),
//!         Err(e) => eprintln!("Subscription error: {}", e),
//!     }
//! }
//! ```
//!
//! ## Using SubscriptionStream
//!
//! ```ignore
//! use crate::core::{BraidClient, BraidRequest};
//! use futures::stream::StreamExt;
//!
//! let client = BraidClient::new();
//! let request = BraidRequest::new().subscribe();
//! let mut stream = client.subscribe("http://example.com/resource", request).await?;
//!
//! while let Some(result) = stream.next().await {
//!     // Handle update
//! }
//! ```
//!
//! # Specification
//!
//! See Section 4 of draft-toomim-httpbis-braid-http for subscription details.

use crate::core::error::{BraidError, Result};
use crate::core::types::Update;
use std::time::Duration;

use futures::Stream;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

// Receiver stream wrapper for Stream trait
pub struct ReceiverStream<T> {
    receiver: async_channel::Receiver<T>,
}

impl<T> ReceiverStream<T> {
    pub fn new(receiver: async_channel::Receiver<T>) -> Self {
        Self { receiver }
    }
}

impl<T: Unpin> Stream for ReceiverStream<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // async-channel's Receiver implements Stream if we use the stream feature or it might be internal
        // For 2.0, let's use the recv() future and handle pinning
        let mut fut = self.receiver.recv();
        unsafe { Pin::new_unchecked(&mut fut) }
            .poll(cx)
            .map(|res| res.ok())
    }
}

// Platform-agnostic timing
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Instant {
    start_ms: f64,
}

#[cfg(target_arch = "wasm32")]
impl Instant {
    pub fn now() -> Self {
        let start_ms = web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0);
        Self { start_ms }
    }

    pub fn elapsed(&self) -> Duration {
        let now = web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0);
        Duration::from_secs_f64(((now - self.start_ms).max(0.0)) / 1000.0)
    }
}

#[cfg(target_arch = "wasm32")]
impl std::ops::Add<Duration> for Instant {
    type Output = Instant;
    fn add(self, other: Duration) -> Instant {
        Instant {
            start_ms: self.start_ms + other.as_secs_f64() * 1000.0,
        }
    }
}

/// Configuration for heartbeat timeout detection.
///
/// When a server sends a `Heartbeats` header with the response, the client
/// can detect if the connection has gone stale by monitoring for heartbeat
/// signals. This config determines the timeout behavior.
///
/// The timeout is calculated as: `1.2 * heartbeat_interval + 3 seconds`
/// (matching the JavaScript reference implementation).
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// The heartbeat interval advertised by the server (in seconds).
    pub interval_secs: f64,
    /// The calculated timeout duration.
    pub timeout: Duration,
}

impl HeartbeatConfig {
    /// Create a new heartbeat config from a server-advertised interval.
    ///
    /// # Arguments
    ///
    /// * `interval_secs` - The heartbeat interval in seconds from the server's response header
    pub fn new(interval_secs: f64) -> Self {
        // Timeout = 1.2 * interval + 3 seconds (matching JS reference)
        let timeout_secs = 1.2 * interval_secs + 3.0;
        Self {
            interval_secs,
            timeout: Duration::from_secs_f64(timeout_secs),
        }
    }

    /// Parse heartbeat config from a header value.
    ///
    /// Parses formats like "5s", "5", "5.5s", "5.5"
    pub fn from_header(value: &str) -> Option<Self> {
        let value = value.trim();
        let num_str = value.strip_suffix('s').unwrap_or(value);
        num_str.parse::<f64>().ok().map(Self::new)
    }
}

/// A subscription to Braid-HTTP updates.
///
/// Represents a long-lived connection that receives streaming updates
/// from a Braid-HTTP server. Updates are delivered asynchronously
/// through the `next()` method.
///
/// # Lifecycle
///
/// 1. Created via `BraidClient::subscribe()`
/// 2. Receives updates through `next().await`
/// 3. Automatically closed when the server connection terminates
///
/// # Error Handling
///
/// The subscription may return errors if:
/// - The connection is lost
/// - The server sends invalid data
/// - Protocol violations occur
/// - Heartbeat timeout is detected (when configured)
///
/// Clients should handle errors appropriately and may need to
/// re-establish the subscription.
pub struct Subscription {
    receiver: async_channel::Receiver<Result<Update>>,
    /// Heartbeat configuration if the server advertised heartbeats
    heartbeat_config: Option<HeartbeatConfig>,
    /// Time of last received data (for heartbeat timeout detection)
    last_activity: Instant,
}

impl Subscription {
    /// Create a new subscription from a receiver channel.
    pub fn new(receiver: async_channel::Receiver<Result<Update>>) -> Self {
        Subscription {
            receiver,
            heartbeat_config: None,
            last_activity: Instant::now(),
        }
    }

    /// Create a subscription with heartbeat timeout detection.
    pub fn with_heartbeat(
        receiver: async_channel::Receiver<Result<Update>>,
        heartbeat_config: HeartbeatConfig,
    ) -> Self {
        Subscription {
            receiver,
            heartbeat_config: Some(heartbeat_config),
            last_activity: Instant::now(),
        }
    }

    /// Receive the next update from the subscription.
    pub async fn next(&mut self) -> Option<Result<Update>> {
        // Check for heartbeat timeout if configured
        if let Some(ref config) = self.heartbeat_config {
            let timeout = config.timeout;

            // Platform-agnostic timeout handling
            #[cfg(not(target_arch = "wasm32"))]
            {
                let deadline = self.last_activity + timeout;
                tokio::select! {
                    result = self.receiver.recv() => {
                        self.last_activity = Instant::now();
                        result.ok()
                    }
                    _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => {
                        Some(Err(BraidError::Timeout))
                    }
                }
            }
            #[cfg(target_arch = "wasm32")]
            {
                // On WASM, we use a racing future for timeout
                use futures::{future::FutureExt, pin_mut, select};
                let recv_fut = self.receiver.recv().fuse();
                let timer_fut =
                    gloo_timers::future::TimeoutFuture::new(timeout.as_millis() as u32).fuse();

                pin_mut!(recv_fut, timer_fut);

                select! {
                    result = recv_fut => {
                        self.last_activity = Instant::now();
                        result.ok()
                    }
                    _ = timer_fut => {
                        Some(Err(BraidError::Timeout))
                    }
                }
            }
        } else {
            let result = self.receiver.recv().await.ok();
            self.last_activity = Instant::now();
            result
        }
    }

    /// Check if the heartbeat has timed out.
    pub fn is_heartbeat_timeout(&self) -> bool {
        if let Some(ref config) = self.heartbeat_config {
            self.last_activity.elapsed() > config.timeout
        } else {
            false
        }
    }
}

impl Stream for Subscription {
    type Item = Result<Update>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = unsafe { self.get_unchecked_mut() };
        let mut fut = this.receiver.recv();
        match unsafe { Pin::new_unchecked(&mut fut) }.poll(cx) {
            Poll::Ready(result) => {
                this.last_activity = Instant::now();
                Poll::Ready(result.ok())
            }
            Poll::Pending => {
                if this.is_heartbeat_timeout() {
                    Poll::Ready(Some(Err(BraidError::Timeout)))
                } else {
                    Poll::Pending
                }
            }
        }
    }
}

/// A streaming subscription that implements the `Stream` trait.
pub struct SubscriptionStream {
    receiver: ReceiverStream<Result<Update>>,
}

impl SubscriptionStream {
    /// Create a new subscription stream from a receiver channel.
    pub fn new(receiver: async_channel::Receiver<Result<Update>>) -> Self {
        SubscriptionStream {
            receiver: ReceiverStream::new(receiver),
        }
    }
}

impl Stream for SubscriptionStream {
    type Item = Result<Update>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = unsafe { self.get_unchecked_mut() };
        unsafe { Pin::new_unchecked(&mut this.receiver) }.poll_next(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_subscription_creation() {
        let (tx, rx) = async_channel::bounded(10);
        let mut subscription = Subscription::new(rx);

        let update = Update::snapshot(crate::core::types::Version::new("v1"), "test");

        tx.send(Ok(update)).await.unwrap();
        let received = subscription.next().await;
        assert!(received.is_some());
    }
}
