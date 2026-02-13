//! # BraidFS - Braid Filesystem Synchronization
//!
//! BraidFS provides bidirectional file synchronization with Braid servers.
//!
//! ## Module Organization
//!
//! - `common/` - Shared utilities (config, rate limiting, activity tracking)
//! - `local/` - Local filesystem operations (watcher, scanner, versions)
//! - `sync/` - Remote synchronization (Braid protocol)
//! - `server/` - HTTP API server for management
//! - `nfs/` - NFS server for P2P sync
//! - `state.rs` - Daemon state and commands
//! - `daemon.rs` - Main event loop
//! - `error.rs` - Error types

pub mod common;
pub mod daemon;
pub mod error;
pub mod local;
pub mod nfs;
pub mod server;
pub mod state;
pub mod sync;

// Re-export main entry point
pub use daemon::run_daemon;

// Re-export commonly used types
pub use common::Config;
pub use state::{Command, DaemonState};
