use braid_blob::{braid_blob_service, BlobStore};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let db_path = PathBuf::from("braid-blobs");
    let meta_db_path = PathBuf::from("braid-blob-meta/meta.sqlite");

    let store = Arc::new(BlobStore::new(db_path, meta_db_path).await?);
    let app = braid_blob_service(store);

    let addr = "127.0.0.1:8880";
    println!("Listening on {}", addr);
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
