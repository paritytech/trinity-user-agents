//! TrUAPI Protocol v0.2 type definitions.
//!
//! The messages whose shape changed after v0.1 and the messages that v0.2 adds
//! live here. A message the new version does not redefine keeps its
//! [`crate::v01`] type in the versioned envelope, so this module stays a delta
//! rather than a copy of the protocol.

mod chat;
mod local_storage;

pub use chat::*;
pub use local_storage::*;
