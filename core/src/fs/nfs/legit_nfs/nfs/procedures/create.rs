//! NFS CREATE procedure

use crate::fs::nfs::legit_nfs::error::nfsstat3;
use crate::fs::nfs::legit_nfs::fs::handle::FileHandle;
use crate::fs::nfs::legit_nfs::fs::trait_::NfsFilesystem;
use crate::fs::nfs::legit_nfs::nfs::types::{post_op_attr, sattr3, CreateMode};
use crate::fs::nfs::legit_nfs::rpc::message::RpcReply;
use crate::fs::nfs::legit_nfs::xdr::{XdrDecodable, XdrDecoder, XdrEncodable, XdrEncoder};

/// Handle NFS CREATE request
pub async fn handle_create<FS: NfsFilesystem>(
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
    
    let create_mode = match decoder.decode_enum(|v| CreateMode::try_from(v).ok()) {
        Ok(m) => m,
        Err(e) => return make_error_reply(xid, e),
    };
    
    let sattr = match sattr3::decode(&mut decoder) {
        Ok(a) => a,
        Err(e) => return make_error_reply(xid, e),
    };
    
    // For exclusive create, there's a verifier - skip it for now
    if create_mode == CreateMode::Exclusive {
        let _ = decoder.decode_u64(); // verifier
    }
    
    // Perform create
    match fs.create(dir_handle, &name, sattr, create_mode).await {
        Ok((handle, attr)) => {
            let mut encoder = XdrEncoder::new();
            encoder.encode_u32(nfsstat3::OK as u32);
            
            // Handle status (1 = handle follows)
            encoder.encode_bool(true);
            handle.encode(&mut encoder).unwrap();
            
            // Post-op attributes
            post_op_attr::Some(attr).encode(&mut encoder).unwrap();
            
            // Directory WCC data
            encoder.encode_bool(false); // No pre-op
            post_op_attr::None.encode(&mut encoder).unwrap();
            
            RpcReply::success(xid, encoder.into_bytes())
        }
        Err(e) => make_error_reply(xid, e),
    }
}

fn make_error_reply(xid: u32, e: nfsstat3) -> RpcReply {
    let mut encoder = XdrEncoder::new();
    encoder.encode_u32(e as u32);
    
    // No handle
    encoder.encode_bool(false);
    
    post_op_attr::None.encode(&mut encoder).unwrap();
    
    // Directory WCC
    encoder.encode_bool(false);
    post_op_attr::None.encode(&mut encoder).unwrap();
    
    RpcReply::success(xid, encoder.into_bytes())
}
