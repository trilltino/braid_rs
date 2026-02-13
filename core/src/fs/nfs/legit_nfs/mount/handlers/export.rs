//! MOUNT EXPORT handler

use crate::fs::nfs::legit_nfs::rpc::message::RpcReply;
use crate::fs::nfs::legit_nfs::xdr::XdrEncoder;

pub async fn handle_export(xid: u32) -> RpcReply {
    // Return empty export list
    let mut encoder = XdrEncoder::new();
    encoder.encode_bool(false); // End of list
    RpcReply::success(xid, encoder.into_bytes())
}
