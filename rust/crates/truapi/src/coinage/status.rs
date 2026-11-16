//! B4 · Operation status and completion: where an operation stands and what
//! it settled, computed from one snapshot.
//!
//! Nothing here is stored. The status is read from the operation's records,
//! C1's rows, B2's observations and marks, and B4's own stored facts, by the
//! first row of B4's table that applies, and so is its value progress
//! (invariant 6). A completion exists once the status is terminal and names
//! only coin accounts, never a recycler entry.

use super::{
	Asset, ChainParams, Snapshot,
	classification::{ExportStatus, classify},
	ledger::{BlockRef, Mark, Read, Subject},
	namespace::coin_key,
	operation::{DeleteArguments, ImportArguments, OpKey, OpKind, OperationEnd, OperationRecord, ReclaimArguments},
	planning::Call,
	registration::RecordFacts,
	start::RecordParams,
};
use crate::durable::{DurableTxFailureKind, DurableTxStatus};

/// Where an operation stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
	/// A transaction waits to be built, and the inputs of one are usable.
	Preparing,
	/// Its transactions wait to be built, and no inputs are usable yet.
	Waiting,
	/// A transaction is submitted.
	Submitted,
	/// A transaction succeeded, not yet finally. Provisional (invariant 2).
	Settling,
	/// Every transaction succeeded finally.
	Done,
	/// Some succeeded finally and the rest failed.
	PartiallyDone,
	/// It ended without settling anything.
	Failed(Failure),
}

impl Status {
	/// Whether the status is the operation's last.
	pub fn is_terminal(self) -> bool {
		matches!(self, Self::Done | Self::PartiallyDone | Self::Failed(_))
	}
}

/// Why a started operation ended `Failed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
	/// Its cancellation was requested (invariant 1).
	Cancelled,
	/// An export released, or never committed before a restart.
	NotCommitted,
	/// The chain refused, and the retry policy allows no more.
	Refused,
	/// Attempts kept expiring until the retry policy gave up.
	Expired,
	/// An import or reclaim moved no coin.
	NothingMoved,
	/// A record was given up (C2 invariant 5), or nothing could cover the
	/// rest of an offload.
	GivenUp,
}

/// An operation's value progress, in cents.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
	/// Settled on a stable verdict.
	pub settled: u64,
	/// Still in flight.
	pub in_flight: u64,
}

/// One item of an operation's status stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusItem {
	/// Where it stands.
	pub status: Status,
	/// Its value progress.
	pub progress: Progress,
}

/// One transaction that succeeded finally.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settled {
	/// The hash of its successful attempt.
	pub hash: [u8; 32],
	/// The finalized block it was included in, when the proof named one.
	pub block: Option<BlockRef>,
	/// The coin accounts it consumed.
	pub consumed: Vec<[u8; 32]>,
	/// The coin accounts it created.
	pub created: Vec<[u8; 32]>,
}

/// What became of one coin of an import or a reclaim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoinResult {
	/// Claimed by the import, or taken back (or never minted) for a reclaim.
	Moved,
	/// Someone else moved it first.
	TakenByOther,
	/// Never seen on chain while the retry policy allowed.
	Absent,
	/// Present but unmoved when the operation ended.
	NotClaimed,
}

/// An ended operation's outcome, readable for as long as the store exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
	/// The operation.
	pub key: OpKey,
	/// Its terminal status.
	pub status: Status,
	/// Its final progress.
	pub progress: Progress,
	/// Its transactions that succeeded finally; for an offload, only its
	/// offboarding ones.
	pub transactions: Vec<Settled>,
	/// For an import or a reclaim, each coin's result.
	pub coins: Vec<([u8; 32], CoinResult)>,
}

/// The operation's current status and progress; none for an unknown key.
pub fn status(view: &Snapshot, key: &OpKey, now: u64, params: &ChainParams) -> Option<StatusItem> {
	let operation = view.operation(key)?;
	let records: Vec<&RecordFacts> = view.registrations().of_operation(key).collect();
	let item = match operation.kind {
		OpKind::Export => export_item(view, operation, &records),
		OpKind::Import => per_coin_item(view, &records, import_results(view, operation, &records)),
		OpKind::Reclaim => per_coin_item(view, &records, reclaim_results(view, key, &records, now, params)),
		OpKind::Offload => offload_item(view, operation, &records),
		OpKind::Reconstruct => StatusItem {
			status: if operation.end.is_some() { Status::Done } else { Status::Preparing },
			progress: Progress::default(),
		},
		OpKind::Delete => {
			let item = records_item(view, operation, &records);
			let closed = operation
				.arguments_as::<DeleteArguments>()
				.and_then(|arguments| view.purse(arguments.purse))
				.is_some_and(|purse| matches!(purse.state, super::ledger::PurseState::Closed { .. }));
			match (closed, records.is_empty()) {
				(true, true) => StatusItem { status: Status::Done, ..item },
				(false, _) if item.status.is_terminal() => StatusItem { status: Status::Waiting, ..item },
				_ => item,
			}
		},
		_ => records_item(view, operation, &records),
	};
	Some(item)
}

/// The operation's completion once its status is terminal, none before.
pub fn completion(view: &Snapshot, key: &OpKey, now: u64, params: &ChainParams, entropy: &[u8]) -> Option<Completion> {
	let item = status(view, key, now, params)?;
	if !item.status.is_terminal() {
		return None;
	}
	let operation = view.operation(key)?;
	let offboarding = |facts: &&RecordFacts| {
		operation.kind != OpKind::Offload ||
			facts.state.plan.as_ref().and_then(|plan| RecordParams::from_params(&plan.params)).is_some_and(
				|params| matches!(params.call, Call::UnloadToExternal(_)),
			)
	};
	let transactions = view
		.registrations()
		.of_operation(key)
		.filter(offboarding)
		.filter_map(|facts| {
			let DurableTxStatus::FinalizedSuccess { at } = facts.state.status else { return None };
			Some(Settled {
				hash: facts.state.tx_hash?.0,
				block: at.and_then(|at| Some(BlockRef { number: u32::try_from(at.number).ok()?, hash: at.hash.0 })),
				consumed: coin_accounts(&facts.rows.inputs, entropy),
				created: coin_accounts(&facts.rows.outputs, entropy),
			})
		})
		.collect();
	let records: Vec<&RecordFacts> = view.registrations().of_operation(key).collect();
	let results = match operation.kind {
		OpKind::Import => import_results(view, operation, &records),
		OpKind::Reclaim => reclaim_results(view, key, &records, now, params),
		_ => Vec::new(),
	};
	let coins = results.into_iter().filter_map(|(coin, result)| Some((coin_account(&coin, entropy)?, result?))).collect();
	Some(Completion { key: key.clone(), status: item.status, progress: item.progress, transactions, coins })
}

/// The operations whose status is not terminal.
pub fn open_operations(view: &Snapshot, now: u64, params: &ChainParams) -> Vec<OpKey> {
	view.operations()
		.filter(|operation| status(view, &operation.key, now, params).is_some_and(|item| !item.status.is_terminal()))
		.map(|operation| operation.key.clone())
		.collect()
}

fn stable(facts: &RecordFacts) -> bool {
	matches!(facts.state.status, DurableTxStatus::FinalizedSuccess { .. } | DurableTxStatus::Failure(_))
}

fn final_success(facts: &RecordFacts) -> bool {
	matches!(facts.state.status, DurableTxStatus::FinalizedSuccess { .. })
}

/// B4's table over the records alone.
fn records_item(view: &Snapshot, operation: &OperationRecord, records: &[&RecordFacts]) -> StatusItem {
	let progress = records_progress(view, records);
	let succeeded = records.iter().filter(|facts| final_success(facts)).count();
	let status = if records.is_empty() {
		Status::Waiting
	} else if records.iter().all(|facts| stable(facts)) {
		match succeeded {
			0 => Status::Failed(failure(operation, records)),
			all if all == records.len() => Status::Done,
			_ => Status::PartiallyDone,
		}
	} else {
		in_flight_status(view, records)
	};
	StatusItem { status, progress }
}

/// The status of records that are not all stable: the first of `Settling`,
/// `Submitted`, `Preparing` and `Waiting` that applies.
fn in_flight_status(view: &Snapshot, records: &[&RecordFacts]) -> Status {
	let any = |test: fn(&DurableTxStatus) -> bool| records.iter().any(|facts| test(&facts.state.status));
	if any(|status| matches!(status, DurableTxStatus::PendingSuccess { .. })) ||
		(any(|status| matches!(status, DurableTxStatus::FinalizedSuccess { .. })))
	{
		return Status::Settling;
	}
	if any(|status| *status == DurableTxStatus::Pending) {
		return Status::Submitted;
	}
	let usable = records.iter().any(|facts| {
		facts.state.status == DurableTxStatus::PendingSubmission &&
			facts.depends_on.iter().all(|record| view.registrations().record(*record).is_some_and(final_success))
	});
	if usable { Status::Preparing } else { Status::Waiting }
}

/// Why records that all failed failed: a cancellation, a record given up,
/// attempts that only expired, or otherwise the chain's refusal.
fn failure(operation: &OperationRecord, records: &[&RecordFacts]) -> Failure {
	let kinds: Vec<DurableTxFailureKind> = records
		.iter()
		.filter_map(|facts| match facts.state.status {
			DurableTxStatus::Failure(kind) => Some(kind),
			_ => None,
		})
		.collect();
	if operation.cancel_requested {
		Failure::Cancelled
	} else if operation.released {
		Failure::NotCommitted
	} else if kinds.contains(&DurableTxFailureKind::Abandoned) {
		Failure::GivenUp
	} else if kinds.iter().all(|kind| *kind == DurableTxFailureKind::Expired) {
		Failure::Expired
	} else {
		Failure::Refused
	}
}

/// The value a record moves: its inputs, or, for a load or an import whose
/// inputs this layer does not value, its outputs.
fn record_value(view: &Snapshot, facts: &RecordFacts) -> u64 {
	let value = |assets: &[Asset]| -> u64 {
		assets.iter().filter_map(|asset| view.asset(asset)).map(|record| record.exponent.value()).sum()
	};
	match value(&facts.rows.inputs) {
		0 => value(&facts.rows.outputs),
		inputs => inputs,
	}
}

fn records_progress(view: &Snapshot, records: &[&RecordFacts]) -> Progress {
	records.iter().fold(Progress::default(), |progress, facts| {
		let value = record_value(view, facts);
		if final_success(facts) {
			Progress { settled: progress.settled + value, ..progress }
		} else if stable(facts) {
			progress
		} else {
			Progress { in_flight: progress.in_flight + value, ..progress }
		}
	})
}

/// An export: `Preparing` until its commit, `Failed(NotCommitted)` once
/// released, then its records; the coins it hands out unchanged settle at
/// the commit.
fn export_item(view: &Snapshot, operation: &OperationRecord, records: &[&RecordFacts]) -> StatusItem {
	let unchanged: u64 = operation
		.exported
		.iter()
		.filter(|(coin, _)| view.registrations().creator(coin).is_none())
		.map(|(_, exponent)| exponent.value())
		.sum();
	if operation.released {
		let item = records_item(view, operation, records);
		return StatusItem { status: Status::Failed(Failure::NotCommitted), progress: Progress { settled: 0, ..item.progress } };
	}
	let committed = operation.exported.iter().all(|(coin, _)| view.mark(coin) == Some(Mark::Committed));
	if !committed {
		let item = records_item(view, operation, records);
		let progress = Progress { settled: 0, in_flight: item.progress.in_flight + unchanged };
		return StatusItem { status: Status::Preparing, progress };
	}
	if records.is_empty() {
		return StatusItem { status: Status::Done, progress: Progress { settled: unchanged, in_flight: 0 } };
	}
	let item = records_item(view, operation, records);
	let progress = Progress { settled: item.progress.settled + unchanged, ..item.progress };
	let status = match item.status {
		Status::Failed(_) if unchanged > 0 => Status::PartiallyDone,
		status => status,
	};
	StatusItem { status, progress }
}

/// An offload: `Waiting` between its phases until its stored end decides,
/// by the value offboarded; progress counts only that value.
fn offload_item(view: &Snapshot, operation: &OperationRecord, records: &[&RecordFacts]) -> StatusItem {
	let settled = super::offload::offboarded(view, &operation.key);
	let in_flight: u64 = records.iter().filter(|facts| !stable(facts)).map(|facts| record_value(view, facts)).sum();
	let progress = Progress { settled, in_flight };
	let status = match operation.end {
		Some(OperationEnd::Done) => Status::Done,
		Some(_) if settled > 0 => Status::PartiallyDone,
		Some(OperationEnd::Cancelled) => Status::Failed(Failure::Cancelled),
		Some(OperationEnd::Failed) => Status::Failed(Failure::GivenUp),
		None if records.iter().all(|facts| stable(facts)) => Status::Waiting,
		None => in_flight_status(view, records),
	};
	StatusItem { status, progress }
}

/// An import or a reclaim, judged per coin: ended once every coin has a
/// result.
fn per_coin_item(view: &Snapshot, records: &[&RecordFacts], results: Vec<(Asset, Option<CoinResult>)>) -> StatusItem {
	let progress = records_progress(view, records);
	if results.iter().any(|(_, result)| result.is_none()) {
		let submitted = records.iter().any(|facts| facts.state.status.awaits_verdict());
		return StatusItem { status: if submitted { Status::Submitted } else { Status::Waiting }, progress };
	}
	let moved = results.iter().filter(|(_, result)| *result == Some(CoinResult::Moved)).count();
	let status = match moved {
		0 => Status::Failed(Failure::NothingMoved),
		all if all == results.len() => Status::Done,
		_ => Status::PartiallyDone,
	};
	StatusItem { status, progress }
}

/// Each imported coin's result, none while it is undecided: moved once its
/// claim succeeded finally and its output is observed; taken by another
/// when its claim's dispatch failed; otherwise, once its claim ended, not
/// claimed if the chain shows it present and absent if not.
fn import_results(
	view: &Snapshot,
	operation: &OperationRecord,
	records: &[&RecordFacts],
) -> Vec<(Asset, Option<CoinResult>)> {
	let Some(arguments) = operation.arguments_as::<ImportArguments>() else { return Vec::new() };
	arguments
		.accounts
		.into_iter()
		.map(|account| {
			let coin = Asset::PeerCoin(account);
			let claim = records.iter().find(|facts| facts.rows.inputs.contains(&coin));
			let present = matches!(
				view.observation(&Subject::Asset(coin)).and_then(|observation| observation.value.as_ref()),
				Some(Read::Coin { now: Some(_), .. })
			);
			let result = match claim.map(|facts| (facts.state.status, facts)) {
				Some((DurableTxStatus::FinalizedSuccess { .. }, facts)) => outputs_observed(view, facts).then_some(CoinResult::Moved),
				Some((DurableTxStatus::Failure(DurableTxFailureKind::DispatchFailed), _)) => Some(CoinResult::TakenByOther),
				Some((DurableTxStatus::Failure(_), _)) | None if present => Some(CoinResult::NotClaimed),
				Some((DurableTxStatus::Failure(_), _)) | None => Some(CoinResult::Absent),
				Some(_) => None,
			};
			(coin, result)
		})
		.collect()
}

fn outputs_observed(view: &Snapshot, facts: &RecordFacts) -> bool {
	facts.rows.outputs.iter().all(|output| {
		view.observation(&Subject::Asset(*output))
			.is_some_and(|observation| matches!(observation.observed, super::ledger::Observed::At(_)))
	})
}

/// Each reclaimed coin's result, from its export status: moved once taken
/// back or never minted, taken by another once the payee's claim is stable,
/// not claimed when the reclaim's records ended with the coin still waiting
/// for its payee.
fn reclaim_results(
	view: &Snapshot,
	key: &OpKey,
	records: &[&RecordFacts],
	now: u64,
	params: &ChainParams,
) -> Vec<(Asset, Option<CoinResult>)> {
	let reclaim = view.operation(key).and_then(OperationRecord::arguments_as::<ReclaimArguments>);
	let Some(export) = reclaim.and_then(|arguments| view.operation(&arguments.export)) else { return Vec::new() };
	let ended = records.iter().all(|facts| stable(facts));
	export
		.exported
		.iter()
		.map(|(coin, _)| {
			let result = match classify(coin, view, now, params).export {
				Some(ExportStatus::Reclaimed { stable: true } | ExportStatus::NeverMinted) => Some(CoinResult::Moved),
				Some(ExportStatus::Claimed { stable: true, .. }) => Some(CoinResult::TakenByOther),
				_ if ended && !records.is_empty() => Some(CoinResult::NotClaimed),
				_ => None,
			};
			(*coin, result)
		})
		.collect()
}

/// The coin accounts among `assets`; recycler entries are never named.
fn coin_accounts(assets: &[Asset], entropy: &[u8]) -> Vec<[u8; 32]> {
	assets.iter().filter_map(|asset| coin_account(asset, entropy)).collect()
}

/// A coin's account: a peer's by the account it names, one of this layer's
/// by its derived key; none for a recycler entry.
fn coin_account(asset: &Asset, entropy: &[u8]) -> Option<[u8; 32]> {
	match asset {
		Asset::PeerCoin(account) => Some(*account),
		Asset::Coin { purse, installation, index } => {
			coin_key(entropy, *purse, *installation, *index).ok().map(|(_, account)| account)
		},
		Asset::Entry { .. } => None,
	}
}

#[cfg(test)]
mod tests;
