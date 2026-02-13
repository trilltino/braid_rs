//! NFS MKDIR procedure

use crate::fs::nfs::legit_nfs::error::nfsstat3;
use crate::fs::nfs::legit_nfs::fs::handle::FileHandle;
use crate::fs::nfs::legit_nfs::fs::trait_::NfsFilesystem;
use crate::fs::nfs::legit_nfs::nfs::types::{post_op_attr, sattr3};
use crate::fs::nfs::legit_nfs::rpc::message::RpcReply;
use crate::fs::nfs::legit_nfs::xdr::{XdrDecodable, XdrDecoder, XdrEncodable, XdrEncoder};

/// Handle NFS MKDIR request
pub async fn handle_mkdir<FS: NfsFilesystem>(
    xid: u32,
    data: &[u8],
    fs: &FS,
) -> RpcReply {
    // Parse request
    let mut decoder = XdrDecoder::new(data);
    
    let dir_handle = match FileHandle::decode(&mut decoder) {
        Ok(h) => h,
        Err(e) => return make_error_reply(xid, e),
    };
    
    let name = match decoder.decode_string() {
        Ok(n) => n,
        Err(e) => return make_error_reply(xid, e),
    };
    
    let sattr = match sattr3::decode(&mut decoder) {
        Ok(a) => a,
        Err(e) => return make_error_reply(xid, e),
    };
    
    // Perform mkdir
    match fs.mkdir(dir_handle, &name, sattr).await {
        Ok((handle, attr)) => {
            let mut encoder = XdrEncoder::new();
            encoder.encode_u32(nfsstat3::OK as u32);
            
            // Handle status
            encoder.encode_bool(true);
            handle.encode(&mut encoder).unwrap();
            
            // Post-op attributes
            post_op_attr::Some(attr).encode(&mut encoder).unwrap();
            
            // Directory WCC
            encoder.encode_bool(false);
            post_op_attr::None.encode(&mut encoder).unwrap();
            
            RpcReply::success(xid, encoder.into_bytes())
        }
        Err(e) => make_error_reply(xid, e),
    }
}

fn make_error_reply(xid: u32, e: nfsstat3) -> RpcReply {
    let mut encoder = XdrEncoder::new();
    encoder.encode_u32(e as u32);
    
    encoder.encode_bool(false);
    post_op_attr::None.encode(&mut encoder).unwrap();
    
    encoder.encode_bool(false);
    post_op_attr::None.encode(&mut encoder).unwrap();
    
    RpcReply::success(xid, encoder.into_bytes())
}
