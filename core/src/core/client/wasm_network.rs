use crate::core::error::{BraidError, Result};
use crate::core::traits::BraidNetwork;
use crate::core::types::{BraidRequest, BraidResponse, Update};
use async_trait::async_trait;

pub struct WasmNetwork;

#[async_trait]
impl BraidNetwork for WasmNetwork {
    async fn fetch(&self, _url: &str, _request: BraidRequest) -> Result<BraidResponse> {
        // In a real implementation, this would use gloo-net or web-sys
        Err(BraidError::Internal(
            "WasmNetwork::fetch not implemented yet".to_string(),
        ))
    }

    async fn subscribe(
        &self,
        _url: &str,
        _request: BraidRequest,
    ) -> Result<async_channel::Receiver<Result<Update>>> {
        Err(BraidError::Internal(
            "WasmNetwork::subscribe not implemented yet".to_string(),
        ))
    }
}
