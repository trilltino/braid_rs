//! NFS FSINFO procedure

use crate::fs::nfs::legit_nfs::error::nfsstat3;
use crate::fs::nfs::legit_nfs::fs::handle::FileHandle;
use crate::fs::nfs::legit_nfs::fs::trait_::NfsFilesystem;
use crate::fs::nfs::legit_nfs::nfs::types::post_op_attr;
use crate::fs::nfs::legit_nfs::rpc::message::RpcReply;
use crate::fs::nfs::legit_nfs::xdr::{XdrDecodable, XdrDecoder, XdrEncodable, XdrEncoder};

/// Handle NFS FSINFO request
pub async fn handle_fsinfo<FS: NfsFilesystem>(
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
    
    // Get fsinfo
    match fs.fsinfo(handle).await {
        Ok(info) => {
            let mut encoder = XdrEncoder::new();
            encoder.encode_u32(nfsstat3::OK as u32);
            
            // Post-op attributes
            post_op_attr::None.encode(&mut encoder).unwrap();
            
            // FSINFO fields
            encoder.encode_u32(info.rtmax);
            encoder.encode_u32(info.rtpref);
            encoder.encode_u32(info.rtmult);
            encoder.encode_u32(info.wtmax);
            encoder.encode_u32(info.wtpref);
            encoder.encode_u32(info.wtmult);
            encoder.encode_u32(info.dtpref);
            encoder.encode_u64(info.maxfilesize);
            info.time_delta.encode(&mut encoder).unwrap();
            encoder.encode_u32(info.properties);
            
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
