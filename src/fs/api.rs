use super::server_handlers::{handle_get_file, handle_put_file};
use crate::core::server::{BraidLayer, BraidState};
use crate::core::types::{Update, Version};
use crate::fs::config::Config;
use anyhow::Result; // Removed Context since unused
use axum::{
    extract::{Query, State},
    routing::{delete, put},
    Json, Router,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<Config>>,
    pub tx_cmd: async_channel::Sender<Command>,
    pub content_cache: Arc<RwLock<HashMap<String, String>>>,
    pub version_store: Arc<RwLock<crate::fs::versions::VersionStore>>,
}

pub enum Command {
    Sync { url: String },
    Unsync { url: String },
    SetCookie { domain: String, value: String },
}

#[derive(Deserialize)]
pub struct SyncParams {
    url: String,
}

#[derive(Deserialize)]
pub struct CookieParams {
    pub domain: String,
    pub value: String,
}

pub async fn run_server(
    port: u16,
    config: Arc<RwLock<Config>>,
    tx_cmd: async_channel::Sender<Command>,
    content_cache: Arc<RwLock<HashMap<String, String>>>,
    version_store: Arc<RwLock<crate::fs::versions::VersionStore>>,
) -> Result<()> {
    let state = AppState {
        config,
        tx_cmd,
        content_cache,
        version_store,
    };

    let app = Router::new()
        .route("/api/sync", put(handle_sync))
        .route("/api/sync", delete(handle_unsync))
        .route("/api/cookie", put(handle_cookie))
        .route(
            "/.braidfs/config",
            axum::routing::get(handle_braidfs_config),
        )
        .route(
            "/.braidfs/errors",
            axum::routing::get(handle_braidfs_errors),
        )
        .route(
            "/.braidfs/get_version/{fullpath}/{hash}",
            axum::routing::get(handle_get_version),
        )
        .route("/{*path}", axum::routing::get(handle_get_file))
        .route("/{*path}", put(handle_put_file))
        .layer(BraidLayer::new().middleware())
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("Daemon API listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn handle_sync(
    State(state): State<AppState>,
    Json(params): Json<SyncParams>,
) -> Json<serde_json::Value> {
    tracing::info!("IPC Command: Sync {}", params.url);

    // Update config persistence handled by main loop or here?
    // Let's send command to main loop to handle everything cleanly (single writer)
    // or just update config here and signal.

    // We'll trust the channel send.
    if let Err(e) = state
        .tx_cmd
        .send(Command::Sync {
            url: params.url.clone(),
        })
        .await
    {
        tracing::error!("Failed to send sync command: {}", e);
        return Json(serde_json::json!({ "status": "error", "message": "Internal channel error" }));
    }

    Json(serde_json::json!({ "status": "ok", "url": params.url }))
}

async fn handle_unsync(
    State(state): State<AppState>,
    Json(params): Json<SyncParams>,
) -> Json<serde_json::Value> {
    tracing::info!("IPC Command: Unsync {}", params.url);

    if let Err(e) = state
        .tx_cmd
        .send(Command::Unsync {
            url: params.url.clone(),
        })
        .await
    {
        tracing::error!("Failed to send unsync command: {}", e);
        return Json(serde_json::json!({ "status": "error", "message": "Internal channel error" }));
    }

    Json(serde_json::json!({ "status": "ok", "url": params.url }))
}

async fn handle_cookie(
    State(state): State<AppState>,
    Json(params): Json<CookieParams>,
) -> Json<serde_json::Value> {
    tracing::info!("IPC Command: SetCookie {}={}", params.domain, params.value);

    if let Err(e) = state
        .tx_cmd
        .send(Command::SetCookie {
            domain: params.domain.clone(),
            value: params.value.clone(),
        })
        .await
    {
        tracing::error!("Failed to send cookie command: {}", e);
        return Json(serde_json::json!({ "status": "error", "message": "Internal channel error" }));
    }

    Json(serde_json::json!({ "status": "ok", "domain": params.domain }))
}

/// Handle /.braidfs/config - returns the current configuration.
async fn handle_braidfs_config(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config = state.config.read().await;
    Json(serde_json::json!({
        "sync": config.sync,
        "cookies": config.cookies,
        "port": config.port,
        "debounce_ms": config.debounce_ms,
        "ignore_patterns": config.ignore_patterns,
    }))
}

/// Error log storage (in-memory for now).
static ERRORS: std::sync::OnceLock<std::sync::Mutex<Vec<String>>> = std::sync::OnceLock::new();

fn get_errors() -> &'static std::sync::Mutex<Vec<String>> {
    ERRORS.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

/// Log an error to the in-memory error log.
pub fn log_error(text: &str) {
    tracing::error!("LOGGING ERROR: {}", text);
    if let Ok(mut errors) = get_errors().lock() {
        errors.push(format!(
            "{}: {}",
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"),
            text
        ));
        // Keep only last 100 errors
        if errors.len() > 100 {
            errors.remove(0);
        }
    }
}

/// Handle /.braidfs/errors - returns the error log.
async fn handle_braidfs_errors() -> String {
    if let Ok(errors) = get_errors().lock() {
        errors.join("\n")
    } else {
        "Error reading error log".to_string()
    }
}

/// Handle /.braidfs/get_version/{fullpath}/{hash} - get version by content hash.
async fn handle_get_version(
    axum::extract::Path((fullpath, hash)): axum::extract::Path<(String, String)>,
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    use percent_encoding::percent_decode_str;
    let fullpath = percent_decode_str(&fullpath)
        .decode_utf8_lossy()
        .to_string();
    let hash = percent_decode_str(&hash).decode_utf8_lossy().to_string();

    tracing::debug!("get_version: {} hash={}", fullpath, hash);

    // Look up version in version store
    let versions = state.version_store.read().await;
    if let Some(version) = versions.get_version_by_hash(&fullpath, &hash) {
        Json(serde_json::json!(version))
    } else {
        Json(serde_json::json!(null))
    }
}

/// Check if a file is read-only.
/// Matches JS `is_read_only()` from braidfs/index.js.
#[cfg(unix)]
pub async fn is_read_only(path: &std::path::Path) -> std::io::Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = tokio::fs::metadata(path).await?;
    let mode = metadata.permissions().mode();
    // Check if write bit is set for owner
    Ok((mode & 0o200) == 0)
}

#[cfg(windows)]
pub async fn is_read_only(path: &std::path::Path) -> std::io::Result<bool> {
    let metadata = tokio::fs::metadata(path).await?;
    Ok(metadata.permissions().readonly())
}

/// Set a file to read-only or writable.
/// Matches JS `set_read_only()` from braidfs/index.js.
#[cfg(unix)]
pub async fn set_read_only(path: &std::path::Path, read_only: bool) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = tokio::fs::metadata(path).await?;
    let mut perms = metadata.permissions();
    let mode = perms.mode();

    let new_mode = if read_only {
        mode & !0o222 // Remove write bits
    } else {
        mode | 0o200 // Add owner write bit
    };

    perms.set_mode(new_mode);
    tokio::fs::set_permissions(path, perms).await
}

#[cfg(windows)]
pub async fn set_read_only(path: &std::path::Path, read_only: bool) -> std::io::Result<()> {
    let metadata = tokio::fs::metadata(path).await?;
    let mut perms = metadata.permissions();
    perms.set_readonly(read_only);
    tokio::fs::set_permissions(path, perms).await
}
