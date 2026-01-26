use crate::fs::mapping;
use crate::fs::state::DaemonState;
use crate::fs::sync::sync_local_to_remote;
use notify::Event;

pub async fn handle_fs_event(event: Event, state: DaemonState) {
    if event.kind.is_access() || event.kind.is_other() {
        return;
    }

    for path in event.paths {
        // We handle both modifications and removals
        let is_removal = event.kind.is_remove() || !path.exists();

        // Skip non-files if it's NOT a removal (e.g. it's a new directory)
        if !is_removal && !path.is_file() {
            tracing::debug!("[BraidFS] Skipping non-file: {:?}", path);
            continue;
        }

        // Skip if this is a dotfile or inside a hidden directory (like .braidfs)
        if path
            .components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        {
            continue;
        }

        // Skip if this was a pending write from us (to avoid echo loops)
        if state.pending.should_ignore(&path) {
            tracing::debug!("[BraidFS] Skipping pending write: {:?}", path);
            continue;
        }

        match mapping::path_to_url(&path) {
            Ok(url) => {
                tracing::info!("[BraidFS] File changed: {:?} -> {}", path, url);

                let content = if is_removal {
                    tracing::info!("[BraidFS] File deleted: {:?}", path);
                    String::new() // Propagate empty string (LWW deletion)
                } else {
                    match tokio::fs::read_to_string(&path).await {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::error!("[BraidFS] Failed to read file {:?}: {}", path, e);
                            continue;
                        }
                    }
                };

                // Get Parents (current version is new parent)
                let parents = {
                    let store = state.version_store.read().await;
                    store
                        .get(&url)
                        .map(|v| v.current_version.clone())
                        .unwrap_or_default()
                };

                // Get Content for Diff logic
                let original_content = {
                    let cache = state.content_cache.read().await;
                    cache.get(&url).cloned()
                };

                tracing::info!(
                    "[BraidFS] Syncing to remote: url={}, parents={:?}, has_original={}",
                    url,
                    parents,
                    original_content.is_some()
                );

                // STARTUP FIX: Server Wins
                if !parents.is_empty() && original_content.is_none() {
                    tracing::warn!("[BraidFS] Startup Conflict: Local file {} exists but cache is empty. Prioritizing Remote.", url);
                    continue;
                }

                println!("BraidFS: Syncing local change to remote: {}", url);
                if let Err(e) = sync_local_to_remote(
                    &path,
                    &url,
                    &parents,
                    original_content,
                    content.clone(),
                    state.clone(),
                )
                .await
                {
                    println!("BraidFS: Failed to sync {}: {:?}", url, e);
                } else {
                    println!("BraidFS: Successfully synced {}", url);
                }
            }
            Err(e) => {
                tracing::debug!("[BraidFS] Ignoring file {:?}: {:?}", path, e);
            }
        }
    }
}
