//! MOUNT UMNT handler

use crate::fs::nfs::legit_nfs::rpc::message::RpcReply;

pub async fn handle_umnt(xid: u32, _data: &[u8]) -> RpcReply {
    // Return success
    RpcReply::success(xid, vec![])
}
