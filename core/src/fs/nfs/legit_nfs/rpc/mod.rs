//! RPC (Remote Procedure Call) protocol handling

pub mod message;
pub mod record;

pub use message::{RpcCall, RpcReply, NFS_PROGRAM, NFS_VERSION, MOUNT_PROGRAM, MOUNT_VERSION};
pub use record::{read_rpc_message, write_rpc_message, RecordMarker};
