use crate::core::{BraidClient, Result};
use crate::fs::api::run_server;
use crate::fs::binary_sync::BinarySyncManager;
use crate::fs::config::{self, Config};
use crate::fs::rate_limiter::ReconnectRateLimiter;
use crate::fs::scanner::{start_scan_loop, ScanState};
use crate::fs::versions::VersionStore;
use notify::{RecursiveMode, Watcher};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use crate::fs::binary_sync;
use crate::fs::mapping;
use crate::fs::state::{ActivityTracker, Command, DaemonState, PendingWrites};
use crate::fs::subscription::spawn_subscription;
use crate::fs::watcher::handle_fs_event;

pub static PEER_ID: once_cell::sync::Lazy<Arc<RwLock<String>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(String::new())));

pub async fn run_daemon(port: u16) -> Result<()> {
    let mut config = Config::load().await?;
    config.port = port;

    // Ensure baseline syncs (e.g. braid.org/tino) are always present
    if !config.sync.contains_key("https://braid.org/tino") {
        config
            .sync
            .insert("https://braid.org/tino".to_string(), true);
    }

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
    merge_registry.register("antimatter", |id| {
        Box::new(crate::core::merge::AntimatterMergeType::new_native(id))
    });
    let merge_registry = Arc::new(merge_registry);
    let active_merges = Arc::new(RwLock::new(HashMap::new()));

    // Cache Warming: Prime content cache from disk on startup
    {
        let cfg = config.read().await;
        let mut cache = content_cache.write().await;
        for (url, enabled) in &cfg.sync {
            if *enabled {
                if let Ok(path) = mapping::url_to_path(url) {
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

    let version_store = VersionStore::load().await?;
    let version_store = Arc::new(RwLock::new(version_store));

    let root_dir =
        config::get_root_dir().map_err(|e| crate::core::BraidError::Fs(e.to_string()))?;
    tokio::fs::create_dir_all(&root_dir)
        .await
        .map_err(|e| crate::core::BraidError::Io(e))?;

    tracing::info!("BraidFS root: {:?}", root_dir);

    let pending_writes = PendingWrites::new();
    let activity_tracker = ActivityTracker::new();

    // Setup file watcher
    let (tx_fs, mut rx_fs) = tokio::sync::mpsc::channel(100);
    let mut watcher =
        notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
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
            .map_err(|e| crate::core::BraidError::Anyhow(e.to_string()))?,
    );

    let binary_sync_manager = BinarySyncManager::new(rate_limiter.clone(), blob_store.clone())
        .map_err(|e| crate::core::BraidError::Anyhow(e.to_string()))?;
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
    tokio::spawn(async move {
        start_scan_loop(
            scan_state_clone,
            sync_urls_clone,
            Duration::from_secs(60),
            |path| {
                tracing::info!("Scanner detected change in {:?}", path);
            },
        )
        .await;
    });

    let braid_client = BraidClient::new();

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
        tx_cmd,
    };

    let state_server = state.clone();
    tokio::spawn(async move {
        if let Err(e) = run_server(port, state_server).await {
            tracing::error!("API Server crashed: {}", e);
        }
    });

    let mut subscriptions: HashMap<String, tokio::task::JoinHandle<()>> = HashMap::new();

    // Initial subscriptions
    {
        let cfg = state.config.read().await;
        for (url, enabled) in &cfg.sync {
            if *enabled {
                spawn_subscription(url.clone(), &mut subscriptions, state.clone());
            }
        }
    }

    // Main Event Loop
    loop {
        tokio::select! {
            Some(event) = rx_fs.recv() => {
                handle_fs_event(event, state.clone()).await;
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
                        spawn_subscription(url.clone(), &mut subscriptions, state.clone());

                        if binary_sync::should_use_binary_sync(&url) {
                            let bsm = state.binary_sync.clone();
                            let url_clone = url.clone();
                            let root = config::get_root_dir()?;
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
                }
            }
        }
    }
}
