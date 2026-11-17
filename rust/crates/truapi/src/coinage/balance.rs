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
	ledger::{EntryOnChain, Read, Subject},
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

/// A purse's balance and the rows behind it, from one snapshot, so a screen
/// that shows both never shows them disagreeing (native interface,
/// section 6).
#[derive(Clone, Debug, PartialEq, Eq)]
#[derive(uniffi::Record)]
pub struct Holdings {
	/// The purse's balance.
	pub balance: Balance,
	/// One row per coin or entry the balance counts.
	pub rows: Vec<Holding>,
}

/// One coin or entry a balance counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[derive(uniffi::Record)]
pub struct Holding {
	/// The coin or entry, which identifies the row across changes.
	pub asset: Asset,
	/// Its denomination.
	pub exponent: Exponent,
	/// Selectable now; otherwise counted as held back or arriving.
	pub ready: bool,
	/// A coin's age as the chain last showed it; none for an entry, and for
	/// a coin not yet seen.
	pub age: Option<u32>,
	/// A coin this layer created by an unload that the chain marked as one
	/// of a batch, with an age of one.
	pub batch_unloaded: bool,
	/// How much of a ring's anonymity the row hides in, in percent: for an
	/// entry, its ring's; for a coin this layer unloaded, the ring it left.
	/// None when that ring, or its unloaded count, is unknown.
	pub fungibility: Option<u8>,
}

/// The holdings of `purse` at `now`.
pub fn holdings(view: &Snapshot, purse: PurseId, now: u64, params: &ChainParams) -> Holdings {
	let rows = view
		.assets(purse)
		.filter_map(|record| {
			let selectability = classify(&record.asset, view, now, params).selectability;
			if selectability == Selectability::Never {
				return None;
			}
			let observed = view.observation(&Subject::Asset(record.asset)).and_then(|observation| observation.value.as_ref());
			let age = match observed {
				Some(Read::Coin { now: Some(coin), .. }) => Some(coin.age),
				_ => None,
			};
			let source = unloaded_from(view, &record.asset);
			let ring = match (record.asset, observed) {
				(Asset::Entry { .. }, Some(Read::Entry(Some(entry)))) => Some(*entry),
				(Asset::Coin { .. }, _) => source,
				_ => None,
			};
			Some(Holding {
				asset: record.asset,
				exponent: record.exponent,
				ready: selectability == Selectability::Now,
				age,
				batch_unloaded: source.is_some() && age == Some(1),
				fungibility: ring.and_then(|ring| Some(fungibility(params.ring_size(), ring.included, ring.unloaded?))),
			})
		})
		.collect();
	Holdings { balance: balance(view, purse, now, params), rows }
}

/// The ring a coin left, as its unload's first input entry was last
/// observed in it, when the coin's creator is an unload.
fn unloaded_from(view: &Snapshot, coin: &Asset) -> Option<EntryOnChain> {
	let creator = view.registrations().creator(coin)?;
	let entry = creator.rows.inputs.iter().find(|input| matches!(input, Asset::Entry { .. }))?;
	match view.observation(&Subject::Asset(*entry))?.value.as_ref()? {
		Read::Entry(Some(entry)) => Some(*entry),
		_ => None,
	}
}

/// `round(100 · (I² − U²) / (I · L))`, as both shipped wallets compute it,
/// with `U` clamped to `I`; zero for an empty ring or a ring of no capacity.
fn fungibility(capacity: u32, included: u32, unloaded: u32) -> u8 {
	if included == 0 || capacity == 0 {
		return 0;
	}
	let (capacity, included, unloaded) = (u64::from(capacity), u64::from(included), u64::from(unloaded.min(included)));
	let numerator = 100 * (included * included - unloaded * unloaded);
	let denominator = included * capacity;
	let rounded = (2 * numerator + denominator) / (2 * denominator);
	u8::try_from(rounded.min(100)).unwrap_or(100)
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
