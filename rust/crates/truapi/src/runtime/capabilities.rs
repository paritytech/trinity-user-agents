//! Capability implementations for the product runtime.

mod account;
mod call_media;
mod chain;
mod chat;
mod expanded_card;
mod game;
mod payment;
mod person_chat;
mod platform;
mod preimage;
mod resources;
mod scanner;
mod signing;
mod usernames;

use crate::{CallError, versioned::FromLatest};

fn from_latest_error<E: FromLatest>(error: CallError<E::Latest>, version: u8) -> CallError<E> {
	error.map_domain(|domain| E::from_latest(domain, version))
}
