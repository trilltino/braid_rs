//! Braid HTTP server implementation.
//!
//! This module provides server-side support for the Braid protocol using Axum.
//! It enables applications to:
//!
//! - **Track resource versions** as a directed acyclic graph (DAG)
//! - **Send updates** as snapshots or patches
//! - **Stream subscription updates** to clients (HTTP 209)
//! - **Handle version conflicts** with merge strategies
//! - **Manage resource state** with CRDT support
//!
//! # Module Organization
//!
//! ```text
//! server/
//! ├── middleware        - BraidLayer and BraidState extractor
//! ├── send_update       - SendUpdateExt trait for responses
//! ├── parse_update      - ParseUpdateExt trait for requests
//! ├── config            - ServerConfig options
//! ├── resource_state    - ResourceStateManager for CRDT state
//! └── conflict_resolver - ConflictResolver for merging
//! ```
//!
//! # Key Types
//!
//! | Type | Description |
//! |------|-------------|
//! | [`BraidLayer`] | Axum middleware layer |
//! | [`BraidState`] | Extracted Braid request state |
//! | [`ServerConfig`] | Server configuration options |
//! | [`ResourceStateManager`] | CRDT-backed resource state |
//! | [`ConflictResolver`] | Version conflict resolution |
//!
//! # Examples
//!
//! ## Creating a BraidLayer
//!
//! ```
//! use crate::core::server::{BraidLayer, ServerConfig};
//!
//! // Default configuration
//! let layer = BraidLayer::new();
//!
//! // Custom configuration
//! let config = ServerConfig {
//!     enable_subscriptions: true,
//!     max_subscription_duration_secs: 3600,
//!     ..Default::default()
//! };
//! let layer = BraidLayer::with_config(config);
//! ```
//!
//! ## Using BraidState
//!
//! ```
//! use crate::core::server::BraidState;
//! use crate::core::Version;
//! use axum::http::HeaderMap;
//!
//! // BraidState is extracted from request headers
//! let headers = HeaderMap::new();
//! let state = BraidState::from_headers(&headers);
//!
//! // Check subscription mode
//! if state.subscribe {
//!     // Handle subscription
//! }
//!
//! // Access version information
//! if let Some(versions) = &state.version {
//!     for v in versions {
//!         println!("Version: {}", v);
//!     }
//! }
//! ```
//!
//! ## Resource State Management
//!
//! ```
//! use crate::core::server::ResourceStateManager;
//!
//! let manager = ResourceStateManager::new();
//!
//! // Get or create a resource with an agent ID
//! let resource = manager.get_or_create_resource("my-resource", "agent-1", None);
//!
//! // Access the resource through the lock
//! {
//!     let mut state = resource.write();
//!     state.crdt.add_insert(0, "Hello");
//! }
//!
//! // Get current content
//! let content = resource.read().crdt.content();
//! assert_eq!(content, "Hello");
//! ```
//!
//! ## Conflict Resolution
//!
//! ```
//! use crate::core::server::{ConflictResolver, ResourceStateManager};
//!
//! let manager = ResourceStateManager::new();
//! let resolver = ConflictResolver::new(manager);
//!
//! // Resolver handles diamond-types CRDT merging
//! // for collaborative text editing
//! ```
//!
//! # HTTP Status Codes
//!
//! | Code | Constant | Description |
//! |------|----------|-------------|
//! | 200 | - | Standard response |
//! | 206 | - | Partial content (patches) |
//! | 209 | `STATUS_SUBSCRIPTION` | Subscription response |
//! | 293 | `STATUS_MERGE_CONFLICT` | Merge conflict |
//! | 410 | `STATUS_GONE` | History dropped |
//!
//! # Specification
//!
//! Based on [draft-toomim-httpbis-braid-http-04]:
//!
//! - **Section 2**: Versioning with DAG support
//! - **Section 3**: Updates as patches or snapshots
//! - **Section 4**: Subscriptions with HTTP 209 status
//!
//! [draft-toomim-httpbis-braid-http-04]: https://datatracker.ietf.org/doc/html/draft-toomim-httpbis-braid-http

mod config;
mod middleware;
mod parse_update;
mod send_update;
pub mod utils;

pub mod conflict_resolver;
pub mod multiplex;
pub mod resource_state;
pub mod subscription;

#[cfg(test)]
mod tests;

pub use config::ServerConfig;
pub use conflict_resolver::ConflictResolver;
pub use middleware::{BraidLayer, BraidState, IsFirefox};
pub use parse_update::ParseUpdateExt;
pub use resource_state::ResourceStateManager;
pub use send_update::{SendUpdateExt, UpdateResponse};

use crate::core::types::Update;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Broadcast channel for sending updates to multiple subscribers.
///
/// Use this type to broadcast updates to all connected subscription clients.
///
/// # Example
///
/// ```
/// use crate::core::server::UpdateBroadcast;
/// use crate::core::{Update, Version};
/// use std::sync::Arc;
/// use tokio::sync::broadcast;
///
/// let (tx, _rx): (UpdateBroadcast, _) = broadcast::channel(100);
///
/// // Broadcast an update
/// let update = Update::snapshot(Version::new("v1"), "data");
/// let _ = tx.send(Arc::new(update));
/// ```
pub type UpdateBroadcast = broadcast::Sender<Arc<Update>>;
