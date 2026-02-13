use crate::core::BraidRequest;
use crate::core::Result;
use crate::fs::daemon::PEER_ID;
use crate::fs::local::mapper;
use crate::fs::state::DaemonState;
use std::collections::HashMap;

pub async fn spawn_subscription(
    url: String,
    subscriptions: &mut HashMap<String, tokio::task::JoinHandle<()>>,
    state: DaemonState,
) {
    if subscriptions.contains_key(&url) {
        return;
    }

    if !state
        .config
        .read()
        .await
        .sync
        .get(&url)
        .cloned()
        .unwrap_or(false)
    {
        return;
    }

    let url_capture = url.clone();
    let state_capture = state.clone();
    let handle = tokio::spawn(async move {
        if let Err(e) = subscribe_loop(url_capture.clone(), state_capture).await {
            tracing::error!("Subscription error for {}: {}", url_capture, e);
        }
    });

    subscriptions.insert(url, handle);
}

pub async fn subscribe_loop(url: String, state: DaemonState) -> Result<()> {
    tracing::info!("Subscribing to {}", url);

    // Get root directory from config
    let root_dir = state.config.read().await.get_root_dir()?;

    let mut req = BraidRequest::new()
        .subscribe()
        .with_header("Accept", "text/plain")
        .with_header("Heartbeats", "30s");

    // Add Cookies
    if let Ok(u) = url::Url::parse(&url) {
        if let Some(domain) = u.domain() {
            let cfg = state.config.read().await;
            if let Some(cookie_val) = cfg.cookies.get(domain) {
                req = req.with_header("Cookie", cookie_val.clone());
            }
        }
    }

    let mut sub = state.client.subscribe(&url, req).await?;
    let mut is_first = true;

    while let Some(update) = sub.next().await {
        let update = update?;

        // Handle 309 Reborn during subscription
        if update.status == 309 {
            tracing::warn!(
                "[BraidFS] Reborn (309) detected during subscription for {}. History reset.",
                url
            );
            is_first = true;
        }

        // Filter echoes (ignore updates from ourselves)
        if let Some(v) = update.primary_version() {
            let v_str = v.to_string();
            let my_id = PEER_ID.read().await;
            if !is_first && v_str.contains(&*my_id) {
                tracing::debug!(
                    "[BraidFS] Ignoring echo from {} (version matches our PEER_ID {})",
                    url,
                    *my_id
                );
                continue;
            }
        }
        is_first = false;

        tracing::debug!("Received update from {}: {:?}", url, update.version);

        // Update version store
        {
            state.tracker.mark(&url);
            let mut store = state.version_store.write().await;
            store.update(&url, update.version.clone(), update.parents.clone());
            let _ = store.save().await;
        }

        let patches = match update.patches.as_ref() {
            Some(p) if !p.is_empty() => p,
            _ => {
                // Check if it is a snapshot
                if let Some(body) = update.body_str() {
                    tracing::info!(
                        "Received snapshot for {}, writing {} bytes",
                        url,
                        body.len()
                    );

                    // Get/Create Merge State
                    let final_content = {
                        let mut merges = state.active_merges.write().await;
                        let peer_id = PEER_ID.read().await.clone();
                        let requested_merge_type = update.merge_type.as_deref().unwrap_or("diamond");
                        let merge = merges.entry(url.clone()).or_insert_with(|| {
                            tracing::info!(
                                "[BraidFS] Creating merge state for {} with type: {}",
                                url,
                                requested_merge_type
                            );
                            let mut m = state
                                .merge_registry
                                .create(requested_merge_type, &peer_id)
                                .or_else(|| state.merge_registry.create("diamond", &peer_id))
                                .expect("Failed to create merge type");
                            m.initialize(body);
                            m
                        });

                        let patch = crate::core::merge::MergePatch {
                            range: "".to_string(),
                            content: serde_json::Value::String(body.to_string()),
                            version: update.primary_version().map(|v| v.to_string()),
                            parents: update.parents.iter().map(|p| p.to_string()).collect(),
                        };
                        merge.apply_patch(patch);
                        merge.get_content()
                    };

                    // Update Content Cache
                    {
                        let mut cache = state.content_cache.write().await;
                        cache.insert(url.clone(), final_content.clone());
                    }

                    if let Ok(path) = mapper::url_to_path(&root_dir, &url) {
                        // Add to pending BEFORE writing to avoid echo loop
                        state.pending.add(path.clone());

                        if let Some(parent) = path.parent() {
                            tokio::fs::create_dir_all(parent).await.ok();
                        }
                        if let Err(e) = tokio::fs::write(&path, final_content).await {
                            tracing::error!("Failed to write snapshot: {}", e);
                            state.pending.remove(&path); // Clean up if failed
                        }
                    }
                }
                continue;
            }
        };

        let path = mapper::url_to_path(&root_dir, &url)?;

        let content = if path.exists() {
            tokio::fs::read_to_string(&path).await.unwrap_or_default()
        } else {
            String::new()
        };

        // Apply Patches via Merge State
        let final_content = {
            let mut merges = state.active_merges.write().await;
            let peer_id = PEER_ID.read().await.clone();
            let requested_merge_type = update.merge_type.as_deref().unwrap_or("diamond");
            let merge = merges.entry(url.clone()).or_insert_with(|| {
                tracing::info!(
                    "[BraidFS] Creating merge state for {} with type: {}",
                    url,
                    requested_merge_type
                );
                let mut m = state
                    .merge_registry
                    .create(requested_merge_type, &peer_id)
                    .or_else(|| state.merge_registry.create("diamond", &peer_id))
                    .expect("Failed to create merge type");
                m.initialize(&content);
                m
            });

            for patch in patches {
                let patch_content = std::str::from_utf8(&patch.content).unwrap_or("");

                let merge_patch = crate::core::merge::MergePatch {
                    range: patch.range.clone(),
                    content: serde_json::Value::String(patch_content.to_string()),
                    version: update.primary_version().map(|v| v.to_string()),
                    parents: update.parents.iter().map(|p| p.to_string()).collect(),
                };
                merge.apply_patch(merge_patch);
            }
            merge.get_content()
        };

        if let Ok(path) = mapper::url_to_path(&root_dir, &url) {
            // Add to pending BEFORE writing to avoid echo loop
            state.pending.add(path.clone());

            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await.ok();
            }

            tokio::fs::write(&path, final_content.clone()).await?;

            // Update Content Cache
            {
                let mut cache = state.content_cache.write().await;
                cache.insert(url.clone(), final_content);
            }
        }

        tracing::info!("Updated local file {}", url);
    }

    Ok(())
}
