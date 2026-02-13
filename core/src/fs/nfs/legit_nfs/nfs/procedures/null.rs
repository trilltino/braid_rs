//! NFS NULL procedure (ping)

use crate::fs::nfs::legit_nfs::rpc::message::RpcReply;

/// Handle NFS NULL request
pub async fn handle_null(xid: u32) -> RpcReply {
    // NULL always returns success with no data
    RpcReply::success(xid, vec![])
}
