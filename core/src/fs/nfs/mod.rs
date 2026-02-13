//! Braid NFS Backend
//!
//! This module exports the legit-nfs implementation and the BraidNfsBackend.

pub mod legit_nfs;

// Export legit-nfs types
pub use legit_nfs::error::nfsstat3;
pub use legit_nfs::fs::{FileHandle, FileHandleManager, FsCapabilities, NfsFilesystem, ReadResult, WriteResult};
pub use legit_nfs::nfs::types::*;
pub use legit_nfs::server::NfsServer;

// Export BraidNfsBackend
mod backend;
pub use backend::BraidNfsBackend;
