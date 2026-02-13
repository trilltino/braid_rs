//! NFSv3 procedure numbers (RFC 1813)

pub const NFSPROC3_NULL: u32 = 0;
pub const NFSPROC3_GETATTR: u32 = 1;
pub const NFSPROC3_SETATTR: u32 = 2;
pub const NFSPROC3_LOOKUP: u32 = 3;
pub const NFSPROC3_ACCESS: u32 = 4;
pub const NFSPROC3_READLINK: u32 = 5;
pub const NFSPROC3_READ: u32 = 6;
pub const NFSPROC3_WRITE: u32 = 7;
pub const NFSPROC3_CREATE: u32 = 8;
pub const NFSPROC3_MKDIR: u32 = 9;
pub const NFSPROC3_SYMLINK: u32 = 10;
pub const NFSPROC3_MKNOD: u32 = 11;
pub const NFSPROC3_REMOVE: u32 = 12;
pub const NFSPROC3_RMDIR: u32 = 13;
pub const NFSPROC3_RENAME: u32 = 14;
pub const NFSPROC3_LINK: u32 = 15;
pub const NFSPROC3_READDIR: u32 = 16;
pub const NFSPROC3_READDIRPLUS: u32 = 17;
pub const NFSPROC3_FSSTAT: u32 = 18;
pub const NFSPROC3_FSINFO: u32 = 19;
pub const NFSPROC3_PATHCONF: u32 = 20;
pub const NFSPROC3_COMMIT: u32 = 21;

/// NFSv3 file handle size (fixed 64-bit)
pub const NFS3_FHSIZE: usize = 8;

/// NFSv3 create modes
pub const NFS3_CREATE_UNCHECKED: u32 = 0;
pub const NFS3_CREATE_GUARDED: u32 = 1;
pub const NFS3_CREATE_EXCLUSIVE: u32 = 2;

/// NFSv3 stable how
pub const NFS3_UNSTABLE: u32 = 0;
pub const NFS3_DATA_SYNC: u32 = 1;
pub const NFS3_FILE_SYNC: u32 = 2;
