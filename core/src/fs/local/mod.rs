//! Local filesystem operations - no network code
//!
//! This module handles all local filesystem interactions.
//! It has no dependencies on sync/ or server/ modules.

pub mod diff;
pub mod mapper;
pub mod scanner;
pub mod versions;
pub mod watcher;

pub use diff::compute_patches;
pub use mapper::{url_to_path, path_to_url};
pub use scanner::{start_scan_loop, ScanState};
pub use versions::VersionStore;
pub use watcher::handle_fs_event;
