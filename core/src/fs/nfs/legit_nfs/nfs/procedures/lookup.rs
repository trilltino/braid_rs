//! NFS LOOKUP procedure

use crate::fs::nfs::legit_nfs::error::nfsstat3;
use crate::fs::nfs::legit_nfs::fs::handle::FileHandle;
use crate::fs::nfs::legit_nfs::fs::trait_::NfsFilesystem;
use crate::fs::nfs::legit_nfs::nfs::types::post_op_attr;
use crate::fs::nfs::legit_nfs::rpc::message::RpcReply;
use crate::fs::nfs::legit_nfs::xdr::{XdrDecodable, XdrDecoder, XdrEncodable, XdrEncoder};

/// Handle NFS LOOKUP request
pub async fn handle_lookup<FS: NfsFilesystem>(
    xid: u32,
    data: &[u8],
    fs: &FS,
) -> RpcReply {
    // Parse directory handle
    let mut decoder = XdrDecoder::new(data);
    let dir_handle = match FileHandle::decode(&mut decoder) {
        Ok(h) => h,
        Err(e) => return make_error_reply(xid, e),
    };
    
    // Parse filename
    let name = match decoder.decode_string() {
        Ok(n) => n,
        Err(e) => return make_error_reply(xid, e),
    };
    
    // Perform lookup
    match fs.lookup(dir_handle, &name).await {
        Ok(handle) => {
            // Get attributes for the found file
            let attr = match fs.getattr(handle).await {
                Ok(a) => post_op_attr::Some(a),
                Err(_) => post_op_attr::None,
            };
            
            let mut encoder = XdrEncoder::new();
            encoder.encode_u32(nfsstat3::OK as u32);
            
            // File handle
            handle.encode(&mut encoder).unwrap();
            
            // Post-op attributes
            attr.encode(&mut encoder).unwrap();
            
            // Directory post-op attributes (none for now)
            post_op_attr::None.encode(&mut encoder).unwrap();
            
            RpcReply::success(xid, encoder.into_bytes())
        }
        Err(e) => make_error_reply(xid, e),
    }
}

fn make_error_reply(xid: u32, e: nfsstat3) -> RpcReply {
    let mut encoder = XdrEncoder::new();
    encoder.encode_u32(e as u32);
    
    // Empty file handle
    encoder.encode_u32(0);
    
    post_op_attr::None.encode(&mut encoder).unwrap();
    post_op_attr::None.encode(&mut encoder).unwrap();
    
    RpcReply::success(xid, encoder.into_bytes())
}
