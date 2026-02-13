//! MOUNT UMNTALL handler

use crate::fs::nfs::legit_nfs::rpc::message::RpcReply;

pub async fn handle_umntall(xid: u32) -> RpcReply {
    // Return success
    RpcReply::success(xid, vec![])
}
