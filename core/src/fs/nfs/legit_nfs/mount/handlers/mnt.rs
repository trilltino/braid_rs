//! MOUNT MNT handler

use crate::fs::nfs::legit_nfs::fs::trait_::NfsFilesystem;
use crate::fs::nfs::legit_nfs::mount::types::MountStat3;
use crate::fs::nfs::legit_nfs::rpc::message::RpcReply;
use crate::fs::nfs::legit_nfs::xdr::{XdrDecoder, XdrEncoder};

pub async fn handle_mnt<FS: NfsFilesystem>(xid: u32, data: &[u8], fs: &FS) -> RpcReply {
    // Parse mount path
    let mut decoder = XdrDecoder::new(data);
    let _path = match decoder.decode_string() {
        Ok(p) => p,
        Err(_) => {
            let mut encoder = XdrEncoder::new();
            encoder.encode_u32(MountStat3::ErrInval as u32);
            return RpcReply::success(xid, encoder.into_bytes());
        }
    };
    
    // For now, accept any path and return root handle
    let root_handle = fs.root_handle();
    let handle_bytes = root_handle.to_bytes();
    
    let mut encoder = XdrEncoder::new();
    encoder.encode_u32(MountStat3::Ok as u32); // Status
    
    // File handle
    encoder.encode_u32(handle_bytes.len() as u32);
    encoder.encode_fixed_opaque(&handle_bytes);
    
    // Auth flavors (auth_none only)
    encoder.encode_u32(1); // count
    encoder.encode_u32(0); // auth_none
    
    RpcReply::success(xid, encoder.into_bytes())
}
