//! NFS request handler

use super::consts::*;
use crate::fs::nfs::legit_nfs::error::nfsstat3;
use crate::fs::nfs::legit_nfs::fs::trait_::NfsFilesystem;
use crate::fs::nfs::legit_nfs::rpc::message::{AcceptStat, RpcCall, RpcReply};

/// Handle an NFS RPC call
pub async fn handle_nfs_call<FS: NfsFilesystem>(call: RpcCall, fs: &FS) -> RpcReply {
    match call.proc {
        // 13 Core procedures for P2P sync
        NFSPROC3_NULL => super::procedures::null::handle_null(call.xid).await,
        NFSPROC3_GETATTR => super::procedures::getattr::handle_getattr(call.xid, &call.data, fs).await,
        NFSPROC3_LOOKUP => super::procedures::lookup::handle_lookup(call.xid, &call.data, fs).await,
        NFSPROC3_READ => super::procedures::read::handle_read(call.xid, &call.data, fs).await,
        NFSPROC3_WRITE => super::procedures::write::handle_write(call.xid, &call.data, fs).await,
        NFSPROC3_CREATE => super::procedures::create::handle_create(call.xid, &call.data, fs).await,
        NFSPROC3_MKDIR => super::procedures::mkdir::handle_mkdir(call.xid, &call.data, fs).await,
        NFSPROC3_REMOVE => super::procedures::remove::handle_remove(call.xid, &call.data, fs).await,
        NFSPROC3_RMDIR => super::procedures::rmdir::handle_rmdir(call.xid, &call.data, fs).await,
        NFSPROC3_RENAME => super::procedures::rename::handle_rename(call.xid, &call.data, fs).await,
        NFSPROC3_READDIR => super::procedures::readdir::handle_readdir(call.xid, &call.data, fs).await,
        NFSPROC3_READDIRPLUS => super::procedures::readdirplus::handle_readdirplus(call.xid, &call.data, fs).await,
        NFSPROC3_FSSTAT => super::procedures::fsstat::handle_fsstat(call.xid, &call.data, fs).await,
        NFSPROC3_FSINFO => super::procedures::fsinfo::handle_fsinfo(call.xid, &call.data, fs).await,
        
        // 9 Stub procedures return NOTSUPP
        NFSPROC3_SETATTR
        | NFSPROC3_ACCESS
        | NFSPROC3_READLINK
        | NFSPROC3_SYMLINK
        | NFSPROC3_MKNOD
        | NFSPROC3_LINK
        | NFSPROC3_PATHCONF
        | NFSPROC3_COMMIT => {
            let mut encoder = crate::fs::nfs::legit_nfs::xdr::XdrEncoder::new();
            encoder.encode_u32(nfsstat3::ERR_NOTSUPP as u32);
            RpcReply::success(call.xid, encoder.into_bytes())
        }
        
        // Unknown procedure
        _ => RpcReply::error(call.xid, AcceptStat::ProcUnavail),
    }
}

/// Handle an MOUNT RPC call
pub async fn handle_mount_call<FS: NfsFilesystem>(call: RpcCall, _fs: &FS) -> RpcReply {
    use crate::fs::nfs::legit_nfs::xdr::XdrEncoder;
    use crate::fs::nfs::legit_nfs::mount::types::MountStat3;
    
    match call.proc {
        0 => {
            // MOUNT NULL - ping
            RpcReply::success(call.xid, vec![])
        }
        1 => {
            // MOUNT MNT - mount a filesystem
            // For now, return success with root file handle
            let mut encoder = XdrEncoder::new();
            encoder.encode_u32(MountStat3::Ok as u32); // Status = OK
            
            // File handle (8 bytes for root)
            encoder.encode_u32(8); // handle length
            encoder.encode_u64(1); // root handle = 1
            
            // Flavors (auth_none only)
            encoder.encode_u32(1); // count
            encoder.encode_u32(0); // auth_none
            
            RpcReply::success(call.xid, encoder.into_bytes())
        }
        2 => {
            // MOUNT DUMP - dump mount list
            RpcReply::success(call.xid, vec![])
        }
        3 => {
            // MOUNT UMNT - unmount
            RpcReply::success(call.xid, vec![])
        }
        4 => {
            // MOUNT UMNTALL - unmount all
            RpcReply::success(call.xid, vec![])
        }
        5 => {
            // MOUNT EXPORT - export list
            RpcReply::success(call.xid, vec![])
        }
        _ => RpcReply::error(call.xid, AcceptStat::ProcUnavail),
    }
}
