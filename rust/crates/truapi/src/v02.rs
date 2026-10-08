//! TrUAPI Protocol v0.2 type definitions.
//!
//! Only the messages whose shape changed after v0.1 live here. A message the
//! new version does not redefine keeps its [`crate::v01`] type in the versioned
//! envelope, so this module stays a delta rather than a copy of the protocol.

mod local_storage;

pub use local_storage::*;
