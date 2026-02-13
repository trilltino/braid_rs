//! Core data types for the Braid-HTTP protocol.

mod content_range;
mod patch;
mod request;
mod response;
mod update;
mod version;

pub use bytes::Bytes;
pub use content_range::ContentRange;
pub use patch::Patch;
pub use request::BraidRequest;
pub use response::BraidResponse;
pub use update::Update;
pub use version::Version;
