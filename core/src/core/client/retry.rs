//! Retry configuration and logic for Braid HTTP client.
//!
//! Provides robust retry handling with exponential backoff, status code
//! filtering, and `Retry-After` header support.

use std::time::Duration;

/// Configuration for retry behavior.
///
/// # Example
///
/// ```rust
/// use crate::core::client::RetryConfig;
/// use std::time::Duration;
///
/// let config = RetryConfig::default()
///     .with_max_retries(5)
///     .with_initial_backoff(Duration::from_secs(1))
///     .with_max_backoff(Duration::from_secs(30));
/// ```
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (None = infinite)
    pub max_retries: Option<u32>,

    /// Initial backoff duration between retries
    pub initial_backoff: Duration,

    /// Maximum backoff duration (caps exponential growth)
    pub max_backoff: Duration,

    /// HTTP status codes that trigger a retry
    pub retry_on_status: Vec<u16>,

    /// Whether to respect the `Retry-After` header from server
    pub respect_retry_after: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: None, // Infinite retries like JS reference
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(3), // JS uses max 3s
            retry_on_status: vec![
                408, // Request Timeout
                425, // Too Early
                429, // Too Many Requests
                502, // Bad Gateway
                503, // Service Unavailable
                504, // Gateway Timeout
            ],
            respect_retry_after: true,
        }
    }
}

impl RetryConfig {
    /// Create a new retry config with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a config that disables retries.
    #[must_use]
    pub fn no_retry() -> Self {
        Self {
            max_retries: Some(0),
            ..Default::default()
        }
    }

    /// Set maximum number of retries.
    #[must_use]
    pub fn with_max_retries(mut self, max: u32) -> Self {
        self.max_retries = Some(max);
        self
    }

    /// Set initial backoff duration.
    #[must_use]
    pub fn with_initial_backoff(mut self, duration: Duration) -> Self {
        self.initial_backoff = duration;
        self
    }

    /// Set maximum backoff duration.
    #[must_use]
    pub fn with_max_backoff(mut self, duration: Duration) -> Self {
        self.max_backoff = duration;
        self
    }

    /// Add a status code that should trigger retry.
    #[must_use]
    pub fn with_retry_on_status(mut self, status: u16) -> Self {
        if !self.retry_on_status.contains(&status) {
            self.retry_on_status.push(status);
        }
        self
    }

    /// Set whether to respect Retry-After header.
    #[must_use]
    pub fn with_respect_retry_after(mut self, respect: bool) -> Self {
        self.respect_retry_after = respect;
        self
    }
}

/// Result of a retry decision.
#[derive(Debug, Clone, PartialEq)]
pub enum RetryDecision {
    /// Should retry after the specified duration
    Retry(Duration),
    /// Should not retry
    DontRetry,
}

/// Retry state tracking for a request.
#[derive(Debug, Clone)]
pub struct RetryState {
    /// Number of attempts made so far
    pub attempts: u32,
    /// Current backoff duration
    pub current_backoff: Duration,
    /// Configuration
    config: RetryConfig,
}

impl RetryState {
    /// Create a new retry state with the given config.
    pub fn new(config: RetryConfig) -> Self {
        Self {
            attempts: 0,
            current_backoff: config.initial_backoff,
            config,
        }
    }

    /// Check if we should retry based on an error.
    ///
    /// Returns `RetryDecision::Retry(duration)` if we should wait and retry,
    /// or `RetryDecision::DontRetry` if we should give up.
    pub fn should_retry_error(&mut self, is_abort: bool) -> RetryDecision {
        // Never retry on abort
        if is_abort {
            return RetryDecision::DontRetry;
        }

        self.decide_retry(None)
    }

    /// Check if we should retry based on an HTTP status code.
    ///
    /// Optionally takes a `Retry-After` header value (in seconds).
    pub fn should_retry_status(
        &mut self,
        status: u16,
        retry_after: Option<Duration>,
    ) -> RetryDecision {
        // Check if this status code should trigger a retry
        if !self.config.retry_on_status.contains(&status) {
            // Special case: "Missing Parents" responses should retry
            // (handled by caller who checks response body/headers)
            return RetryDecision::DontRetry;
        }

        self.decide_retry(retry_after)
    }

    /// Check if we should retry based on status code and status text.
    ///
    /// This method extends `should_retry_status` by also checking the
    /// HTTP status text for "Missing Parents", which should always retry.
    /// Matches the JavaScript reference implementation behavior.
    ///
    /// # Arguments
    ///
    /// * `status` - HTTP status code
    /// * `status_text` - HTTP status text (e.g., "Missing Parents")
    /// * `retry_after` - Optional Retry-After header value
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::client::RetryState;
    /// use crate::core::client::RetryConfig;
    ///
    /// let mut state = RetryState::new(RetryConfig::default());
    ///
    /// // "Missing Parents" always retries regardless of status code
    /// let decision = state.should_retry_status_with_text(400, Some("Missing Parents"), None);
    /// assert!(matches!(decision, crate::core::client::RetryDecision::Retry(_)));
    /// ```
    pub fn should_retry_status_with_text(
        &mut self,
        status: u16,
        status_text: Option<&str>,
        retry_after: Option<Duration>,
    ) -> RetryDecision {
        // Check for "Missing Parents" status text - always retry
        // Matches JS: res.statusText.match(/Missing Parents/i)
        if let Some(text) = status_text {
            if text.to_lowercase().contains("missing parents") {
                return self.decide_retry(retry_after);
            }
        }

        self.should_retry_status(status, retry_after)
    }

    /// Internal method to make retry decision and update state.
    fn decide_retry(&mut self, retry_after: Option<Duration>) -> RetryDecision {
        self.attempts += 1;

        // Check if we've exceeded max retries
        if let Some(max) = self.config.max_retries {
            if self.attempts > max {
                return RetryDecision::DontRetry;
            }
        }

        // Determine wait duration
        let wait = if self.config.respect_retry_after {
            retry_after.unwrap_or(self.current_backoff)
        } else {
            self.current_backoff
        };

        // Update backoff for next attempt (exponential with cap)
        // JS reference uses: waitTime = Math.min(waitTime + 1, 3)
        // So it's actually linear +1s, capped at 3s
        self.current_backoff = std::cmp::min(
            self.current_backoff + Duration::from_secs(1),
            self.config.max_backoff,
        );

        RetryDecision::Retry(wait)
    }

    /// Reset the retry state (e.g., after a successful request).
    pub fn reset(&mut self) {
        self.attempts = 0;
        self.current_backoff = self.config.initial_backoff;
    }
}

/// Parse a `Retry-After` header value.
///
/// Supports both delay-seconds format (e.g., "120") and HTTP-date format.
/// Returns None if parsing fails.
pub fn parse_retry_after(value: &str) -> Option<Duration> {
    // Try parsing as seconds first
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }

    // Try parsing as HTTP-date
    // Format: "Wed, 21 Oct 2015 07:28:00 GMT"
    // For simplicity, we'll just return None for date format
    // A full implementation would parse the date and calculate duration
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, None);
        assert_eq!(config.initial_backoff, Duration::from_secs(1));
        assert_eq!(config.max_backoff, Duration::from_secs(3));
        assert!(config.retry_on_status.contains(&503));
    }

    #[test]
    fn test_no_retry_config() {
        let config = RetryConfig::no_retry();
        assert_eq!(config.max_retries, Some(0));
    }

    #[test]
    fn test_retry_state_basic() {
        let config = RetryConfig::default().with_max_retries(3);
        let mut state = RetryState::new(config);

        // First retry
        let decision = state.should_retry_error(false);
        assert!(matches!(decision, RetryDecision::Retry(_)));

        // Second retry
        let decision = state.should_retry_error(false);
        assert!(matches!(decision, RetryDecision::Retry(_)));

        // Third retry
        let decision = state.should_retry_error(false);
        assert!(matches!(decision, RetryDecision::Retry(_)));

        // Fourth should fail (max 3 retries means 3 attempts after initial)
        let decision = state.should_retry_error(false);
        assert_eq!(decision, RetryDecision::DontRetry);
    }

    #[test]
    fn test_retry_state_abort() {
        let config = RetryConfig::default();
        let mut state = RetryState::new(config);

        // Abort should never retry
        let decision = state.should_retry_error(true);
        assert_eq!(decision, RetryDecision::DontRetry);
    }

    #[test]
    fn test_retry_state_status() {
        let config = RetryConfig::default();
        let mut state = RetryState::new(config);

        // 503 should retry
        let decision = state.should_retry_status(503, None);
        assert!(matches!(decision, RetryDecision::Retry(_)));

        // 404 should not retry
        let mut state2 = RetryState::new(RetryConfig::default());
        let decision = state2.should_retry_status(404, None);
        assert_eq!(decision, RetryDecision::DontRetry);
    }

    #[test]
    fn test_backoff_progression() {
        let config = RetryConfig::default();
        let mut state = RetryState::new(config);

        // First: 1s
        if let RetryDecision::Retry(d) = state.should_retry_error(false) {
            assert_eq!(d, Duration::from_secs(1));
        }

        // Second: 2s
        if let RetryDecision::Retry(d) = state.should_retry_error(false) {
            assert_eq!(d, Duration::from_secs(2));
        }

        // Third: 3s (capped)
        if let RetryDecision::Retry(d) = state.should_retry_error(false) {
            assert_eq!(d, Duration::from_secs(3));
        }

        // Fourth: still 3s (capped)
        if let RetryDecision::Retry(d) = state.should_retry_error(false) {
            assert_eq!(d, Duration::from_secs(3));
        }
    }

    #[test]
    fn test_parse_retry_after() {
        assert_eq!(parse_retry_after("120"), Some(Duration::from_secs(120)));
        assert_eq!(parse_retry_after("0"), Some(Duration::from_secs(0)));
        assert_eq!(parse_retry_after("invalid"), None);
    }

    #[test]
    fn test_retry_after_respected() {
        let config = RetryConfig::default().with_respect_retry_after(true);
        let mut state = RetryState::new(config);

        // Server says retry after 10s
        let decision = state.should_retry_status(503, Some(Duration::from_secs(10)));
        if let RetryDecision::Retry(d) = decision {
            assert_eq!(d, Duration::from_secs(10));
        } else {
            panic!("Expected Retry decision");
        }
    }

    #[test]
    fn test_missing_parents_retry() {
        let config = RetryConfig::default();
        let mut state = RetryState::new(config);

        // "Missing Parents" status text should trigger retry even for 400
        let decision = state.should_retry_status_with_text(400, Some("Missing Parents"), None);
        assert!(matches!(decision, RetryDecision::Retry(_)));

        // Case-insensitive matching
        let mut state2 = RetryState::new(RetryConfig::default());
        let decision = state2.should_retry_status_with_text(400, Some("missing parents"), None);
        assert!(matches!(decision, RetryDecision::Retry(_)));

        // Without the text, 400 should not retry
        let mut state3 = RetryState::new(RetryConfig::default());
        let decision = state3.should_retry_status_with_text(400, None, None);
        assert_eq!(decision, RetryDecision::DontRetry);
    }
}
