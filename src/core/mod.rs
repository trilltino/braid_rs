//! Braid HTTP Protocol Implementation for Rust
//!
//! A complete Rust implementation of the Braid-HTTP protocol for
//! collaborative state synchronization with full Axum integration.
//!
//! # Overview
//!
//! Braid-HTTP extends HTTP from a state transfer protocol into a state
//! synchronization protocol, enabling:
//!
//! - **Version Tracking**: Track resource history as a DAG
//! - **Live Subscriptions**: Stream updates in real-time via HTTP 209
//! - **Incremental Updates**: Send patches instead of full snapshots
//! - **Conflict Resolution**: Handle concurrent edits with merge types
//!
//! # Modules
//!
//! - [`client`] - HTTP client with Braid protocol support
//! - [`server`] - Axum middleware and handlers for Braid servers
//! - [`protocol`] - Core protocol types, headers, and parsing
//! - [`types`] - Version, Update, Patch, and request/response types
//! - [`merge`] - Diamond-types CRDT integration
//!
//! # Quick Start
//!
//! ```ignore
//! use crate::core::{BraidClient, BraidRequest, Version};
//!
//! // Create a client
//! let client = BraidClient::new();
//!
//! // Make a versioned request
//! let request = BraidRequest::new()
//!     .with_version(Version::new("v1"))
//!     .subscribe();
//!
//! // Subscribe to updates
//! let mut subscription = client.subscribe("http://example.com/doc", request).await?;
//! while let Some(update) = subscription.next().await {
//!     println!("Received update: {:?}", update);
//! }
//! ```

pub mod client;
pub mod error;
// pub mod fs; // Moved to braidfs crate
pub mod merge;
pub mod protocol;
#[cfg(feature = "server")]
pub mod server;
pub mod types;

// Re-export commonly used types at crate root
pub use error::{BraidError, Result};
pub use types::{BraidRequest, BraidResponse, Patch, Update, Version};

// Re-export client types (works on all platforms)
#[cfg(feature = "client")]
pub use client::{
    BraidClient, ClientConfig, HeartbeatConfig, Message, MessageParser, ParseState, RetryConfig,
    RetryDecision, RetryState, Subscription, SubscriptionStream,
};

// Re-export server types
#[cfg(feature = "server")]
pub use server::{BraidLayer, BraidState, ServerConfig};
