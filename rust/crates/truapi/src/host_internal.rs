//! Host-logic internals shared by the runtime, the host core, and the
//! bridges. Everything here is crate-internal: the module is private, so its
//! `pub` items stay out of the public API surface.

pub mod bulletin;
pub mod extrinsic;
pub mod permissions;
pub mod product_manifest;
pub mod sso_messages;
pub mod sso_wire;
pub mod transaction;
