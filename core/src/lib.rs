//! braid_rs: Unified Braid Protocol implementation in Rust.
//!
//! This crate consolidates several Braid-related components into a single library:
//!
//! - **core**: The core Braid-HTTP protocol implementation (types, parser, client, server).
//! - **antimatter**: Conflict resolution and state management.
//! - **blob**: Braid-Blob storage and synchronization service.
//! - **fs**: Filesystem synchronization client logic.

pub mod core;
pub mod vendor;

#[cfg(feature = "antimatter")]
pub mod antimatter;

#[cfg(feature = "blob")]
pub mod blob;

#[cfg(feature = "fs")]
pub mod fs;

// Top-level re-exports for common usage
pub use crate::core::error::{BraidError, Result};
pub use crate::core::types;
pub use crate::core::types::{BraidRequest, BraidResponse, Patch, Update, Version};

#[cfg(feature = "client")]
pub use crate::core::client;
#[cfg(feature = "client")]
pub use crate::core::client::{BraidClient, ClientConfig, Subscription};

#[cfg(feature = "server")]
pub use crate::core::server;
#[cfg(feature = "server")]
pub use crate::core::server::{BraidLayer, BraidState, ConflictResolver, ServerConfig};

#[cfg(not(target_arch = "wasm32"))]
pub use crate::core::merge;

#[cfg(feature = "antimatter")]
pub use crate::antimatter::AntimatterCrdt;

#[cfg(feature = "blob")]
pub use crate::blob::BlobStore;
