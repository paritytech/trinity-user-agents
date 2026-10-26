//! Handing items off: their secrets leave the host, so their keys are spent
//! for good. Layer 1 only records the handoff under an opaque tag; what the
//! secrets are sealed into is the caller's business.

use std::collections::BTreeSet;

use rusqlite::Connection;

use super::{
	dao,
	keys::{ItemKey, ItemSlot},
	purse::PurseId,
	standing::{ItemRefusal, idle_items},
};
use crate::{runtime::AuthorityError, store::DbError};

/// Opaque bytes naming one handoff, shared by every key handed off together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandoffTag(pub Vec<u8>);

/// Keys of one purse to hand off together under one tag.
#[derive(Clone)]
pub struct HandoffRequest {
	/// The purse every key is handed off from.
	pub from: PurseId,
	/// The keys, in the order their secrets are given to the seal.
	pub keys: BTreeSet<ItemKey>,
	/// Names the handoff. No other handoff may use it.
	pub tag: HandoffTag,
}

/// Why a handoff did not happen. Nothing was written.
#[derive(Debug, thiserror::Error)]
pub enum HandoffError<E> {
	/// The handoff names no key.
	#[error("a handoff needs at least one item, got none")]
	NoItems,
	/// A handoff was already recorded under the same tag.
	#[error("a handoff with tag {} is already recorded", hex::encode(&.0.0))]
	TagInUse(HandoffTag),
	/// A key is not in the purse, holds no NFT, or is already spent.
	#[error(transparent)]
	Item(#[from] ItemRefusal),
	/// The item secrets could not be exported.
	#[error("item secrets are unavailable: {0}")]
	Secrets(AuthorityError),
	/// The caller's seal refused the secrets.
	#[error("sealing the item secrets failed: {0}")]
	Seal(E),
	/// The database failed.
	#[error(transparent)]
	Db(#[from] DbError),
}

/// The slot of each key of `request`, in order, when the tag is unused and
/// every key is idle in its purse.
pub fn idle_handoff_slots<E>(
	conn: &Connection,
	request: &HandoffRequest,
) -> Result<Vec<ItemSlot>, HandoffError<E>> {
	if dao::handoff_tag_in_use(conn, &request.tag)? {
		return Err(HandoffError::TagInUse(request.tag.clone()));
	}
	let items = idle_items(conn, request.from, &request.keys)??;
	Ok(items.into_iter().map(|item| item.slot).collect())
}

/// Records every key of `request` as handed off, after checking again that the
/// tag is unused and every key is idle.
pub fn record_handoff<E>(
	tx: &rusqlite::Transaction<'_>,
	request: &HandoffRequest,
) -> Result<(), HandoffError<E>> {
	idle_handoff_slots::<E>(tx, request)?;
	let keys: Vec<ItemKey> = request.keys.iter().copied().collect();
	Ok(dao::insert_handoffs(tx, &keys, &request.tag)?)
}

#[cfg(test)]
mod tests;
