//! Allocating item keys inside one write.

use std::num::NonZeroU32;

use rusqlite::Transaction;
use truapi::latest::GenericError;

use super::{
	dao,
	keys::{ItemIndex, ItemKey, ItemKeyDeriver, ItemSlot},
	purse::PurseId,
};
use crate::{installation::InstallationId, runtime::AuthorityError, store::DbError};

/// Why items could not be allocated.
#[derive(Debug, thiserror::Error)]
pub enum AllocateItemsError {
	/// No purse has this id.
	#[error("purse {0:?} does not exist")]
	UnknownPurse(PurseId),
	/// The purse's indices under this installation would pass `u32::MAX`.
	#[error("purse {purse:?} has {next} items under this installation, cannot add {count}")]
	IndicesExhausted {
		/// The purse.
		purse: PurseId,
		/// Its next free index.
		next: u32,
		/// How many items were asked for.
		count: u32,
	},
	/// The current installation could not be obtained.
	#[error("current installation unavailable: {}", .0.reason)]
	Installation(GenericError),
	/// Deriving a key failed, for example without a session.
	#[error(transparent)]
	Derive(#[from] AuthorityError),
	/// The deriver answered a different number of keys than it was asked for.
	#[error("derived {derived} keys for {slots} slots")]
	DerivedKeyCount {
		/// How many slots it was asked for.
		slots: usize,
		/// How many keys it gave.
		derived: usize,
	},
	/// The database failed.
	#[error(transparent)]
	Db(#[from] DbError),
}

/// A page of a purse's items, already checked against its bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemPage {
	/// Zero-based page index.
	pub index: u32,
	/// Items per page.
	pub size: NonZeroU32,
}

/// One page of items, and whether a later page holds more.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemsPage {
	/// The items, ordered by slot.
	pub items: Vec<ItemKey>,
	/// Whether a later page holds more.
	pub has_next: bool,
}

impl ItemsPage {
	/// The page of `size` items in `overread`, which holds one item more
	/// when a later page follows.
	pub fn from_overread(mut overread: Vec<ItemKey>, size: usize) -> Self {
		let has_next = overread.len() > size;
		overread.truncate(size);
		Self { items: overread, has_next }
	}
}

/// `count` new items of `purse` under `installation`.
pub struct Allocation {
	/// The purse they go into.
	pub purse: PurseId,
	/// The installation they are derived under.
	pub installation: InstallationId,
	/// How many.
	pub count: NonZeroU32,
}

impl Allocation {
	/// Its slots, from index `next` on.
	fn slots_from(&self, next: u32) -> Result<Vec<ItemSlot>, AllocateItemsError> {
		let count = self.count.get();
		let exhausted = AllocateItemsError::IndicesExhausted { purse: self.purse, next, count };
		let end = next.checked_add(count).ok_or(exhausted)?;
		let slot = |index| ItemSlot {
			purse: self.purse,
			installation: self.installation,
			index: ItemIndex(index),
		};
		Ok((next..end).map(slot).collect())
	}
}

/// Derives and records `allocation` in `tx`. The outer error is the
/// database's; the inner one says why nothing was allocated.
pub fn allocate_items_in(
	tx: &Transaction<'_>,
	allocation: Allocation,
	keys: &dyn ItemKeyDeriver,
) -> Result<Result<Vec<ItemKey>, AllocateItemsError>, DbError> {
	let slots = match free_slots(tx, &allocation)? {
		Ok(slots) => slots,
		Err(refused) => return Ok(Err(refused)),
	};
	let items = match derive_at(keys, slots) {
		Ok(items) => items,
		Err(refused) => return Ok(Err(refused)),
	};
	dao::insert_items(tx, &items)?;
	Ok(Ok(items.into_iter().map(|(_, key)| key).collect()))
}

/// Each of `slots` with the key derived at it.
fn derive_at(
	keys: &dyn ItemKeyDeriver,
	slots: Vec<ItemSlot>,
) -> Result<Vec<(ItemSlot, ItemKey)>, AllocateItemsError> {
	let derived = keys.derive_item_keys(&slots)?;
	if derived.len() != slots.len() {
		return Err(AllocateItemsError::DerivedKeyCount {
			slots: slots.len(),
			derived: derived.len(),
		});
	}
	Ok(slots.into_iter().zip(derived).collect())
}

/// The next free slots for `allocation`, or why there are none.
fn free_slots(
	tx: &Transaction<'_>,
	allocation: &Allocation,
) -> Result<Result<Vec<ItemSlot>, AllocateItemsError>, DbError> {
	if dao::purse(tx, allocation.purse)?.is_none() {
		return Ok(Err(AllocateItemsError::UnknownPurse(allocation.purse)));
	}
	let next = dao::next_item_index(tx, allocation.purse, &allocation.installation)?;
	Ok(allocation.slots_from(next))
}
