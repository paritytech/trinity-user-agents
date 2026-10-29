#![doc = include_str!("../README.md")]

mod canonical;
mod descriptor;
mod dotns;
mod envelope;
mod error;
mod format;
mod ijson;
mod pointer;
mod publish;
mod reader;
mod spec;
mod stack_reader;
#[cfg(feature = "js")]
mod wasm;

pub use canonical::{canonical, content_hash};
pub use descriptor::{
	Asset, Block, Chain, Coinage, Descriptor, Fork, ForkSource, SERVICE_KINDS, Service, SpecFile,
	VERSION,
};
pub use dotns::{
	DOTNS_KEYS, DotnsAnswer, DotnsCall, DotnsDrift, DotnsLookup, DotnsStep, dotns_drift,
};
pub use envelope::{Document, Verified, open, public_key, sign};
pub use error::Error;
pub use pointer::{DEFAULT_LINK_BASE, Pointer};
pub use publish::{PublishState, Rendered, render};
pub use reader::{Accepted, Fetched, Status, accept};
pub use spec::light_spec_genesis;
pub use stack_reader::{
	CurrentStack, FetchRequest, Refresh, Refreshed, Served, StackRead, StackReader,
};
