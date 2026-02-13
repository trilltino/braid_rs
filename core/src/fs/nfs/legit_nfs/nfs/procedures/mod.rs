//! NFSv3 procedure handlers

// 13 Core procedures for P2P sync
pub mod null;
pub mod getattr;
pub mod lookup;
pub mod read;
pub mod write;
pub mod create;
pub mod mkdir;
pub mod remove;
pub mod rmdir;
pub mod rename;
pub mod readdir;
pub mod readdirplus;
pub mod fsstat;
pub mod fsinfo;

// Note: 9 stub procedures (setattr, access, readlink, symlink, mknod, link, pathconf, commit)
// return NOTSUPP directly in handlers.rs without separate modules
