//! Queries over the `durable_tx` ledger, one SQL constant and one function
//! each.

use futures::stream::BoxStream;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Type, ValueRef};
use rusqlite::{Connection, Row, Transaction, named_params};
use subxt::utils::H256;

use super::model::{
    DomainId, DurableTxEntry, DurableTxId, DurableTxState, DurableTxStatus, GroupId, PolicyId,
    PolicyRef, ScheduledDurableTx, Verdict,
};
use crate::chain::{HashAndNumber, MortalExtrinsic, Mortality};
use crate::store::{Db, DbError};

const INSERT: &str = "INSERT INTO durable_tx
    (domain, group_id, tx_hash, birth_number, birth_hash, period, status, policy_id,
     policy_params)
    VALUES (:domain, :group_id, :tx_hash, :birth_number, :birth_hash, :period, 'PENDING',
     :policy_id, :policy_params)
    RETURNING id";

/// Records a presigned extrinsic as a `Pending` row, rebuilt by `policy` after
/// a failure it accepts to retry.
pub fn insert(
    tx: &Transaction<'_>,
    domain: &DomainId,
    group: Option<&GroupId>,
    extrinsic: &MortalExtrinsic,
    policy: Option<&PolicyRef>,
) -> Result<DurableTxId, DbError> {
    let birth = extrinsic.mortality.birth();
    let id = tx.prepare_cached(INSERT)?.query_row(
        named_params! {
            ":domain": domain.as_str(),
            ":group_id": group.map(GroupId::as_str),
            ":tx_hash": extrinsic.extrinsic.hash().0,
            ":birth_number": birth.number,
            ":birth_hash": birth.hash.0,
            ":period": extrinsic.mortality.period(),
            ":policy_id": policy.map(|policy| policy.id.as_str()),
            ":policy_params": policy.map(|policy| policy.params.as_slice()),
        },
        |row| row.get("id").map(DurableTxId),
    )?;
    Ok(id)
}

const SCHEDULE: &str = "INSERT INTO durable_tx
    (domain, group_id, status, policy_id, policy_params)
    VALUES (:domain, :group_id, 'PENDING_SUBMISSION', :policy_id, :policy_params)
    RETURNING id";

/// Records a transaction `policy` builds later, as a `PendingSubmission` row
/// with no attempt.
pub fn schedule(
    tx: &Transaction<'_>,
    domain: &DomainId,
    group: Option<&GroupId>,
    policy: &PolicyRef,
) -> Result<DurableTxId, DbError> {
    let id = tx.prepare_cached(SCHEDULE)?.query_row(
        named_params! {
            ":domain": domain.as_str(),
            ":group_id": group.map(GroupId::as_str),
            ":policy_id": policy.id.as_str(),
            ":policy_params": policy.params,
        },
        |row| row.get("id").map(DurableTxId),
    )?;
    Ok(id)
}

const START_ATTEMPT: &str = "UPDATE durable_tx
    SET status = 'PENDING', tx_hash = :tx_hash, birth_number = :birth_number,
    birth_hash = :birth_hash, period = :period, success_number = NULL, success_hash = NULL
    WHERE id = :id AND status = 'PENDING_SUBMISSION'";

/// Gives a waiting row the attempt `extrinsic` makes. Returns whether the row
/// was still waiting.
pub fn start_attempt(
    tx: &Transaction<'_>,
    id: DurableTxId,
    extrinsic: &MortalExtrinsic,
) -> Result<bool, DbError> {
    let birth = extrinsic.mortality.birth();
    let changed = tx.prepare_cached(START_ATTEMPT)?.execute(named_params! {
        ":tx_hash": extrinsic.extrinsic.hash().0,
        ":birth_number": birth.number,
        ":birth_hash": birth.hash.0,
        ":period": extrinsic.mortality.period(),
        ":id": id.0,
    })?;
    Ok(changed == 1)
}

const ABANDON_SUBMISSION: &str = "UPDATE durable_tx SET status = 'FAILURE'
    WHERE id = :id AND status = 'PENDING_SUBMISSION'";

/// Fails a row that is still waiting to be built. Returns whether it was.
pub fn abandon_submission(tx: &Transaction<'_>, id: DurableTxId) -> Result<bool, DbError> {
    let changed = tx
        .prepare_cached(ABANDON_SUBMISSION)?
        .execute(named_params! { ":id": id.0 })?;
    Ok(changed == 1)
}

const SUBMISSION_POLICY: &str = "SELECT policy_id, policy_params FROM durable_tx
    WHERE id = :id AND policy_id IS NOT NULL";

/// The policy that rebuilds the row with `id`, when it has one.
pub fn submission_policy(conn: &Connection, id: DurableTxId) -> Result<Option<PolicyRef>, DbError> {
    let mut stmt = conn.prepare_cached(SUBMISSION_POLICY)?;
    let mut rows = stmt.query(named_params! { ":id": id.0 })?;
    Ok(rows.next()?.map(PolicyRef::from_row).transpose()?)
}

// `tx_hash` is selected only so a row handed back with a new previous attempt
// differs from how it was last reported: an observer emits nothing for a
// result equal to its last one.
const PENDING_SUBMISSIONS: &str = "SELECT id, domain, group_id, policy_id, policy_params, tx_hash
    FROM durable_tx WHERE status = 'PENDING_SUBMISSION' ORDER BY id";

/// Every row waiting to be built, in registration order, re-read after every
/// commit that changes them.
pub fn observe_pending_submissions(
    db: &Db,
) -> BoxStream<'static, Result<Vec<ScheduledDurableTx>, DbError>> {
    db.observe(PENDING_SUBMISSIONS, |stmt| {
        stmt.query_map([], ScheduledDurableTx::from_row)
    })
}

const HAS_PENDING_SUBMISSIONS: &str = "SELECT EXISTS (SELECT 1 FROM durable_tx
    WHERE status = 'PENDING_SUBMISSION') AS waiting";

/// Whether any row is waiting to be built.
pub fn has_pending_submissions(conn: &Connection) -> Result<bool, DbError> {
    Ok(conn
        .prepare_cached(HAS_PENDING_SUBMISSIONS)?
        .query_row([], |row| row.get("waiting"))?)
}

const BUCKET_SUBMISSIONS: &str = "SELECT id, domain, group_id, policy_id, policy_params
    FROM durable_tx WHERE status = 'PENDING_SUBMISSION' AND domain = :domain
    AND policy_id = :policy_id AND group_id IS :group_id ORDER BY id";

/// The rows of one domain, policy and group waiting to be built, in
/// registration order.
pub fn bucket_submissions(
    conn: &Connection,
    domain: &DomainId,
    policy: &PolicyId,
    group: Option<&GroupId>,
) -> Result<Vec<ScheduledDurableTx>, DbError> {
    Ok(conn
        .prepare_cached(BUCKET_SUBMISSIONS)?
        .query_map(
            named_params! {
                ":domain": domain.as_str(),
                ":policy_id": policy.as_str(),
                ":group_id": group.map(GroupId::as_str),
            },
            ScheduledDurableTx::from_row,
        )?
        .collect::<rusqlite::Result<_>>()?)
}

const COMPARE_AND_SET: &str = "UPDATE durable_tx
    SET status = :status, success_number = :success_number, success_hash = :success_hash
    WHERE id = :id AND status = :expected AND tx_hash = :expected_hash
    AND status IN ('PENDING', 'PENDING_SUCCESS')";

/// Writes `verdict` only while the row still awaits a verdict in the status
/// and attempt `observed` holds. Returns whether it wrote.
pub fn compare_and_set(
    tx: &Transaction<'_>,
    observed: &DurableTxEntry,
    verdict: &Verdict,
) -> Result<bool, DbError> {
    let success = verdict.success_detected_at;
    let changed = tx.prepare_cached(COMPARE_AND_SET)?.execute(named_params! {
        ":status": verdict.status,
        ":success_number": success.map(|block| block.number),
        ":success_hash": success.map(|block| block.hash.0),
        ":id": observed.id.0,
        ":expected": observed.status,
        ":expected_hash": observed.tx_hash.0,
    })?;
    Ok(changed == 1)
}

const ENTRY: &str = "SELECT * FROM durable_tx
    WHERE id = :id AND status != 'PENDING_SUBMISSION' AND tx_hash IS NOT NULL";

/// The row with `id`, while it holds a submitted attempt.
pub fn entry(conn: &Connection, id: DurableTxId) -> Result<Option<DurableTxEntry>, DbError> {
    let mut stmt = conn.prepare_cached(ENTRY)?;
    let mut rows = stmt.query(named_params! { ":id": id.0 })?;
    Ok(rows.next()?.map(DurableTxEntry::from_row).transpose()?)
}

const STATUS: &str = "SELECT status FROM durable_tx WHERE id = :id";

/// The status of the row with `id`.
pub fn status(conn: &Connection, id: DurableTxId) -> Result<Option<DurableTxStatus>, DbError> {
    let mut stmt = conn.prepare_cached(STATUS)?;
    let mut rows = stmt.query(named_params! { ":id": id.0 })?;
    Ok(rows.next()?.map(|row| row.get("status")).transpose()?)
}

/// [`status`], re-read after every commit that changes it.
pub fn observe_status(
    db: &Db,
    id: DurableTxId,
) -> BoxStream<'static, Result<Option<DurableTxStatus>, DbError>> {
    db.observe(STATUS, move |stmt| {
        stmt.query_optional(named_params! { ":id": id.0 }, |row| row.get("status"))
    })
}

const GROUP: &str = "SELECT id, status FROM durable_tx
    WHERE domain = :domain AND group_id = :group_id ORDER BY id";

/// The transactions of one group, in registration order.
pub fn group(
    conn: &Connection,
    domain: &DomainId,
    group: &GroupId,
) -> Result<Vec<DurableTxState>, DbError> {
    Ok(conn
        .prepare_cached(GROUP)?
        .query_map(
            named_params! { ":domain": domain.as_str(), ":group_id": group.as_str() },
            DurableTxState::from_row,
        )?
        .collect::<rusqlite::Result<_>>()?)
}

/// [`group`], re-read after every commit that changes it.
pub fn observe_group(
    db: &Db,
    domain: DomainId,
    group: GroupId,
) -> BoxStream<'static, Result<Vec<DurableTxState>, DbError>> {
    db.observe(GROUP, move |stmt| {
        stmt.query_map(
            named_params! { ":domain": domain.as_str(), ":group_id": group.as_str() },
            DurableTxState::from_row,
        )
    })
}

const DOMAIN_ENTRIES: &str = "SELECT * FROM durable_tx
    WHERE domain = :domain AND status != 'PENDING_SUBMISSION' AND tx_hash IS NOT NULL
    ORDER BY id";

/// Every row of one domain with a submitted attempt, in registration order.
pub fn domain_entries(
    conn: &Connection,
    domain: &DomainId,
) -> Result<Vec<DurableTxEntry>, DbError> {
    Ok(conn
        .prepare_cached(DOMAIN_ENTRIES)?
        .query_map(
            named_params! { ":domain": domain.as_str() },
            DurableTxEntry::from_row,
        )?
        .collect::<rusqlite::Result<_>>()?)
}

const LIVE_DOMAINS: &str = "SELECT DISTINCT domain FROM durable_tx
    WHERE status IN ('PENDING', 'PENDING_SUCCESS') ORDER BY domain";

/// Every domain with a transaction still awaiting a verdict.
pub fn live_domains(conn: &Connection) -> Result<Vec<DomainId>, DbError> {
    Ok(conn
        .prepare_cached(LIVE_DOMAINS)?
        .query_map([], |row| row.get::<_, String>("domain").map(DomainId::new))?
        .collect::<rusqlite::Result<_>>()?)
}

const HAS_LIVE: &str = "SELECT EXISTS (SELECT 1 FROM durable_tx
    WHERE status IN ('PENDING_SUBMISSION', 'PENDING', 'PENDING_SUCCESS')) AS live";

/// Whether any transaction is waiting to be built or awaits a verdict,
/// re-read after every commit that changes it.
pub fn observe_has_live(db: &Db) -> BoxStream<'static, Result<bool, DbError>> {
    db.observe(HAS_LIVE, |stmt| stmt.query_row([], |row| row.get("live")))
}

impl DurableTxEntry {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: DurableTxId(row.get("id")?),
            domain: DomainId::new(row.get::<_, String>("domain")?),
            group: row.get::<_, Option<String>>("group_id")?.map(GroupId::new),
            tx_hash: H256(row.get("tx_hash")?),
            mortality: mortality_from_row(row)?,
            status: row.get("status")?,
            success_detected_at: success_from_row(row)?,
        })
    }
}

/// The era of the row's attempt, checked as [`Mortality::new`] checks it.
fn mortality_from_row(row: &Row<'_>) -> rusqlite::Result<Mortality> {
    let birth = HashAndNumber {
        hash: H256(row.get("birth_hash")?),
        number: row.get("birth_number")?,
    };
    Mortality::new(birth, row.get("period")?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, Type::Integer, Box::new(error))
    })
}

/// The block the row's success was recorded at, when it has one.
fn success_from_row(row: &Row<'_>) -> rusqlite::Result<Option<HashAndNumber>> {
    let number: Option<u64> = row.get("success_number")?;
    let hash: Option<[u8; 32]> = row.get("success_hash")?;
    Ok(number.zip(hash).map(|(number, hash)| HashAndNumber {
        hash: H256(hash),
        number,
    }))
}

impl ScheduledDurableTx {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: DurableTxId(row.get("id")?),
            domain: DomainId::new(row.get::<_, String>("domain")?),
            group: row.get::<_, Option<String>>("group_id")?.map(GroupId::new),
            policy: PolicyRef::from_row(row)?,
        })
    }
}

impl PolicyRef {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: PolicyId::new(row.get::<_, String>("policy_id")?),
            params: row.get("policy_params")?,
        })
    }
}

impl DurableTxState {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: DurableTxId(row.get("id")?),
            status: row.get("status")?,
        })
    }
}

impl ToSql for DurableTxStatus {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(match self {
            Self::PendingSubmission => "PENDING_SUBMISSION",
            Self::Pending => "PENDING",
            Self::PendingSuccess => "PENDING_SUCCESS",
            Self::FinalizedSuccess => "FINALIZED_SUCCESS",
            Self::Failure => "FAILURE",
        }))
    }
}

impl FromSql for DurableTxStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "PENDING_SUBMISSION" => Ok(Self::PendingSubmission),
            "PENDING" => Ok(Self::Pending),
            "PENDING_SUCCESS" => Ok(Self::PendingSuccess),
            "FINALIZED_SUCCESS" => Ok(Self::FinalizedSuccess),
            "FAILURE" => Ok(Self::Failure),
            other => Err(FromSqlError::Other(
                format!(
                    "unknown durable status {other:?}, expected PENDING_SUBMISSION, PENDING, \
                     PENDING_SUCCESS, FINALIZED_SUCCESS or FAILURE"
                )
                .into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;
    use futures::{FutureExt, StreamExt};

    use super::*;
    use crate::durable::model::FailureKind;
    use crate::durable::testing::{block, extrinsic, open_db};

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

    const POLICY: PolicyId = PolicyId::from_static("rebuild");

    fn policy_ref(params: u8) -> PolicyRef {
        PolicyRef {
            id: POLICY,
            params: vec![params],
        }
    }

    fn schedule_in(db: &Db, domain: DomainId, group: Option<GroupId>) -> DurableTxId {
        block_on(db.write(move |tx| schedule(tx, &domain, group.as_ref(), &policy_ref(1)))).unwrap()
    }

    fn pending(db: &Db) -> Vec<ScheduledDurableTx> {
        block_on(observe_pending_submissions(db).next())
            .unwrap()
            .unwrap()
    }

    fn start(db: &Db, id: DurableTxId, tag: u8) -> bool {
        block_on(db.write(move |tx| start_attempt(tx, id, &extrinsic(tag, 110, 32)))).unwrap()
    }

    fn abandon(db: &Db, id: DurableTxId) -> bool {
        block_on(db.write(move |tx| abandon_submission(tx, id))).unwrap()
    }

    #[test]
    fn a_scheduled_row_waits_for_its_policy_with_no_attempt() {
        let (_dir, db) = open_db();
        let id = schedule_in(&db, DOMAIN, Some(GroupId::new("payment")));

        assert_eq!(
            pending(&db),
            vec![ScheduledDurableTx {
                id,
                domain: DOMAIN,
                group: Some(GroupId::new("payment")),
                policy: policy_ref(1),
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

        assert!(start(&db, id, 9));
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
        assert_eq!(pending(&db), vec![]);
    }

    /// Two builds of one row must not both become attempts: only the first
    /// finds it still waiting.
    #[test]
    fn an_attempt_starts_only_on_a_row_still_waiting() {
        let (_dir, db) = open_db();
        let waiting = schedule_in(&db, DOMAIN, None);
        let submitted = register(&db, None, 1);
        start(&db, waiting, 9);

        assert!(!start(&db, waiting, 10));
        assert!(!start(&db, submitted, 10));
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

        assert!(abandon(&db, waiting));
        assert!(!abandon(&db, waiting));
        assert!(!abandon(&db, submitted));
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
        let id = block_on(db.write(|tx| {
            insert(
                tx,
                &DOMAIN,
                None,
                &extrinsic(1, 100, 64),
                Some(&policy_ref(3)),
            )
        }))
        .unwrap();
        let observed = read(&db, move |conn| entry(conn, id)).unwrap();

        assert!(write_verdict(
            &db,
            observed,
            verdict(DurableTxStatus::PendingSubmission, None)
        ));
        assert_eq!(
            pending(&db),
            vec![ScheduledDurableTx {
                id,
                domain: DOMAIN,
                group: None,
                policy: policy_ref(3),
            }]
        );
    }

    #[test]
    fn a_row_names_the_policy_it_was_registered_with() {
        let (_dir, db) = open_db();
        let with_policy = block_on(db.write(|tx| {
            insert(
                tx,
                &DOMAIN,
                None,
                &extrinsic(1, 100, 64),
                Some(&policy_ref(3)),
            )
        }))
        .unwrap();
        let without = register(&db, None, 2);

        assert_eq!(
            read(&db, move |conn| submission_policy(conn, with_policy)),
            Some(policy_ref(3))
        );
        assert_eq!(
            read(&db, move |conn| submission_policy(conn, without)),
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
                bucket_submissions(conn, &DOMAIN, &POLICY, group.as_ref())
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
        abandon(&db, never_built);
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

        start(&db, id, 9);
        let attempt = read(&db, move |conn| entry(conn, id)).unwrap();
        write_verdict(
            &db,
            attempt,
            verdict(DurableTxStatus::PendingSubmission, None),
        );

        crate::test_support::wait_until(
            || matches!(waiting.next().now_or_never(), Some(Some(Ok(rows))) if rows.len() == 1),
            "the handed-back row is reported",
        );
    }
}
