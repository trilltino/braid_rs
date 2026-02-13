//! NFS READDIRPLUS procedure

use crate::fs::nfs::legit_nfs::error::nfsstat3;
use crate::fs::nfs::legit_nfs::fs::handle::FileHandle;
use crate::fs::nfs::legit_nfs::fs::trait_::NfsFilesystem;
use crate::fs::nfs::legit_nfs::nfs::types::post_op_attr;
use crate::fs::nfs::legit_nfs::rpc::message::RpcReply;
use crate::fs::nfs::legit_nfs::xdr::{XdrDecodable, XdrDecoder, XdrEncodable, XdrEncoder};

/// Handle NFS READDIRPLUS request
pub async fn handle_readdirplus<FS: NfsFilesystem>(
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
    
    let cookie = match decoder.decode_u64() {
        Ok(c) => c,
        Err(e) => return make_error_reply(xid, e),
    };
    
    // dircount (max size of directory data) - skip
    let _ = decoder.decode_u32();
    
    let maxcount = match decoder.decode_u32() {
        Ok(c) => c,
        Err(e) => return make_error_reply(xid, e),
    };
    
    // Perform readdirplus
    match fs.readdirplus(dir_handle, cookie, maxcount).await {
        Ok(result) => {
            let mut encoder = XdrEncoder::new();
            encoder.encode_u32(nfsstat3::OK as u32);
            
            // Directory post-op attributes
            post_op_attr::None.encode(&mut encoder).unwrap();
            
            // Entries
            for entry in result.entries {
                encoder.encode_bool(true); // Entry follows
                encoder.encode_u64(entry.fileid);
                encoder.encode_string(&entry.name);
                encoder.encode_u64(entry.cookie);
                
                // Attributes
                entry.attr.encode(&mut encoder).unwrap();
                
                // File handle
                if let Some(handle) = entry.handle {
                    encoder.encode_bool(true);
                    handle.encode(&mut encoder).unwrap();
                } else {
                    encoder.encode_bool(false);
                }
            }
            encoder.encode_bool(false); // End of entries
            
            // EOF
            encoder.encode_bool(result.end);
            
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
