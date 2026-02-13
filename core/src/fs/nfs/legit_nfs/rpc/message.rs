//! RPC message handling

use crate::fs::nfs::legit_nfs::error::nfsstat3;
use crate::fs::nfs::legit_nfs::xdr::{XdrDecoder, XdrEncoder};

/// RPC program numbers
pub const NFS_PROGRAM: u32 = 100003;
pub const NFS_VERSION: u32 = 3;
pub const MOUNT_PROGRAM: u32 = 100005;
pub const MOUNT_VERSION: u32 = 3;

/// RPC message type
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MsgType {
    Call = 0,
    Reply = 1,
}

/// RPC reply status
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyStat {
    Accepted = 0,
    Denied = 1,
}

/// RPC accepted reply status
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceptStat {
    Success = 0,
    ProgUnavail = 1,
    ProgMismatch = 2,
    ProcUnavail = 3,
    GarbageArgs = 4,
    SystemErr = 5,
}

/// RPC call message
#[derive(Clone, Debug)]
pub struct RpcCall {
    pub xid: u32,
    pub prog: u32,
    pub vers: u32,
    pub proc: u32,
    pub cred: Vec<u8>,
    pub verf: Vec<u8>,
    pub data: Vec<u8>,
}

impl RpcCall {
    /// Parse RPC call from bytes
    pub fn parse(data: &[u8]) -> Result<Self, nfsstat3> {
        let mut decoder = XdrDecoder::new(data);
        
        // Transaction ID
        let xid = decoder.decode_u32()?;
        
        // Message type (should be Call = 0)
        let msg_type = decoder.decode_u32()?;
        if msg_type != 0 {
            return Err(nfsstat3::ERR_INVAL);
        }
        
        // RPC version (should be 2)
        let rpc_vers = decoder.decode_u32()?;
        if rpc_vers != 2 {
            return Err(nfsstat3::ERR_INVAL);
        }
        
        // Program number
        let prog = decoder.decode_u32()?;
        
        // Program version
        let vers = decoder.decode_u32()?;
        
        // Procedure number
        let proc = decoder.decode_u32()?;
        
        // Credentials (opaque)
        let cred = decoder.decode_opaque()?;
        
        // Verifier (opaque)
        let verf = decoder.decode_opaque()?;
        
        // Remaining data is procedure-specific
        let remaining = &data[decoder.pos()..];
        let proc_data = remaining.to_vec();
        
        Ok(RpcCall {
            xid,
            prog,
            vers,
            proc,
            cred,
            verf,
            data: proc_data,
        })
    }
}

/// RPC reply message
#[derive(Clone, Debug)]
pub struct RpcReply {
    pub xid: u32,
    pub data: Vec<u8>,
}

impl RpcReply {
    /// Create a successful reply
    pub fn success(xid: u32, data: Vec<u8>) -> Self {
        let mut encoder = XdrEncoder::new();
        
        // XID
        encoder.encode_u32(xid);
        
        // Message type = Reply
        encoder.encode_u32(1);
        
        // Reply status = Accepted
        encoder.encode_u32(0);
        
        // Verifier (empty auth_none)
        encoder.encode_opaque(&[]);
        
        // Accept status = Success
        encoder.encode_u32(0);
        
        // Data
        let mut result = encoder.into_bytes();
        result.extend_from_slice(&data);
        
        Self {
            xid,
            data: result,
        }
    }
    
    /// Create an error reply
    pub fn error(xid: u32, stat: AcceptStat) -> Self {
        let mut encoder = XdrEncoder::new();
        
        // XID
        encoder.encode_u32(xid);
        
        // Message type = Reply
        encoder.encode_u32(1);
        
        // Reply status = Accepted
        encoder.encode_u32(0);
        
        // Verifier (empty auth_none)
        encoder.encode_opaque(&[]);
        
        // Accept status
        encoder.encode_u32(stat as u32);
        
        Self {
            xid,
            data: encoder.into_bytes(),
        }
    }
    
    /// Create a program mismatch reply
    pub fn prog_mismatch(xid: u32, low: u32, high: u32) -> Self {
        let mut encoder = XdrEncoder::new();
        
        // XID
        encoder.encode_u32(xid);
        
        // Message type = Reply
        encoder.encode_u32(1);
        
        // Reply status = Accepted
        encoder.encode_u32(0);
        
        // Verifier (empty auth_none)
        encoder.encode_opaque(&[]);
        
        // Accept status = ProgMismatch
        encoder.encode_u32(2);
        
        // Low and high version
        encoder.encode_u32(low);
        encoder.encode_u32(high);
        
        Self {
            xid,
            data: encoder.into_bytes(),
        }
    }
}
