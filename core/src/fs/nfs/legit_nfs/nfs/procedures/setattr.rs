//! NFS SETATTR procedure (stub)

use crate::fs::nfs::legit_nfs::error::nfsstat3;
use crate::fs::nfs::legit_nfs::fs::trait_::NfsFilesystem;
use crate::fs::nfs::legit_nfs::rpc::message::RpcReply;

/// Handle NFS SETATTR request
pub async fn handle_setattr<FS: NfsFilesystem>(
    xid: u32,
    _data: &[u8],
    _fs: &FS,
) -> RpcReply {
    // Return NOTSUPP - not needed for P2P sync
    let mut encoder = crate::fs::nfs::legit_nfs::xdr::XdrEncoder::new();
    encoder.encode_u32(nfsstat3::ERR_NOTSUPP as u32);
    RpcReply::success(xid, encoder.into_bytes())
}
