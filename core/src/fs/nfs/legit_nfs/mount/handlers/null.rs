//! MOUNT NULL handler

use crate::fs::nfs::legit_nfs::rpc::message::RpcReply;

pub async fn handle_mount_null(xid: u32) -> RpcReply {
    RpcReply::success(xid, vec![])
}
