//! Optional TCP proxy bridge for legacy HTTP clients.
//!
//! Lets a browser or curl talk to the P2P Braid network through a local
//! TCP listener. Requests to `http://localhost:<port>/resource` get
//! forwarded over iroh to the target peer.
//!
//! Requires the `proxy` feature flag.

#[cfg(feature = "proxy")]
pub mod bridge {
    use iroh::{Endpoint, EndpointId};
    use std::net::SocketAddr;

    /// Start a local TCP listener that bridges HTTP/1.1 requests to an
    /// iroh peer. Not yet implemented — placeholder for Phase 4.
    pub async fn start_proxy(
        _endpoint: &Endpoint,
        _listen_addr: SocketAddr,
        _default_peer: EndpointId,
    ) -> anyhow::Result<()> {
        // TODO: Phase 4 — wire up iroh-proxy-utils DownstreamProxy
        tracing::info!("TCP proxy bridge not yet implemented");
        Ok(())
    }
}
