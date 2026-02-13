//! Subscription handling for Braid protocol.

use crate::core::error::{BraidError, Result};
use crate::core::types::Update;
use futures::Stream;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

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
        let mut fut = self.receiver.recv();
        unsafe { Pin::new_unchecked(&mut fut) }
            .poll(cx)
            .map(|res| res.ok())
    }
}

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
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    pub interval_secs: f64,
    pub timeout: Duration,
}

impl HeartbeatConfig {
    pub fn new(interval_secs: f64) -> Self {
        let timeout_secs = 1.2 * interval_secs + 3.0;
        Self {
            interval_secs,
            timeout: Duration::from_secs_f64(timeout_secs),
        }
    }

    pub fn from_header(value: &str) -> Option<Self> {
        value
            .trim()
            .strip_suffix('s')
            .unwrap_or(value)
            .parse::<f64>()
            .ok()
            .map(Self::new)
    }
}

pub struct Subscription {
    receiver: async_channel::Receiver<Result<Update>>,
    heartbeat_config: Option<HeartbeatConfig>,
    last_activity: Instant,
}

impl Subscription {
    pub fn new(receiver: async_channel::Receiver<Result<Update>>) -> Self {
        Subscription {
            receiver,
            heartbeat_config: None,
            last_activity: Instant::now(),
        }
    }

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

    pub async fn next(&mut self) -> Option<Result<Update>> {
        if let Some(ref config) = self.heartbeat_config {
            let timeout = config.timeout;
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
                    _ = timer_fut => Some(Err(BraidError::Timeout))
                }
            }
        } else {
            let result = self.receiver.recv().await.ok();
            self.last_activity = Instant::now();
            result
        }
    }

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

pub struct SubscriptionStream {
    receiver: ReceiverStream<Result<Update>>,
}

impl SubscriptionStream {
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
    use crate::core::types::Version;

    // ========== HeartbeatConfig Tests ==========

    #[test]
    fn test_heartbeat_config_new() {
        let config = HeartbeatConfig::new(5.0);
        assert_eq!(config.interval_secs, 5.0);
        // timeout = 1.2 * 5 + 3 = 9 seconds
        assert_eq!(config.timeout, Duration::from_secs(9));
    }

    #[test]
    fn test_heartbeat_config_one_second() {
        let config = HeartbeatConfig::new(1.0);
        assert_eq!(config.interval_secs, 1.0);
        // timeout = 1.2 * 1 + 3 = 4.2 seconds
        assert_eq!(config.timeout, Duration::from_millis(4200));
    }

    #[test]
    fn test_heartbeat_config_from_header_seconds() {
        let config = HeartbeatConfig::from_header("5s").unwrap();
        assert_eq!(config.interval_secs, 5.0);
    }

    #[test]
    fn test_heartbeat_config_from_header_plain() {
        let config = HeartbeatConfig::from_header("10").unwrap();
        assert_eq!(config.interval_secs, 10.0);
    }

    #[test]
    fn test_heartbeat_config_from_header_whitespace() {
        let config = HeartbeatConfig::from_header("  5s  ").unwrap();
        assert_eq!(config.interval_secs, 5.0);
    }

    #[test]
    fn test_heartbeat_config_invalid() {
        assert!(HeartbeatConfig::from_header("invalid").is_none());
        assert!(HeartbeatConfig::from_header("").is_none());
        assert!(HeartbeatConfig::from_header("abc").is_none());
    }

    // ========== Subscription Basic Tests ==========

    #[test]
    fn test_subscription_new() {
        let (_tx, rx) = async_channel::unbounded::<Result<Update>>();
        let sub = Subscription::new(rx);
        
        assert!(sub.heartbeat_config.is_none());
        assert!(!sub.is_heartbeat_timeout());
    }

    #[test]
    fn test_subscription_with_heartbeat() {
        let (_tx, rx) = async_channel::unbounded::<Result<Update>>();
        let config = HeartbeatConfig::new(5.0);
        let sub = Subscription::with_heartbeat(rx, config);
        
        assert!(sub.heartbeat_config.is_some());
        assert_eq!(sub.heartbeat_config.unwrap().interval_secs, 5.0);
    }

    #[test]
    fn test_subscription_heartbeat_timeout_detection() {
        let (_tx, rx) = async_channel::unbounded::<Result<Update>>();
        let config = HeartbeatConfig::new(0.001); // 1ms interval, ~3s timeout
        let sub = Subscription::with_heartbeat(rx, config);
        
        // Immediately after creation, should not be timed out
        assert!(!sub.is_heartbeat_timeout());
        
        // Note: Actual timeout would take ~3 seconds, so we don't test that here
    }

    #[test]
    fn test_subscription_no_timeout_without_heartbeat() {
        let (_tx, rx) = async_channel::unbounded::<Result<Update>>();
        let sub = Subscription::new(rx);
        
        // Should not timeout even after long wait
        std::thread::sleep(Duration::from_millis(50));
        assert!(!sub.is_heartbeat_timeout());
    }

    // ========== ReceiverStream Tests ==========

    #[test]
    fn test_receiver_stream_new() {
        let (_tx, rx) = async_channel::unbounded::<String>();
        let stream = ReceiverStream::new(rx);
        // Just verify it constructs
        drop(stream);
    }

    #[test]
    fn test_subscription_stream_new() {
        let (_tx, rx) = async_channel::unbounded::<Result<Update>>();
        let stream = SubscriptionStream::new(rx);
        // Just verify it constructs
        drop(stream);
    }
}
