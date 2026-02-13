//! Braid HTTP Protocol Implementation for Rust

#[cfg(feature = "client")]
pub mod client;
pub mod error;
pub mod merge;
pub mod protocol;
#[cfg(feature = "server")]
pub mod server;
pub mod traits;
pub mod types;

pub use error::{BraidError, Result};
pub use types::{BraidRequest, BraidResponse, Patch, Update, Version};

#[cfg(feature = "client")]
pub use client::{
    BraidClient, ClientConfig, HeartbeatConfig, Message, MessageParser, ParseState, RetryConfig,
    RetryDecision, RetryState, Subscription, SubscriptionStream,
};

#[cfg(feature = "server")]
pub use server::{BraidLayer, BraidState, ServerConfig};
