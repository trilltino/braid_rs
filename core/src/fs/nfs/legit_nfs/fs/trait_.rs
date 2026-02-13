//! NfsFilesystem trait - the interface between NFS protocol and actual filesystem

use crate::fs::nfs::legit_nfs::error::nfsstat3;
use crate::fs::nfs::legit_nfs::fs::handle::FileHandle;
use crate::fs::nfs::legit_nfs::nfs::types::*;
use async_trait::async_trait;

/// Result of a read operation
#[derive(Debug)]
pub struct ReadResult {
    pub data: Vec<u8>,
    pub attr: fattr3,
    pub eof: bool,
}

/// Result of a write operation
#[derive(Debug)]
pub struct WriteResult {
    pub count: u32,
    pub attr: fattr3,
    pub committed: StableHow,
    pub verf: u64, // Write verifier
}

/// Filesystem capabilities
#[derive(Clone, Copy, Debug)]
pub struct FsCapabilities {
    pub read_only: bool,
    pub supports_symlinks: bool,
    pub supports_hardlinks: bool,
    pub case_insensitive: bool,
}

impl Default for FsCapabilities {
    fn default() -> Self {
        Self {
            read_only: false,
            supports_symlinks: false,
            supports_hardlinks: false,
            case_insensitive: false,
        }
    }
}

/// Core trait for NFS filesystem implementations
#[async_trait]
pub trait NfsFilesystem: Send + Sync + 'static {
    /// Get filesystem capabilities
    fn capabilities(&self) -> FsCapabilities {
        FsCapabilities::default()
    }

    /// Get root file handle
    fn root_handle(&self) -> FileHandle;

    /// Lookup a name in a directory
    async fn lookup(&self, dir: FileHandle, name: &str) -> NfsResult<FileHandle>;

    /// Get file attributes
    async fn getattr(&self, handle: FileHandle) -> NfsResult<fattr3>;

    /// Set file attributes
    async fn setattr(&self, handle: FileHandle, attr: sattr3) -> NfsResult<fattr3>;

    /// Read from a file
    async fn read(&self, handle: FileHandle, offset: u64, count: u32) -> NfsResult<ReadResult>;

    /// Write to a file
    async fn write(
        &self,
        handle: FileHandle,
        offset: u64,
        data: &[u8],
        stable: StableHow,
    ) -> NfsResult<WriteResult>;

    /// Create a regular file
    async fn create(
        &self,
        dir: FileHandle,
        name: &str,
        attr: sattr3,
        mode: CreateMode,
    ) -> NfsResult<(FileHandle, fattr3)>;

    /// Create a directory
    async fn mkdir(&self, dir: FileHandle, name: &str, attr: sattr3) -> NfsResult<(FileHandle, fattr3)>;

    /// Remove a file
    async fn remove(&self, dir: FileHandle, name: &str) -> NfsResult<()>;

    /// Remove a directory
    async fn rmdir(&self, dir: FileHandle, name: &str) -> NfsResult<()>;

    /// Rename a file or directory
    async fn rename(
        &self,
        from_dir: FileHandle,
        from_name: &str,
        to_dir: FileHandle,
        to_name: &str,
    ) -> NfsResult<()>;

    /// Read directory entries
    async fn readdir(
        &self,
        dir: FileHandle,
        cookie: u64,
        count: u32,
    ) -> NfsResult<ReadDirResult>;

    /// Read directory entries with attributes
    async fn readdirplus(
        &self,
        dir: FileHandle,
        cookie: u64,
        count: u32,
    ) -> NfsResult<ReadDirPlusResult>;

    /// Create a symbolic link
    async fn symlink(
        &self,
        dir: FileHandle,
        name: &str,
        link_text: &str,
        attr: sattr3,
    ) -> NfsResult<(FileHandle, fattr3)>;

    /// Read a symbolic link
    async fn readlink(&self, handle: FileHandle) -> NfsResult<String>;

    /// Create a hard link
    async fn link(&self, handle: FileHandle, dir: FileHandle, name: &str) -> NfsResult<fattr3>;

    /// Create a special file (block/char device, fifo, socket)
    async fn mknod(
        &self,
        dir: FileHandle,
        name: &str,
        ftype: ftype3,
        attr: sattr3,
        rdev: specdata3,
    ) -> NfsResult<(FileHandle, fattr3)>;

    /// Check access permissions
    async fn access(&self, handle: FileHandle, access: u32) -> NfsResult<u32>;

    /// Get filesystem statistics
    async fn fsstat(&self, handle: FileHandle) -> NfsResult<fsstat3>;

    /// Get filesystem info
    async fn fsinfo(&self, handle: FileHandle) -> NfsResult<fsinfo3>;

    /// Get path configuration
    async fn pathconf(&self, handle: FileHandle) -> NfsResult<pathconf3>;

    /// Commit cached writes to stable storage
    async fn commit(&self, handle: FileHandle, offset: u64, count: u32) -> NfsResult<fattr3>;
}

type NfsResult<T> = Result<T, nfsstat3>;
