//! Remote synchronization - Braid protocol implementation
//!
//! This module handles syncing with remote Braid servers.
//! It depends on `local` for filesystem operations but `local` has no dependencies on `sync`.

pub mod binary;
pub mod engine;
pub mod subscription;

pub use binary::BinarySyncManager;
pub use engine::sync_local_to_remote;
pub use subscription::spawn_subscription;
