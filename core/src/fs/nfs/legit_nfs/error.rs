//! NFS error types

/// NFSv3 status codes (RFC 1813)
/// Note: Naming follows RFC 1813 spec, not Rust conventions
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum nfsstat3 {
    OK = 0,
    ERR_PERM = 1,
    ERR_NOENT = 2,
    ERR_IO = 5,
    ERR_NXIO = 6,
    ERR_ACCES = 13,
    ERR_EXIST = 17,
    ERR_XDEV = 18,
    ERR_NODEV = 19,
    ERR_NOTDIR = 20,
    ERR_ISDIR = 21,
    ERR_INVAL = 22,
    ERR_FBIG = 27,
    ERR_NOSPC = 28,
    ERR_ROFS = 30,
    ERR_MLINK = 31,
    ERR_NAMETOOLONG = 63,
    ERR_NOTEMPTY = 66,
    ERR_DQUOT = 69,
    ERR_STALE = 70,
    ERR_REMOTE = 71,
    ERR_BADHANDLE = 10001,
    ERR_NOT_SYNC = 10002,
    ERR_BAD_COOKIE = 10003,
    ERR_NOTSUPP = 10004,
    ERR_TOOSMALL = 10005,
    ERR_SERVERFAULT = 10006,
    ERR_BADTYPE = 10007,
    ERR_JUKEBOX = 10008,
}

impl From<nfsstat3> for u32 {
    fn from(val: nfsstat3) -> u32 {
        val as u32
    }
}
