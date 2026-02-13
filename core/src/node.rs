use crate::core::client::BraidClient;
use crate::core::types::BraidRequest;
use futures::StreamExt;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::sync::Arc;
use tokio::sync::Mutex;

#[napi]
pub struct BraidNode {
    client: Arc<BraidClient>,
}

#[napi]
impl BraidNode {
    #[napi(constructor)]
    pub fn new() -> Result<Self> {
        let client = BraidClient::new().map_err(|e| Error::from_reason(e.to_string()))?;
        Ok(Self {
            client: Arc::new(client),
        })
    }

    #[napi]
    pub async fn subscribe(&self, url: String, callback: JsFunction) -> Result<()> {
        let client = self.client.clone();
        let mut sub = client
            .subscribe(&url, BraidRequest::new().subscribe())
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?;

        let callback = Arc::new(Mutex::new(callback));

        tokio::spawn(async move {
            while let Some(update) = sub.next().await {
                if let Ok(update) = update {
                    if let Ok(json) = serde_json::to_string(&update) {
                        let cb = callback.lock().await;
                        let _ = cb.call(None, &[json]);
                    }
                }
            }
        });

        Ok(())
    }
}
