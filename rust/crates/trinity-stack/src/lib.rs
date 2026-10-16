#![doc = include_str!("../README.md")]

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
	Asset, Block, Chain, Coinage, Descriptor, Fork, ForkSource, SERVICE_KINDS, Service, SpecFile,
	VERSION,
};
pub use envelope::{Document, Verified, open, public_key, sign};
pub use error::Error;
pub use pointer::{DEFAULT_LINK_BASE, Pointer};
#[cfg(feature = "qr")]
pub use qr::{qr_svg, qr_terminal};
pub use reader::{Accepted, Fetched, Status, accept};
pub use spec::light_spec_genesis;
