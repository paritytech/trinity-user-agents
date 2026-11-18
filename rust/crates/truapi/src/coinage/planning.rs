//! E1 · Transaction planning: what an operation is asked to do, as an
//! ordered set of transactions, each with its call, its inputs, the outputs
//! it creates and where, the token it presents, and the records it depends
//! on.
//!
//! Planning is deterministic (invariant 5): outputs are listed in a fixed
//! order, over-cap unload groups are cut at a fixed size, and tokens are
//! assigned to unload transactions lowest slot first.

use std::collections::BTreeMap;

use parity_scale_codec::{Decode, Encode};

use super::{
	Asset, ChainParams, CoinSecret, Exponent, PurseId, Snapshot,
	denominations::denominations,
	ledger::{AssetKind, Read, Subject, TokenId},
	selection::Selection,
};
use crate::{durable::DurableTxId, installation::InstallationId};

/// Where an operation delivers its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub enum Destination {
	/// Fresh accounts of this purse.
	Purse(PurseId),
	/// Fresh accounts of the payer's purse, whose secrets are handed out.
	Export(PurseId),
	/// An account of the external asset.
	External([u8; 32]),
}

/// What an operation is asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanInput {
	/// Spend a selection: B3's, or inputs a sweep or an offload phase chose.
	Spend {
		/// The inputs and what they deliver.
		selection: Selection,
		/// Where the value goes.
		to: Destination,
	},
	/// Claim peer coins, each named by the account its secret controls.
	Import {
		/// The coins' secrets.
		coins: Vec<CoinSecret>,
		/// The purse that receives them.
		into: PurseId,
	},
	/// Move marked coins of this layer back into fresh accounts.
	Reclaim {
		/// The coins.
		coins: Vec<Asset>,
		/// The purse that receives them.
		into: PurseId,
	},
	/// Load entries from an external account: a top-up.
	Load {
		/// The account that pays.
		from: [u8; 32],
		/// How many cents.
		amount: u64,
		/// The purse that receives the entries.
		into: PurseId,
	},
	/// Recycle coins into entries: a recycling sweep or an offload phase.
	Recycle {
		/// The coins.
		coins: Vec<Asset>,
		/// The purse that receives the entries.
		into: PurseId,
	},
	/// Register the current installation on chain.
	RegisterInstallation(InstallationId),
}

/// The call a planned transaction makes (section 1, the chain).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub enum Call {
	/// Split a coin.
	Split,
	/// Transfer a coin.
	Transfer,
	/// Recycle a coin into an entry.
	Recycle,
	/// Unload entries into coins.
	UnloadToCoins,
	/// Unload entries into the external asset, at this account.
	UnloadToExternal([u8; 32]),
	/// Load entries from the external asset, paid by this account.
	Load([u8; 32]),
	/// Append the installation's record to the registry.
	AppendRegistryRecord(InstallationId),
}

/// One output of a planned transaction: an asset the caller binds to a fresh
/// index (E3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Output {
	/// A coin or an entry.
	pub kind: AssetKind,
	/// The purse it goes to.
	pub purse: PurseId,
	/// Its denomination.
	pub exponent: Exponent,
	/// It carries the delivered value rather than change.
	pub delivered: bool,
}

/// One planned transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Planned {
	/// Its call.
	pub call: Call,
	/// What it consumes.
	pub inputs: Vec<Asset>,
	/// What it creates.
	pub outputs: Vec<Output>,
	/// The unload token it presents.
	pub token: Option<TokenId>,
	/// The records that mint one of its inputs.
	pub depends_on: Vec<DurableTxId>,
}

/// An operation's transactions, and for an export the coins handed out as
/// they are, with no transaction (G1 invariant 5).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
	/// The transactions, in order.
	pub transactions: Vec<Planned>,
	/// The coins an export hands out unchanged.
	pub handed_out: Vec<Asset>,
}

/// How many unload transactions [`plan`] produces for `input`: the tokens
/// the operation needs.
pub fn tokens_needed(input: &PlanInput, view: &Snapshot, params: &ChainParams) -> u32 {
	match input {
		PlanInput::Spend { selection, .. } => unload_groups(&selection.inputs, view, params).len() as u32,
		_ => 0,
	}
}

/// The transactions that carry out `input`, presenting `tokens` in order on
/// its unload transactions.
pub fn plan(input: &PlanInput, tokens: &[TokenId], view: &Snapshot, params: &ChainParams) -> Plan {
	match input {
		PlanInput::Spend { selection, to } => spend(selection, *to, tokens, view, params),
		PlanInput::Import { coins, into } => {
			let transactions = coins
				.iter()
				.filter_map(|secret| {
					let coin = Asset::PeerCoin(account_of(secret)?);
					let exponent = observed_exponent(view, &coin).or(Exponent::new(0))?;
					Some(transfer(coin, *into, exponent, view))
				})
				.collect();
			Plan { transactions, handed_out: Vec::new() }
		},
		PlanInput::Reclaim { coins, into } => {
			let transactions =
				coins.iter().filter_map(|coin| Some(transfer(*coin, *into, view.asset(coin)?.exponent, view))).collect();
			Plan { transactions, handed_out: Vec::new() }
		},
		PlanInput::Load { from, amount, into } => {
			let entries = denominations(*amount, params);
			let cap = params.max_load_entries().max(1) as usize;
			let transactions = entries
				.chunks(cap)
				.map(|chunk| Planned {
					call: Call::Load(*from),
					inputs: Vec::new(),
					outputs: chunk.iter().map(|exponent| output(AssetKind::Entry, *into, *exponent, true)).collect(),
					token: None,
					depends_on: Vec::new(),
				})
				.collect();
			Plan { transactions, handed_out: Vec::new() }
		},
		PlanInput::Recycle { coins, into } => {
			let transactions = coins
				.iter()
				.filter_map(|coin| {
					let exponent = view.asset(coin)?.exponent;
					Some(Planned {
						call: Call::Recycle,
						inputs: vec![*coin],
						outputs: vec![output(AssetKind::Entry, *into, exponent, true)],
						token: None,
						depends_on: minters(&[*coin], view),
					})
				})
				.collect();
			Plan { transactions, handed_out: Vec::new() }
		},
		PlanInput::RegisterInstallation(installation) => Plan {
			transactions: vec![Planned {
				call: Call::AppendRegistryRecord(*installation),
				inputs: Vec::new(),
				outputs: Vec::new(),
				token: None,
				depends_on: Vec::new(),
			}],
			handed_out: Vec::new(),
		},
	}
}

/// A spend's transactions: its splits, transfers of coins already in shape
/// and one unload per unload group (invariants 1 to 4).
fn spend(selection: &Selection, to: Destination, tokens: &[TokenId], view: &Snapshot, params: &ChainParams) -> Plan {
	let source = selection.purse;
	let recycle_move =
		matches!(to, Destination::Purse(purse) if purse != source) && view.policy().move_through_recycler;
	let target = match to {
		Destination::Purse(purse) | Destination::Export(purse) => purse,
		Destination::External(_) => source,
	};
	let split_coins: Vec<Asset> = selection.splits.iter().map(|(coin, _)| *coin).collect();
	let whole: Vec<Asset> = selection
		.inputs
		.iter()
		.filter(|asset| matches!(asset, Asset::Coin { .. }) && !split_coins.contains(asset))
		.copied()
		.collect();
	let entries: Vec<Asset> =
		selection.inputs.iter().filter(|asset| matches!(asset, Asset::Entry { .. })).copied().collect();

	let mut wanted: BTreeMap<Exponent, usize> = BTreeMap::new();
	for exponent in &selection.outputs {
		*wanted.entry(*exponent).or_default() += 1;
	}
	for coin in &whole {
		if let Some(record) = view.asset(coin) &&
			let Some(count) = wanted.get_mut(&record.exponent)
		{
			*count = count.saturating_sub(1);
		}
	}

	let mut plan = Plan::default();
	for (coin, produce) in &selection.splits {
		let outputs = produce
			.iter()
			.map(|exponent| {
				let delivered = wanted.get_mut(exponent).is_some_and(|count| {
					let take = *count > 0;
					*count = count.saturating_sub(usize::from(take));
					take
				});
				let purse = if delivered { target } else { source };
				output(AssetKind::Coin, purse, *exponent, delivered)
			})
			.collect();
		plan.transactions.push(Planned {
			call: Call::Split,
			inputs: vec![*coin],
			outputs,
			token: None,
			depends_on: minters(&[*coin], view),
		});
	}
	for coin in &whole {
		let Some(exponent) = view.asset(coin).map(|record| record.exponent) else { continue };
		match to {
			Destination::Export(_) => plan.handed_out.push(*coin),
			Destination::Purse(_) if recycle_move => plan.transactions.push(Planned {
				call: Call::Recycle,
				inputs: vec![*coin],
				outputs: vec![output(AssetKind::Entry, target, exponent, true)],
				token: None,
				depends_on: minters(&[*coin], view),
			}),
			Destination::Purse(_) | Destination::External(_) => plan.transactions.push(transfer(*coin, target, exponent, view)),
		}
	}

	let total_delivered: u64 = selection.outputs.iter().map(|exponent| exponent.value()).sum();
	let whole_delivered: u64 =
		whole.iter().filter_map(|coin| view.asset(coin)).map(|record| record.exponent.value()).sum();
	let split_delivered: u64 = plan
		.transactions
		.iter()
		.filter(|planned| planned.call == Call::Split)
		.flat_map(|planned| &planned.outputs)
		.filter(|output| output.delivered)
		.map(|output| output.exponent.value())
		.sum();
	let mut owed = total_delivered.saturating_sub(whole_delivered + split_delivered);
	let mut tokens = tokens.iter();
	for group in unload_groups(&entries, view, params) {
		let value: u64 = group.iter().filter_map(|entry| view.asset(entry)).map(|record| record.exponent.value()).sum();
		let delivered = value.min(owed);
		owed -= delivered;
		let change = value - delivered;
		let (call, delivered_kind) = match to {
			Destination::External(account) => (Call::UnloadToExternal(account), None),
			_ => (Call::UnloadToCoins, Some(AssetKind::Coin)),
		};
		let change_kind = if delivered_kind.is_none() { AssetKind::Entry } else { AssetKind::Coin };
		let mut outputs: Vec<Output> = match delivered_kind {
			Some(kind) => denominations(delivered, params).into_iter().map(|e| output(kind, target, e, true)).collect(),
			None => Vec::new(),
		};
		outputs.extend(denominations(change, params).into_iter().map(|e| output(change_kind, source, e, false)));
		plan.transactions.push(Planned {
			call,
			inputs: group,
			outputs,
			token: tokens.next().copied(),
			depends_on: Vec::new(),
		});
	}
	plan
}

/// The unload groups of `entries`: one ring at one revision, cut at the
/// unload cap, in the order of the entries' ring and then identity.
fn unload_groups(entries: &[Asset], view: &Snapshot, params: &ChainParams) -> Vec<Vec<Asset>> {
	let mut rings: BTreeMap<RingKey, Vec<Asset>> = BTreeMap::new();
	for entry in entries.iter().filter(|asset| matches!(asset, Asset::Entry { .. })) {
		let Some(record) = view.asset(entry) else { continue };
		let key = RingKey { exponent: record.exponent, ring: ring_of(view, entry) };
		rings.entry(key).or_default().push(*entry);
	}
	let cap = params.max_unload_entries().max(1) as usize;
	rings
		.into_values()
		.flat_map(|mut members| {
			members.sort();
			members.chunks(cap).map(<[Asset]>::to_vec).collect::<Vec<_>>()
		})
		.collect()
}

/// One ring at one revision: entries of one exponent whose proofs verify
/// together.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RingKey {
	exponent: Exponent,
	ring: Option<(u32, u32)>,
}

fn ring_of(view: &Snapshot, asset: &Asset) -> Option<(u32, u32)> {
	match view.observation(&Subject::Asset(*asset))?.value.as_ref()? {
		Read::Entry(Some(entry)) => Some((entry.ring, entry.revision)),
		_ => None,
	}
}

fn observed_exponent(view: &Snapshot, coin: &Asset) -> Option<Exponent> {
	match view.observation(&Subject::Asset(*coin))?.value.as_ref()? {
		Read::Coin { now: Some(coin), .. } => Some(coin.exponent),
		_ => None,
	}
}

fn transfer(coin: Asset, into: PurseId, exponent: Exponent, view: &Snapshot) -> Planned {
	Planned {
		call: Call::Transfer,
		inputs: vec![coin],
		outputs: vec![output(AssetKind::Coin, into, exponent, true)],
		token: None,
		depends_on: minters(&[coin], view),
	}
}

fn output(kind: AssetKind, purse: PurseId, exponent: Exponent, delivered: bool) -> Output {
	Output { kind, purse, exponent, delivered }
}

/// The records still minting one of `inputs` (C1 invariant 5).
fn minters(inputs: &[Asset], view: &Snapshot) -> Vec<DurableTxId> {
	inputs
		.iter()
		.filter_map(|input| view.registrations().creator(input))
		.filter(|facts| !matches!(facts.state.status, crate::durable::DurableTxStatus::FinalizedSuccess { .. }))
		.map(|facts| facts.record)
		.collect()
}

/// The account a coin secret controls; none for a secret that is not a
/// canonical sr25519 key, which G1 refuses before planning.
pub fn account_of(secret: &CoinSecret) -> Option<[u8; 32]> {
	schnorrkel::SecretKey::from_bytes(&secret.0).ok().map(|key| key.to_public().to_bytes())
}

#[cfg(test)]
mod tests;
