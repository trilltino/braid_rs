//! NFS RENAME procedure

use crate::fs::nfs::legit_nfs::error::nfsstat3;
use crate::fs::nfs::legit_nfs::fs::handle::FileHandle;
use crate::fs::nfs::legit_nfs::fs::trait_::NfsFilesystem;
use crate::fs::nfs::legit_nfs::nfs::types::post_op_attr;
use crate::fs::nfs::legit_nfs::rpc::message::RpcReply;
use crate::fs::nfs::legit_nfs::xdr::{XdrDecodable, XdrDecoder, XdrEncodable, XdrEncoder};

/// Handle NFS RENAME request
pub async fn handle_rename<FS: NfsFilesystem>(
    xid: u32,
    data: &[u8],
    fs: &FS,
) -> RpcReply {
    // Parse request
    let mut decoder = XdrDecoder::new(data);
    
    let from_dir = match FileHandle::decode(&mut decoder) {
        Ok(h) => h,
        Err(e) => return make_error_reply(xid, e),
    };
    
    let from_name = match decoder.decode_string() {
        Ok(n) => n,
        Err(e) => return make_error_reply(xid, e),
    };
    
    let to_dir = match FileHandle::decode(&mut decoder) {
        Ok(h) => h,
        Err(e) => return make_error_reply(xid, e),
    };
    
    let to_name = match decoder.decode_string() {
        Ok(n) => n,
        Err(e) => return make_error_reply(xid, e),
    };
    
    // Perform rename
    match fs.rename(from_dir, &from_name, to_dir, &to_name).await {
        Ok(()) => {
            let mut encoder = XdrEncoder::new();
            encoder.encode_u32(nfsstat3::OK as u32);
            
            // From directory WCC
            encoder.encode_bool(false);
            post_op_attr::None.encode(&mut encoder).unwrap();
            
            // To directory WCC
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
