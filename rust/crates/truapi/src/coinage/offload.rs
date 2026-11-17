//! F3 · Offload phases: moving an amount out of a purse into the external
//! asset, planned one phase at a time.
//!
//! Each phase registers its transactions under the offload's own key, and
//! the next phase is planned only once the previous one is stable, so an
//! offload has no dependencies between its own transactions (invariant 2).
//! The phase is read from the offload's records, so a restart resumes where
//! they stand (invariant 4).

use parity_scale_codec::Decode;

use super::{
	Asset, ChainParams, HeldBackSpend, Initiator, PrivacyPolicy, PurseId, Snapshot,
	classification::{Arriving, HeldBack, Selectability, classify},
	ledger::{self, Read, Subject},
	operation::{self, OperationEnd, OpKey, Retry},
	planning::{Call, Destination, PlanInput},
	policy::effective,
	selection::{Forms, SelectRequest, Selection, Want, select},
	start::{Launcher, Request, StartError, start},
};
use crate::{
	durable::{DurableTxFailureKind, DurableTxStatus},
	store::{Db, DbError},
};

/// What one offload was asked to do, stored as its operation's arguments
/// with its retry policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
pub struct Offload {
	/// The purse it offloads from.
	pub from: PurseId,
	/// How many cents.
	pub amount: u64,
	/// The external asset's account that receives them.
	pub to: [u8; 32],
	/// Who asked.
	pub initiator: Initiator,
	/// The user confirmed spending held-back value.
	pub confirmed: bool,
}

/// The next phase of an offload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
	/// Unload these entries into the destination.
	Offboard(Selection),
	/// Recycle these coins into entries of the purse.
	Recycle(Vec<Asset>),
	/// Wait: until this time, in Unix seconds, or, when none, until the next
	/// `run_due`.
	Wait(Option<u64>),
	/// The amount is offboarded.
	Done,
	/// Nothing in the purse covers the rest, or a phase's transaction failed
	/// after reaching the chain (invariant 4).
	Failed,
}

/// The value the offload's offboarding transactions have delivered, on a
/// stable verdict (B4 invariant 6): their inputs less their reloaded change.
pub fn offboarded(view: &Snapshot, key: &OpKey) -> u64 {
	view.registrations()
		.of_operation(key)
		.filter(|facts| matches!(facts.state.status, DurableTxStatus::FinalizedSuccess { .. }))
		.filter(|facts| {
			facts.state.plan.as_ref().and_then(|plan| super::start::RecordParams::from_params(&plan.params)).is_some_and(
				|params| matches!(params.call, Call::UnloadToExternal(_)),
			)
		})
		.map(|facts| {
			let value = |assets: &[Asset]| -> u64 {
				assets.iter().filter_map(|asset| view.asset(asset)).map(|record| record.exponent.value()).sum()
			};
			value(&facts.rows.inputs).saturating_sub(value(&facts.rows.outputs))
		})
		.sum()
}

/// The next phase, a pure function of the snapshot, the remaining amount and
/// `now`.
pub fn plan(view: &Snapshot, key: &OpKey, offload: &Offload, now: u64, params: &ChainParams) -> Phase {
	let remaining = offload.amount.saturating_sub(offboarded(view, key));
	if remaining == 0 {
		return Phase::Done;
	}
	if failed_on_chain(view, key) {
		return Phase::Failed;
	}
	let policy = effective(view.policy(), params);
	let held_back = match policy.held_back_for(offload.initiator) {
		HeldBackSpend::Always => true,
		HeldBackSpend::AfterConfirmation => offload.confirmed,
		HeldBackSpend::Never => false,
	};
	let mut usable = 0;
	let mut later = 0;
	let mut ripening_until: Option<u64> = None;
	let mut arriving = false;
	let mut usable_coins = Vec::new();
	for record in view.assets(offload.from) {
		let value = record.exponent.value();
		let selectability = classify(&record.asset, view, now, params).selectability;
		match (record.asset, selectability) {
			(Asset::Entry { .. }, Selectability::Now) => usable += value,
			(Asset::Entry { .. }, Selectability::HeldBack(_)) if held_back => usable += value,
			(Asset::Entry { .. }, Selectability::HeldBack(HeldBack::Dwell)) => {
				later += value;
				let ready = ready_at(view, &record.asset, &policy, params).unwrap_or(now + 60);
				ripening_until = Some(ripening_until.map_or(ready, |earliest| earliest.min(ready)));
			},
			(Asset::Entry { .. }, Selectability::Arriving(_)) => {
				later += value;
				arriving = true;
			},
			(Asset::Coin { .. }, Selectability::Now) => usable_coins.push((record.asset, value)),
			(Asset::Coin { .. }, Selectability::Arriving(Arriving::Minting | Arriving::Locked)) => arriving = true,
			_ => {},
		}
	}
	if usable >= remaining {
		let request = SelectRequest {
			purse: offload.from,
			want: Want::Amount(remaining),
			forms: Forms::EntriesOnly,
			entries_first: true,
			held_back,
		};
		if let Ok(selection) = select(view, request, now, params) {
			return Phase::Offboard(selection);
		}
	}
	let deficit = remaining.saturating_sub(usable + later);
	let coin_value: u64 = usable_coins.iter().map(|(_, value)| value).sum();
	if deficit > 0 && coin_value >= deficit {
		usable_coins.sort_by(|(a, a_value), (b, b_value)| b_value.cmp(a_value).then_with(|| a.cmp(b)));
		let mut taken = 0;
		let coins = usable_coins
			.into_iter()
			.take_while(|(_, value)| {
				let take = taken < deficit;
				taken += value;
				take
			})
			.map(|(coin, _)| coin)
			.collect();
		return Phase::Recycle(coins);
	}
	if deficit == 0 {
		if let Some(until) = ripening_until {
			return Phase::Wait(Some(until));
		}
		if arriving {
			return Phase::Wait(None);
		}
	}
	Phase::Failed
}

/// Whether a transaction of the offload failed after reaching the chain, so
/// a failure that repeats does not cost one token per Plan (invariant 4).
fn failed_on_chain(view: &Snapshot, key: &OpKey) -> bool {
	view.registrations()
		.of_operation(key)
		.any(|facts| matches!(facts.state.status, DurableTxStatus::Failure(DurableTxFailureKind::DispatchFailed)))
}

/// When a ripening entry becomes ready: the end of the shortest dwell among
/// the readiness rules whose member count its ring reaches, counted from its
/// key's inclusion.
fn ready_at(view: &Snapshot, entry: &Asset, policy: &PrivacyPolicy, params: &ChainParams) -> Option<u64> {
	let observation = view.observation(&Subject::Asset(*entry))?;
	let Some(Read::Entry(Some(place))) = &observation.value else { return None };
	let included_at = observation.included_at?;
	policy
		.ready_when
		.iter()
		.filter(|rule| place.included >= rule.members.members(params.ring_size()))
		.map(|rule| included_at + rule.dwell.as_secs())
		.min()
}

/// Runs the offload's next plan: starts its phase through E3, or stores its
/// wait or its end in B4. Nothing runs after a stored end.
pub async fn advance(
	launcher: &Launcher<'_>,
	store: &Db,
	key: &OpKey,
	offload: &Offload,
	retry: Retry,
	now: u64,
) -> Result<Phase, DbError> {
	let view = store.read(ledger::snapshot).await?;
	if view.operation(key).is_some_and(|operation| operation.end.is_some()) {
		return Ok(Phase::Done);
	}
	let phase = plan(&view, key, offload, now, launcher.params);
	let wait_key = key.clone();
	store.write(move |tx| operation::clear_offload_wait(tx, &wait_key)).await?;
	let input = match &phase {
		Phase::Offboard(selection) => {
			Some(PlanInput::Spend { selection: selection.clone(), to: Destination::External(offload.to) })
		},
		Phase::Recycle(coins) => Some(PlanInput::Recycle { coins: coins.clone(), into: offload.from }),
		Phase::Wait(until) => {
			let (key, until) = (key.clone(), *until);
			store.write(move |tx| operation::record_offload_wait(tx, &key, until)).await?;
			None
		},
		Phase::Done | Phase::Failed => {
			let end = if phase == Phase::Done { OperationEnd::Done } else { OperationEnd::Failed };
			let key = key.clone();
			store.write(move |tx| operation::record_end(tx, &key, end)).await?;
			None
		},
	};
	if let Some(input) = input &&
		let Err(StartError::Db(error)) = start(launcher, key, retry, Request::Given(input), now).await
	{
		return Err(error);
	}
	Ok(phase)
}

/// Runs the next plan of every offload that has not ended, whose records
/// are all stable, and whose stored wait has ended (F2's step 3). Returns
/// the offloads it advanced.
pub async fn resume_due(launcher: &Launcher<'_>, store: &Db, now: u64) -> Result<Vec<OpKey>, DbError> {
	let view = store.read(ledger::snapshot).await?;
	let mut advanced = Vec::new();
	for operation in view.operations() {
		if operation.kind != operation::OpKind::Offload || operation.end.is_some() {
			continue;
		}
		let waiting = matches!(operation.offload_wait, Some(Some(until)) if until > now);
		let in_flight = view.registrations().of_operation(&operation.key).any(|facts| {
			!matches!(facts.state.status, DurableTxStatus::FinalizedSuccess { .. } | DurableTxStatus::Failure(_))
		});
		let Ok((offload, retry)) = <(Offload, Retry)>::decode(&mut operation.arguments.as_slice()) else { continue };
		if waiting || in_flight {
			continue;
		}
		advance(launcher, store, &operation.key, &offload, retry, now).await?;
		advanced.push(operation.key.clone());
	}
	Ok(advanced)
}

#[cfg(test)]
mod tests;
