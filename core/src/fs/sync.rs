use super::ActivityTracker;
use crate::core::types::Version;
use crate::core::BraidRequest;
use crate::core::Result;
use crate::fs::state::DaemonState;
use crate::fs::PEER_ID;
use std::path::PathBuf;

pub async fn sync_local_to_remote(
    _path: &PathBuf,
    url: &str,
    parents: &[String],
    original_content: Option<String>,
    new_content: String,
    state: DaemonState,
) -> Result<()> {
    state.tracker.mark(url);
    tracing::info!("[BraidFS-PUT] Starting sync_local_to_remote for: {}", url);

    let config = state.config.read().await;

    // 1. Capture Identity / Author
    let author_ident = if let Ok(u) = url::Url::parse(url) {
        if let Some(domain) = u.domain() {
            config.identities.get(domain).cloned()
        } else {
            None
        }
    } else {
        None
    };

    // 2. Prepare Request Template
    let my_id = PEER_ID.read().await.clone();
    let seq = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let new_version = format!("{}-{}", my_id, seq);

    let mut request = BraidRequest::new()
        .with_method("PUT")
        .with_header("Peer", &my_id)
        .with_header("User-Agent", "BraidFS/0.1.2")
        .with_version(crate::core::types::Version::new(&new_version));

    if let Some(author) = author_ident.clone() {
        request = request.with_header("Author", author);
    }

    if let Ok(u) = url::Url::parse(url) {
        if let Some(domain) = u.domain() {
            if let Some(cookie_val) = config.cookies.get(domain) {
                request = request.with_header("Cookie", cookie_val.clone());
            }
        }
    }

    if !parents.is_empty() {
        request = request.with_parents(
            parents
                .iter()
                .map(|p| crate::core::types::Version::new(p.trim_matches('"')))
                .collect(),
        );
    }

    // 2.5 Prepare Content (Strip BOM and handle authorship if needed)
    let author_email = author_ident.clone().unwrap_or_else(|| "anon".to_string());
    let clean_content = new_content
        .strip_prefix("\u{feff}")
        .unwrap_or(&new_content)
        .to_string();

    let final_body = if !url.contains("/blob") && !url.contains("tino") {
        prepare_antimatter_json(&clean_content, &author_email)
    } else {
        clean_content.clone()
    };

    // 3. Compute Patches or Snapshot
    let (request_with_payload, _is_patch) = {
        let mut merges = state.active_merges.write().await;
        if let Some(merge) = merges.get_mut(url) {
            let mp_content = serde_json::Value::String(final_body.clone());
            let edit_res = merge.local_edit(crate::core::merge::MergePatch::new("", mp_content));
            if !edit_res.success || edit_res.rebased_patches.is_empty() {
                (request.clone().with_body(final_body.clone()), false)
            } else {
                let mut braid_patches = Vec::new();
                for p in &edit_res.rebased_patches {
                    let rp_raw = if let serde_json::Value::String(s) = &p.content {
                        s.clone()
                    } else {
                        p.content.to_string()
                    };

                    log_braid_inspector(
                        "PUT (Patch)",
                        url,
                        &edit_res.version.clone().unwrap_or_default(),
                        &parents,
                        author_ident.as_deref(),
                        &rp_raw,
                        &format!("text {}", p.range),
                        &state.tracker,
                    );

                    braid_patches.push(crate::core::types::Patch {
                        unit: "text".to_string(),
                        range: p.range.clone(),
                        content: rp_raw.as_bytes().to_vec().into(),
                        content_length: Some(rp_raw.len()),
                    });
                }
                let mut req = request
                    .clone()
                    .with_patches(braid_patches)
                    .with_merge_type(merge.name());
                if let Some(v) = &edit_res.version {
                    req = req.with_version(crate::core::types::Version::new(v));
                }
                (req, true)
            }
        } else if let Some(original) = &original_content {
            let patches = crate::fs::diff::compute_patches(original, &new_content);
            if patches.is_empty() {
                return Ok(());
            }
            (request.clone().with_patches(patches), true)
        } else {
            (request.clone().with_body(new_content.clone()), false)
        }
    };

    // 4. Send Request
    let res = state.client.fetch(url, request_with_payload).await?;

    if (200..300).contains(&res.status) {
        state.failed_syncs.write().await.remove(url);

        let v_header = res.headers.get("version").cloned().unwrap_or_default();
        if !v_header.is_empty() {
            let mut store = state.version_store.write().await;
            store.update(
                url,
                vec![crate::core::types::Version::new(&v_header)],
                parents
                    .iter()
                    .map(|p| crate::core::types::Version::new(p))
                    .collect(),
            );
            let _ = store.save().await;
        }

        state
            .content_cache
            .write()
            .await
            .insert(url.to_string(), new_content);
        return Ok(());
    }

    // 5. Reborn Handling (309)
    if res.status == 309 {
        let retry_req = request
            .clone()
            .with_parents(vec![])
            .with_body(new_content.clone());

        tracing::warn!(
            "[BraidFS] 309 Reborn for {}. Retrying with empty parents.",
            url
        );
        let res2 = state.client.fetch(url, retry_req).await?;
        if (200..300).contains(&res2.status) {
            state
                .content_cache
                .write()
                .await
                .insert(url.to_string(), new_content);
            return Ok(());
        }
    }

    state
        .failed_syncs
        .write()
        .await
        .insert(url.to_string(), (res.status, std::time::Instant::now()));
    return Err(crate::core::BraidError::Http(format!(
        "Failed to sync local change to remote: HTTP {}",
        res.status
    )));
}

pub fn prepare_antimatter_json(content: &str, author_email: &str) -> String {
    if content.trim().starts_with('{') || content.trim().starts_with('[') {
        content.to_string()
    } else {
        serde_json::json!({
            "content": content,
            "author": author_email,
            "time": chrono::Utc::now().to_rfc3339(),
        })
        .to_string()
    }
}

pub fn log_braid_inspector(
    method: &str,
    url: &str,
    version: &str,
    parents: &[String],
    author: Option<&str>,
    body: &str,
    patches: &str,
    tracker: &ActivityTracker,
) {
    if !tracker.is_active(url) {
        return;
    }

    let parents_str = parents.join(", ");
    let author_str = author.unwrap_or("anon");

    tracing::debug!(
        "[BRAID-INSPECTOR] {} {} | Version: {} | Parents: [{}] | Author: {} | Patches: {} | Body: {} bytes",
        method,
        url,
        version,
        parents_str,
        author_str,
        patches,
        body.len()
    );
}
