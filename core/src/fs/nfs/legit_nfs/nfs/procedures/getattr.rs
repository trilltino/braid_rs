//! NFS GETATTR procedure

use crate::fs::nfs::legit_nfs::error::nfsstat3;
use crate::fs::nfs::legit_nfs::fs::handle::FileHandle;
use crate::fs::nfs::legit_nfs::fs::trait_::NfsFilesystem;
use crate::fs::nfs::legit_nfs::nfs::types::post_op_attr;
use crate::fs::nfs::legit_nfs::rpc::message::RpcReply;
use crate::fs::nfs::legit_nfs::xdr::{XdrDecodable, XdrDecoder, XdrEncodable, XdrEncoder};

/// Handle NFS GETATTR request
pub async fn handle_getattr<FS: NfsFilesystem>(
    xid: u32,
    data: &[u8],
    fs: &FS,
) -> RpcReply {
    // Parse file handle
    let mut decoder = XdrDecoder::new(data);
    let handle = match FileHandle::decode(&mut decoder) {
        Ok(h) => h,
        Err(e) => return make_error_reply(xid, e),
    };
    
    // Get attributes
    match fs.getattr(handle).await {
        Ok(attr) => {
            let mut encoder = XdrEncoder::new();
            encoder.encode_u32(nfsstat3::OK as u32);
            
            // Post-op attributes
            post_op_attr::Some(attr).encode(&mut encoder).unwrap();
            
            RpcReply::success(xid, encoder.into_bytes())
        }
        Err(e) => make_error_reply(xid, e),
    }
}

fn make_error_reply(xid: u32, e: nfsstat3) -> RpcReply {
    let mut encoder = XdrEncoder::new();
    encoder.encode_u32(e as u32);
    post_op_attr::None.encode(&mut encoder).unwrap();
    RpcReply::success(xid, encoder.into_bytes())
}
