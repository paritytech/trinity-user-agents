//! B1 · Asset classification: one coin's or entry's state, computed from one
//! snapshot, the current time and the chain parameters.
//!
//! No asset has a stored status. Its state is computed from facts that each
//! have one owner: the records that create and consume it (the durability
//! layer, with C1's rows), its disclosure mark and observation (B2), and the
//! effective policy (A4). Selectability is recomputed at every read, because
//! four of an entry's conditions change with the clock alone (invariant 3).


use super::{
	Asset, ChainParams, PurseId, RecycleAge, Snapshot,
	ledger::{CoinOnChain, EntryOnChain, Observation, Observed, Read, Subject},
	operation::{OpKey, OpKind},
	policy::{PrivacyPolicy, effective},
	registration::RecordFacts,
	tokens,
};
use crate::durable::DurableTxStatus;

/// A coin's class: the first line that applies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoinClass {
	/// Its secret was handed out.
	Exported,
	/// Its creating transaction failed; the account was never funded.
	Void,
	/// Its consumer succeeded finally.
	Spent,
	/// It is an input of a transaction, of this operation, that has not
	/// failed.
	InUse(OpKey),
	/// Its creating transaction has neither failed nor succeeded finally.
	Minting,
	/// Present on chain, and nothing consumes it.
	Available,
}

/// An entry's place in its ring, as observed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingAxis {
	/// The key is in no ring yet.
	Missing,
	/// The ring has fewer members than any readiness rule needs, or the
	/// rule's dwell has not passed.
	Waiting,
	/// A readiness rule holds.
	Ready,
	/// The ring closed with this many members, below every rule's count.
	Degraded(u32),
	/// The ring was cleaned up while the entry was unconsumed.
	Expired,
}

/// An entry's local axis, computed like a coin's class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryLocal {
	/// Its creating transaction has neither failed nor succeeded finally.
	Minting,
	/// Nothing consumes it.
	Available,
	/// It is an input of a transaction, of this operation, that has not
	/// failed.
	InUse(OpKey),
	/// Its consumer succeeded finally.
	Consumed,
	/// Its creating transaction failed.
	Void,
}

/// A coin's class, or an entry's two axes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Class {
	/// A coin.
	Coin(CoinClass),
	/// A recycler entry.
	Entry {
		/// Its ring axis.
		ring: RingAxis,
		/// Its local axis.
		local: EntryLocal,
	},
}

/// Why an asset is unavailable on purpose, to gain privacy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeldBack {
	/// Some rule's member count is reached, but no rule's dwell has passed.
	Dwell,
	/// No rule's member count is reached.
	RingBelowThreshold,
	/// The recycling sweep takes it.
	RecycleDue,
}

/// Why an asset is unavailable, by no one's choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arriving {
	/// The chain has never reported on it.
	NeverObserved,
	/// It is still being minted, or its key is in no ring yet.
	Minting,
	/// A chain lock has not elapsed.
	Locked,
	/// Its key is not yet included in its ring's root.
	NotIncluded,
	/// Its ring is within the rescue margin of expiry.
	RescueMargin,
	/// The coin is at or past the age cutoff.
	AgeCutoff,
}

/// Whether, and why not, an asset may be selected now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selectability {
	/// Selectable now.
	Now,
	/// Unavailable on purpose.
	HeldBack(HeldBack),
	/// Unavailable by no one's choice.
	Arriving(Arriving),
	/// In no bucket: in use, spent, void, exported, expired or consumed.
	Never,
}

/// What became of an exported coin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[derive(uniffi::Enum)]
pub enum ExportStatus {
	/// The transaction creating it has not succeeded finally.
	NotYetMinted,
	/// It is on chain, unclaimed.
	AwaitingPayee,
	/// The payee moved it, by this transaction.
	Claimed {
		/// The move is stable.
		stable: bool,
		/// The transaction that moved it.
		by: [u8; 32],
	},
	/// This layer's reclaim moved it back.
	Reclaimed {
		/// The reclaim is stable.
		stable: bool,
	},
	/// Its creating transaction failed, or a reclaim cancelled it; the
	/// secret controls nothing.
	NeverMinted,
}

/// One asset's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetState {
	/// Its class or axes.
	pub class: Class,
	/// Whether the chain has reported on it.
	pub observed: Observed,
	/// Whether, and why not, it may be selected now.
	pub selectability: Selectability,
	/// For an exported coin, what became of it.
	pub export: Option<ExportStatus>,
}

/// One asset's state at `now`. `classify` is never asked about a peer's
/// coin, and answers `Never` for one.
pub fn classify(asset: &Asset, view: &Snapshot, now: u64, params: &ChainParams) -> AssetState {
	let policy = effective(view.policy(), params);
	let observation = view.observation(&Subject::Asset(*asset));
	let observed = observation.map_or(Observed::Never, |observation| observation.observed);
	let state = |class, selectability, export| AssetState { class, observed, selectability, export };
	match asset {
		Asset::PeerCoin(_) => state(Class::Coin(CoinClass::Available), Selectability::Never, None),
		Asset::Coin { purse, .. } => {
			let class = coin_class(asset, view);
			let export = (class == CoinClass::Exported).then(|| export_status(asset, view, observation));
			let selectability = coin_selectability(&class, observation, now, params, || {
				recycle_due(view, *purse, now, params).contains(asset)
			});
			state(Class::Coin(class), selectability, export)
		},
		Asset::Entry { .. } => {
			let (ring, local, selectability) = entry_state(asset, view, observation, &policy, now, params);
			state(Class::Entry { ring, local }, selectability, None)
		},
	}
}

/// The coins of `purse` the recycling sweep takes now, in the order it takes
/// them: none while the token supply runs low; otherwise the available coins
/// old enough to recycle, oldest first, admitted while the purse's
/// unavailable value is below the policy's share of its total.
pub fn recycle_due(view: &Snapshot, purse: PurseId, now: u64, params: &ChainParams) -> Vec<Asset> {
	if tokens::running_low(&tokens::supply(now, view, params)) {
		return Vec::new();
	}
	let policy = effective(view.policy(), params);
	let RecycleAge::Age(recycle_at) = policy.recycle_at else { return Vec::new() };
	let mut total = 0u128;
	let mut unavailable = 0u128;
	let mut candidates = Vec::new();
	for record in view.assets(purse) {
		let observation = view.observation(&Subject::Asset(record.asset));
		let value = u128::from(record.exponent.value());
		match record.asset {
			Asset::Coin { .. } => match coin_class(&record.asset, view) {
				CoinClass::Exported | CoinClass::Void | CoinClass::Spent | CoinClass::InUse(_) => {},
				CoinClass::Minting => {
					total += value;
					unavailable += value;
				},
				CoinClass::Available => {
					total += value;
					if let Some(coin) = coin_on_chain(observation) &&
						coin.age >= recycle_at
					{
						candidates.push((coin.age, record));
					}
				},
			},
			Asset::Entry { .. } => {
				let (_, _, selectability) = entry_state(&record.asset, view, observation, &policy, now, params);
				if selectability != Selectability::Never {
					total += value;
				}
				if !matches!(selectability, Selectability::Never | Selectability::Now) {
					unavailable += value;
				}
			},
			Asset::PeerCoin(_) => {},
		}
	}
	candidates.sort_by(|(age, record), (other_age, other)| {
		other_age.cmp(age).then_with(|| record.asset.cmp(&other.asset))
	});
	let cap = total * u128::from(policy.max_unavailable_share);
	let mut due = Vec::new();
	for (_, record) in candidates {
		if unavailable * 100 >= cap {
			break;
		}
		unavailable += u128::from(record.exponent.value());
		due.push(record.asset);
	}
	due
}

/// A coin's class, the first line that applies.
fn coin_class(asset: &Asset, view: &Snapshot) -> CoinClass {
	if view.mark(asset).is_some() {
		return CoinClass::Exported;
	}
	match local_axis(asset, view) {
		EntryLocal::Void => CoinClass::Void,
		EntryLocal::Consumed => CoinClass::Spent,
		EntryLocal::InUse(operation) => CoinClass::InUse(operation),
		EntryLocal::Minting => CoinClass::Minting,
		EntryLocal::Available => CoinClass::Available,
	}
}

/// The class every asset shares: from the record that creates it and the
/// records that consume it.
fn local_axis(asset: &Asset, view: &Snapshot) -> EntryLocal {
	let registrations = view.registrations();
	let creator = registrations.creator(asset);
	if creator.is_some_and(failed) {
		return EntryLocal::Void;
	}
	let mut consumers = registrations.consumers(asset).filter(|facts| !failed(facts));
	if let Some(holder) = consumers.next() {
		let mut all = core::iter::once(holder).chain(consumers);
		if all.any(|facts| matches!(facts.state.status, DurableTxStatus::FinalizedSuccess { .. })) {
			return EntryLocal::Consumed;
		}
		return EntryLocal::InUse(holder.operation.clone());
	}
	if creator.is_some_and(|facts| !matches!(facts.state.status, DurableTxStatus::FinalizedSuccess { .. })) {
		return EntryLocal::Minting;
	}
	EntryLocal::Available
}

fn failed(facts: &RecordFacts) -> bool {
	matches!(facts.state.status, DurableTxStatus::Failure(_))
}

fn coin_on_chain(observation: Option<&Observation>) -> Option<CoinOnChain> {
	match observation?.value.as_ref()? {
		Read::Coin { now, .. } => *now,
		_ => None,
	}
}

fn entry_on_chain(observation: Option<&Observation>) -> Option<EntryOnChain> {
	match observation?.value.as_ref()? {
		Read::Entry(entry) => *entry,
		_ => None,
	}
}

/// A coin's selectability: the first condition that fails, the arriving ones
/// before the held-back one.
fn coin_selectability(
	class: &CoinClass,
	observation: Option<&Observation>,
	now: u64,
	params: &ChainParams,
	recycle_due: impl FnOnce() -> bool,
) -> Selectability {
	if !matches!(class, CoinClass::Minting | CoinClass::Available) {
		return Selectability::Never;
	}
	if observation.is_none_or(|observation| observation.observed == Observed::Never) {
		return Selectability::Arriving(Arriving::NeverObserved);
	}
	if *class == CoinClass::Minting {
		return Selectability::Arriving(Arriving::Minting);
	}
	let Some(coin) = coin_on_chain(observation) else {
		return Selectability::Arriving(Arriving::NeverObserved);
	};
	if coin.locked_until.is_some_and(|until| until > now) {
		return Selectability::Arriving(Arriving::Locked);
	}
	if coin.age >= u32::from(params.age_cutoff()) {
		return Selectability::Arriving(Arriving::AgeCutoff);
	}
	if recycle_due() {
		return Selectability::HeldBack(HeldBack::RecycleDue);
	}
	Selectability::Now
}

/// An entry's ring axis, local axis and selectability.
fn entry_state(
	asset: &Asset,
	view: &Snapshot,
	observation: Option<&Observation>,
	policy: &PrivacyPolicy,
	now: u64,
	params: &ChainParams,
) -> (RingAxis, EntryLocal, Selectability) {
	let local = local_axis(asset, view);
	let entry = entry_on_chain(observation);
	let included_at = observation.and_then(|observation| observation.included_at);
	let readiness = entry.map(|entry| readiness(&entry, included_at, policy, now, params));
	let ring = match (entry, readiness) {
		(Some(entry), _) if entry.expires_at.is_some_and(|expiry| expiry <= now) => RingAxis::Expired,
		(None, _) if included_at.is_some() => RingAxis::Expired,
		(None, _) => RingAxis::Missing,
		(Some(_), Some(Readiness::Ready)) => RingAxis::Ready,
		(Some(entry), Some(Readiness::BelowEveryRule)) if entry.closed => RingAxis::Degraded(entry.members),
		(Some(_), _) => RingAxis::Waiting,
	};
	let never = matches!(local, EntryLocal::InUse(_) | EntryLocal::Consumed | EntryLocal::Void) ||
		ring == RingAxis::Expired;
	let selectability = if never {
		Selectability::Never
	} else if observation.is_none_or(|observation| observation.observed == Observed::Never) {
		Selectability::Arriving(Arriving::NeverObserved)
	} else if let (Some(entry), EntryLocal::Available) = (entry, &local) {
		let page_size = params.config().ring_page_size;
		let rescue_from = |expiry: u64| expiry.saturating_sub(params.config().rescue_margin.as_secs());
		if entry.locked_until.is_some_and(|until| until > now) {
			Selectability::Arriving(Arriving::Locked)
		} else if !entry.key_included(page_size) {
			Selectability::Arriving(Arriving::NotIncluded)
		} else if entry.expires_at.is_some_and(|expiry| rescue_from(expiry) <= now) {
			Selectability::Arriving(Arriving::RescueMargin)
		} else {
			match readiness {
				Some(Readiness::Ready) => Selectability::Now,
				Some(Readiness::DwellPending) => Selectability::HeldBack(HeldBack::Dwell),
				_ => Selectability::HeldBack(HeldBack::RingBelowThreshold),
			}
		}
	} else {
		Selectability::Arriving(Arriving::Minting)
	};
	(ring, local, selectability)
}

/// How far an entry is from a readiness rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Readiness {
	/// Some rule holds.
	Ready,
	/// Some rule's member count is reached, but no rule's dwell has passed.
	DwellPending,
	/// No rule's member count is reached.
	BelowEveryRule,
}

fn readiness(
	entry: &EntryOnChain,
	included_at: Option<u64>,
	policy: &PrivacyPolicy,
	now: u64,
	params: &ChainParams,
) -> Readiness {
	let mut counted = false;
	for rule in &policy.ready_when {
		if entry.included < rule.members.members(params.ring_size()) {
			continue;
		}
		counted = true;
		if included_at.is_some_and(|at| at + rule.dwell.as_secs() <= now) {
			return Readiness::Ready;
		}
	}
	if counted { Readiness::DwellPending } else { Readiness::BelowEveryRule }
}

/// What became of an exported coin: the first status that applies.
fn export_status(asset: &Asset, view: &Snapshot, observation: Option<&Observation>) -> ExportStatus {
	let registrations = view.registrations();
	let creator = registrations.creator(asset);
	if creator.is_some_and(failed) {
		return ExportStatus::NeverMinted;
	}
	if creator.is_some_and(|facts| !matches!(facts.state.status, DurableTxStatus::FinalizedSuccess { .. })) {
		return ExportStatus::NotYetMinted;
	}
	for consumer in registrations.consumers(asset) {
		let stable = match consumer.state.status {
			DurableTxStatus::FinalizedSuccess { .. } => true,
			DurableTxStatus::PendingSuccess { .. } => false,
			_ => continue,
		};
		match view.operation(&consumer.operation).map(|operation| operation.kind) {
			Some(OpKind::Reclaim) => return ExportStatus::Reclaimed { stable },
			_ => {
				let by = consumer.state.tx_hash.map_or([0; 32], |hash| hash.0);
				return ExportStatus::Claimed { stable, by };
			},
		}
	}
	match observation.and_then(|observation| observation.value.as_ref()) {
		Some(Read::Coin { now: None, emptied_by: Some(emptied) }) => ExportStatus::Claimed { stable: emptied.stable, by: emptied.by },
		_ => ExportStatus::AwaitingPayee,
	}
}

#[cfg(test)]
mod tests;
