//! Filesystem module errors

use crate::core::BraidError;

/// Result type for filesystem operations
pub type Result<T> = std::result::Result<T, BraidError>;

/// File system specific error helpers
#[derive(Debug, thiserror::Error)]
pub enum FsError {
    #[error("Path mapping error: {0}")]
    PathMapping(String),
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Sync error: {0}")]
    Sync(String),
    
    #[error("NFS error: {0}")]
    Nfs(String),
}

impl From<FsError> for BraidError {
    fn from(e: FsError) -> Self {
        BraidError::Fs(e.to_string())
    }
}
