//! Reads, verifies and signs Trinity stack descriptors, as
//! [RFC: Trinity stack descriptor](../../../docs/rfcs/trinity-stack-descriptor.md) defines them.
//!
//! The crate has no I/O: callers fetch, store and keep time, so the same rules run in the host
//! core, in tools and, through the `js` feature, in `@parity/trinity-stack`.

mod descriptor;
mod envelope;
mod error;
mod format;
mod ijson;
#[cfg(feature = "js")]
mod js;
mod pointer;
#[cfg(feature = "qr")]
mod qr;
mod reader;
mod spec;

pub use descriptor::{
	Asset, Block, Chain, Coinage, Descriptor, Fork, ForkSource, MAX_SEQUENCE, SERVICE_KINDS,
	Service, SpecFile, VERSION,
};
pub use envelope::{Document, Verified, open, public_key, sign};
pub use error::Error;
pub use pointer::{DEFAULT_LINK_BASE, Pointer};
#[cfg(feature = "qr")]
pub use qr::{qr_svg, qr_terminal};
pub use reader::{Accepted, Fetched, Status, accept};
pub use spec::light_spec_genesis;
