//! Configuration for the Braid HTTP client.
//!
//! This module defines the [`ClientConfig`] struct that controls the behavior
//! of the Braid HTTP client, including retry policies, timeouts, logging,
//! and subscription limits.
//!
//! # Configuration Options
//!
//! | Option | Default | Description |
//! |--------|---------|-------------|
//! | `max_retries` | 3 | Maximum retry attempts |
//! | `retry_delay_ms` | 1000 | Base delay between retries |
//! | `connection_timeout_secs` | 30 | Connection timeout |
//! | `enable_logging` | false | Enable request logging |
//! | `max_subscriptions` | 100 | Max concurrent subscriptions |
//!
//! # Examples
//!
//! ## Default Configuration
//!
//! ```
//! use crate::core::client::ClientConfig;
//!
//! let config = ClientConfig::default();
//! assert_eq!(config.max_retries, 3);
//! assert_eq!(config.retry_delay_ms, 1000);
//! ```
//!
//! ## Custom Configuration
//!
//! ```
//! use crate::core::client::ClientConfig;
//!
//! let config = ClientConfig {
//!     max_retries: 5,
//!     retry_delay_ms: 2000,
//!     connection_timeout_secs: 60,
//!     enable_logging: true,
//!     max_subscriptions: 50,
//! };
//! ```
//!
//! ## Partial Override
//!
//! ```
//! use crate::core::client::ClientConfig;
//!
//! let config = ClientConfig {
//!     max_retries: 5,
//!     ..Default::default()
//! };
//! assert_eq!(config.max_retries, 5);
//! assert_eq!(config.retry_delay_ms, 1000); // Default
//! ```

/// Configuration for the Braid HTTP client.
///
/// Controls retry behavior, timeouts, logging, and subscription limits
/// for the Braid client instance.
///
/// # Example
///
/// ```
/// use crate::core::client::ClientConfig;
///
/// let config = ClientConfig {
///     max_retries: 5,
///     retry_delay_ms: 2000,
///     ..Default::default()
/// };
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientConfig {
    /// Maximum retries for failed requests.
    ///
    /// When a request fails with a retryable error (e.g., 503 Service Unavailable),
    /// the client will retry up to this many times before returning an error.
    pub max_retries: u32,

    /// Base retry delay in milliseconds.
    ///
    /// The actual delay uses exponential backoff: `delay = base * 2^attempt`.
    /// For example, with base 1000ms: 1s, 2s, 4s, 8s, ...
    pub retry_delay_ms: u64,

    /// Connection timeout in seconds.
    ///
    /// Maximum time to wait for a connection to be established.
    pub connection_timeout_secs: u64,

    /// Enable request logging.
    ///
    /// When enabled, logs request/response details using the `tracing` crate.
    pub enable_logging: bool,

    /// Maximum concurrent subscriptions.
    ///
    /// Limits the number of active subscription connections to prevent
    /// resource exhaustion.
    pub max_subscriptions: usize,

    /// Threshold for auto-multiplexing.
    ///
    /// When the number of active subscriptions to a host exceeds this value,
    /// the client will automatically switch to using a multiplexed connection.
    /// Default: 3.
    pub auto_multiplex_threshold: usize,

    /// Enable multiplexing for subscription requests.
    ///
    /// Matches JS `braid_fetch.enable_multiplex` setting.
    /// Default: `true`.
    pub enable_multiplex: bool,

    /// Proxy URL (optional).
    ///
    /// If set, requests will be routed through this proxy.
    pub proxy_url: String,

    /// Request timeout in milliseconds.
    ///
    /// Maximum time to wait for a request to complete.
    pub request_timeout_ms: u64,

    /// Maximum total connections in the pool.
    pub max_total_connections: u32,
}

impl Default for ClientConfig {
    fn default() -> Self {
        ClientConfig {
            max_retries: 3,
            retry_delay_ms: 1000,
            connection_timeout_secs: 30,
            enable_logging: false,
            max_subscriptions: 100,
            auto_multiplex_threshold: 3,
            enable_multiplex: true,
            proxy_url: String::new(),
            request_timeout_ms: 30000,
            max_total_connections: 100,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ClientConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 1000);
        assert_eq!(config.connection_timeout_secs, 30);
        assert!(!config.enable_logging);
        assert_eq!(config.max_subscriptions, 100);
    }

    #[test]
    fn test_custom_config() {
        let config = ClientConfig {
            max_retries: 5,
            retry_delay_ms: 2000,
            connection_timeout_secs: 60,
            enable_logging: true,
            max_subscriptions: 50,
            auto_multiplex_threshold: 5,
            enable_multiplex: true,
            proxy_url: "http://proxy".to_string(),
            request_timeout_ms: 1000,
            max_total_connections: 50,
        };
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.retry_delay_ms, 2000);
        assert!(config.enable_logging);
    }

    #[test]
    fn test_partial_override() {
        let config = ClientConfig {
            max_retries: 10,
            ..Default::default()
        };
        assert_eq!(config.max_retries, 10);
        assert_eq!(config.retry_delay_ms, 1000);
    }

    #[test]
    fn test_clone() {
        let config = ClientConfig::default();
        let cloned = config.clone();
        assert_eq!(config, cloned);
    }

    #[test]
    fn test_debug() {
        let config = ClientConfig::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("ClientConfig"));
        assert!(debug.contains("max_retries"));
    }
}
