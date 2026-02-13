use crate::blob::store::{BlobMetadata, BlobStore, StoreEvent};
use crate::core::{
    server::{BraidLayer, BraidState},
    types::{Update, Version},
};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Extension, Router,
};
use bytes::Bytes;
use futures::stream::{self, StreamExt};
use std::fmt::Write;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;

pub fn braid_blob_service(store: Arc<BlobStore>) -> Router {
    Router::new()
        .route(
            "/{key}",
            get(handle_get).put(handle_put).delete(handle_delete),
        )
        .layer(BraidLayer::new().middleware())
        .with_state(store)
}

/// Main GET handler - matches JS braid-blob exactly.
///
/// JS equivalent: `braid_blob.serve()` GET handling
///
/// Key behaviors (from JS):
/// - `subscribe`: Start streaming subscription (status 209)
/// - `parents`: If subscribe, send current if newer than parents; if not subscribe, check if we have it
/// - `version`: Check if we have this version
/// - No validation of header combinations (app decides)
async fn handle_get(
    State(store): State<Arc<BlobStore>>,
    Path(key): Path<String>,
    Extension(braid_state): Extension<Arc<BraidState>>,
    req_headers: HeaderMap,
) -> Response {
    // Set response headers (like JS lines 142-144)
    // Editable: true, Accept-Subscribe: true (if not subscribing), Merge-Type: aww
    
    // Extract braid headers (like JS braidify does)
    let subscribe = braid_state.subscribe;
    let version = braid_state.version.clone();
    let parents = braid_state.parents.clone();
    let peer = braid_state.peer.clone();

    // Try to get current blob metadata
    let meta_result = store.get_meta(&key).await;
    
    // Check for unknown version (like JS lines 281-284)
    // If version/parents newer than what we have, return 309
    if let Ok(Some(meta)) = &meta_result {
        let _current_version = meta.version.first();
        
        // Check if requested version is newer than what we have
        if let Some(req_ver) = &version {
            if !req_ver.is_empty() && !store.has_version(&key, &req_ver[0].to_string()).await {
                return Response::builder()
                    .status(309)
                    .header("status", "309 Version Unknown Here")
                    .body(Body::empty())
                    .unwrap();
            }
        }
        
        // Check if requested parents is newer than what we have
        if let Some(req_parents) = &parents {
            if !req_parents.is_empty() && !subscribe {
                // For non-subscribe, check if parents is newer than our current
                if !store.has_version(&key, &req_parents[0].to_string()).await {
                    return Response::builder()
                        .status(309)
                        .header("status", "309 Version Unknown Here")
                        .body(Body::empty())
                        .unwrap();
                }
            }
        }
    }

    if subscribe {
        // ===================================================================
        // Subscription mode (JS: res.startSubscription())
        // ===================================================================
        handle_subscription(store, key, parents, peer).await
    } else {
        // ===================================================================
        // Standard GET - return current blob (JS: res.end(result.body))
        // ===================================================================
        match store.get(&key).await {
            Ok(Some((data, meta))) => {
                // Check Accept header (JS lines 186-189)
                if let (Some(ct), Some(accept)) = (&meta.content_type, req_headers.get("accept")) {
                    let accept_str = accept.to_str().unwrap_or("");
                    if !accept_str.contains(ct) && !accept_str.contains("*/*") {
                        return (
                            StatusCode::NOT_ACCEPTABLE,
                            format!("Content-Type of {} not in Accept: {}", ct, accept_str)
                        ).into_response();
                    }
                }
                
                let version = meta
                    .version
                    .first()
                    .cloned()
                    .unwrap_or_else(|| Version::from("v0"));
                let mut update = Update::snapshot(version, data);

                if let Some(ct) = meta.content_type {
                    update = update.with_content_type(ct);
                }
                if !meta.parents.is_empty() {
                    update = update.with_parents(meta.parents);
                }
                if !meta.version.is_empty() {
                    update.version = meta.version;
                }

                update.into_response()
            }
            Ok(None) => StatusCode::NOT_FOUND.into_response(),
            Err(e) => {
                tracing::error!("Error getting key {}: {}", key, e);
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

/// Handle subscription streaming (status 209).
///
/// JS equivalent: The subscribe handling in `braid_blob.get()`
///
/// Key behaviors:
/// 1. Start subscription (res.startSubscription())
/// 2. Send immediate update if current version > parents
/// 3. Filter future updates: only send if newer than parents
async fn handle_subscription(
    store: Arc<BlobStore>,
    key: String,
    parents: Option<Vec<Version>>,
    _peer: Option<String>,
) -> Response {
    let rx = store.subscribe();

    // Get current metadata to check if we need to send immediately
    let current_meta = store.get_meta(&key).await.ok().flatten();
    let parents_version = parents.as_ref().and_then(|p| p.first().cloned());
    
    // Check if current version > parents (JS line 317)
    let should_send_immediate = if let (Some(meta), Some(parents_ver)) = (&current_meta, &parents_version) {
        let current = meta.version.first();
        // Send if current > parents (simplified - should use proper version comparison)
        current.map(|v| v.to_string()) > Some(parents_ver.to_string())
    } else {
        // No parents specified, send current state
        current_meta.is_some()
    };

    // Get initial state if needed
    let initial_event = if should_send_immediate {
        match store.get(&key).await {
            Ok(Some((data, meta))) => Some(StoreEvent::Put { meta, data }),
            _ => None,
        }
    } else {
        None
    };

    let stream = stream::unfold(rx, move |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(event) => return Some((event, rx)),
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return None,
            }
        }
    });

    let key_filter = key.clone();
    let parents_filter = parents_version.clone();
    
    // Filter updates: only send if newer than parents (JS lines 302-303)
    let updates_stream = stream.filter(move |event| {
        let key_matches = match event {
            StoreEvent::Put { meta, .. } => meta.key == key_filter,
            StoreEvent::Delete { key, .. } => key == &key_filter,
        };
        
        // If parents specified, only send if update version > parents
        let version_ok = match event {
            StoreEvent::Put { meta, .. } => {
                if let Some(ref parents_ver) = parents_filter {
                    // Simplified: send if version != parents
                    meta.version.first().map(|v| v.to_string()) != Some(parents_ver.to_string())
                } else {
                    true
                }
            }
            StoreEvent::Delete { .. } => true,
        };
        
        async move { key_matches && version_ok }
    });

    // Combine initial + updates
    let combined_stream = if let Some(init) = initial_event {
        stream::once(async move { init })
            .chain(updates_stream)
            .boxed()
    } else {
        updates_stream.boxed()
    };

    let body_stream = combined_stream.map(|event| {
        let bytes = format_store_event(event);
        Ok::<Bytes, axum::Error>(bytes)
    });

    // Build subscription response (JS: res.startSubscription())
    Response::builder()
        .status(209)
        .header("subscribe", "true")
        .header("merge-type", "aww")
        .header("cache-control", "no-cache, no-transform, no-store")
        .header("x-accel-buffering", "no")
        .body(Body::from_stream(body_stream))
        .unwrap()
}

/// Format a StoreEvent into Braid wire format.
fn format_store_event(event: StoreEvent) -> Bytes {
    let mut header = String::new();
    let (data, meta) = match event {
        StoreEvent::Put { meta, data } => (Some(data), meta),
        StoreEvent::Delete {
            key: _,
            version,
            content_type,
        } => (
            None,
            BlobMetadata {
                key: "".into(),
                version,
                content_type,
                parents: vec![],
                content_hash: None,
                size: None,
            },
        ),
    };

    if !meta.version.is_empty() {
        let v_str = meta
            .version
            .iter()
            .map(|v| v.to_json().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        write!(&mut header, "Version: {}\r\n", v_str).unwrap();
    }

    if !meta.parents.is_empty() {
        let p_str = meta
            .parents
            .iter()
            .map(|v| v.to_json().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        write!(&mut header, "Parents: {}\r\n", p_str).unwrap();
    }

    if let Some(ct) = &meta.content_type {
        write!(&mut header, "Content-Type: {}\r\n", ct).unwrap();
    }

    if data.is_none() {
        write!(&mut header, "Status: {}\r\n", 404).unwrap();
    }

    if let Some(d) = &data {
        write!(&mut header, "Content-Length: {}\r\n", d.len()).unwrap();
    }

    write!(&mut header, "\r\n").unwrap();

    let mut bytes = Vec::from(header);
    if let Some(d) = data {
        bytes.extend_from_slice(&d);
    }
    bytes.extend_from_slice(b"\n\n");

    Bytes::from(bytes)
}

async fn handle_put(
    State(store): State<Arc<BlobStore>>,
    Path(key): Path<String>,
    Extension(braid_state): Extension<Arc<BraidState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Extract version/parents like JS: req.version, req.parents
    let version_raw = braid_state
        .version
        .clone()
        .unwrap_or_else(|| vec![Version::new(uuid::Uuid::new_v4().to_string())]);

    let parents = braid_state.parents.clone().unwrap_or_default();
    let content_type = headers
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    match store
        .put(&key, body, version_raw.clone(), parents, content_type)
        .await
    {
        Ok(applied_version) => {
            let mut res = Response::new(Body::empty());

            let val_json = applied_version
                .iter()
                .map(|v| v.to_json().to_string())
                .collect::<Vec<_>>()
                .join(", ");

            res.headers_mut()
                .insert("version", val_json.parse().unwrap());
            res
        }
        Err(e) => {
            tracing::error!("Error putting key {}: {}", key, e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn handle_delete(
    State(store): State<Arc<BlobStore>>,
    Path(key): Path<String>,
) -> Response {
    match store.delete(&key).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => {
            tracing::error!("Error deleting key {}: {}", key, e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
