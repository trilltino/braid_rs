//! Main daemon event loop - wires all concerns together
//!
//! This is the central coordinator that:
//! - Initializes all subsystems
//! - Runs the main event loop
//! - Dispatches commands to appropriate handlers

use crate::core::{BraidClient, BraidError};
use crate::fs::common::{ActivityTracker, Config, PendingWrites, ReconnectRateLimiter};
use crate::fs::local::{start_scan_loop, ScanState, VersionStore};
use crate::fs::nfs;
use crate::fs::server::run_server;
use crate::fs::state::{Command, DaemonState};
use crate::fs::sync::{spawn_subscription, BinarySyncManager};

use notify::{Event, RecursiveMode, Watcher};
use std::collections::HashMap;

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

lazy_static::lazy_static! {
    pub static ref PEER_ID: Arc<RwLock<String>> = Arc::new(RwLock::new(String::new()));
}

/// Run the BraidFS daemon
pub async fn run_daemon(port: u16) -> crate::core::Result<()> {
    let mut config = Config::load().await?;
    config.port = port;

    config.save().await?;
    let config = Arc::new(RwLock::new(config));

    // Set global PEER_ID from config
    {
        let cfg = config.read().await;
        let mut id = PEER_ID.write().await;
        *id = cfg.peer_id.clone();
    }

    let content_cache = Arc::new(RwLock::new(std::collections::HashMap::new()));

    // Initialize Merge Registry
    let mut merge_registry = crate::core::merge::MergeTypeRegistry::new();
    merge_registry.register("simpleton", |id| {
        Box::new(crate::core::merge::SimpletonMergeType::new(id))
    });
    let merge_registry = Arc::new(merge_registry);
    let active_merges = Arc::new(RwLock::new(HashMap::new()));

    // Cache Warming: Prime content cache from disk on startup
    let root_dir_for_warmup = {
        let cfg = config.read().await;
        cfg.get_root_dir().map_err(|e| BraidError::Fs(e.to_string()))?
    };
    {
        let cfg = config.read().await;
        let mut cache = content_cache.write().await;
        for (url, enabled) in &cfg.sync {
            if *enabled {
                if let Ok(path) = crate::fs::local::url_to_path(&root_dir_for_warmup, url) {
                    if path.exists() {
                        if let Ok(content) = tokio::fs::read_to_string(&path).await {
                            tracing::info!("[BraidFS] Priming cache for {} from {:?}", url, path);
                            cache.insert(url.clone(), content);
                        }
                    }
                }
            }
        }
    }

    let version_store = {
        let cfg = config.read().await;
        VersionStore::load(&*cfg).await?
    };
    let version_store = Arc::new(RwLock::new(version_store));

    let cfg = config.read().await;
    let root_dir = cfg.get_root_dir().map_err(|e| BraidError::Fs(e.to_string()))?;
    drop(cfg);
    tokio::fs::create_dir_all(&root_dir)
        .await
        .map_err(BraidError::Io)?;

    tracing::info!("BraidFS root: {:?}", root_dir);

    let pending_writes = PendingWrites::new();
    let activity_tracker = ActivityTracker::new();

    // Setup file watcher
    let (tx_fs, mut rx_fs) = tokio::sync::mpsc::channel(100);
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| match res {
        Ok(event) => {
            let _ = tx_fs.blocking_send(event);
        }
        Err(e) => tracing::error!("Watch error: {:?}", e),
    })?;

    watcher.watch(&root_dir, RecursiveMode::Recursive)?;

    let (tx_cmd, rx_cmd) = async_channel::unbounded();
    let rate_limiter = Arc::new(ReconnectRateLimiter::new(100));
    let scan_state = Arc::new(RwLock::new(ScanState::new()));

    // Initialize BlobStore
    let braidfs_dir = root_dir.join(".braidfs");
    let blob_store = Arc::new(
        crate::blob::BlobStore::new(braidfs_dir.join("blobs"), braidfs_dir.join("meta.sqlite"))
            .await
            .map_err(|e| BraidError::Anyhow(e.to_string()))?,
    );

    // Initialize inode database for NFS
    let inode_db_path = braidfs_dir.join("inodes.sqlite");
    let inode_conn = rusqlite::Connection::open(&inode_db_path)
        .map_err(|e| BraidError::Fs(e.to_string()))?;
    inode_conn.execute(
        "CREATE TABLE IF NOT EXISTS inodes (
            id INTEGER PRIMARY KEY,
            path TEXT UNIQUE NOT NULL
        )",
        [],
    ).map_err(|e| BraidError::Fs(e.to_string()))?;
    let inode_db = Arc::new(parking_lot::Mutex::new(inode_conn));

    let binary_sync_manager = BinarySyncManager::new(
            rate_limiter.clone(),
            blob_store.clone(),
            &braidfs_dir,
        )
        .map_err(|e| BraidError::Anyhow(e.to_string()))?;
    let binary_sync_manager = Arc::new(binary_sync_manager);

    // Track recently failed syncs to avoid log spam
    let failed_syncs = Arc::new(RwLock::new(HashMap::new()));

    // Track synced URLs for scanner
    let sync_urls_map = Arc::new(RwLock::new({
        let cfg = config.read().await;
        cfg.sync
            .iter()
            .map(|(u, e)| (u.clone(), *e))
            .collect::<HashMap<String, bool>>()
    }));

    // Start scan loop
    let scan_state_clone = scan_state.clone();
    let sync_urls_clone = sync_urls_map.clone();
    let root_dir_clone = root_dir.clone();
    tokio::spawn(async move {
        start_scan_loop(
            root_dir_clone,
            scan_state_clone,
            sync_urls_clone,
            Duration::from_secs(60),
            |path| {
                tracing::info!("Scanner detected change in {:?}", path);
            },
        )
        .await;
    });

    let braid_client = BraidClient::new()?;

    let state = DaemonState {
        config,
        content_cache,
        version_store,
        tracker: activity_tracker,
        merge_registry,
        active_merges,
        pending: pending_writes,
        client: braid_client,
        failed_syncs,
        binary_sync: binary_sync_manager,
        inode_db,
        tx_cmd,
    };

    let state_server = state.clone();
    tokio::spawn(async move {
        if let Err(e) = run_server(port, state_server).await {
            tracing::error!("API Server crashed: {}", e);
        }
    });

    let mut nfs_handle: Option<tokio::task::JoinHandle<()>> = None;

    let mut subscriptions: HashMap<String, tokio::task::JoinHandle<()>> = HashMap::new();

    // Initial subscriptions
    {
        let cfg = state.config.read().await;
        for (url, enabled) in &cfg.sync {
            if *enabled {
                spawn_subscription(url.clone(), &mut subscriptions, state.clone()).await;
            }
        }
    }

    // Main Event Loop
    loop {
        tokio::select! {
            Some(event) = rx_fs.recv() => {
                crate::fs::local::handle_fs_event(event, state.clone()).await;
            }

            Ok(cmd) = rx_cmd.recv() => {
                match cmd {
                    Command::Sync { url } => {
                        tracing::info!("Enable Sync: {}", url);
                        {
                            let mut cfg = state.config.write().await;
                            cfg.sync.insert(url.clone(), true);
                            let _ = cfg.save().await;
                        }
                        spawn_subscription(url.clone(), &mut subscriptions, state.clone()).await;

                        if crate::fs::sync::binary::should_use_binary_sync(&url) {
                            let bsm = state.binary_sync.clone();
                            let url_clone = url.clone();
                            let root = state.config.read().await.get_root_dir()?;
                            let fullpath = root.join(url.trim_start_matches('/'));
                            tokio::spawn(async move {
                                let _ = bsm.init_binary_sync(&url_clone, &fullpath).await;
                            });
                        }
                        sync_urls_map.write().await.insert(url, true);
                    }
                    Command::Unsync { url } => {
                        tracing::info!("Disable Sync: {}", url);
                        {
                            let mut cfg = state.config.write().await;
                            cfg.sync.remove(&url);
                            let _ = cfg.save().await;
                        }
                        if let Some(handle) = subscriptions.remove(&url) {
                            handle.abort();
                        }
                        sync_urls_map.write().await.remove(&url);
                    }
                    Command::SetCookie { domain, value } => {
                        tracing::info!("Set Cookie: {} for {}", value, domain);
                        let mut cfg = state.config.write().await;
                        cfg.cookies.insert(domain, value);
                        let _ = cfg.save().await;
                    }
                    Command::SetIdentity { domain, email } => {
                        tracing::info!("Set Identity: {} for {}", email, domain);
                        let mut cfg = state.config.write().await;
                        cfg.identities.insert(domain, email);
                        let _ = cfg.save().await;
                    }
                    Command::Mount { port } => {
                        if nfs_handle.is_some() {
                            tracing::warn!("NFS Server already running");
                        } else {
                            let state_nfs = state.clone();
                            let handle = tokio::spawn(async move {
                                // Use the new legit-nfs backend
                                let backend = nfs::BraidNfsBackend::new(state_nfs.clone(), state_nfs.binary_sync.blob_store());
                                tracing::info!("Starting legit-nfs server on port {}", port);
                                let server = nfs::legit_nfs::NfsServer::new(backend)
                                    .bind_addr(format!("127.0.0.1:{}", port));
                                if let Err(e) = server.run().await {
                                    tracing::error!("NFS Server error: {}", e);
                                }
                            });
                            nfs_handle = Some(handle);
                        }
                    }
                    Command::Unmount => {
                        if let Some(handle) = nfs_handle.take() {
                            tracing::info!("Stopping NFS server");
                            handle.abort();
                        }
                    }
                }
            }
        }
    }
}
