//! NFS WRITE procedure

use crate::fs::nfs::legit_nfs::error::nfsstat3;
use crate::fs::nfs::legit_nfs::fs::handle::FileHandle;
use crate::fs::nfs::legit_nfs::fs::trait_::NfsFilesystem;
use crate::fs::nfs::legit_nfs::nfs::types::{post_op_attr, StableHow};
use crate::fs::nfs::legit_nfs::rpc::message::RpcReply;
use crate::fs::nfs::legit_nfs::xdr::{XdrDecodable, XdrDecoder, XdrEncodable, XdrEncoder};

/// Handle NFS WRITE request
pub async fn handle_write<FS: NfsFilesystem>(
    xid: u32,
    data: &[u8],
    fs: &FS,
) -> RpcReply {
    // Parse request
    let mut decoder = XdrDecoder::new(data);
    
    let handle = match FileHandle::decode(&mut decoder) {
        Ok(h) => h,
        Err(e) => return make_error_reply(xid, e),
    };
    
    let offset = match decoder.decode_u64() {
        Ok(o) => o,
        Err(e) => return make_error_reply(xid, e),
    };
    
    let count = match decoder.decode_u32() {
        Ok(c) => c,
        Err(e) => return make_error_reply(xid, e),
    };
    
    let stable = match decoder.decode_enum(|v| StableHow::try_from(v).ok()) {
        Ok(s) => s,
        Err(e) => return make_error_reply(xid, e),
    };
    
    // Read the data
    let write_data = match decoder.decode_opaque() {
        Ok(d) => d,
        Err(e) => return make_error_reply(xid, e),
    };
    
    // Verify count matches
    if write_data.len() != count as usize {
        return make_error_reply(xid, nfsstat3::ERR_INVAL);
    }
    
    // Perform write
    match fs.write(handle, offset, &write_data, stable).await {
        Ok(result) => {
            let mut encoder = XdrEncoder::new();
            encoder.encode_u32(nfsstat3::OK as u32);
            
            // Weak cache consistency data (none)
            encoder.encode_bool(false); // No pre-op attributes
            post_op_attr::Some(result.attr).encode(&mut encoder).unwrap();
            
            // Count written
            encoder.encode_u32(result.count);
            
            // Stability
            encoder.encode_enum(result.committed);
            
            // Write verifier (64-bit)
            encoder.encode_u64(result.verf);
            
            RpcReply::success(xid, encoder.into_bytes())
        }
        Err(e) => make_error_reply(xid, e),
    }
}

fn make_error_reply(xid: u32, e: nfsstat3) -> RpcReply {
    let mut encoder = XdrEncoder::new();
    encoder.encode_u32(e as u32);
    
    // Weak cache consistency data
    encoder.encode_bool(false);
    post_op_attr::None.encode(&mut encoder).unwrap();
    
    // Count = 0
    encoder.encode_u32(0);
    
    // Stability = unstable
    encoder.encode_u32(0);
    
    // Verifier = 0
    encoder.encode_u64(0);
    
    RpcReply::success(xid, encoder.into_bytes())
}
