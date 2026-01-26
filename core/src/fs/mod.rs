//! # BraidFS Core Logic
//!
//! This module implements the synchronization daemon.

pub mod api;
pub mod binary_sync;
pub mod config;
pub mod daemon;
pub mod diff;
pub mod mapping;
pub mod rate_limiter;
pub mod scanner;
pub mod server_handlers;
pub mod state;
pub mod subscription;
pub mod sync;
pub mod versions;
pub mod watcher;

pub use daemon::{run_daemon, PEER_ID};
pub use state::{ActivityTracker, DaemonState, PendingWrites};
