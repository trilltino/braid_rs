//! # BraidFS Core Logic
//!
//! This module implements the synchronization daemon.
//!
//! ## Architecture vs JS
//! - **Daemon**: `run_daemon` corresponds to the `main()` function in `index.js` + `http.createServer`.
//! - **File Watching**: Uses `notify` (vs `chokidar`).
//! - **Loop Avoidance**: `PendingWrites` prevents echo-loops (vs timestamp checks in JS).
//! - **Diffing**: Unlike JS `diff.js`, this implementation currently uses **Snapshot Updates** (sending full content)
//!   while maintaining correct Braid `Parents` headers.
//!
//! See `porting_guide.md` for full details.

use crate::core::{BraidClient, BraidRequest, Version};
use crate::fs::api::{run_server, AppState, Command};
use crate::fs::binary_sync::{should_use_binary_sync, BinarySyncManager};
use crate::fs::config::Config;
use crate::fs::rate_limiter::ReconnectRateLimiter;
use crate::fs::scanner::{normalize_url, start_scan_loop, ScanState};
use crate::fs::versions::VersionStore;
use anyhow::Result;
use futures::StreamExt;
use notify::{Event, RecursiveMode, Watcher};
use reqwest::Client;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::RwLock;

pub mod api;
pub mod binary_sync;
pub mod config;
pub mod diff;
pub mod mapping;
pub mod rate_limiter;
pub mod scanner;
pub mod server_handlers;
pub mod versions;

#[derive(Clone)]
struct PendingWrites {
    paths: Arc<Mutex<HashSet<PathBuf>>>,
}

impl PendingWrites {
    fn new() -> Self {
        Self {
            paths: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    fn add(&self, path: PathBuf) {
        self.paths.lock().unwrap().insert(path);
    }

    fn remove(&self, path: &PathBuf) -> bool {
        self.paths.lock().unwrap().remove(path)
    }
}

pub async fn run_daemon(port: u16) -> Result<()> {
    let mut config = Config::load().await?;
    config.port = port;
    config.save().await?;

    let config = Arc::new(RwLock::new(config));

    let content_cache = Arc::new(RwLock::new(std::collections::HashMap::new()));

    let version_store = VersionStore::load().await?;
    let version_store = Arc::new(RwLock::new(version_store));

    // Get root dir from config
    let root_dir = {
        let _cfg = config.read().await; // Suppress unused var warning with _
        config::get_root_dir()?
    };

    tokio::fs::create_dir_all(&root_dir).await?;

    tracing::info!("BraidFS root: {:?}", root_dir);
    tracing::info!("Watching for changes...");

    let pending_writes = PendingWrites::new();
    let pending_writes_watcher = pending_writes.clone();

    // Setup file watcher
    let (tx_fs, mut rx_fs) = tokio::sync::mpsc::channel(100);

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| match res {
        Ok(event) => {
            let _ = tx_fs.blocking_send(event);
        }
        Err(e) => tracing::error!("Watch error: {:?}", e),
    })?;

    watcher.watch(&root_dir, RecursiveMode::Recursive)?;

    tracing::info!("BraidFS daemon running on port {}", port);

    let (tx_cmd, rx_cmd) = async_channel::unbounded();

    // Initialize rate limiter, scanner state, and binary sync manager
    let rate_limiter = Arc::new(ReconnectRateLimiter::new(100));
    let scan_state = Arc::new(RwLock::new(ScanState::new()));
    let binary_sync_manager = BinarySyncManager::new(rate_limiter.clone())?;
    let binary_sync_manager = Arc::new(binary_sync_manager);

    // Track synced URLs for scanner
    let sync_urls: Vec<(String, bool)> = {
        let cfg = config.read().await;
        cfg.sync.iter().map(|(u, e)| (u.clone(), *e)).collect()
    };
    let sync_urls_map = Arc::new(RwLock::new(
        sync_urls.into_iter().collect::<HashMap<String, bool>>(),
    ));

    // Start scan loop
    let scan_state_clone = scan_state.clone();
    let sync_urls_clone = sync_urls_map.clone();
    tokio::spawn(async move {
        start_scan_loop(
            scan_state_clone,
            sync_urls_clone,
            Duration::from_secs(60), // Scan every minute
            |path| {
                tracing::info!("Scanner detected change in {:?}", path);
                // In a full implementation, we'd trigger a sync here
            },
        )
        .await;
    });

    let config_for_server = config.clone();
    let content_cache_for_server = content_cache.clone();
    let version_store_for_server = version_store.clone();
    tokio::spawn(async move {
        if let Err(e) = run_server(
            port,
            config_for_server,
            tx_cmd,
            content_cache_for_server,
            version_store_for_server,
        )
        .await
        {
            tracing::error!("API Server crashed: {}", e);
        }
    });

    let client = Client::new();
    let braid_client = BraidClient::new();

    let mut subscriptions: std::collections::HashMap<String, tokio::task::JoinHandle<()>> =
        std::collections::HashMap::new();

    // Initial subscriptions
    {
        let cfg = config.read().await;
        for (url, enabled) in &cfg.sync {
            if *enabled {
                spawn_subscription(
                    url.clone(),
                    &mut subscriptions,
                    braid_client.clone(),
                    pending_writes.clone(),
                    version_store.clone(),
                    content_cache.clone(),
                );
            }
        }
    }

    // Main Event Loop
    loop {
        tokio::select! {
            Some(event) = rx_fs.recv() => {
                let config_read = config.read().await;
                handle_fs_event(event, &client, &pending_writes_watcher, &version_store, &content_cache, &config_read).await;
            }


            Ok(cmd) = rx_cmd.recv() => {
                match cmd {
                    Command::Sync { url } => {
                        tracing::info!("Enable Sync: {}", url);
                        {
                            let mut cfg = config.write().await;
                            cfg.sync.insert(url.clone(), true);
                            let _ = cfg.save().await;
                        }
                        spawn_subscription(
                            url.clone(),
                            &mut subscriptions,
                            braid_client.clone(),
                            pending_writes.clone(),
                            version_store.clone(),
                            content_cache.clone()
                        );

                        // Handle binary sync if needed
                        if should_use_binary_sync(&url) {
                            let bsm = binary_sync_manager.clone();
                            let url_clone = url.clone();
                            // We need fullpath, which isn't easy here.
                            // JS logic calculates it from URL.
                            let root = config::get_root_dir()?;
                            let fullpath = root.join(url.trim_start_matches('/'));
                            tokio::spawn(async move {
                                let _ = bsm.init_binary_sync(&url_clone, &fullpath).await;
                            });
                        }

                        // Update scanner map
                        sync_urls_map.write().await.insert(url, true);

                    }
                    Command::Unsync { url } => {
                        tracing::info!("Disable Sync: {}", url);
                        {
                            let mut cfg = config.write().await;
                            cfg.sync.remove(&url);
                            let _ = cfg.save().await;
                        }
                        if let Some(handle) = subscriptions.remove(&url) {
                            handle.abort();
                        }
                    }
                    Command::SetCookie { domain, value } => {
                         tracing::info!("Set Cookie: {} for {}", value, domain);
                         {
                             let mut cfg = config.write().await;
                             cfg.cookies.insert(domain, value);
                             let _ = cfg.save().await;
                         }
                    }
                }
            }
        }
    }
}

fn spawn_subscription(
    url: String,
    subscriptions: &mut std::collections::HashMap<String, tokio::task::JoinHandle<()>>,
    client: BraidClient,
    pending: PendingWrites,
    versions: Arc<RwLock<VersionStore>>,
    content_cache: Arc<RwLock<std::collections::HashMap<String, String>>>,
) {
    if subscriptions.contains_key(&url) {
        return;
    }

    let url_capture = url.clone();
    let handle = tokio::spawn(async move {
        if let Err(e) = subscribe_loop(client, url_capture, pending, versions, content_cache).await
        {
            tracing::error!("Subscription error: {:?}", e);
        }
    });

    subscriptions.insert(url, handle);
}

async fn subscribe_loop(
    client: BraidClient,
    url: String,
    pending: PendingWrites,
    versions: Arc<RwLock<VersionStore>>,
    content_cache: Arc<RwLock<std::collections::HashMap<String, String>>>,
) -> Result<()> {
    tracing::info!("Subscribing to {}", url);

    let req = BraidRequest::new().subscribe();
    let mut sub = client.subscribe(&url, req).await?;

    while let Some(update) = sub.next().await {
        let update = update?;
        tracing::debug!("Received update from {}: {:?}", url, update.version);

        // Update version store
        {
            let mut store = versions.write().await;
            store.update(&url, update.version.clone(), update.parents.clone());
            let _ = store.save().await;
        }

        let patches = match update.patches {
            Some(p) if !p.is_empty() => p,
            _ => continue,
        };

        let path = mapping::url_to_path(&url)?;

        let mut content = if path.exists() {
            tokio::fs::read_to_string(&path).await.unwrap_or_default()
        } else {
            String::new()
        };

        for patch in patches {
            let (start, end) = match parse_range(&patch.range) {
                Some(r) => r,
                None => continue,
            };

            // Check unit
            if patch.unit != "bytes" && patch.unit != "text" {
                tracing::warn!("Unsupported patch unit: {}", patch.unit);
                // Try parse anyway? No.
            }

            let patch_content = std::str::from_utf8(&patch.content).unwrap_or("");

            if start <= content.len() && end <= content.len() {
                content.replace_range(start..end, patch_content);
            } else {
                if content.is_empty() && start == 0 && end == 0 {
                    content.push_str(patch_content);
                }
            }
        }

        // Update Content Cache with new full content
        {
            let mut cache = content_cache.write().await;
            cache.insert(url.clone(), content.clone());
        }

        pending.add(path.clone());
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, &content).await?;

        tracing::info!("Updated local file {}", url);
    }

    Ok(())
}

fn parse_range(range: &str) -> Option<(usize, usize)> {
    let inner = range.trim().trim_matches(|c| c == '[' || c == ']');
    let (s, e) = inner.split_once(':').or_else(|| inner.split_once('-'))?;
    let start = s.trim().parse().ok()?;
    let end = e.trim().parse().ok()?;
    Some((start, end))
}

async fn handle_fs_event(
    event: Event,
    client: &Client,
    pending: &PendingWrites,
    versions: &Arc<RwLock<VersionStore>>,
    content_cache: &Arc<RwLock<std::collections::HashMap<String, String>>>,
    config: &Config,
) {
    tracing::debug!("FS Event: {:?}", event);

    for path in event.paths {
        if path.is_file() {
            if pending.remove(&path) {
                continue;
            }

            match mapping::path_to_url(&path) {
                Ok(url) => {
                    tracing::info!("File changed: {:?} -> {}", path, url);

                    // Get Parents (current version is new parent)
                    let parents = {
                        let store = versions.read().await;
                        store
                            .get(&url)
                            .map(|v| v.current_version.clone())
                            .unwrap_or_default()
                    };

                    // Get Content for Diff logic
                    let original_content = {
                        let cache = content_cache.read().await;
                        cache.get(&url).cloned()
                    };

                    if let Err(e) = sync_local_to_remote(
                        client,
                        &path,
                        &url,
                        &parents,
                        original_content,
                        config,
                    )
                    .await
                    {
                        tracing::error!("Failed to sync local change for {}: {:?}", url, e);
                    }
                }
                Err(e) => {
                    tracing::debug!("Ignoring file {:?}: {:?}", path, e);
                }
            }
        }
    }
}

async fn sync_local_to_remote(
    client: &Client,
    path: &PathBuf,
    url: &str,
    parents: &[String],
    original_content: Option<String>,
    config: &Config,
) -> Result<()> {
    let content = tokio::fs::read_to_string(path).await?;

    // Decide between Diff or Snapshot
    let (body, patches) = if let Some(original) = original_content {
        // Compute Diffs
        let patches = crate::fs::diff::compute_patches(&original, &content);
        if patches.is_empty() {
            return Ok(()); // No change
        }
        (String::new(), Some(patches))
    } else {
        (content, None)
    };

    let mut req = client.put(url);

    // Add Cookies
    // Assuming simple mapping for now: check if domain matches and add cookie string
    if let Ok(u) = url::Url::parse(url) {
        if let Some(domain) = u.domain() {
            if let Some(cookie_val) = config.cookies.get(domain) {
                req = req.header("Cookie", cookie_val);
            }
        }
    }

    if let Some(p_vec) = patches {
        req = req.header("Patches", serde_json::to_string(&p_vec)?);
        // Braid spec: body should be empty if Patches header is present and sufficient?
        // Or we send patches in body?
        // Typically Patches header contains JSON patches or Range-Patch.
        // braid_rs server expects patches in body for PATCH usually, but PUT with Patches header is valid too.
        // Let's send empty body for now as patches are in header.
    } else {
        req = req.body(body).header("Content-Type", "text/plain");
    }

    // Add Parents header if we have them
    if !parents.is_empty() {
        let parents_str = parents
            .iter()
            .map(|p| format!("\"{}\"", p))
            .collect::<Vec<_>>()
            .join(", ");
        req = req.header("Parents", parents_str);
    }

    let res = req.send().await?;

    if !res.status().is_success() {
        return Err(anyhow::anyhow!("Remote server returned {}", res.status()));
    }

    if let Some(v_header) = res.headers().get("version") {
        if let Ok(v_str) = v_header.to_str() {
            tracing::info!("Synced {} (Version: {})", url, v_str);
        }
    } else {
        tracing::info!("Synced {}", url);
    }

    Ok(())
}
