use crate::fs::local::mapper;
use crate::core::server::BraidState;
use crate::core::types::{Update, Version};
use crate::fs::state::DaemonState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension,
};
use std::sync::Arc;

pub async fn handle_get_file(
    Path(path): Path<String>,
    State(state): State<DaemonState>,
    Extension(braid_state): Extension<Arc<BraidState>>,
) -> Response {
    tracing::info!("GET /{} (subscribe={})", path, braid_state.subscribe);

    // Get config values
    let (root_dir, app_name, port) = {
        let cfg = state.config.read().await;
        match cfg.get_root_dir() {
            Ok(dir) => (dir, cfg.app_name.clone(), cfg.port),
            Err(e) => {
                tracing::error!("Failed to get root directory: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, "Configuration error").into_response();
            }
        }
    };

    let config_path = format!(".{}/config", app_name);
    let errors_path = format!(".{}/errors", app_name);

    // Only serve meta config and errors, everything else is 404
    if path != config_path && path != errors_path {
        tracing::debug!("Path not found: {}", path);
        return (
            StatusCode::NOT_FOUND,
            axum::response::Html(
                format!(r#"Nothing to see here. You can go to <a href=".{0}/config">.{0}/config</a> or <a href=".{0}/errors">.{0}/errors</a>"#, app_name)
            )
        ).into_response();
    }

    // Map URL path to filesystem path
    let file_path = match mapper::url_to_path(&root_dir, &path) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Path mapping error: {:?}", e);
            return (StatusCode::BAD_REQUEST, format!("Invalid path: {}", e)).into_response();
        }
    };

    // Read file content
    let content = match tokio::fs::read_to_string(&file_path).await {
        Ok(c) => c,
        Err(_) => {
            if let Some(parent) = file_path.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            let empty_content = if path == config_path {
                &format!(r#"{{"sync":{{}},"cookies":{{}},"port":{}}}"#, port)
            } else {
                ""
            };
            let _ = tokio::fs::write(&file_path, empty_content).await;
            empty_content.to_string()
        }
    };

    // Get current version
    let version = {
        let store = state.version_store.read().await;
        store
            .get(&path)
            .map(|v| v.current_version.clone())
            .unwrap_or_else(|| vec!["initial".to_string()])
    };

    // Send snapshot response
    let update = Update::snapshot(Version::new(version[0].clone()), content.clone());
    update.into_response()
}

pub async fn handle_put_file(
    Path(path): Path<String>,
    State(state): State<DaemonState>,
    Extension(braid_state): Extension<Arc<BraidState>>,
    _headers: axum::http::HeaderMap,
    body: String,
) -> Response {
    tracing::info!("PUT /{}", path);

    // Get config values
    let (root_dir, app_name) = {
        let cfg = state.config.read().await;
        match cfg.get_root_dir() {
            Ok(dir) => (dir, cfg.app_name.clone()),
            Err(e) => {
                tracing::error!("Failed to get root directory: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, "Configuration error").into_response();
            }
        }
    };

    let config_path = format!(".{}/config", app_name);
    let errors_path = format!(".{}/errors", app_name);

    // Only allow PUT to meta config and errors
    if path != config_path && path != errors_path {
        tracing::warn!("PUT not allowed for path: {}", path);
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }

    // Map URL path to filesystem path
    let file_path = match mapper::url_to_path(&root_dir, &path) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Path mapping error: {:?}", e);
            return (StatusCode::BAD_REQUEST, format!("Invalid path: {}", e)).into_response();
        }
    };

    // Write content to file
    if let Some(parent) = file_path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            tracing::error!("Failed to create directory: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Directory creation failed",
            )
                .into_response();
        }
    }

    if let Err(e) = tokio::fs::write(&file_path, &body).await {
        tracing::error!("Failed to write file: {:?}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "File write failed").into_response();
    }

    // Update content cache
    {
        let mut cache = state.content_cache.write().await;
        cache.insert(path.clone(), body);
    }

    // Update version store if version was provided
    if let Some(version) = &braid_state.version {
        let mut store = state.version_store.write().await;
        store.update(
            &path,
            version.clone(),
            braid_state.parents.clone().unwrap_or_default(),
        );
        let _ = store.save().await;
    }

    tracing::info!("File written: {}", path);
    (StatusCode::OK, "File updated").into_response()
}
