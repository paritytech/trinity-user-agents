use futures::executor::block_on;
use futures::{FutureExt, StreamExt};

use super::*;
use crate::durable::testing::{REBUILD_POLICY, insert_retryable, rebuild_plan};
use crate::test_support::wait_until;

fn schedule_in(db: &Db, domain: DomainId, group: Option<GroupId>) -> DurableTxId {
    let policy = rebuild_plan(vec![1]);
    let write = db.write(move |tx| schedule(tx, &domain, group.as_ref(), &policy));
    block_on(write).unwrap()
}

fn pending_rows(db: &Db) -> Vec<ScheduledDurableTx> {
    next(&mut observe_pending_submissions(db))
}

fn start_attempt_tagged(db: &Db, id: DurableTxId, tag: u8) -> bool {
    block_on(db.write(move |tx| start_attempt(tx, id, &extrinsic(tag, 110, 32)))).unwrap()
}

fn abandon_waiting(db: &Db, id: DurableTxId) -> bool {
    block_on(db.write(move |tx| abandon_submission(tx, id))).unwrap()
}

fn insert_rebuilt_by(db: &Db, params: u8) -> DurableTxId {
    insert_retryable(
        db,
        &DOMAIN,
        extrinsic(1, 100, 64),
        rebuild_plan(vec![params]),
    )
}

#[test]
fn a_scheduled_row_waits_for_its_policy_with_no_attempt() {
    let (_dir, db) = open_db();
    let id = schedule_in(&db, DOMAIN, Some(GroupId::new("payment")));

    assert_eq!(
        pending_rows(&db),
        vec![ScheduledDurableTx {
            id,
            domain: DOMAIN,
            group: Some(GroupId::new("payment")),
            plan: rebuild_plan(vec![1]),
        }]
    );
    assert_eq!(
        read(&db, move |conn| status(conn, id)),
        Some(DurableTxStatus::PendingSubmission)
    );
    assert_eq!(read(&db, move |conn| entry(conn, id)), None);
}

/// Something is still to be done, so the host must keep recovery running,
/// but no pass can decide a transaction that was never submitted.
#[test]
fn a_transaction_waiting_to_be_built_is_live_but_not_decidable() {
    let (_dir, db) = open_db();
    schedule_in(&db, DOMAIN, None);

    assert!(next(&mut observe_has_live(&db)));
    assert!(read(&db, has_pending_submissions));
    assert_eq!(read(&db, live_domains), Vec::<DomainId>::new());
    assert_eq!(read(&db, |conn| domain_entries(conn, &DOMAIN)), vec![]);
}

#[test]
fn starting_an_attempt_gives_a_waiting_row_its_extrinsic() {
    let (_dir, db) = open_db();
    let id = schedule_in(&db, DOMAIN, None);

    assert!(start_attempt_tagged(&db, id, 9));
    assert_eq!(
        read(&db, move |conn| entry(conn, id)),
        Some(DurableTxEntry {
            id,
            domain: DOMAIN,
            group: None,
            tx_hash: extrinsic(9, 110, 32).extrinsic.hash(),
            mortality: extrinsic(9, 110, 32).mortality,
            status: DurableTxStatus::Pending,
            success_detected_at: None,
        })
    );
    assert_eq!(pending_rows(&db), vec![]);
}

/// Two builds of one row must not both become attempts: only the first
/// finds it still waiting.
#[test]
fn an_attempt_starts_only_on_a_row_still_waiting() {
    let (_dir, db) = open_db();
    let waiting = schedule_in(&db, DOMAIN, None);
    let submitted = register(&db, None, 1);
    start_attempt_tagged(&db, waiting, 9);

    assert!(!start_attempt_tagged(&db, waiting, 10));
    assert!(!start_attempt_tagged(&db, submitted, 10));
    assert_eq!(
        read(&db, move |conn| entry(conn, waiting)).unwrap().tx_hash,
        extrinsic(9, 110, 32).extrinsic.hash()
    );
}

#[test]
fn only_a_row_still_waiting_can_be_abandoned() {
    let (_dir, db) = open_db();
    let waiting = schedule_in(&db, DOMAIN, None);
    let submitted = register(&db, None, 1);

    assert!(abandon_waiting(&db, waiting));
    assert!(!abandon_waiting(&db, waiting));
    assert!(!abandon_waiting(&db, submitted));
    assert_eq!(
        read(&db, move |conn| status(conn, waiting)),
        Some(DurableTxStatus::Failure)
    );
    assert_eq!(
        read(&db, move |conn| status(conn, submitted)),
        Some(DurableTxStatus::Pending)
    );
}

/// A proven failure the policy accepts to retry sends the row back to be
/// built, through the same compare-and-set as any verdict.
#[test]
fn a_verdict_can_send_a_submitted_row_back_to_its_policy() {
    let (_dir, db) = open_db();
    let id = insert_rebuilt_by(&db, 3);
    let observed = read(&db, move |conn| entry(conn, id)).unwrap();

    assert!(write_verdict(
        &db,
        observed,
        verdict(DurableTxStatus::PendingSubmission, None)
    ));
    assert_eq!(
        pending_rows(&db),
        vec![ScheduledDurableTx {
            id,
            domain: DOMAIN,
            group: None,
            plan: rebuild_plan(vec![3]),
        }]
    );
}

#[test]
fn a_row_names_the_policy_it_was_registered_with() {
    let (_dir, db) = open_db();
    let with_policy = insert_rebuilt_by(&db, 3);
    let without = register(&db, None, 2);

    assert_eq!(
        read(&db, move |conn| submission_plan(conn, with_policy)),
        Some(rebuild_plan(vec![3]))
    );
    assert_eq!(
        read(&db, move |conn| submission_plan(conn, without)),
        None
    );
}

/// A bucket builds one domain's rows of one policy and group. A row with
/// no group is a bucket of its own, not a match for every group.
#[test]
fn a_bucket_lists_only_its_domain_policy_and_group() {
    let (_dir, db) = open_db();
    let grouped = schedule_in(&db, DOMAIN, Some(GroupId::new("g1")));
    let ungrouped = schedule_in(&db, DOMAIN, None);
    schedule_in(
        &db,
        DomainId::from_static("other"),
        Some(GroupId::new("g1")),
    );
    schedule_in(&db, DOMAIN, Some(GroupId::new("g2")));
    let bucket = |group: Option<GroupId>| {
        read(&db, move |conn| {
            bucket_submissions(conn, &DOMAIN, &REBUILD_POLICY, group.as_ref())
        })
        .iter()
        .map(|scheduled| scheduled.id)
        .collect::<Vec<_>>()
    };

    assert_eq!(bucket(Some(GroupId::new("g1"))), vec![grouped]);
    assert_eq!(bucket(None), vec![ungrouped]);
}

/// A transaction given up before it was ever built has no attempt. It
/// must not make the rest of its domain unreadable to recovery.
#[test]
fn a_transaction_given_up_before_it_was_built_leaves_its_domain_readable() {
    let (_dir, db) = open_db();
    let never_built = schedule_in(&db, DOMAIN, None);
    abandon_waiting(&db, never_built);
    let live = register(&db, None, 1);

    let listed = read(&db, |conn| domain_entries(conn, &DOMAIN));

    assert_eq!(
        listed.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        vec![live]
    );
    assert_eq!(read(&db, move |conn| entry(conn, never_built)), None);
}

/// A row built and handed back between two reads of the observer looks
/// the same by id. It must still be reported, or nothing builds it again.
#[test]
fn a_row_handed_back_between_two_reads_is_reported_again() {
    let (_dir, db) = open_db();
    let mut waiting = observe_pending_submissions(&db);
    assert_eq!(next(&mut waiting), vec![]);
    let id = schedule_in(&db, DOMAIN, None);
    assert_eq!(next(&mut waiting).len(), 1);

    start_attempt_tagged(&db, id, 9);
    let attempt = read(&db, move |conn| entry(conn, id)).unwrap();
    write_verdict(
        &db,
        attempt,
        verdict(DurableTxStatus::PendingSubmission, None),
    );

    let reported_again = || {
        let emitted = waiting.next().now_or_never();
        matches!(emitted, Some(Some(Ok(rows))) if rows.len() == 1)
    };
    wait_until(reported_again, "the handed-back row is reported");
}
