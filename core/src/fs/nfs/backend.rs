//! Braid NFS Backend - uses legit-nfs instead of nfsserve
//!
//! This is adapted from braid_tauri/crates/braid-core/src/fs/nfs.rs

use crate::blob::BlobStore;
use crate::core::traits::BraidStorage;
use crate::fs::local::mapper;
use crate::fs::state::DaemonState;
use async_trait::async_trait;
use parking_lot::RwLock as PRwLock;
use rusqlite::params;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tracing::info;
use url::Url;

// Use legit-nfs types instead of nfsserve
use crate::fs::nfs::legit_nfs::error::nfsstat3;
use crate::fs::nfs::legit_nfs::fs::handle::FileHandle;
use crate::fs::nfs::legit_nfs::fs::trait_::{FsCapabilities, NfsFilesystem, ReadResult, WriteResult};
use crate::fs::nfs::legit_nfs::nfs::types::*;

/// Braid NFS backend implementation using legit-nfs
pub struct BraidNfsBackend {
    state: DaemonState,
    blob_store: Arc<BlobStore>,
    id_to_path: Arc<PRwLock<HashMap<u64, String>>>,
    path_to_id: Arc<PRwLock<HashMap<String, u64>>>,
    next_id: Arc<PRwLock<u64>>,
}

impl BraidNfsBackend {
    pub fn new(state: DaemonState, blob_store: Arc<BlobStore>) -> Self {
        let mut id_to_path = HashMap::new();
        let mut path_to_id = HashMap::new();
        let mut max_id = 1;

        // Warm cache from database
        {
            let conn = state.inode_db.lock();
            let mut stmt = conn.prepare("SELECT id, path FROM inodes").unwrap();
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, i64>(0)? as u64, row.get::<_, String>(1)?))
                })
                .unwrap();

            for row in rows {
                if let Ok((id, path)) = row {
                    id_to_path.insert(id, path.clone());
                    path_to_id.insert(path, id);
                    if id > max_id {
                        max_id = id;
                    }
                }
            }
        }

        // Ensure Root is ID 1
        if !id_to_path.contains_key(&1) {
            id_to_path.insert(1, "/".to_string());
            path_to_id.insert("/".to_string(), 1);
            let conn = state.inode_db.lock();
            let _ = conn.execute(
                "INSERT OR IGNORE INTO inodes (id, path) VALUES (1, '/')",
                [],
            );
        }

        Self {
            state,
            blob_store,
            id_to_path: Arc::new(PRwLock::new(id_to_path)),
            path_to_id: Arc::new(PRwLock::new(path_to_id)),
            next_id: Arc::new(PRwLock::new(max_id + 1)),
        }
    }

    fn get_path(&self, id: u64) -> Option<String> {
        self.id_to_path.read().get(&id).cloned()
    }

    fn get_or_create_id(&self, path: &str) -> u64 {
        let path = if path.is_empty() { "/" } else { path };
        if let Some(id) = self.path_to_id.read().get(path) {
            return *id;
        }

        let mut next_id_lock = self.next_id.write();
        let id = *next_id_lock;
        *next_id_lock += 1;

        // Persist to DB
        {
            let conn = self.state.inode_db.lock();
            if let Err(e) = conn.execute(
                "INSERT INTO inodes (id, path) VALUES (?, ?)",
                params![id as i64, path],
            ) {
                tracing::error!("Failed to persist inode mapping: {}", e);
            }
        }

        self.path_to_id.write().insert(path.to_string(), id);
        self.id_to_path.write().insert(id, path.to_string());
        id
    }

    fn get_attr(&self, id: u64, ftype: ftype3, size: u64) -> fattr3 {
        fattr3 {
            ftype,
            mode: if matches!(ftype, ftype3::NF3DIR) {
                0o755
            } else {
                0o644
            },
            nlink: 1,
            uid: 0,
            gid: 0,
            size,
            used: size,
            rdev: specdata3 {
                specdata1: 0,
                specdata2: 0,
            },
            fsid: 0,
            fileid: id,
            atime: nfstime3 {
                seconds: 0,
                nseconds: 0,
            },
            mtime: nfstime3 {
                seconds: 0,
                nseconds: 0,
            },
            ctime: nfstime3 {
                seconds: 0,
                nseconds: 0,
            },
        }
    }

    fn url_to_vpath(&self, url_str: &str) -> std::result::Result<String, anyhow::Error> {
        let url = Url::parse(url_str)?;
        let host = url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("URL missing host"))?;
        let port = url.port();
        let mut vpath = host.to_string();
        if let Some(p) = port {
            vpath.push_str(&format!("+{}", p));
        }
        for segment in url.path_segments().unwrap_or_else(|| "".split('/')) {
            if !segment.is_empty() {
                vpath.push('/');
                vpath.push_str(segment);
            }
        }
        if url.path().ends_with('/') {
            vpath.push_str("/index");
        }
        Ok(format!("/{}", vpath))
    }

    /// Check if a path is the virtual /blobs directory
    #[allow(dead_code)]
    fn is_blobs_dir(&self, path: &str) -> bool {
        path == "/blobs"
    }

    /// Check if a path is inside the virtual /blobs directory
    fn is_in_blobs<'a>(&self, path: &'a str) -> Option<&'a str> {
        path.strip_prefix("/blobs/")
    }
}

#[async_trait]
impl NfsFilesystem for BraidNfsBackend {
    fn capabilities(&self) -> FsCapabilities {
        FsCapabilities {
            read_only: false,
            supports_symlinks: false,
            supports_hardlinks: false,
            case_insensitive: cfg!(windows),
        }
    }

    fn root_handle(&self) -> FileHandle {
        FileHandle::from_id(1)
    }

    async fn lookup(&self, parent_id: FileHandle, name: &str) -> Result<FileHandle, nfsstat3> {
        let parent_path = self.get_path(parent_id.to_id()).ok_or(nfsstat3::ERR_STALE)?;

        // Handle virtual /blobs/ path
        let full_path = if parent_path == "/" && name == "blobs" {
            "/blobs".to_string()
        } else {
            mapper::path_join(&parent_path, name)
        };

        Ok(FileHandle::from_id(self.get_or_create_id(&full_path)))
    }

    async fn getattr(&self, id: FileHandle) -> Result<fattr3, nfsstat3> {
        let vpath = self.get_path(id.to_id()).ok_or(nfsstat3::ERR_STALE)?;

        // Special Case: Virtual /blobs directory
        if vpath == "/blobs" {
            return Ok(self.get_attr(id.to_id(), ftype3::NF3DIR, 4096));
        }

        // Special Case: Files inside /blobs/
        if let Some(key) = self.is_in_blobs(&vpath) {
            if let Ok(Some(meta)) = self.blob_store.get_meta(key).await {
                return Ok(self.get_attr(id.to_id(), ftype3::NF3REG, meta.size.unwrap_or(0)));
            }
        }

        let root = self.state.config.read().await.get_root_dir().map_err(|_| nfsstat3::ERR_IO)?;
        let path = root.join(vpath.trim_start_matches('/'));

        let metadata = tokio::fs::metadata(&path).await.ok();
        let (ftype, size) = if let Some(meta) = metadata {
            if meta.is_dir() {
                (ftype3::NF3DIR, 4096)
            } else {
                // If it's a file, check if it's an HTML shell we should filter
                if let Ok(content) = tokio::fs::read_to_string(&path).await {
                    let filtered = mapper::extract_markdown(&content);
                    (ftype3::NF3REG, filtered.len() as u64)
                } else {
                    (ftype3::NF3REG, meta.len())
                }
            }
        } else {
            let version_store = self.state.version_store.read().await;
            let is_vdir = version_store.file_versions.keys().any(|url| {
                if let Ok(vp) = self.url_to_vpath(url) {
                    vp.starts_with(&vpath) && vp != vpath
                } else {
                    false
                }
            });

            if is_vdir || vpath == "/" {
                (ftype3::NF3DIR, 4096)
            } else {
                return Err(nfsstat3::ERR_NOENT);
            }
        };

        Ok(self.get_attr(id.to_id(), ftype, size))
    }

    async fn setattr(&self, id: FileHandle, _attr: sattr3) -> Result<fattr3, nfsstat3> {
        // For now, just return current attributes
        self.getattr(id).await
    }

    async fn read(&self, id: FileHandle, offset: u64, count: u32) -> Result<ReadResult, nfsstat3> {
        let vpath = self.get_path(id.to_id()).ok_or(nfsstat3::ERR_STALE)?;

        // Special Case: Read from BlobStore
        if let Some(key) = self.is_in_blobs(&vpath) {
            if let Ok(Some((data, _meta))) = self.blob_store.get(key).await {
                let start = offset as usize;
                if start >= data.len() {
                    return Ok(ReadResult {
                        data: vec![],
                        attr: self.get_attr(id.to_id(), ftype3::NF3REG, data.len() as u64),
                        eof: true,
                    });
                }
                let end = std::cmp::min(start + count as usize, data.len());
                let slice = &data[start..end];
                let eof = end == data.len();
                return Ok(ReadResult {
                    data: slice.to_vec(),
                    attr: self.get_attr(id.to_id(), ftype3::NF3REG, data.len() as u64),
                    eof,
                });
            }
            return Err(nfsstat3::ERR_NOENT);
        }

        let root = self.state.config.read().await.get_root_dir().map_err(|_| nfsstat3::ERR_IO)?;
        let path = root.join(vpath.trim_start_matches('/'));

        if !path.exists() {
            return Err(nfsstat3::ERR_NOENT);
        }

        // Try content cache first
        let url = mapper::path_to_url(&root, &path).ok();
        if let Some(url_str) = url {
            let cache = self.state.content_cache.read().await;
            if let Some(content) = cache.get(&url_str) {
                let filtered = mapper::extract_markdown(content).into_bytes();
                let start = offset as usize;
                if start >= filtered.len() {
                    return Ok(ReadResult {
                        data: vec![],
                        attr: self.get_attr(id.to_id(), ftype3::NF3REG, filtered.len() as u64),
                        eof: true,
                    });
                }
                let end = std::cmp::min(start + count as usize, filtered.len());
                let slice = &filtered[start..end];
                let eof = end == filtered.len();
                return Ok(ReadResult {
                    data: slice.to_vec(),
                    attr: self.get_attr(id.to_id(), ftype3::NF3REG, filtered.len() as u64),
                    eof,
                });
            }
        }

        // Fallback: Streaming read from disk
        use tokio::io::AsyncReadExt;
        let mut file = tokio::fs::File::open(&path)
            .await
            .map_err(|_| nfsstat3::ERR_IO)?;

        let metadata = file.metadata().await.map_err(|_| nfsstat3::ERR_IO)?;
        
        // For small files, try markdown filtering
        if metadata.len() < 1024 * 1024 {
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                let filtered = mapper::extract_markdown(&content).into_bytes();
                let start = offset as usize;
                if start >= filtered.len() {
                    return Ok(ReadResult {
                        data: vec![],
                        attr: self.get_attr(id.to_id(), ftype3::NF3REG, filtered.len() as u64),
                        eof: true,
                    });
                }
                let end = std::cmp::min(start + count as usize, filtered.len());
                let slice = &filtered[start..end];
                let eof = end == filtered.len();
                return Ok(ReadResult {
                    data: slice.to_vec(),
                    attr: self.get_attr(id.to_id(), ftype3::NF3REG, filtered.len() as u64),
                    eof,
                });
            }
        }

        // Large file - read directly
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|_| nfsstat3::ERR_IO)?;
        let mut buffer = vec![0u8; count as usize];
        let n = file.read(&mut buffer).await.map_err(|_| nfsstat3::ERR_IO)?;
        buffer.truncate(n);
        let eof = offset + n as u64 >= metadata.len();
        
        Ok(ReadResult {
            data: buffer,
            attr: self.get_attr(id.to_id(), ftype3::NF3REG, metadata.len()),
            eof,
        })
    }

    async fn write(&self, id: FileHandle, offset: u64, data: &[u8], _stable: StableHow) -> Result<WriteResult, nfsstat3> {
        let vpath = self.get_path(id.to_id()).ok_or(nfsstat3::ERR_STALE)?;
        let root = self.state.config.read().await.get_root_dir().map_err(|_| nfsstat3::ERR_IO)?;
        let path = root.join(vpath.trim_start_matches('/'));

        // Logic for re-wrapping Braid shells
        if offset == 0 && data.len() < 1024 * 1024 {
            let new_content_str = String::from_utf8_lossy(data).to_string();
            let url = mapper::path_to_url(&root, &path).ok();

            let mut wrapped_content = None;

            // Try cache first for the "schema" or "shell"
            if let Some(url_str) = &url {
                let cache = self.state.content_cache.read().await;
                if let Some(old_content) = cache.get(url_str) {
                    let wrapped = mapper::wrap_markdown(old_content, &new_content_str);
                    if wrapped != new_content_str {
                        wrapped_content = Some(wrapped);
                    }
                }
            }

            // If not in cache, check disk
            if wrapped_content.is_none() {
                if let Ok(old_content) = tokio::fs::read_to_string(&path).await {
                    let wrapped = mapper::wrap_markdown(&old_content, &new_content_str);
                    if wrapped != new_content_str {
                        wrapped_content = Some(wrapped);
                    }
                }
            }

            if let Some(wrapped) = wrapped_content {
                info!("NFS Write: Re-wrapping markdown for {}", vpath);
                let temp_folder = path
                    .parent()
                    .unwrap_or(std::path::Path::new("."))
                    .join(".braid_tmp");
                crate::blob::atomic_write(&path, wrapped.as_bytes(), &temp_folder)
                    .await
                    .map_err(|_| nfsstat3::ERR_IO)?;

                let metadata = tokio::fs::metadata(&path)
                    .await
                    .map_err(|_| nfsstat3::ERR_IO)?;
                return Ok(WriteResult {
                    count: data.len() as u32,
                    attr: self.get_attr(id.to_id(), ftype3::NF3REG, metadata.len()),
                    committed: StableHow::FileSync,
                    verf: 0,
                });
            }
        }

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|_| nfsstat3::ERR_IO)?;
        }

        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&path)
            .await
            .map_err(|_| nfsstat3::ERR_IO)?;

        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|_| nfsstat3::ERR_IO)?;
        file.write_all(data)
            .await
            .map_err(|_| nfsstat3::ERR_IO)?;

        let metadata = tokio::fs::metadata(&path)
            .await
            .map_err(|_| nfsstat3::ERR_IO)?;
        Ok(WriteResult {
            count: data.len() as u32,
            attr: self.get_attr(id.to_id(), ftype3::NF3REG, metadata.len()),
            committed: StableHow::FileSync,
            verf: 0,
        })
    }

    async fn create(&self, dir_id: FileHandle, name: &str, _attr: sattr3, _mode: CreateMode) -> Result<(FileHandle, fattr3), nfsstat3> {
        let dir_path = self.get_path(dir_id.to_id()).ok_or(nfsstat3::ERR_STALE)?;
        let name_str = name.to_string();
        let full_path = mapper::path_join(&dir_path, &name_str);
        let root = self.state.config.read().await.get_root_dir().map_err(|_| nfsstat3::ERR_IO)?;
        let path = root.join(full_path.trim_start_matches('/'));

        info!("NFS Create: {} (vpath={})", path.display(), full_path);

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|_| nfsstat3::ERR_IO)?;
        }

        tokio::fs::File::create(&path)
            .await
            .map_err(|_| nfsstat3::ERR_IO)?;

        let id = self.get_or_create_id(&full_path);
        let attr = self.get_attr(id, ftype3::NF3REG, 0);
        Ok((FileHandle::from_id(id), attr))
    }

    async fn mkdir(&self, dir_id: FileHandle, name: &str, _attr: sattr3) -> Result<(FileHandle, fattr3), nfsstat3> {
        let dir_path = self.get_path(dir_id.to_id()).ok_or(nfsstat3::ERR_STALE)?;
        let name_str = name.to_string();
        let full_path = mapper::path_join(&dir_path, &name_str);
        let root = self.state.config.read().await.get_root_dir().map_err(|_| nfsstat3::ERR_IO)?;
        let path = root.join(full_path.trim_start_matches('/'));

        info!("NFS Mkdir: {} (vpath={})", path.display(), full_path);

        tokio::fs::create_dir_all(&path)
            .await
            .map_err(|_| nfsstat3::ERR_IO)?;

        let id = self.get_or_create_id(&full_path);
        let attr = self.get_attr(id, ftype3::NF3DIR, 4096);
        Ok((FileHandle::from_id(id), attr))
    }

    async fn remove(&self, dir_id: FileHandle, name: &str) -> Result<(), nfsstat3> {
        let dir_path = self.get_path(dir_id.to_id()).ok_or(nfsstat3::ERR_STALE)?;
        let name_str = name.to_string();
        let full_path = mapper::path_join(&dir_path, &name_str);
        let root = self.state.config.read().await.get_root_dir().map_err(|_| nfsstat3::ERR_IO)?;
        let path = root.join(full_path.trim_start_matches('/'));

        info!("NFS Remove: {} (vpath={})", path.display(), full_path);

        tokio::fs::remove_file(&path)
            .await
            .map_err(|_| nfsstat3::ERR_IO)?;
        Ok(())
    }

    async fn rmdir(&self, dir_id: FileHandle, name: &str) -> Result<(), nfsstat3> {
        let dir_path = self.get_path(dir_id.to_id()).ok_or(nfsstat3::ERR_STALE)?;
        let name_str = name.to_string();
        let full_path = mapper::path_join(&dir_path, &name_str);
        let root = self.state.config.read().await.get_root_dir().map_err(|_| nfsstat3::ERR_IO)?;
        let path = root.join(full_path.trim_start_matches('/'));

        tokio::fs::remove_dir(&path)
            .await
            .map_err(|_| nfsstat3::ERR_IO)?;
        Ok(())
    }

    async fn rename(&self, old_dir: FileHandle, old_name: &str, new_dir: FileHandle, new_name: &str) -> Result<(), nfsstat3> {
        let old_dir_path = self.get_path(old_dir.to_id()).ok_or(nfsstat3::ERR_STALE)?;
        let old_name_str = old_name.to_string();
        let old_full_path = mapper::path_join(&old_dir_path, &old_name_str);

        let new_dir_path = self.get_path(new_dir.to_id()).ok_or(nfsstat3::ERR_STALE)?;
        let new_name_str = new_name.to_string();
        let new_full_path = mapper::path_join(&new_dir_path, &new_name_str);

        let root = self.state.config.read().await.get_root_dir().map_err(|_| nfsstat3::ERR_IO)?;
        let old_path = root.join(old_full_path.trim_start_matches('/'));
        let new_path = root.join(new_full_path.trim_start_matches('/'));

        info!("NFS Rename: {:?} -> {:?}", old_path, new_path);

        if let Some(parent) = new_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|_| nfsstat3::ERR_IO)?;
        }

        tokio::fs::rename(old_path, new_path)
            .await
            .map_err(|_| nfsstat3::ERR_IO)?;

        Ok(())
    }

    async fn readdir(&self, dir_id: FileHandle, cookie: u64, _count: u32) -> Result<ReadDirResult, nfsstat3> {
        let dir_path = self.get_path(dir_id.to_id()).ok_or(nfsstat3::ERR_STALE)?;
        let mut entries = Vec::new();

        if dir_path == "/" {
            // Add virtual /blobs entry
            let blob_id = self.get_or_create_id("/blobs");
            entries.push(DirEntry {
                fileid: blob_id,
                name: "blobs".to_string(),
                cookie: blob_id,
            });
        }

        if dir_path == "/blobs" {
            // List actual blobs from BlobStore
            if let Ok(keys) = self.blob_store.list_keys().await {
                for key in keys {
                    let full_path = format!("/blobs/{}", key);
                    let id = self.get_or_create_id(&full_path);
                    entries.push(DirEntry {
                        fileid: id,
                        name: key,
                        cookie: id,
                    });
                }
            }
        } else {
            let prefix = if dir_path == "/" {
                "/".to_string()
            } else {
                format!("{}/", dir_path)
            };

            // Query DB for immediate children
            let conn = self.state.inode_db.lock();
            let mut stmt = conn
                .prepare("SELECT id, path FROM inodes WHERE path LIKE ? AND path != ?")
                .unwrap();
            let rows = stmt
                .query_map(params![format!("{}%", prefix), dir_path], |row| {
                    Ok((row.get::<_, i64>(0)? as u64, row.get::<_, String>(1)?))
                })
                .unwrap();

            for row in rows {
                if let Ok((id, path)) = row {
                    let relative = if dir_path == "/" {
                        &path[1..]
                    } else {
                        &path[dir_path.len() + 1..]
                    };

                    // Only immediate children (no more slashes)
                    if !relative.is_empty() && !relative.contains('/') {
                        entries.push(DirEntry {
                            fileid: id,
                            name: relative.to_string(),
                            cookie: id,
                        });
                    }
                }
            }
        }

        let start = cookie as usize;
        let paged_entries = if start < entries.len() {
            entries.into_iter().skip(start).collect()
        } else {
            vec![]
        };
        Ok(ReadDirResult {
            entries: paged_entries,
            end: true,
        })
    }

    async fn readdirplus(&self, dir_id: FileHandle, cookie: u64, count: u32) -> Result<ReadDirPlusResult, nfsstat3> {
        // For now, delegate to readdir and build plus info
        let result = self.readdir(dir_id, cookie, count).await?;
        let mut entries = Vec::new();

        for entry in result.entries {
            let handle = FileHandle::from_id(entry.fileid);
            let attr = self.getattr(handle).await.ok().map(post_op_attr::Some).unwrap_or(post_op_attr::None);
            entries.push(DirEntryPlus {
                fileid: entry.fileid,
                name: entry.name,
                cookie: entry.cookie,
                attr,
                handle: Some(handle),
            });
        }

        Ok(ReadDirPlusResult {
            entries,
            end: result.end,
        })
    }

    async fn access(&self, _handle: FileHandle, access: u32) -> Result<u32, nfsstat3> {
        // For now, grant all requested access
        Ok(access)
    }

    async fn fsinfo(&self, _handle: FileHandle) -> Result<fsinfo3, nfsstat3> {
        Ok(fsinfo3 {
            rtmax: 1024 * 1024,      // 1MB max read
            rtpref: 64 * 1024,       // 64KB preferred read
            rtmult: 4096,            // 4KB read multiple
            wtmax: 1024 * 1024,      // 1MB max write
            wtpref: 64 * 1024,       // 64KB preferred write
            wtmult: 4096,            // 4KB write multiple
            dtpref: 8192,            // 8KB readdir preferred
            maxfilesize: u64::MAX,
            time_delta: nfstime3 { seconds: 0, nseconds: 1 },
            properties: 0x1 | 0x2 | 0x4 | 0x8 | 0x10, // Various FS properties
        })
    }

    async fn fsstat(&self, _handle: FileHandle) -> Result<fsstat3, nfsstat3> {
        Ok(fsstat3 {
            tbytes: 0,
            fbytes: 0,
            abytes: 0,
            tfiles: 0,
            ffiles: 0,
            afiles: 0,
            invarsec: 0,
        })
    }

    async fn pathconf(&self, _handle: FileHandle) -> Result<pathconf3, nfsstat3> {
        Ok(pathconf3 {
            link_max: 1,
            name_max: 255,
            no_trunc: true,
            chown_restricted: true,
            case_insensitive: cfg!(windows),
            case_preserving: true,
        })
    }

    async fn commit(&self, handle: FileHandle, _offset: u64, _count: u32) -> Result<fattr3, nfsstat3> {
        self.getattr(handle).await
    }

    // Symlinks not supported
    async fn readlink(&self, _handle: FileHandle) -> Result<String, nfsstat3> {
        Err(nfsstat3::ERR_NOTSUPP)
    }

    async fn symlink(&self, _dir: FileHandle, _name: &str, _link_text: &str, _attr: sattr3) -> Result<(FileHandle, fattr3), nfsstat3> {
        Err(nfsstat3::ERR_NOTSUPP)
    }

    async fn link(&self, _handle: FileHandle, _dir: FileHandle, _name: &str) -> Result<fattr3, nfsstat3> {
        Err(nfsstat3::ERR_NOTSUPP)
    }

    async fn mknod(&self, _dir: FileHandle, _name: &str, _ftype: ftype3, _attr: sattr3, _rdev: specdata3) -> Result<(FileHandle, fattr3), nfsstat3> {
        Err(nfsstat3::ERR_NOTSUPP)
    }
}
