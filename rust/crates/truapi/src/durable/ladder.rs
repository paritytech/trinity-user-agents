//! The completion ladder: decides one transaction against one pinned view of
//! its chain. The first rule that matches wins.

use subxt::utils::H256;

use super::{
	model::{
		DurableTxEntry, DurableTxStatus, HeadKind, Verdict, VerdictFailureKind, VerdictStatus,
	},
	oracle::PassScope,
};
use crate::chain::{DispatchOutcome, HashAndNumber, Heads};

/// One view of a chain, read once for a whole pass.
#[async_trait::async_trait]
pub trait PinnedView: Send + Sync {
	/// The heads the pass decides at.
	fn heads(&self) -> Heads;

	/// Looks for `tx_hash` in the canonical blocks `from..=to`, reading its
	/// dispatch outcome where it is found.
	async fn search(&self, from: u64, to: u64, tx_hash: H256) -> SearchResult;
}

/// Where a transaction was found, if anywhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchResult {
	/// In `block`, with its outcome, or `None` when the outcome could not be
	/// read: inclusion alone is not success.
	Found {
		/// The block that includes it.
		block: HashAndNumber,
		/// How it dispatched there.
		outcome: Option<DispatchOutcome>,
	},
	/// Not in any block read.
	NotFound {
		/// The last block of the unbroken read from the start of the range,
		/// `None` when its first block could not be read. Absence is proven
		/// only up to it.
		read_advanced_to: Option<u64>,
	},
}

/// What the ladder concluded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuleOutcome {
	/// A verdict, which may restate the current status.
	Decided(Verdict),
	/// A read the transaction depended on failed; it keeps its status.
	Undecided,
}

/// Whether the block a transaction's success was recorded at is still on the
/// canonical chain.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RecordedCanonicity {
	/// The canonical block at that height is the recorded one.
	Canonical,
	/// Another block, or none, is canonical at that height now.
	Reorged,
	/// The read failed, or the transaction has no recorded success.
	Unknown,
}

/// Decides `tx`. The pass resolves `recorded_canonical` for every recorded
/// transaction at once.
pub async fn evaluate_ladder(
	tx: &DurableTxEntry,
	scope: &dyn PassScope,
	view: &dyn PinnedView,
	recorded_canonical: RecordedCanonicity,
) -> RuleOutcome {
	let heads = view.heads();
	if let DurableTxStatus::PendingSuccess { at } = tx.status {
		return recorded_inclusion(tx, at, scope, &heads, recorded_canonical);
	}
	// One block past the era's death, as on Android, so a terminal verdict
	// never rests on the death block itself.
	let window_closed = heads.finalized.number > tx.window.death();
	if let Some(outcome) = oracle_rules(tx, scope, &heads, window_closed) {
		return outcome;
	}
	search_rule(tx, view, window_closed).await
}

/// Rules 1 to 4: what the domain's oracle proved, if it proved anything.
fn oracle_rules(
	tx: &DurableTxEntry,
	scope: &dyn PassScope,
	heads: &Heads,
	window_closed: bool,
) -> Option<RuleOutcome> {
	if scope.proven_completed(tx, HeadKind::Finalized) {
		return Some(decided(VerdictStatus::FinalizedSuccess { at: None }));
	}
	if scope.proven_completed(tx, HeadKind::Best) {
		return Some(decided(VerdictStatus::PendingSuccess { at: heads.best }));
	}
	if window_closed && scope.proven_not_completed(tx, HeadKind::Finalized) {
		return Some(decided(VerdictStatus::Failure(VerdictFailureKind::Expired)));
	}
	// Past the era the search is the only thing left that can decide it.
	let short_circuit = !window_closed && scope.proven_not_completed(tx, HeadKind::Best);
	short_circuit.then(|| decided(VerdictStatus::Pending))
}

/// Rule 5: nothing above decided it, so look for the transaction itself in
/// the blocks after those already searched, up to the earlier of its death
/// and the finalized head.
async fn search_rule(
	tx: &DurableTxEntry,
	view: &dyn PinnedView,
	window_closed: bool,
) -> RuleOutcome {
	let from = tx.scanned_to.map_or(tx.window.birth, |scanned| scanned + 1);
	let to = tx.window.death().min(view.heads().finalized.number);
	if from > to {
		return absent(tx, None, window_closed);
	}
	let result = view.search(from, to, tx.tx_hash).await;
	searched(tx, result, window_closed)
}

/// The verdict a search result supports.
fn searched(tx: &DurableTxEntry, result: SearchResult, window_closed: bool) -> RuleOutcome {
	match result {
		SearchResult::Found { block, outcome: Some(DispatchOutcome::Succeeded) } => {
			decided(VerdictStatus::FinalizedSuccess { at: Some(block) })
		},
		SearchResult::Found { outcome: Some(DispatchOutcome::Failed(_)), .. } => {
			decided(VerdictStatus::Failure(VerdictFailureKind::DispatchFailed))
		},
		SearchResult::NotFound { read_advanced_to } => absent(tx, read_advanced_to, window_closed),
		SearchResult::Found { outcome: None, .. } => decided(VerdictStatus::Pending),
	}
}

/// Not found in any block searched so far, now read through
/// `read_advanced_to` or the recorded cursor. Absence fails the transaction
/// only once a closed window was read through its death.
fn absent(tx: &DurableTxEntry, read_advanced_to: Option<u64>, window_closed: bool) -> RuleOutcome {
	let scanned_to = read_advanced_to.or(tx.scanned_to);
	let read_to_death = scanned_to.is_some_and(|scanned| scanned >= tx.window.death());
	if window_closed && read_to_death {
		return decided(VerdictStatus::Failure(VerdictFailureKind::Expired));
	}
	RuleOutcome::Decided(Verdict { status: VerdictStatus::Pending, scanned_to: read_advanced_to })
}

/// Rule 0: the transaction was seen included at `recorded`; check that block
/// is still canonical. The record only exists where completion was already
/// proven.
fn recorded_inclusion(
	tx: &DurableTxEntry,
	recorded: HashAndNumber,
	scope: &dyn PassScope,
	heads: &Heads,
	recorded_canonical: RecordedCanonicity,
) -> RuleOutcome {
	match recorded_canonical {
		RecordedCanonicity::Unknown => RuleOutcome::Undecided,
		RecordedCanonicity::Canonical if recorded.number <= heads.finalized.number => {
			decided(VerdictStatus::FinalizedSuccess { at: Some(recorded) })
		},
		RecordedCanonicity::Canonical => decided(VerdictStatus::PendingSuccess { at: recorded }),
		RecordedCanonicity::Reorged if scope.proven_completed(tx, HeadKind::Finalized) => {
			decided(VerdictStatus::FinalizedSuccess { at: Some(heads.finalized) })
		},
		RecordedCanonicity::Reorged if scope.proven_completed(tx, HeadKind::Best) => {
			decided(VerdictStatus::PendingSuccess { at: heads.best })
		},
		RecordedCanonicity::Reorged => decided(VerdictStatus::Pending),
	}
}

/// `status`, leaving the recorded search cursor as it is.
fn decided(status: VerdictStatus) -> RuleOutcome {
	RuleOutcome::Decided(status.into())
}

#[cfg(test)]
mod tests;
