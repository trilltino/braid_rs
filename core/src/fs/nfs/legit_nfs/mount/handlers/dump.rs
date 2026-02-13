//! MOUNT DUMP handler

use crate::fs::nfs::legit_nfs::rpc::message::RpcReply;

pub async fn handle_dump(xid: u32) -> RpcReply {
    // Return empty mount list for now
    RpcReply::success(xid, vec![])
}
