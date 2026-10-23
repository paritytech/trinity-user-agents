//! Building extrinsics for the chains the core talks to.
//!
//! A caller names the call and the extensions it cares about. The builder
//! fills every other extension of the runtime's pipeline, computes each
//! extension's explicit value from the bytes of the extensions after it, and
//! signs. [`ExtrinsicService`] reads the chain and the host for each build;
//! submitting the result is the caller's job.

mod builder;
mod error;
mod extension;
mod service;
mod signature;
mod version;
mod version_map;
mod well_known;

#[cfg(test)]
mod tests;

pub use builder::ExtrinsicBuilder;
pub use error::ExtrinsicBuildError;
pub use extension::{
	ExtensionError, InheritedImplication, PreencodedValue, RawExtension, TransactionExtension,
};
pub use service::{ExtrinsicService, ExtrinsicServiceDeps};
pub use version::ExtrinsicVersion;
