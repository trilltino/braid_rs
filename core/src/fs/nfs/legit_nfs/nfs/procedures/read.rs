//! NFS READ procedure

use crate::fs::nfs::legit_nfs::error::nfsstat3;
use crate::fs::nfs::legit_nfs::fs::handle::FileHandle;
use crate::fs::nfs::legit_nfs::fs::trait_::NfsFilesystem;
use crate::fs::nfs::legit_nfs::nfs::types::post_op_attr;
use crate::fs::nfs::legit_nfs::rpc::message::RpcReply;
use crate::fs::nfs::legit_nfs::xdr::{XdrDecodable, XdrDecoder, XdrEncodable, XdrEncoder};

/// Handle NFS READ request
pub async fn handle_read<FS: NfsFilesystem>(
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

    // Perform read
    match fs.read(handle, offset, count).await {
        Ok(result) => {
            let mut encoder = XdrEncoder::new();
            encoder.encode_u32(nfsstat3::OK as u32);
            
            // Post-op attributes
            post_op_attr::Some(result.attr).encode(&mut encoder).unwrap();
            
            // Count
            encoder.encode_u32(result.data.len() as u32);
            
            // EOF flag
            encoder.encode_u32(if result.eof { 1 } else { 0 });
            
            // Data
            encoder.encode_opaque(&result.data);
            
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
