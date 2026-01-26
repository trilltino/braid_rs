use axum::{response::Response, routing::get, Router};
use braid_rs::{BraidClient, BraidLayer, BraidRequest, ServerConfig};
use tokio::net::TcpListener;

#[tokio::test]
async fn test_multiplexing() {
    // 1. Setup server
    let app = Router::new()
        .route(
            "/data",
            get(|| async {
                Response::builder()
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from("{\"hello\": \"world\"}"))
                    .unwrap()
            }),
        )
        .layer(
            BraidLayer::with_config(ServerConfig {
                enable_multiplex: true,
                ..Default::default()
            })
            .middleware(),
        );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // 2. Setup client
    let client = BraidClient::new();
    let url = format!("http://{}/data", addr);

    // 3. Make multiplexed request
    let response = client
        .fetch_multiplexed(&url, BraidRequest::new())
        .await
        .unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(response.body, "{\"hello\": \"world\"}");
}
