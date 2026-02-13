//! File handle management

use crate::fs::nfs::legit_nfs::error::nfsstat3;
use crate::fs::nfs::legit_nfs::xdr::{XdrDecodable, XdrDecoder, XdrEncodable, XdrEncoder};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A file handle is an opaque 64-bit identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileHandle(pub u64);

impl FileHandle {
    /// Create handle from raw ID
    pub fn from_id(id: u64) -> Self {
        FileHandle(id)
    }
    
    /// Get the ID
    pub fn to_id(&self) -> u64 {
        self.0
    }
    
    /// Convert to bytes for XDR encoding
    pub fn to_bytes(&self) -> [u8; 8] {
        self.0.to_be_bytes()
    }
    
    /// Parse from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, nfsstat3> {
        if bytes.len() < 8 {
            return Err(nfsstat3::ERR_BADHANDLE);
        }
        let id = u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        Ok(FileHandle(id))
    }
}

impl XdrEncodable for FileHandle {
    fn encode(&self, encoder: &mut XdrEncoder) -> Result<(), nfsstat3> {
        encoder.encode_u64(self.0);
        Ok(())
    }
}

impl XdrDecodable for FileHandle {
    fn decode(decoder: &mut XdrDecoder) -> Result<Self, nfsstat3> {
        let id = decoder.decode_u64()?;
        Ok(FileHandle(id))
    }
}

/// Manages file handle allocation and lookup
pub struct FileHandleManager {
    next_id: Arc<RwLock<u64>>,
    handle_to_path: Arc<RwLock<HashMap<u64, String>>>,
    path_to_handle: Arc<RwLock<HashMap<String, u64>>>,
}

impl FileHandleManager {
    pub fn new() -> Self {
        let mut handle_to_path = HashMap::new();
        let mut path_to_handle = HashMap::new();
        
        // Root handle is always 1
        handle_to_path.insert(1, "/".to_string());
        path_to_handle.insert("/".to_string(), 1);
        
        Self {
            next_id: Arc::new(RwLock::new(2)),
            handle_to_path: Arc::new(RwLock::new(handle_to_path)),
            path_to_handle: Arc::new(RwLock::new(path_to_handle)),
        }
    }
    
    /// Get or create a handle for a path
    pub async fn get_or_create(&self, path: &str) -> FileHandle {
        let path = if path.is_empty() { "/" } else { path };
        
        // Check if exists
        {
            let path_to_handle = self.path_to_handle.read().await;
            if let Some(&id) = path_to_handle.get(path) {
                return FileHandle(id);
            }
        }
        
        // Create new
        let mut next_id = self.next_id.write().await;
        let id = *next_id;
        *next_id += 1;
        
        let mut handle_to_path = self.handle_to_path.write().await;
        let mut path_to_handle = self.path_to_handle.write().await;
        
        handle_to_path.insert(id, path.to_string());
        path_to_handle.insert(path.to_string(), id);
        
        FileHandle(id)
    }
    
    /// Get path for a handle
    pub async fn get_path(&self, handle: FileHandle) -> Option<String> {
        let handle_to_path = self.handle_to_path.read().await;
        handle_to_path.get(&handle.0).cloned()
    }
    
    /// Get handle for a path
    pub async fn get_handle(&self, path: &str) -> Option<FileHandle> {
        let path_to_handle = self.path_to_handle.read().await;
        path_to_handle.get(path).map(|&id| FileHandle(id))
    }
}

impl Default for FileHandleManager {
    fn default() -> Self {
        Self::new()
    }
}
