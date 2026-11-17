//! B2's balance: a purse's holdings as a three-way partition, computed from
//! one snapshot and never waiting on a chain read.
//!
//! Every holding is in exactly one bucket. *Held back* is value the user
//! could have now by choosing a different policy; *arriving* is value no
//! choice makes available sooner. An entry is *spendable* only while this
//! period's free tokens can unload it, at one token per unload transaction.

use std::collections::BTreeMap;

use super::{
	Asset, ChainParams, Exponent, PurseId, Snapshot,
	classification::{Selectability, classify},
	ledger::{Read, Subject},
	tokens::supply,
};

/// A purse's holdings, partitioned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[derive(uniffi::Record)]
pub struct Balance {
	/// Selectable now, under the effective policy.
	pub spendable: u64,
	/// Unavailable on purpose, to gain privacy.
	pub held_back: u64,
	/// Unavailable by no one's choice.
	pub arriving: u64,
}

/// The balance of `purse` at `now`.
///
/// The period's free tokens go to unload groups in one fixed order: the main
/// purse first, then purses by identifier; within a purse, the groups of
/// larger value first. A group is one ring at one revision, at most the
/// per-unload cap of entries, and needs one token.
pub fn balance(view: &Snapshot, purse: PurseId, now: u64, params: &ChainParams) -> Balance {
	let mut tokens = supply(now, view, params).free().count();
	let mut order: Vec<PurseId> = view.purses().map(|record| record.id).filter(|id| *id != PurseId::MAIN).collect();
	order.sort();
	order.insert(0, PurseId::MAIN);
	for current in order {
		let (balance, used) = purse_balance(view, current, now, params, tokens);
		if current == purse {
			return balance;
		}
		tokens -= used;
	}
	purse_balance(view, purse, now, params, tokens).0
}

/// The balance of one purse given `tokens` free tokens, and how many it uses.
fn purse_balance(view: &Snapshot, purse: PurseId, now: u64, params: &ChainParams, tokens: usize) -> (Balance, usize) {
	let mut balance = Balance::default();
	let mut groups: BTreeMap<(Exponent, u32, u32), Vec<u64>> = BTreeMap::new();
	for record in view.assets(purse) {
		let value = record.exponent.value();
		match classify(&record.asset, view, now, params).selectability {
			Selectability::Never => {},
			Selectability::HeldBack(_) => balance.held_back += value,
			Selectability::Arriving(_) => balance.arriving += value,
			Selectability::Now => match (record.asset, ring_of(view, &record.asset)) {
				(Asset::Entry { .. }, Some((ring, revision))) => {
					groups.entry((record.exponent, ring, revision)).or_default().push(value)
				},
				_ => balance.spendable += value,
			},
		}
	}
	let cap = params.max_unload_entries().max(1) as usize;
	let mut chunks: Vec<u64> =
		groups.values().flat_map(|values| values.chunks(cap).map(|chunk| chunk.iter().sum())).collect();
	chunks.sort_by(|a, b| b.cmp(a));
	let used = chunks.len().min(tokens);
	balance.spendable += chunks[..used].iter().sum::<u64>();
	balance.arriving += chunks[used..].iter().sum::<u64>();
	(balance, used)
}

/// The ring and revision an entry was last observed in.
fn ring_of(view: &Snapshot, asset: &Asset) -> Option<(u32, u32)> {
	match view.observation(&Subject::Asset(*asset))?.value.as_ref()? {
		Read::Entry(Some(entry)) => Some((entry.ring, entry.revision)),
		_ => None,
	}
}

#[cfg(test)]
mod tests;
