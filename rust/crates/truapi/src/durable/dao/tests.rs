use futures::StreamExt;
use futures::executor::block_on;

use super::*;
use crate::durable::model::FailureKind;
use crate::durable::testing::{block, extrinsic, open_db};

mod submission;

const DOMAIN: DomainId = DomainId::from_static("test");

fn register(db: &Db, group: Option<GroupId>, tag: u8) -> DurableTxId {
    block_on(
        db.write(move |tx| insert(tx, &DOMAIN, group.as_ref(), &extrinsic(tag, 100, 64), None)),
    )
    .unwrap()
}

fn read<T: Send + 'static>(
    db: &Db,
    f: impl FnOnce(&Connection) -> Result<T, DbError> + Send + 'static,
) -> T {
    block_on(db.read(f)).unwrap()
}

fn write_verdict(db: &Db, observed: DurableTxEntry, verdict: Verdict) -> bool {
    block_on(db.write(move |tx| compare_and_set(tx, &observed, &verdict))).unwrap()
}

fn verdict(status: DurableTxStatus, success_detected_at: Option<HashAndNumber>) -> Verdict {
    Verdict {
        status,
        success_detected_at,
        failure: None,
    }
}

#[test]
fn an_inserted_row_reads_back_pending_with_its_attempt() {
    let (_dir, db) = open_db();
    let id = register(&db, Some(GroupId::new("payment")), 7);

    assert_eq!(
        read(&db, move |conn| entry(conn, id)),
        Some(DurableTxEntry {
            id,
            domain: DOMAIN,
            group: Some(GroupId::new("payment")),
            tx_hash: extrinsic(7, 100, 64).extrinsic.hash(),
            mortality: extrinsic(7, 100, 64).mortality,
            status: DurableTxStatus::Pending,
            success_detected_at: None,
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
        read(&db, |conn| Ok((
            entry(conn, DurableTxId(9))?,
            status(conn, DurableTxId(9))?
        ))),
        (None, None)
    );
}

#[test]
fn a_verdict_is_written_against_the_status_and_attempt_it_was_derived_from() {
    let (_dir, db) = open_db();
    let id = register(&db, None, 1);
    let observed = read(&db, move |conn| entry(conn, id)).unwrap();
    let at = block(120);

    let wrote = write_verdict(
        &db,
        observed.clone(),
        verdict(DurableTxStatus::PendingSuccess, Some(at)),
    );

    assert!(wrote);
    assert_eq!(
        read(&db, move |conn| entry(conn, id)).unwrap(),
        DurableTxEntry {
            status: DurableTxStatus::PendingSuccess,
            success_detected_at: Some(at),
            ..observed
        }
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
    write_verdict(
        &db,
        observed.clone(),
        verdict(DurableTxStatus::PendingSuccess, Some(block(120))),
    );

    let wrote = write_verdict(&db, observed, verdict(DurableTxStatus::Failure, None));

    assert!(!wrote);
    assert_eq!(
        read(&db, move |conn| status(conn, id)),
        Some(DurableTxStatus::PendingSuccess)
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

    assert!(!write_verdict(
        &db,
        observed,
        verdict(DurableTxStatus::FinalizedSuccess, None)
    ));
}

#[test]
fn a_terminal_row_is_never_rewritten() {
    let (_dir, db) = open_db();
    let id = register(&db, None, 1);
    let observed = read(&db, move |conn| entry(conn, id)).unwrap();
    write_verdict(
        &db,
        observed.clone(),
        Verdict {
            status: DurableTxStatus::Failure,
            success_detected_at: None,
            failure: Some(FailureKind::Expired),
        },
    );
    let terminal = DurableTxEntry {
        status: DurableTxStatus::Failure,
        ..observed
    };

    assert!(!write_verdict(
        &db,
        terminal,
        verdict(DurableTxStatus::FinalizedSuccess, None)
    ));
}

/// Two domains may pick the same group name, such as an operation id; a
/// group is only ever read within its domain.
#[test]
fn a_group_lists_its_own_domain_in_registration_order() {
    let (_dir, db) = open_db();
    let group_id = GroupId::new("payment");
    let first = register(&db, Some(group_id.clone()), 1);
    let second = register(&db, Some(group_id.clone()), 2);
    register(&db, None, 3);
    let other = group_id.clone();
    block_on(db.write(move |tx| {
        insert(
            tx,
            &DomainId::from_static("other"),
            Some(&other),
            &extrinsic(4, 100, 64),
            None,
        )
    }))
    .unwrap();

    assert_eq!(
        read(&db, move |conn| group(conn, &DOMAIN, &group_id)),
        vec![
            DurableTxState {
                id: first,
                status: DurableTxStatus::Pending
            },
            DurableTxState {
                id: second,
                status: DurableTxStatus::Pending
            },
        ]
    );
}

#[test]
fn domain_entries_list_every_row_of_the_domain() {
    let (_dir, db) = open_db();
    let ids = [register(&db, None, 1), register(&db, None, 2)];

    let listed = read(&db, |conn| domain_entries(conn, &DOMAIN));

    assert_eq!(listed.iter().map(|entry| entry.id).collect::<Vec<_>>(), ids);
}

#[test]
fn only_domains_with_a_transaction_awaiting_a_verdict_are_live() {
    let (_dir, db) = open_db();
    assert_eq!(read(&db, live_domains), Vec::<DomainId>::new());
    let id = register(&db, None, 1);
    assert_eq!(read(&db, live_domains), vec![DOMAIN]);

    let observed = read(&db, move |conn| entry(conn, id)).unwrap();
    write_verdict(
        &db,
        observed,
        verdict(DurableTxStatus::FinalizedSuccess, None),
    );

    assert_eq!(read(&db, live_domains), Vec::<DomainId>::new());
}

#[test]
fn observed_queries_follow_commits() {
    let (_dir, db) = open_db();
    let group_id = GroupId::new("payment");
    let mut live = observe_has_live(&db);
    let mut grouped = observe_group(&db, DOMAIN, group_id.clone());
    assert!(!next(&mut live));
    assert_eq!(next(&mut grouped), vec![]);

    let id = register(&db, Some(group_id), 1);
    let mut status = observe_status(&db, id);

    assert!(next(&mut live));
    assert_eq!(
        next(&mut grouped),
        vec![DurableTxState {
            id,
            status: DurableTxStatus::Pending
        }]
    );
    assert_eq!(next(&mut status), Some(DurableTxStatus::Pending));
}

fn next<T>(stream: &mut BoxStream<'static, Result<T, DbError>>) -> T {
    block_on(stream.next()).unwrap().unwrap()
}
