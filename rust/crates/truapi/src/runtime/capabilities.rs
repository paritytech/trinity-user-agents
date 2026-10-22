//! Capability implementations for the product runtime.

mod account;
mod call_media;
mod chain;
mod chat;
mod expanded_card;
mod game;
mod nft_purse;
mod payment;
mod person_chat;
mod platform;
mod preimage;
mod resources;
mod scanner;
mod signing;

use crate::{CallError, versioned::FromLatest};

/// Convert a latest domain error to the caller's version. Upstream capabilities
/// run inside `versioning::version(request).exec(..)`; the chat, person-chat,
/// call-media and usernames capabilities still convert per return path.
fn from_latest_error<E: FromLatest>(error: CallError<E::Latest>, version: u8) -> CallError<E> {
	error.map_domain(|domain| E::from_latest(domain, version))
}
