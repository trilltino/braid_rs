//! Retry configuration and logic for Braid HTTP client.

use crate::core::types::BraidResponse;
use std::sync::Arc;
use std::time::Duration;

/// Callback type for custom retry decisions.
/// 
/// Returns `true` if the request should be retried, `false` to give up.
/// Called after receiving a response with an error status.
pub type RetryResCallback = Arc<dyn Fn(&BraidResponse) -> bool + Send + Sync>;

/// Callback type for response notification.
/// 
/// Called after each response is received (for retry mode).
/// Can be used for logging, metrics, or custom handling.
pub type OnResCallback = Arc<dyn Fn(&BraidResponse) + Send + Sync>;

/// Configuration for retry behavior.
#[derive(Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (None = infinite)
    pub max_retries: Option<u32>,
    /// Initial backoff duration
    pub initial_backoff: Duration,
    /// Maximum backoff duration
    pub max_backoff: Duration,
    /// HTTP status codes that trigger a retry
    pub retry_on_status: Vec<u16>,
    /// Whether to respect the `Retry-After` header
    pub respect_retry_after: bool,
    /// Custom retry decision callback (JS: retryRes)
    /// Called when response status indicates error
    pub retry_res: Option<RetryResCallback>,
    /// Response notification callback (JS: onRes)
    /// Called after each response received
    pub on_res: Option<OnResCallback>,
}

impl std::fmt::Debug for RetryConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetryConfig")
            .field("max_retries", &self.max_retries)
            .field("initial_backoff", &self.initial_backoff)
            .field("max_backoff", &self.max_backoff)
            .field("retry_on_status", &self.retry_on_status)
            .field("respect_retry_after", &self.respect_retry_after)
            .field("has_retry_res", &self.retry_res.is_some())
            .field("has_on_res", &self.on_res.is_some())
            .finish()
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: None,
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(3),
            retry_on_status: vec![408, 425, 429, 502, 503, 504],
            respect_retry_after: true,
            retry_res: None,
            on_res: None,
        }
    }
}

impl RetryConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn no_retry() -> Self {
        Self {
            max_retries: Some(0),
            ..Default::default()
        }
    }

    #[must_use]
    pub fn with_max_retries(mut self, max: u32) -> Self {
        self.max_retries = Some(max);
        self
    }

    #[must_use]
    pub fn with_initial_backoff(mut self, duration: Duration) -> Self {
        self.initial_backoff = duration;
        self
    }

    #[must_use]
    pub fn with_max_backoff(mut self, duration: Duration) -> Self {
        self.max_backoff = duration;
        self
    }

    #[must_use]
    pub fn with_retry_on_status(mut self, status: u16) -> Self {
        if !self.retry_on_status.contains(&status) {
            self.retry_on_status.push(status);
        }
        self
    }

    #[must_use]
    pub fn with_respect_retry_after(mut self, respect: bool) -> Self {
        self.respect_retry_after = respect;
        self
    }

    /// Set custom retry decision callback (JS equivalent: retryRes)
    /// 
    /// The callback receives the response and returns `true` to retry, `false` to give up.
    #[must_use]
    pub fn with_retry_res<F>(mut self, callback: F) -> Self
    where
        F: Fn(&BraidResponse) -> bool + Send + Sync + 'static,
    {
        self.retry_res = Some(Arc::new(callback));
        self
    }

    /// Set response notification callback (JS equivalent: onRes)
    /// 
    /// Called after each response is received during retry mode.
    #[must_use]
    pub fn with_on_res<F>(mut self, callback: F) -> Self
    where
        F: Fn(&BraidResponse) + Send + Sync + 'static,
    {
        self.on_res = Some(Arc::new(callback));
        self
    }

    /// Check if we should retry based on custom callback or default logic.
    pub fn should_retry_response(&self, response: &BraidResponse) -> bool {
        if let Some(ref callback) = self.retry_res {
            callback(response)
        } else {
            // Default logic: retry on specific status codes
            self.retry_on_status.contains(&response.status)
        }
    }

    /// Notify response callback if set.
    pub fn notify_response(&self, response: &BraidResponse) {
        if let Some(ref callback) = self.on_res {
            callback(response);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RetryDecision {
    Retry(Duration),
    DontRetry,
}

#[derive(Debug, Clone)]
pub struct RetryState {
    pub attempts: u32,
    pub current_backoff: Duration,
    config: RetryConfig,
}

impl RetryState {
    pub fn new(config: RetryConfig) -> Self {
        Self {
            attempts: 0,
            current_backoff: config.initial_backoff,
            config,
        }
    }

    pub fn should_retry_error(&mut self, is_abort: bool) -> RetryDecision {
        if is_abort {
            return RetryDecision::DontRetry;
        }
        self.decide_retry(None)
    }

    pub fn should_retry_status(
        &mut self,
        status: u16,
        retry_after: Option<Duration>,
    ) -> RetryDecision {
        if !self.config.retry_on_status.contains(&status) {
            return RetryDecision::DontRetry;
        }
        self.decide_retry(retry_after)
    }

    pub fn should_retry_status_with_text(
        &mut self,
        status: u16,
        status_text: Option<&str>,
        retry_after: Option<Duration>,
    ) -> RetryDecision {
        if let Some(text) = status_text {
            if text.to_lowercase().contains("missing parents") {
                return self.decide_retry(retry_after);
            }
        }
        self.should_retry_status(status, retry_after)
    }

    fn decide_retry(&mut self, retry_after: Option<Duration>) -> RetryDecision {
        self.attempts += 1;
        if let Some(max) = self.config.max_retries {
            if self.attempts > max {
                return RetryDecision::DontRetry;
            }
        }

        let wait = if self.config.respect_retry_after {
            retry_after.unwrap_or(self.current_backoff)
        } else {
            self.current_backoff
        };

        self.current_backoff = std::cmp::min(
            self.current_backoff + Duration::from_secs(1),
            self.config.max_backoff,
        );

        RetryDecision::Retry(wait)
    }

    pub fn reset(&mut self) {
        self.attempts = 0;
        self.current_backoff = self.config.initial_backoff;
    }
}

pub fn parse_retry_after(value: &str) -> Option<Duration> {
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, None);
        assert!(config.retry_on_status.contains(&503));
    }

    #[test]
    fn test_retry_state_basic() {
        let config = RetryConfig::default().with_max_retries(1);
        let mut state = RetryState::new(config);
        assert!(matches!(
            state.should_retry_error(false),
            RetryDecision::Retry(_)
        ));
        assert_eq!(state.should_retry_error(false), RetryDecision::DontRetry);
    }

    #[test]
    fn test_retry_res_callback() {
        let response = BraidResponse::new(500, "");
        
        // Test callback that retries on 500
        let config = RetryConfig::default()
            .with_retry_res(|res| res.status == 500);
        
        assert!(config.should_retry_response(&response));
        
        // Test callback that doesn't retry
        let config2 = RetryConfig::default()
            .with_retry_res(|_| false);
        
        assert!(!config2.should_retry_response(&response));
    }

    #[test]
    fn test_on_res_callback() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();
        
        let config = RetryConfig::default()
            .with_on_res(move |_| {
                call_count_clone.fetch_add(1, Ordering::SeqCst);
            });
        
        let response = BraidResponse::new(200, "");
        config.notify_response(&response);
        
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }
}
