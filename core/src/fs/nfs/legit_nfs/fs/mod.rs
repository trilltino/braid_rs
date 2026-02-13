//! Filesystem abstraction

pub mod handle;
pub mod trait_;

pub use handle::{FileHandle, FileHandleManager};
pub use trait_::{FsCapabilities, NfsFilesystem, ReadResult, WriteResult};
