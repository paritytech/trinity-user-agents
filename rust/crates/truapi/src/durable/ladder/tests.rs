use futures::executor::block_on;

use super::*;
use crate::durable::{
	model::DurableTxId,
	oracle::{CompletionOracle, LedgerView, Unobservable},
	testing::{FakeView, ScriptedScope, block, entry},
};

const BIRTH: u64 = 100;
const PERIOD: u64 = 64;
const DEATH: u64 = BIRTH + PERIOD;

fn tx() -> DurableTxEntry {
	entry(DurableTxId(1), BIRTH, PERIOD)
}

/// Seen included at the block with `number`, not yet finalized.
fn recorded_at(number: u64) -> DurableTxEntry {
	DurableTxEntry { status: DurableTxStatus::PendingSuccess { at: block(number) }, ..tx() }
}

async fn evaluate(
	tx: DurableTxEntry,
	scope: ScriptedScope,
	finalized: u64,
	recorded_canonical: RecordedCanonicity,
	search: SearchResult,
) -> RuleOutcome {
	evaluate_ladder(&tx, &scope, &FakeView::new(finalized, search), recorded_canonical).await
}

fn decided(status: VerdictStatus) -> RuleOutcome {
	RuleOutcome::Decided(status.into())
}

const INCOMPLETE: SearchResult = SearchResult::NotFound { read_advanced_to: None };
const ABSENT: SearchResult = SearchResult::NotFound { read_advanced_to: Some(DEATH) };

/// Not found in any block, every one read through `number`.
fn read_to(number: u64) -> SearchResult {
	SearchResult::NotFound { read_advanced_to: Some(number) }
}

fn pending_scanned_to(number: u64) -> RuleOutcome {
	RuleOutcome::Decided(Verdict { scanned_to: Some(number), status: VerdictStatus::Pending })
}

fn scanned_to(number: u64) -> DurableTxEntry {
	DurableTxEntry { scanned_to: Some(number), ..tx() }
}

fn says() -> ScriptedScope {
	ScriptedScope::default()
}

#[test]
fn rule_0_a_canonical_record_at_or_below_finalized_finalizes() {
	block_on(async {
		let outcome =
			evaluate(recorded_at(120), says(), 130, RecordedCanonicity::Canonical, INCOMPLETE)
				.await;

		assert_eq!(outcome, decided(VerdictStatus::FinalizedSuccess { at: Some(block(120)) }));
	})
}

#[test]
fn rule_0_a_canonical_record_above_finalized_holds_pending_success() {
	block_on(async {
		let outcome =
			evaluate(recorded_at(140), says(), 130, RecordedCanonicity::Canonical, INCOMPLETE)
				.await;

		assert_eq!(outcome, decided(VerdictStatus::PendingSuccess { at: block(140) }));
	})
}

/// A reorg removed the block the success was recorded at and the domain
/// sees no effect any more: the transaction is pending again, so its
/// effects are no longer trusted.
#[test]
fn rule_0_a_gone_record_with_nothing_visible_demotes_and_clears() {
	block_on(async {
		let outcome =
			evaluate(recorded_at(120), says(), 130, RecordedCanonicity::Reorged, INCOMPLETE).await;

		assert_eq!(outcome, decided(VerdictStatus::Pending));
	})
}

/// The transaction was re-included on the new branch, so its effect is
/// still visible at the best head.
#[test]
fn rule_0_a_gone_record_still_completed_at_best_re_records_the_best_head() {
	block_on(async {
		let scope = ScriptedScope { completed_at_best: true, ..says() };
		let view = FakeView::new(130, INCOMPLETE);

		let outcome =
			evaluate_ladder(&recorded_at(120), &scope, &view, RecordedCanonicity::Reorged).await;

		assert_eq!(outcome, decided(VerdictStatus::PendingSuccess { at: view.heads().best }));
	})
}

/// Asked before the best head, or a chain that reorgs its head between
/// passes would keep the transaction above F and never finalize it.
#[test]
fn rule_0_a_gone_record_completed_at_finalized_finalizes_there() {
	block_on(async {
		let scope =
			ScriptedScope { completed_at_finalized: true, completed_at_best: true, ..says() };
		let view = FakeView::new(130, INCOMPLETE);

		let outcome =
			evaluate_ladder(&recorded_at(120), &scope, &view, RecordedCanonicity::Reorged).await;

		assert_eq!(
			outcome,
			decided(VerdictStatus::FinalizedSuccess { at: Some(view.heads().finalized) })
		);
	})
}

/// The recorded block's height could not be read. Even a domain that proves
/// completion waits: the record has to be checked first.
#[test]
fn rule_0_an_unreadable_canonicity_check_is_undecided() {
	block_on(async {
		let scope = ScriptedScope { completed_at_finalized: true, ..says() };

		let outcome =
			evaluate(recorded_at(120), scope, 130, RecordedCanonicity::Unknown, INCOMPLETE).await;

		assert_eq!(outcome, RuleOutcome::Undecided);
	})
}

#[test]
fn rule_1_completion_proven_at_finalized_finalizes() {
	block_on(async {
		let scope = ScriptedScope { completed_at_finalized: true, ..says() };

		let outcome = evaluate(tx(), scope, 130, RecordedCanonicity::Unknown, INCOMPLETE).await;

		assert_eq!(outcome, decided(VerdictStatus::FinalizedSuccess { at: None }));
	})
}

#[test]
fn rule_2_completion_proven_only_at_best_is_pending_success() {
	block_on(async {
		let scope = ScriptedScope { completed_at_best: true, ..says() };
		let view = FakeView::new(130, INCOMPLETE);

		let outcome = evaluate_ladder(&tx(), &scope, &view, RecordedCanonicity::Unknown).await;

		assert_eq!(outcome, decided(VerdictStatus::PendingSuccess { at: view.heads().best }));
	})
}

#[test]
fn rule_1_wins_over_rule_2_on_the_same_evidence() {
	block_on(async {
		let scope =
			ScriptedScope { completed_at_finalized: true, completed_at_best: true, ..says() };

		let outcome = evaluate(tx(), scope, 130, RecordedCanonicity::Unknown, INCOMPLETE).await;

		assert_eq!(outcome, decided(VerdictStatus::FinalizedSuccess { at: None }));
	})
}

#[test]
fn rule_3_proven_non_completion_after_the_era_fails() {
	block_on(async {
		let scope = ScriptedScope { not_completed_at_finalized: true, ..says() };

		let outcome =
			evaluate(tx(), scope, DEATH + 1, RecordedCanonicity::Unknown, INCOMPLETE).await;

		assert_eq!(outcome, decided(VerdictStatus::Failure(VerdictFailureKind::Expired)));
	})
}

/// The transaction can still be included until its era ends, so absence of
/// its effect means nothing yet.
#[test]
fn rule_3_proven_non_completion_before_the_era_ends_does_not_fail() {
	block_on(async {
		let scope = ScriptedScope { not_completed_at_finalized: true, ..says() };

		let outcome =
			evaluate(tx(), scope, BIRTH + 1, RecordedCanonicity::Unknown, read_to(BIRTH + 1)).await;

		assert_eq!(outcome, pending_scanned_to(BIRTH + 1));
	})
}

/// The window stays open through the death block itself, one block more
/// than the era strictly allows, as on Android.
#[test]
fn rule_3_the_window_is_still_open_at_the_death_block() {
	block_on(async {
		let scope = ScriptedScope { not_completed_at_finalized: true, ..says() };

		let outcome = evaluate(tx(), scope, DEATH, RecordedCanonicity::Unknown, ABSENT).await;

		assert_eq!(outcome, pending_scanned_to(DEATH));
	})
}

/// A domain that answers both ways cannot fail a transaction that ran.
#[test]
fn proven_completion_beats_proven_non_completion() {
	block_on(async {
		let scope = ScriptedScope {
			completed_at_finalized: true,
			not_completed_at_finalized: true,
			..says()
		};

		let outcome =
			evaluate(tx(), scope, DEATH + 1, RecordedCanonicity::Unknown, INCOMPLETE).await;

		assert_eq!(outcome, decided(VerdictStatus::FinalizedSuccess { at: None }));
	})
}

/// A transaction with no evidence yet must not cost a body search on every
/// new head.
#[test]
fn rule_4_short_circuits_the_search_while_the_window_is_open() {
	block_on(async {
		let scope = ScriptedScope { not_completed_at_best: true, ..says() };
		let view = FakeView::new(BIRTH + 1, INCOMPLETE);

		let outcome = evaluate_ladder(&tx(), &scope, &view, RecordedCanonicity::Unknown).await;

		assert_eq!(outcome, decided(VerdictStatus::Pending));
		assert_eq!(view.searches(), 0);
	})
}

/// Past the era the search is the only thing left that can decide it.
#[test]
fn rule_4_does_not_fire_once_the_era_has_ended() {
	block_on(async {
		let scope = ScriptedScope { not_completed_at_best: true, ..says() };
		let view = FakeView::new(DEATH + 1, INCOMPLETE);

		evaluate_ladder(&tx(), &scope, &view, RecordedCanonicity::Unknown).await;

		assert_eq!(view.searches(), 1);
	})
}

#[test]
fn rule_5_a_successful_dispatch_found_finalizes_at_its_block() {
	block_on(async {
		let found =
			SearchResult::Found { block: block(110), outcome: Some(DispatchOutcome::Succeeded) };

		let outcome = evaluate(tx(), says(), 130, RecordedCanonicity::Unknown, found).await;

		assert_eq!(outcome, decided(VerdictStatus::FinalizedSuccess { at: Some(block(110)) }));
	})
}

/// The extrinsic was included and its dispatch failed: inclusion is not
/// success.
#[test]
fn rule_5_a_failed_dispatch_found_fails_as_dispatch_failed() {
	block_on(async {
		let found = SearchResult::Found {
			block: block(110),
			outcome: Some(DispatchOutcome::Failed(String::new())),
		};

		let outcome = evaluate(tx(), says(), 130, RecordedCanonicity::Unknown, found).await;

		assert_eq!(outcome, decided(VerdictStatus::Failure(VerdictFailureKind::DispatchFailed)));
	})
}

/// The extrinsic is in a block whose events are pruned on this node.
#[test]
fn rule_5_an_unreadable_outcome_stays_pending() {
	block_on(async {
		let found = SearchResult::Found { block: block(110), outcome: None };

		let outcome = evaluate(tx(), says(), DEATH + 1, RecordedCanonicity::Unknown, found).await;

		assert_eq!(outcome, decided(VerdictStatus::Pending));
	})
}

/// The extrinsic never made it into any block of its era, and every block
/// was read.
#[test]
fn rule_5_absence_over_the_whole_closed_window_expires() {
	block_on(async {
		let outcome = evaluate(tx(), says(), DEATH + 1, RecordedCanonicity::Unknown, ABSENT).await;

		assert_eq!(outcome, decided(VerdictStatus::Failure(VerdictFailureKind::Expired)));
	})
}

/// Some blocks of the era could not be read, so absence proves nothing.
#[test]
fn rule_5_a_partially_read_window_stays_pending() {
	block_on(async {
		let outcome =
			evaluate(tx(), says(), DEATH + 1, RecordedCanonicity::Unknown, INCOMPLETE).await;

		assert_eq!(outcome, decided(VerdictStatus::Pending));
	})
}

#[test]
fn rule_5_searches_from_birth_to_the_earlier_of_death_and_finalized() {
	block_on(async {
		let early = FakeView::new(130, INCOMPLETE);
		let late = FakeView::new(DEATH + 50, INCOMPLETE);

		evaluate_ladder(&tx(), &says(), &early, RecordedCanonicity::Unknown).await;
		evaluate_ladder(&tx(), &says(), &late, RecordedCanonicity::Unknown).await;

		assert_eq!(early.searched_ranges(), vec![(BIRTH, 130)]);
		assert_eq!(late.searched_ranges(), vec![(BIRTH, DEATH)]);
	})
}

/// The extrinsic was signed against a best block finality has not reached
/// yet.
#[test]
fn a_birth_above_finalized_stays_pending_without_searching() {
	block_on(async {
		let view = FakeView::new(BIRTH - 1, INCOMPLETE);

		let outcome = evaluate_ladder(&tx(), &says(), &view, RecordedCanonicity::Unknown).await;

		assert_eq!(outcome, decided(VerdictStatus::Pending));
		assert_eq!(view.searches(), 0);
	})
}

#[test]
fn an_unobservable_domain_is_still_decided_by_the_search() {
	block_on(async {
		let view = FakeView::new(
			DEATH + 1,
			SearchResult::Found { block: block(110), outcome: Some(DispatchOutcome::Succeeded) },
		);
		let scope = Unobservable(H256::zero())
			.open_pass(&[], &LedgerView::new(vec![]), &view.heads())
			.await
			.unwrap();

		let outcome =
			evaluate_ladder(&tx(), scope.as_ref(), &view, RecordedCanonicity::Unknown).await;

		assert_eq!(outcome, decided(VerdictStatus::FinalizedSuccess { at: Some(block(110)) }));
	})
}

/// Blocks read on an earlier pass were finalized, so they cannot have
/// changed: the next pass reads only the blocks finalized since.
#[test]
fn rule_5_searches_only_past_the_blocks_already_read() {
	block_on(async {
		let view = FakeView::new(123, INCOMPLETE);

		evaluate_ladder(&scanned_to(120), &says(), &view, RecordedCanonicity::Unknown).await;

		assert_eq!(view.searched_ranges(), vec![(121, 123)]);
	})
}

#[test]
fn rule_5_records_how_far_the_search_read() {
	block_on(async {
		let outcome =
			evaluate(scanned_to(120), says(), 123, RecordedCanonicity::Unknown, read_to(123)).await;

		assert_eq!(outcome, pending_scanned_to(123));
	})
}

/// The first new block could not be read, so the cursor stays where it
/// was and the next pass tries that block again.
#[test]
fn rule_5_an_unreadable_first_block_leaves_the_cursor() {
	block_on(async {
		let outcome =
			evaluate(scanned_to(120), says(), 123, RecordedCanonicity::Unknown, INCOMPLETE).await;

		assert_eq!(outcome, decided(VerdictStatus::Pending));
	})
}

/// Earlier passes read the window through its death; once it closes there
/// is nothing left to read and absence is already proven.
#[test]
fn rule_5_a_window_read_through_its_death_expires_without_searching() {
	block_on(async {
		let view = FakeView::new(DEATH + 1, INCOMPLETE);

		let outcome =
			evaluate_ladder(&scanned_to(DEATH), &says(), &view, RecordedCanonicity::Unknown).await;

		assert_eq!(outcome, decided(VerdictStatus::Failure(VerdictFailureKind::Expired)));
		assert_eq!(view.searches(), 0);
	})
}

/// The last blocks of the era were read on this pass, the earlier ones on
/// previous passes.
#[test]
fn rule_5_absence_read_across_passes_expires() {
	block_on(async {
		let outcome =
			evaluate(scanned_to(150), says(), DEATH + 1, RecordedCanonicity::Unknown, ABSENT).await;

		assert_eq!(outcome, decided(VerdictStatus::Failure(VerdictFailureKind::Expired)));
	})
}

/// Nothing past the cursor is finalized yet.
#[test]
fn rule_5_nothing_newly_finalized_keeps_the_cursor_without_searching() {
	block_on(async {
		let view = FakeView::new(120, INCOMPLETE);

		let outcome =
			evaluate_ladder(&scanned_to(120), &says(), &view, RecordedCanonicity::Unknown).await;

		assert_eq!(outcome, decided(VerdictStatus::Pending));
		assert_eq!(view.searches(), 0);
	})
}
