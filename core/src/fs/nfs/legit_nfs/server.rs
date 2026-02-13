//! NFS server implementation

use super::error::nfsstat3;
use super::fs::trait_::NfsFilesystem;
use super::nfs::handlers::handle_nfs_call;
use super::rpc::message::{RpcCall, RpcReply, NFS_PROGRAM, MOUNT_PROGRAM};
use super::rpc::{read_rpc_message, write_rpc_message};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info};

/// NFS Server
pub struct NfsServer<FS: NfsFilesystem> {
    fs: Arc<FS>,
    bind_addr: String,
}

impl<FS: NfsFilesystem> NfsServer<FS> {
    /// Create a new NFS server
    pub fn new(fs: FS) -> Self {
        Self {
            fs: Arc::new(fs),
            bind_addr: "127.0.0.1:2049".to_string(),
        }
    }
    
    /// Set bind address
    pub fn bind_addr(mut self, addr: impl Into<String>) -> Self {
        self.bind_addr = addr.into();
        self
    }
    
    /// Run the server
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(&self.bind_addr).await?;
        info!("NFS server listening on {}", self.bind_addr);
        
        loop {
            let (socket, addr) = listener.accept().await?;
            let fs = Arc::clone(&self.fs);
            
            tokio::spawn(async move {
                if let Err(e) = handle_client(socket, addr, fs).await {
                    error!("Client {} error: {}", addr, e);
                }
            });
        }
    }
}

async fn handle_client<FS: NfsFilesystem>(
    mut socket: TcpStream,
    addr: SocketAddr,
    fs: Arc<FS>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("NFS client connected: {}", addr);
    
    loop {
        // Read RPC message
        let msg = match read_rpc_message(&mut socket).await {
            Ok(msg) => msg,
            Err(e) => {
                if e == nfsstat3::ERR_IO {
                    // Client disconnected
                    break;
                }
                error!("Read error from {}: {:?}", addr, e);
                continue;
            }
        };
        
        // Parse RPC call
        let call = match RpcCall::parse(&msg) {
            Ok(call) => call,
            Err(e) => {
                error!("Parse error from {}: {:?}", addr, e);
                continue;
            }
        };
        
        // Handle based on program
        let reply = match call.prog {
            NFS_PROGRAM => handle_nfs_call(call, fs.as_ref()).await,
            MOUNT_PROGRAM => {
                use super::nfs::handlers::handle_mount_call;
                handle_mount_call(call, fs.as_ref()).await
            }
            _ => RpcReply::error(call.xid, super::rpc::message::AcceptStat::ProgUnavail),
        };
        
        // Send reply
        if let Err(e) = write_rpc_message(&mut socket, &reply.data).await {
            error!("Write error to {}: {:?}", addr, e);
            break;
        }
    }
    
    info!("NFS client disconnected: {}", addr);
    Ok(())
}
