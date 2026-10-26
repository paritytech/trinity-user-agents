//! Whether items can be spent from a purse: they hold an NFT there and
//! nothing spends them.

use std::collections::BTreeSet;

use rusqlite::Connection;

use super::{
	dao::{self, KeyStanding},
	keys::{ItemKey, ItemSlot},
	purse::PurseId,
};
use crate::store::DbError;

/// An item that holds an NFT and that nothing spends.
pub struct IdleItem {
	/// The item's key.
	pub key: ItemKey,
	/// Where the key sits in the derivation tree.
	pub slot: ItemSlot,
	/// The instance it holds.
	pub instance: u64,
}

/// Why an item cannot be spent from a purse.
#[derive(Debug, thiserror::Error)]
pub enum ItemRefusal {
	/// The item is not in the purse, or holds no NFT.
	#[error("item {} is not held in the purse", hex::encode(.0.0))]
	NotFound(ItemKey),
	/// The item is handed off or spent by a live transaction.
	#[error("item {} is in use", hex::encode(.0.0))]
	Unavailable(ItemKey),
}

/// Each of `items` with its slot and instance, in order, when every one is
/// idle in `purse`, or the first one that is not. Reads every standing in one
/// query.
pub fn idle_items(
	conn: &Connection,
	purse: PurseId,
	items: &BTreeSet<ItemKey>,
) -> Result<Result<Vec<IdleItem>, ItemRefusal>, DbError> {
	let keys: Vec<ItemKey> = items.iter().copied().collect();
	let standings = dao::key_standings(conn, purse, &keys)?;
	let idle_item = |key: &ItemKey| match standings.get(key) {
		Some(KeyStanding::Idle { instance, slot }) => {
			Ok(IdleItem { key: *key, slot: *slot, instance: *instance })
		},
		Some(KeyStanding::Spent) => Err(ItemRefusal::Unavailable(*key)),
		None => Err(ItemRefusal::NotFound(*key)),
	};
	Ok(keys.iter().map(idle_item).collect())
}
