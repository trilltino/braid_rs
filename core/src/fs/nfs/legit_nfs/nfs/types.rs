//! NFSv3 types (RFC 1813)
//! 
//! Note: Type names follow RFC 1813 spec (lowercase with numbers), not Rust naming conventions.
#![allow(non_camel_case_types)]

use crate::fs::nfs::legit_nfs::error::nfsstat3;
use crate::fs::nfs::legit_nfs::fs::handle::FileHandle;
use crate::fs::nfs::legit_nfs::xdr::{XdrDecodable, XdrDecoder, XdrEncodable, XdrEncoder};

/// File types
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ftype3 {
    NF3REG = 1,   // Regular file
    NF3DIR = 2,   // Directory
    NF3BLK = 3,   // Block device
    NF3CHR = 4,   // Character device
    NF3LNK = 5,   // Symbolic link
    NF3SOCK = 6,  // Socket
    NF3FIFO = 7,  // Named pipe
}

impl TryFrom<u32> for ftype3 {
    type Error = ();
    fn try_from(val: u32) -> Result<Self, ()> {
        match val {
            1 => Ok(ftype3::NF3REG),
            2 => Ok(ftype3::NF3DIR),
            3 => Ok(ftype3::NF3BLK),
            4 => Ok(ftype3::NF3CHR),
            5 => Ok(ftype3::NF3LNK),
            6 => Ok(ftype3::NF3SOCK),
            7 => Ok(ftype3::NF3FIFO),
            _ => Err(()),
        }
    }
}

impl From<ftype3> for u32 {
    fn from(val: ftype3) -> u32 {
        val as u32
    }
}

/// NFS time structure
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct nfstime3 {
    pub seconds: u32,
    pub nseconds: u32,
}

impl XdrEncodable for nfstime3 {
    fn encode(&self, encoder: &mut XdrEncoder) -> Result<(), nfsstat3> {
        encoder.encode_u32(self.seconds);
        encoder.encode_u32(self.nseconds);
        Ok(())
    }
}

impl XdrDecodable for nfstime3 {
    fn decode(decoder: &mut XdrDecoder) -> Result<Self, nfsstat3> {
        Ok(nfstime3 {
            seconds: decoder.decode_u32()?,
            nseconds: decoder.decode_u32()?,
        })
    }
}

/// Special device data
#[derive(Clone, Copy, Debug, Default)]
pub struct specdata3 {
    pub specdata1: u32,
    pub specdata2: u32,
}

impl XdrEncodable for specdata3 {
    fn encode(&self, encoder: &mut XdrEncoder) -> Result<(), nfsstat3> {
        encoder.encode_u32(self.specdata1);
        encoder.encode_u32(self.specdata2);
        Ok(())
    }
}

impl XdrDecodable for specdata3 {
    fn decode(decoder: &mut XdrDecoder) -> Result<Self, nfsstat3> {
        Ok(specdata3 {
            specdata1: decoder.decode_u32()?,
            specdata2: decoder.decode_u32()?,
        })
    }
}

/// File attributes
#[derive(Clone, Debug)]
pub struct fattr3 {
    pub ftype: ftype3,
    pub mode: u32,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    pub used: u64,
    pub rdev: specdata3,
    pub fsid: u64,
    pub fileid: u64,
    pub atime: nfstime3,
    pub mtime: nfstime3,
    pub ctime: nfstime3,
}

impl XdrEncodable for fattr3 {
    fn encode(&self, encoder: &mut XdrEncoder) -> Result<(), nfsstat3> {
        encoder.encode_enum(self.ftype);
        encoder.encode_u32(self.mode);
        encoder.encode_u32(self.nlink);
        encoder.encode_u32(self.uid);
        encoder.encode_u32(self.gid);
        encoder.encode_u64(self.size);
        encoder.encode_u64(self.used);
        self.rdev.encode(encoder)?;
        encoder.encode_u64(self.fsid);
        encoder.encode_u64(self.fileid);
        self.atime.encode(encoder)?;
        self.mtime.encode(encoder)?;
        self.ctime.encode(encoder)?;
        Ok(())
    }
}

impl XdrDecodable for fattr3 {
    fn decode(decoder: &mut XdrDecoder) -> Result<Self, nfsstat3> {
        Ok(fattr3 {
            ftype: decoder.decode_enum(|v| ftype3::try_from(v).ok())?,
            mode: decoder.decode_u32()?,
            nlink: decoder.decode_u32()?,
            uid: decoder.decode_u32()?,
            gid: decoder.decode_u32()?,
            size: decoder.decode_u64()?,
            used: decoder.decode_u64()?,
            rdev: specdata3::decode(decoder)?,
            fsid: decoder.decode_u64()?,
            fileid: decoder.decode_u64()?,
            atime: nfstime3::decode(decoder)?,
            mtime: nfstime3::decode(decoder)?,
            ctime: nfstime3::decode(decoder)?,
        })
    }
}

/// Settable attributes
#[derive(Clone, Debug, Default)]
pub struct sattr3 {
    pub mode: Option<u32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub size: Option<u64>,
    pub atime: Option<nfstime3>,
    pub mtime: Option<nfstime3>,
}

impl XdrDecodable for sattr3 {
    fn decode(decoder: &mut XdrDecoder) -> Result<Self, nfsstat3> {
        Ok(sattr3 {
            mode: decoder.decode_option(|d| d.decode_u32())?,
            uid: decoder.decode_option(|d| d.decode_u32())?,
            gid: decoder.decode_option(|d| d.decode_u32())?,
            size: decoder.decode_option(|d| d.decode_u64())?,
            atime: decoder.decode_option(nfstime3::decode)?,
            mtime: decoder.decode_option(nfstime3::decode)?,
        })
    }
}

/// Post-operation attributes (optional)
#[derive(Clone, Debug)]
pub enum post_op_attr {
    None,
    Some(fattr3),
}

impl XdrEncodable for post_op_attr {
    fn encode(&self, encoder: &mut XdrEncoder) -> Result<(), nfsstat3> {
        match self {
            post_op_attr::None => encoder.encode_bool(false),
            post_op_attr::Some(attr) => {
                encoder.encode_bool(true);
                attr.encode(encoder)?;
            }
        }
        Ok(())
    }
}

/// Pre-operation attributes
#[derive(Clone, Debug)]
pub struct pre_op_attr {
    pub size: u64,
    pub mtime: nfstime3,
    pub ctime: nfstime3,
}

impl XdrEncodable for pre_op_attr {
    fn encode(&self, encoder: &mut XdrEncoder) -> Result<(), nfsstat3> {
        encoder.encode_u64(self.size);
        self.mtime.encode(encoder)?;
        self.ctime.encode(encoder)?;
        Ok(())
    }
}

/// Weak cache consistency data
#[derive(Clone, Debug)]
pub struct wcc_data {
    pub before: Option<pre_op_attr>,
    pub after: post_op_attr,
}

impl XdrEncodable for wcc_data {
    fn encode(&self, encoder: &mut XdrEncoder) -> Result<(), nfsstat3> {
        encoder.encode_option(&self.before)?;
        self.after.encode(encoder)?;
        Ok(())
    }
}

/// Write stability levels
#[repr(u32)]
#[derive(Clone, Copy, Debug)]
pub enum StableHow {
    Unstable = 0,
    DataSync = 1,
    FileSync = 2,
}

impl TryFrom<u32> for StableHow {
    type Error = ();
    fn try_from(val: u32) -> Result<Self, ()> {
        match val {
            0 => Ok(StableHow::Unstable),
            1 => Ok(StableHow::DataSync),
            2 => Ok(StableHow::FileSync),
            _ => Err(()),
        }
    }
}

impl From<StableHow> for u32 {
    fn from(val: StableHow) -> u32 {
        val as u32
    }
}

/// Directory entry
#[derive(Clone, Debug)]
pub struct DirEntry {
    pub fileid: u64,
    pub name: String,
    pub cookie: u64,
}

/// Directory entry with attributes (for READDIRPLUS)
#[derive(Clone, Debug)]
pub struct DirEntryPlus {
    pub fileid: u64,
    pub name: String,
    pub cookie: u64,
    pub attr: post_op_attr,
    pub handle: Option<FileHandle>,
}

/// Directory listing result
#[derive(Clone, Debug)]
pub struct ReadDirResult {
    pub entries: Vec<DirEntry>,
    pub end: bool,
}

/// Directory listing result (plus)
#[derive(Clone, Debug)]
pub struct ReadDirPlusResult {
    pub entries: Vec<DirEntryPlus>,
    pub end: bool,
}

/// NFS string (Vec<u8> for UTF-8 compatibility)
pub type nfsstring = Vec<u8>;

/// Time setting (SETATTR)
#[derive(Clone, Debug)]
pub enum TimeSetting {
    DontChange,
    SetToServerTime,
    SetToClientTime(nfstime3),
}

impl XdrDecodable for TimeSetting {
    fn decode(decoder: &mut XdrDecoder) -> Result<Self, nfsstat3> {
        let val = decoder.decode_u32()?;
        match val {
            0 => Ok(TimeSetting::DontChange),
            1 => Ok(TimeSetting::SetToServerTime),
            2 => Ok(TimeSetting::SetToClientTime(nfstime3::decode(decoder)?)),
            _ => Err(nfsstat3::ERR_INVAL),
        }
    }
}

/// Create mode (CREATE)
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CreateMode {
    Unchecked = 0,
    Guarded = 1,
    Exclusive = 2,
}

impl TryFrom<u32> for CreateMode {
    type Error = ();
    fn try_from(val: u32) -> Result<Self, ()> {
        match val {
            0 => Ok(CreateMode::Unchecked),
            1 => Ok(CreateMode::Guarded),
            2 => Ok(CreateMode::Exclusive),
            _ => Err(()),
        }
    }
}

impl From<CreateMode> for u32 {
    fn from(val: CreateMode) -> u32 {
        val as u32
    }
}

/// Filesystem statistics (FSSTAT)
#[derive(Clone, Debug)]
pub struct fsstat3 {
    pub tbytes: u64,
    pub fbytes: u64,
    pub abytes: u64,
    pub tfiles: u64,
    pub ffiles: u64,
    pub afiles: u64,
    pub invarsec: u32,
}

/// Filesystem info (FSINFO)
#[derive(Clone, Debug)]
pub struct fsinfo3 {
    pub rtmax: u32,
    pub rtpref: u32,
    pub rtmult: u32,
    pub wtmax: u32,
    pub wtpref: u32,
    pub wtmult: u32,
    pub dtpref: u32,
    pub maxfilesize: u64,
    pub time_delta: nfstime3,
    pub properties: u32,
}

/// Path configuration (PATHCONF)
#[derive(Clone, Debug)]
pub struct pathconf3 {
    pub link_max: u32,
    pub name_max: u32,
    pub no_trunc: bool,
    pub chown_restricted: bool,
    pub case_insensitive: bool,
    pub case_preserving: bool,
}
