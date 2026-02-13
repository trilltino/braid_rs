//! MOUNT protocol types

/// MOUNT procedure numbers
pub const MOUNTPROC3_NULL: u32 = 0;
pub const MOUNTPROC3_MNT: u32 = 1;
pub const MOUNTPROC3_DUMP: u32 = 2;
pub const MOUNTPROC3_UMNT: u32 = 3;
pub const MOUNTPROC3_UMNTALL: u32 = 4;
pub const MOUNTPROC3_EXPORT: u32 = 5;

/// Mount status
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MountStat3 {
    Ok = 0,
    ErrPerm = 1,
    ErrNoEnt = 2,
    ErrIo = 5,
    ErrAcces = 13,
    ErrNotDir = 20,
    ErrInval = 22,
    ErrNameTooLong = 63,
    ErrNotSupp = 10004,
    ErrServerFault = 10006,
}

impl From<MountStat3> for u32 {
    fn from(val: MountStat3) -> u32 {
        val as u32
    }
}
