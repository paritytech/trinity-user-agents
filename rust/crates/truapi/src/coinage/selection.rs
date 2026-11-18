//! B3 · Selection: which assets an operation consumes to produce an amount.
//!
//! Selection reads classifications, never the policy (invariant 1). It
//! tries three strategies, first over the inputs usable now and then over all
//! inputs (invariant 5), in an order fixed by who asked for the spend
//! (invariant 2): spend coins that already sum to the amount, reshape coins
//! through A5's plan, or unload recycler entries. Every tie is broken by
//! exponent, then by the asset's own identity, so two implementations choose
//! the same assets.

use std::collections::BTreeMap;

use super::{
	Asset, ChainParams, Exponent, PurseId, Snapshot,
	classification::{Arriving, HeldBack, Selectability, classify},
	denominations::denominations,
	ledger::{Mark, Read, Subject},
	operation::OpKind,
	split::plan_splits,
	tokens::supply,
};
use crate::durable::{DurableTxId, DurableTxStatus};

/// How much to select.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Want {
	/// This many cents.
	Amount(u64),
	/// Every asset the purse holds that is not `Never`: a drain.
	Everything,
}

/// Which kinds of asset may be selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Forms {
	/// Coins and entries.
	Any,
	/// Entries only: an offload's offboarding phase.
	EntriesOnly,
}

/// What to select from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectRequest {
	/// The purse, already resolved.
	pub purse: PurseId,
	/// How much.
	pub want: Want,
	/// Which kinds.
	pub forms: Forms,
	/// Entries before coins: a spend layer 2 makes for a product.
	pub entries_first: bool,
	/// Held-back inputs count as usable now.
	pub held_back: bool,
}

/// The assets an operation consumes, and what it delivers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
	/// The purse selected from.
	pub purse: PurseId,
	/// The inputs, in the order chosen.
	pub inputs: Vec<Asset>,
	/// The coins to split, each with the exponents it splits into (A5).
	pub splits: Vec<(Asset, Vec<Exponent>)>,
	/// The denominations delivered to the destination.
	pub outputs: Vec<Exponent>,
	/// The records still minting an input.
	pub depends_on: Vec<DurableTxId>,
}

/// Why nothing was selected (invariant 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum SelectError {
	/// The purse's value, counting what it may take now or later, does not
	/// cover the request.
	#[display("insufficient funds: {available} cents available")]
	InsufficientFunds {
		/// What the purse may take now or later.
		available: u64,
	},
	/// Only entries cover the request, and the period's free tokens cannot
	/// unload them.
	#[display("no unload token is free")]
	NoUnloadToken,
}

/// One asset selection may take.
#[derive(Clone, Copy, Debug)]
struct Candidate {
	asset: Asset,
	exponent: Exponent,
	ring: Option<(u32, u32)>,
	minted_by: Option<DurableTxId>,
	gaining_privacy: bool,
}

/// Chooses inputs from `request.purse` for `request.want`.
pub fn select(
	view: &Snapshot,
	request: SelectRequest,
	now: u64,
	params: &ChainParams,
) -> Result<Selection, SelectError> {
	let (usable_now, usable_later) = candidates(view, request, now, params);
	let all: Vec<Candidate> = usable_now.iter().chain(&usable_later).copied().collect();
	let amount = match request.want {
		Want::Everything => return Ok(everything(request.purse, &all, params)),
		Want::Amount(amount) => amount,
	};
	let available: u64 = all.iter().map(|candidate| candidate.exponent.value()).sum();
	if available < amount {
		return Err(SelectError::InsufficientFunds { available });
	}
	let tokens = supply(now, view, params).free().count();
	let mut short_of_tokens = false;
	for pool in [usable_now, all] {
		match strategies(&pool, amount, request, tokens, params) {
			Outcome::Selected(selection) => return Ok(Selection { purse: request.purse, ..selection }),
			Outcome::ShortOfTokens => short_of_tokens = true,
			Outcome::Uncovered => {},
		}
	}
	if short_of_tokens {
		Err(SelectError::NoUnloadToken)
	} else {
		Err(SelectError::InsufficientFunds { available })
	}
}

/// The purse's assets selection may take, usable now and usable later, each
/// in selection's order: entries still gaining privacy after the others, then
/// largest exponent first, then by asset.
fn candidates(view: &Snapshot, request: SelectRequest, now: u64, params: &ChainParams) -> (Vec<Candidate>, Vec<Candidate>) {
	let mut usable_now = Vec::new();
	let mut usable_later = Vec::new();
	for record in view.assets(request.purse) {
		let is_entry = matches!(record.asset, Asset::Entry { .. });
		if request.forms == Forms::EntriesOnly && !is_entry {
			continue;
		}
		let state = classify(&record.asset, view, now, params);
		let minting = view
			.registrations()
			.creator(&record.asset)
			.filter(|facts| !matches!(facts.state.status, DurableTxStatus::FinalizedSuccess { .. }));
		let candidate = Candidate {
			asset: record.asset,
			exponent: record.exponent,
			ring: ring_of(view, &record.asset),
			minted_by: minting.map(|facts| facts.record),
			gaining_privacy: is_entry && matches!(state.selectability, Selectability::HeldBack(_)),
		};
		match (state.selectability, is_entry) {
			(Selectability::Now, _) => usable_now.push(candidate),
			(Selectability::HeldBack(_), _) if request.held_back => usable_now.push(candidate),
			(Selectability::HeldBack(HeldBack::RecycleDue), false) => {},
			(Selectability::Arriving(Arriving::Locked), false) => usable_later.push(candidate),
			(Selectability::Arriving(Arriving::Minting), false) if mintable(view, &candidate) => {
				usable_later.push(candidate)
			},
			(Selectability::HeldBack(HeldBack::Dwell), true) => usable_later.push(candidate),
			_ => {},
		}
	}
	let order = |a: &Candidate, b: &Candidate| {
		a.gaining_privacy
			.cmp(&b.gaining_privacy)
			.then_with(|| b.exponent.cmp(&a.exponent))
			.then_with(|| a.asset.cmp(&b.asset))
	};
	usable_now.sort_by(order);
	usable_later.sort_by(order);
	(usable_now, usable_later)
}

/// Whether a coin still being minted may be taken as a dependency: never one
/// an export mints before its handoff is committed (C2 invariant 5).
fn mintable(view: &Snapshot, candidate: &Candidate) -> bool {
	let Some(record) = candidate.minted_by.and_then(|record| view.registrations().record(record)) else {
		return false;
	};
	let exporting = view.operation(&record.operation).is_some_and(|operation| operation.kind == OpKind::Export);
	let uncommitted = record.rows.outputs.iter().any(|output| view.mark(output) == Some(Mark::Uncommitted));
	!(exporting && uncommitted)
}

fn ring_of(view: &Snapshot, asset: &Asset) -> Option<(u32, u32)> {
	match view.observation(&Subject::Asset(*asset))?.value.as_ref()? {
		Read::Entry(Some(entry)) => Some((entry.ring, entry.revision)),
		_ => None,
	}
}

/// A drain: every candidate, coins transferred as they are and entries
/// unloaded into the fewest coins of their value.
fn everything(purse: PurseId, all: &[Candidate], params: &ChainParams) -> Selection {
	let coins = all.iter().filter(|candidate| matches!(candidate.asset, Asset::Coin { .. }));
	let entry_value: u64 = all
		.iter()
		.filter(|candidate| matches!(candidate.asset, Asset::Entry { .. }))
		.map(|candidate| candidate.exponent.value())
		.sum();
	let outputs = coins.map(|candidate| candidate.exponent).chain(denominations(entry_value, params)).collect();
	Selection {
		purse,
		inputs: all.iter().map(|candidate| candidate.asset).collect(),
		splits: Vec::new(),
		outputs,
		depends_on: dependencies(all),
	}
}

enum Outcome {
	Selected(Selection),
	ShortOfTokens,
	Uncovered,
}

/// The three strategies over one pool, in the order the request fixes, and
/// both coins and entries when neither alone covers the amount.
fn strategies(pool: &[Candidate], amount: u64, request: SelectRequest, tokens: usize, params: &ChainParams) -> Outcome {
	let coins: Vec<Candidate> =
		pool.iter().filter(|candidate| matches!(candidate.asset, Asset::Coin { .. })).copied().collect();
	let entries: Vec<Candidate> =
		pool.iter().filter(|candidate| matches!(candidate.asset, Asset::Entry { .. })).copied().collect();
	let mut short_of_tokens = false;
	let mut entry_strategy = || match unload(&entries, amount, tokens, params) {
		Ok(selection) => Some(selection),
		Err(short) => {
			short_of_tokens |= short;
			None
		},
	};
	let found = if request.entries_first {
		entry_strategy().or_else(|| exact(&coins, amount)).or_else(|| split(&coins, amount, params))
	} else {
		exact(&coins, amount).or_else(|| split(&coins, amount, params)).or_else(entry_strategy)
	};
	if let Some(selection) = found {
		return Outcome::Selected(selection);
	}
	match both(&coins, &entries, amount, tokens, params) {
		Ok(selection) => return Outcome::Selected(selection),
		Err(short) => short_of_tokens |= short,
	}
	if short_of_tokens { Outcome::ShortOfTokens } else { Outcome::Uncovered }
}

/// Coins that already sum to the amount: largest first, as long as the coin
/// fits what is left.
fn exact(coins: &[Candidate], amount: u64) -> Option<Selection> {
	let mut left = amount;
	let mut chosen = Vec::new();
	for coin in coins {
		if coin.exponent.value() <= left {
			left -= coin.exponent.value();
			chosen.push(*coin);
		}
	}
	(left == 0 && amount > 0).then(|| Selection {
		purse: PurseId::MAIN,
		inputs: chosen.iter().map(|coin| coin.asset).collect(),
		splits: Vec::new(),
		outputs: chosen.iter().map(|coin| coin.exponent).collect(),
		depends_on: dependencies(&chosen),
	})
}

/// Coins reshaped by A5's plan, applied exactly as returned (invariant 3):
/// the plan names exponents, and selection consumes the first coin of each in
/// its own order; then the coins and split outputs pay the amount, largest
/// first, split outputs before whole coins of the same exponent.
fn split(coins: &[Candidate], amount: u64, params: &ChainParams) -> Option<Selection> {
	let total: u64 = coins.iter().map(|coin| coin.exponent.value()).sum();
	if total < amount || amount == 0 {
		return None;
	}
	let mut counts: BTreeMap<Exponent, u32> = BTreeMap::new();
	for coin in coins {
		*counts.entry(coin.exponent).or_default() += 1;
	}
	let plan = plan_splits(amount, &counts.into_iter().collect::<Vec<_>>(), params)?;
	let mut whole: Vec<Candidate> = coins.to_vec();
	let mut splits = Vec::new();
	for step in &plan {
		let position = whole.iter().position(|coin| coin.exponent == step.consume)?;
		splits.push((whole.remove(position), step.produce.clone()));
	}
	let mut pieces: Vec<(Exponent, Option<Candidate>)> = splits
		.iter()
		.flat_map(|(_, produce)| produce.iter().map(|exponent| (*exponent, None)))
		.chain(whole.iter().map(|coin| (coin.exponent, Some(*coin))))
		.collect();
	pieces.sort_by(|(a, a_coin), (b, b_coin)| b.cmp(a).then_with(|| a_coin.is_some().cmp(&b_coin.is_some())));
	let mut left = amount;
	let mut outputs = Vec::new();
	let mut paying = Vec::new();
	for (exponent, coin) in pieces {
		if exponent.value() <= left {
			left -= exponent.value();
			outputs.push(exponent);
			paying.extend(coin);
		}
	}
	if left != 0 {
		return None;
	}
	let split_coins: Vec<Candidate> = splits.iter().map(|(coin, _)| *coin).collect();
	let inputs: Vec<Candidate> = split_coins.iter().chain(&paying).copied().collect();
	Some(Selection {
		purse: PurseId::MAIN,
		inputs: inputs.iter().map(|coin| coin.asset).collect(),
		splits: splits.into_iter().map(|(coin, produce)| (coin.asset, produce)).collect(),
		outputs,
		depends_on: dependencies(&inputs),
	})
}

/// Entries unloaded into the fewest coins of the amount: the largest first
/// until they cover it. Err(true) when they cover it but the free tokens do
/// not reach their unload transactions.
fn unload(entries: &[Candidate], amount: u64, tokens: usize, params: &ChainParams) -> Result<Selection, bool> {
	let mut sum = 0;
	let mut chosen = Vec::new();
	for entry in entries {
		if sum >= amount {
			break;
		}
		sum += entry.exponent.value();
		chosen.push(*entry);
	}
	if sum < amount || amount == 0 {
		return Err(false);
	}
	if unload_transactions(&chosen, params) > tokens {
		return Err(true);
	}
	Ok(Selection {
		purse: PurseId::MAIN,
		inputs: chosen.iter().map(|entry| entry.asset).collect(),
		splits: Vec::new(),
		outputs: denominations(amount, params),
		depends_on: Vec::new(),
	})
}

/// Every coin, which together fall short of the amount, spent whole, and the
/// entries [`unload`] chooses for the rest. Err(true) when the free tokens do
/// not reach those entries' unload transactions.
fn both(coins: &[Candidate], entries: &[Candidate], amount: u64, tokens: usize, params: &ChainParams) -> Result<Selection, bool> {
	let coin_value: u64 = coins.iter().map(|coin| coin.exponent.value()).sum();
	if coins.is_empty() || coin_value >= amount {
		return Err(false);
	}
	let unloaded = unload(entries, amount - coin_value, tokens, params)?;
	Ok(Selection {
		purse: PurseId::MAIN,
		inputs: coins.iter().map(|coin| coin.asset).chain(unloaded.inputs).collect(),
		splits: Vec::new(),
		outputs: coins.iter().map(|coin| coin.exponent).chain(unloaded.outputs).collect(),
		depends_on: dependencies(coins),
	})
}

/// How many unload transactions the entries need: one per ring revision and
/// per unload cap (E1 invariant 4).
fn unload_transactions(entries: &[Candidate], params: &ChainParams) -> usize {
	let cap = params.max_unload_entries().max(1) as usize;
	let mut groups: BTreeMap<(Exponent, Option<(u32, u32)>), usize> = BTreeMap::new();
	for entry in entries {
		*groups.entry((entry.exponent, entry.ring)).or_default() += 1;
	}
	groups.values().map(|count| count.div_ceil(cap)).sum()
}

fn dependencies(candidates: &[Candidate]) -> Vec<DurableTxId> {
	let mut records: Vec<DurableTxId> = candidates.iter().filter_map(|candidate| candidate.minted_by).collect();
	records.sort();
	records.dedup();
	records
}

#[cfg(test)]
mod tests;
