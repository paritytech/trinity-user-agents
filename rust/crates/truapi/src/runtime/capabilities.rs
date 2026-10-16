//! Capability implementations for the product runtime.

mod account;
mod chain;
mod expanded_card;
mod game;
mod payment;
mod platform;
mod preimage;
mod resources;
mod scanner;
mod signing;

use crate::{CallError, versioned::FromLatest};

fn from_latest_error<E: FromLatest>(error: CallError<E::Latest>, version: u8) -> CallError<E> {
	error.map_domain(|domain| E::from_latest(domain, version))
}
