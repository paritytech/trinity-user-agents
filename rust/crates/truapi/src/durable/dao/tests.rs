use futures::{FutureExt, StreamExt, executor::block_on};

use super::*;
use crate::{
	durable::testing::{block, extrinsic, insert_retryable, open_db, pending_presigned, test_plan},
	test_support::wait_until,
};

fn domain() -> DomainId {
	DomainId::new("test")
}

fn register(db: &Db, group: Option<GroupId>, tag: u8) -> DurableTxId {
	block_on(
		db.write(move |tx| insert(tx, &domain(), group.as_ref(), &extrinsic(tag, 100, 64), None)),
	)
	.unwrap()
}

fn read<T: Send + 'static>(
	db: &Db,
	f: impl FnOnce(&Connection) -> Result<T, DbError> + Send + 'static,
) -> T {
	block_on(db.read(f)).unwrap()
}

fn write_status(db: &Db, observed: DurableTxEntry, status: DurableTxStatus) -> bool {
	write_scanned(db, observed, status, None)
}

fn write_scanned(
	db: &Db,
	observed: DurableTxEntry,
	status: DurableTxStatus,
	scanned_to: Option<u64>,
) -> bool {
	block_on(db.write(move |tx| compare_and_set(tx, &observed, status, scanned_to))).unwrap()
}

#[test]
fn an_inserted_row_reads_back_pending_with_its_attempt() {
	let (_dir, db) = open_db();
	let id = register(&db, Some(GroupId::new("payment")), 7);

	assert_eq!(
		read(&db, move |conn| entry(conn, id)),
		Some(DurableTxEntry {
			id,
			domain: domain(),
			group: Some(GroupId::new("payment")),
			tx_hash: extrinsic(7, 100, 64).extrinsic.hash(),
			mortality: extrinsic(7, 100, 64).mortality,
			status: DurableTxStatus::Pending,
			scanned_to: None,
			plan: None,
		})
	);
}

#[test]
fn ids_follow_registration_order() {
	let (_dir, db) = open_db();
	let ids = [register(&db, None, 1), register(&db, None, 2)];

	assert!(ids[0] < ids[1]);
}

#[test]
fn a_missing_row_reads_as_none() {
	let (_dir, db) = open_db();

	assert_eq!(
		read(&db, |conn| Ok((entry(conn, DurableTxId(9))?, status(conn, DurableTxId(9))?))),
		(None, None)
	);
}

#[test]
fn a_verdict_is_written_against_the_status_and_attempt_it_was_derived_from() {
	let (_dir, db) = open_db();
	let id = register(&db, None, 1);
	let observed = read(&db, move |conn| entry(conn, id)).unwrap();
	let at = block(120);

	let wrote = write_status(&db, observed.clone(), DurableTxStatus::PendingSuccess { at });

	assert!(wrote);
	assert_eq!(
		read(&db, move |conn| entry(conn, id)).unwrap(),
		DurableTxEntry { status: DurableTxStatus::PendingSuccess { at }, ..observed }
	);
}

/// A pass and a watch both read a row as Pending; the watch records the
/// inclusion first. The pass's verdict, derived from the stale status, must
/// not overwrite it.
#[test]
fn a_verdict_is_refused_once_the_status_moved() {
	let (_dir, db) = open_db();
	let id = register(&db, None, 1);
	let observed = read(&db, move |conn| entry(conn, id)).unwrap();
	write_status(&db, observed.clone(), DurableTxStatus::PendingSuccess { at: block(120) });
	let expired = DurableTxStatus::Failure(DurableTxFailureKind::Expired);

	let wrote = write_status(&db, observed, expired);

	assert!(!wrote);
	assert_eq!(
		read(&db, move |conn| status(conn, id)),
		Some(DurableTxStatus::PendingSuccess { at: block(120) })
	);
}

/// A verdict derived from one attempt must not land on a row whose attempt
/// has since changed, as it will once a policy rebuilds a transaction under
/// the same id.
#[test]
fn a_verdict_is_refused_for_another_attempt() {
	let (_dir, db) = open_db();
	let id = register(&db, None, 1);
	let mut observed = read(&db, move |conn| entry(conn, id)).unwrap();
	observed.tx_hash = H256::repeat_byte(0xee);

	assert!(!write_status(&db, observed, DurableTxStatus::FinalizedSuccess { at: None }));
}

#[test]
fn a_terminal_row_is_never_rewritten() {
	let (_dir, db) = open_db();
	let id = register(&db, None, 1);
	let observed = read(&db, move |conn| entry(conn, id)).unwrap();
	write_status(&db, observed.clone(), DurableTxStatus::Failure(DurableTxFailureKind::Expired));
	let terminal = DurableTxEntry {
		status: DurableTxStatus::Failure(DurableTxFailureKind::Expired),
		..observed
	};

	assert!(!write_status(&db, terminal, DurableTxStatus::FinalizedSuccess { at: None }));
}

/// Each status is read back with the evidence it was written with: the
/// block a success was seen at, and why a transaction failed.
#[test]
fn a_status_reads_back_with_its_evidence() {
	let (_dir, db) = open_db();
	let written = [
		DurableTxStatus::PendingSuccess { at: block(120) },
		DurableTxStatus::FinalizedSuccess { at: Some(block(121)) },
		DurableTxStatus::FinalizedSuccess { at: None },
		DurableTxStatus::Failure(DurableTxFailureKind::Expired),
		DurableTxStatus::Failure(DurableTxFailureKind::DispatchFailed),
		DurableTxStatus::Failure(DurableTxFailureKind::Rejected),
	];

	let read_back: Vec<_> = (1..)
		.zip(written)
		.map(|(tag, status)| {
			let id = register(&db, None, tag);
			let observed = read(&db, move |conn| entry(conn, id)).unwrap();
			write_status(&db, observed, status);
			read(&db, move |conn| super::status(conn, id)).unwrap()
		})
		.collect();

	assert_eq!(read_back, written);
}

/// Two domains may pick the same group name, such as an operation id; a
/// group is only ever read within its domain.
/// A watch's verdict knows nothing of what recovery already searched, so
/// writing it must not throw that away.
#[test]
fn a_verdict_without_a_cursor_keeps_the_recorded_one() {
	let (_dir, db) = open_db();
	let id = register(&db, None, 7);
	let observed = read(&db, move |conn| entry(conn, id)).unwrap();
	write_scanned(&db, observed, DurableTxStatus::Pending, Some(120));
	let scanned_entry = read(&db, move |conn| entry(conn, id)).unwrap();

	write_status(&db, scanned_entry, DurableTxStatus::PendingSuccess { at: block(125) });

	let recorded = read(&db, move |conn| entry(conn, id)).unwrap();
	assert_eq!(recorded.scanned_to, Some(120));
}

#[test]
fn a_group_lists_its_own_domain_in_registration_order() {
	let (_dir, db) = open_db();
	let group_id = GroupId::new("payment");
	let first = register(&db, Some(group_id.clone()), 1);
	let second = register(&db, Some(group_id.clone()), 2);
	register(&db, None, 3);
	let other = group_id.clone();
	block_on(db.write(move |tx| {
		insert(tx, &DomainId::new("other"), Some(&other), &extrinsic(4, 100, 64), None)
	}))
	.unwrap();

	assert_eq!(
		read(&db, move |conn| group(conn, &domain(), &group_id)),
		vec![pending_presigned(first, 1), pending_presigned(second, 2)]
	);
}

#[test]
fn undecided_entries_list_only_rows_awaiting_a_verdict() {
	let (_dir, db) = open_db();
	let ids = [register(&db, None, 1), register(&db, None, 2), register(&db, None, 3)];
	let decided = read(&db, move |conn| entry(conn, ids[1])).unwrap();
	write_status(&db, decided, DurableTxStatus::FinalizedSuccess { at: None });

	let listed = read(&db, |conn| undecided_entries(conn, &domain()));

	assert_eq!(listed.iter().map(|entry| entry.id).collect::<Vec<_>>(), [ids[0], ids[2]]);
}

#[test]
fn only_domains_with_a_transaction_awaiting_a_verdict_are_live() {
	let (_dir, db) = open_db();
	assert_eq!(read(&db, live_domains), Vec::<DomainId>::new());
	let id = register(&db, None, 1);
	assert_eq!(read(&db, live_domains), vec![domain()]);

	let observed = read(&db, move |conn| entry(conn, id)).unwrap();
	write_status(&db, observed, DurableTxStatus::FinalizedSuccess { at: None });

	assert_eq!(read(&db, live_domains), Vec::<DomainId>::new());
}

#[test]
fn observed_queries_follow_commits() {
	let (_dir, db) = open_db();
	let group_id = GroupId::new("payment");
	let mut live = observe_live_transaction_domains(&db);
	let mut grouped = observe_group(&db, domain(), group_id.clone());
	assert_eq!(next(&mut live), vec![]);
	assert_eq!(next(&mut grouped), vec![]);

	let id = register(&db, Some(group_id), 1);
	let mut status = observe_status(&db, id);

	assert_eq!(next(&mut live), vec![domain()]);
	assert_eq!(next(&mut grouped), vec![pending_presigned(id, 1)]);
	assert_eq!(next(&mut status), Some(DurableTxStatus::Pending));
}

fn next<T>(stream: &mut BoxStream<'static, Result<T, DbError>>) -> T {
	block_on(stream.next()).unwrap().unwrap()
}

fn defer_in(db: &Db, domain: DomainId, group: Option<GroupId>) -> DurableTxId {
	let plan = test_plan(vec![1]);
	let write = db.write(move |tx| defer(tx, &domain, group.as_ref(), &plan));
	block_on(write).unwrap()
}

fn pending_rows(db: &Db) -> Vec<DeferredDurableTx> {
	next(&mut observe_pending_submissions(db))
}

fn start_attempt_tagged(db: &Db, id: DurableTxId, tag: u8) -> bool {
	block_on(db.write(move |tx| start_attempt(tx, id, &extrinsic(tag, 110, 32), None))).unwrap()
}

fn abandon_waiting(db: &Db, id: DurableTxId) -> bool {
	block_on(db.write(move |tx| abandon_submission(tx, id))).unwrap()
}

fn insert_rebuildable(db: &Db, params: u8) -> DurableTxId {
	insert_retryable(db, &domain(), extrinsic(1, 100, 64), test_plan(vec![params]))
}

#[test]
fn a_deferred_row_waits_for_its_policy_with_no_attempt() {
	let (_dir, db) = open_db();
	let id = defer_in(&db, domain(), Some(GroupId::new("payment")));

	assert_eq!(
		pending_rows(&db),
		vec![DeferredDurableTx {
			id,
			domain: domain(),
			group: Some(GroupId::new("payment")),
			plan: test_plan(vec![1]),
			attempts: 0,
		}]
	);
	assert_eq!(read(&db, move |conn| status(conn, id)), Some(DurableTxStatus::PendingSubmission));
	assert_eq!(read(&db, move |conn| entry(conn, id)), None);
}

/// Something is still to be done, so the host must keep recovery running,
/// but no pass can decide a transaction that was never submitted.
#[test]
fn a_transaction_waiting_to_be_built_is_live_but_not_decidable() {
	let (_dir, db) = open_db();
	defer_in(&db, domain(), None);

	assert_eq!(next(&mut observe_live_transaction_domains(&db)), vec![domain()]);
	assert_eq!(read(&db, waiting_domains), vec![domain()]);
	assert_eq!(read(&db, live_domains), Vec::<DomainId>::new());
	assert_eq!(read(&db, |conn| undecided_entries(conn, &domain())), vec![]);
}

#[test]
fn starting_an_attempt_gives_a_waiting_row_its_extrinsic() {
	let (_dir, db) = open_db();
	let id = defer_in(&db, domain(), None);

	assert!(start_attempt_tagged(&db, id, 9));
	assert_eq!(
		read(&db, move |conn| entry(conn, id)),
		Some(DurableTxEntry {
			id,
			domain: domain(),
			group: None,
			tx_hash: extrinsic(9, 110, 32).extrinsic.hash(),
			mortality: extrinsic(9, 110, 32).mortality,
			status: DurableTxStatus::Pending,
			scanned_to: None,
			plan: Some(test_plan(vec![1])),
		})
	);
	assert_eq!(pending_rows(&db), vec![]);
}

/// A policy records what it learned building an attempt, such as the
/// revision it signed, so an oracle judges the attempt that is live.
#[test]
fn an_attempt_can_replace_the_rows_params() {
	let (_dir, db) = open_db();
	let id = defer_in(&db, domain(), None);
	let replaced = PolicyParams(vec![2]);

	let started = block_on(
		db.write(move |tx| start_attempt(tx, id, &extrinsic(9, 110, 32), Some(&replaced))),
	)
	.unwrap();

	assert!(started);
	let plan = read(&db, move |conn| entry(conn, id)).and_then(|entry| entry.plan);
	assert_eq!(plan, Some(test_plan(vec![2])));
}

/// Two builds of one row must not both become attempts: only the first
/// finds it still waiting.
#[test]
fn an_attempt_starts_only_on_a_row_still_waiting() {
	let (_dir, db) = open_db();
	let waiting = defer_in(&db, domain(), None);
	let submitted = register(&db, None, 1);
	start_attempt_tagged(&db, waiting, 9);

	assert!(!start_attempt_tagged(&db, waiting, 10));
	assert!(!start_attempt_tagged(&db, submitted, 10));
	assert_eq!(
		read(&db, move |conn| entry(conn, waiting)).unwrap().tx_hash,
		extrinsic(9, 110, 32).extrinsic.hash()
	);
}

/// The rebuild cooldown grows with every attempt a row was built into, and
/// it must keep growing after a restart, so the count lives in the row.
#[test]
fn every_attempt_a_row_is_built_into_is_counted() {
	let (_dir, db) = open_db();
	let id = defer_in(&db, domain(), None);
	for tag in [9, 10] {
		start_attempt_tagged(&db, id, tag);
		let attempt = read(&db, move |conn| entry(conn, id)).unwrap();
		write_status(&db, attempt, DurableTxStatus::PendingSubmission);
	}

	let attempts: Vec<u32> = pending_rows(&db).iter().map(|row| row.attempts).collect();

	assert_eq!(attempts, vec![2]);
}

/// A rebuilt attempt has its own hash and birth, so the blocks searched
/// for the previous one prove nothing about it.
#[test]
fn a_rebuilt_attempt_starts_with_nothing_searched() {
	let (_dir, db) = open_db();
	let id = defer_in(&db, domain(), None);
	start_attempt_tagged(&db, id, 9);
	let first = read(&db, move |conn| entry(conn, id)).unwrap();
	write_scanned(&db, first, DurableTxStatus::Pending, Some(120));
	let searched = read(&db, move |conn| entry(conn, id)).unwrap();
	write_status(&db, searched, DurableTxStatus::PendingSubmission);

	start_attempt_tagged(&db, id, 10);

	let rebuilt = read(&db, move |conn| entry(conn, id)).unwrap();
	assert_eq!(rebuilt.scanned_to, None);
}

#[test]
fn only_a_row_still_waiting_can_be_abandoned() {
	let (_dir, db) = open_db();
	let waiting = defer_in(&db, domain(), None);
	let submitted = register(&db, None, 1);

	assert!(abandon_waiting(&db, waiting));
	assert!(!abandon_waiting(&db, waiting));
	assert!(!abandon_waiting(&db, submitted));
	assert_eq!(
		read(&db, move |conn| status(conn, waiting)),
		Some(DurableTxStatus::Failure(DurableTxFailureKind::Abandoned))
	);
	assert_eq!(read(&db, move |conn| status(conn, submitted)), Some(DurableTxStatus::Pending));
}

/// A proven failure the policy accepts to retry sends the row back to be
/// built, through the same compare-and-set as any verdict.
#[test]
fn a_verdict_can_send_a_submitted_row_back_to_its_policy() {
	let (_dir, db) = open_db();
	let id = insert_rebuildable(&db, 3);
	let observed = read(&db, move |conn| entry(conn, id)).unwrap();

	assert!(write_status(&db, observed, DurableTxStatus::PendingSubmission));
	assert_eq!(
		pending_rows(&db),
		vec![DeferredDurableTx {
			id,
			domain: domain(),
			group: None,
			plan: test_plan(vec![3]),
			attempts: 0,
		}]
	);
}

/// A row has a plan exactly when it stores params, so a plan whose params
/// are empty must still read back as a plan.
#[test]
fn a_row_keeps_the_plan_it_was_registered_with() {
	let (_dir, db) = open_db();
	let with_plan = insert_rebuildable(&db, 3);
	let with_empty_plan =
		insert_retryable(&db, &domain(), extrinsic(3, 100, 64), test_plan(vec![]));
	let without = register(&db, None, 2);

	assert_eq!(read(&db, move |conn| submission_plan(conn, with_plan)), Some(test_plan(vec![3])));
	assert_eq!(
		read(&db, move |conn| submission_plan(conn, with_empty_plan)),
		Some(test_plan(vec![]))
	);
	assert_eq!(read(&db, move |conn| submission_plan(conn, without)), None);
}

/// A domain's worker builds all of its waiting rows, whatever their group,
/// and never another domain's: each domain has its own policy.
#[test]
fn a_domain_lists_its_rows_of_every_group_and_no_other_domain() {
	let (_dir, db) = open_db();
	let grouped = defer_in(&db, domain(), Some(GroupId::new("g1")));
	let ungrouped = defer_in(&db, domain(), None);
	defer_in(&db, DomainId::new("other"), Some(GroupId::new("g1")));
	let other_group = defer_in(&db, domain(), Some(GroupId::new("g2")));

	let listed = read(&db, |conn| domain_submissions(conn, &domain()));

	let ids: Vec<_> = listed.iter().map(|deferred| deferred.id).collect();
	assert_eq!(ids, vec![grouped, ungrouped, other_group]);
}

/// A transaction given up before it was ever built has no attempt. It
/// must not make the rest of its domain unreadable to recovery.
#[test]
fn a_transaction_given_up_before_it_was_built_leaves_its_domain_readable() {
	let (_dir, db) = open_db();
	let never_built = defer_in(&db, domain(), None);
	abandon_waiting(&db, never_built);
	let live = register(&db, None, 1);

	let listed = read(&db, |conn| undecided_entries(conn, &domain()));

	assert_eq!(listed.iter().map(|entry| entry.id).collect::<Vec<_>>(), vec![live]);
	assert_eq!(read(&db, move |conn| entry(conn, never_built)), None);
}

/// A row built and handed back between two reads of the observer looks
/// the same by id. It must still be reported, or nothing builds it again.
#[test]
fn a_row_handed_back_between_two_reads_is_reported_again() {
	let (_dir, db) = open_db();
	let mut waiting = observe_pending_submissions(&db);
	assert_eq!(next(&mut waiting), vec![]);
	let id = defer_in(&db, domain(), None);
	assert_eq!(next(&mut waiting).len(), 1);

	start_attempt_tagged(&db, id, 9);
	let attempt = read(&db, move |conn| entry(conn, id)).unwrap();
	write_status(&db, attempt, DurableTxStatus::PendingSubmission);

	let reported_again = || {
		let emitted = waiting.next().now_or_never();
		matches!(emitted, Some(Some(Ok(rows))) if rows.len() == 1)
	};
	wait_until(reported_again, "the handed-back row is reported");
}
