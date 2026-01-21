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

async fn handle_get(
    State(store): State<Arc<BlobStore>>,
    Path(key): Path<String>,
    Extension(braid_state): Extension<Arc<BraidState>>,
) -> Response {
    if braid_state.subscribe {
        handle_subscription(store, key).await
    } else {
        handle_standard_get(store, key, braid_state.as_ref().clone()).await
    }
}

async fn handle_subscription(store: Arc<BlobStore>, key: String) -> Response {
    let rx = store.subscribe();

    // Get initial state if exists
    let initial_event = match store.get(&key).await {
        Ok(Some((data, meta))) => {
            // Ideally check parents against meta.version to decide if we send snapshot
            // For now, always send snapshot if found (simplest for MVP)
            Some(StoreEvent::Put { meta, data })
        }
        _ => None,
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
    let updates_stream = stream.filter(move |event| {
        let matches = match event {
            StoreEvent::Put { meta, .. } => meta.key == key_filter,
            StoreEvent::Delete { key, .. } => key == &key_filter,
        };
        async move { matches }
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
        let bytes = format_update(event);
        Ok::<Bytes, axum::Error>(bytes)
    });

    Response::builder()
        .status(209)
        .header("subscribe", "true")
        .header("merge-type", "aww")
        // Cache-Control: no-cache?
        .body(Body::from_stream(body_stream))
        .unwrap()
}

fn format_update(event: StoreEvent) -> Bytes {
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

    if let Some(ct) = &meta.content_type {
        write!(&mut header, "Content-Type: {}\r\n", ct).unwrap();
    }

    if data.is_none() {
        // DELETE implies 404 status in the update headers?
        // Reference implementation doesn't seem to verify specific Delete header but sends 404 status.
        // We add it as a header in the update block
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

async fn handle_standard_get(
    store: Arc<BlobStore>,
    key: String,
    _braid_state: BraidState,
) -> Response {
    match store.get(&key).await {
        Ok(Some((data, meta))) => {
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
            eprintln!("Error getting key {}: {}", key, e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn handle_put(
    State(store): State<Arc<BlobStore>>,
    Path(key): Path<String>,
    Extension(braid_state): Extension<Arc<BraidState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
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
            eprintln!("Error putting key {}: {}", key, e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn handle_delete(State(store): State<Arc<BlobStore>>, Path(key): Path<String>) -> Response {
    match store.delete(&key).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => {
            eprintln!("Error deleting key {}: {}", key, e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
