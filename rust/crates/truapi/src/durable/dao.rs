//! Queries over the `durable_tx` ledger, one SQL constant and one function
//! each.

use futures::stream::BoxStream;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Type, ValueRef};
use rusqlite::{Connection, Row, Transaction, named_params};
use subxt::utils::H256;

use super::model::{
    DomainId, DurableTxEntry, DurableTxId, DurableTxState, DurableTxStatus, GroupId, Verdict,
};
use crate::chain::{HashAndNumber, MortalExtrinsic, Mortality};
use crate::store::{Db, DbError};

const INSERT: &str = "INSERT INTO durable_tx
    (domain, group_id, tx_hash, birth_number, birth_hash, period, status)
    VALUES (:domain, :group_id, :tx_hash, :birth_number, :birth_hash, :period, 'PENDING')
    RETURNING id";

/// Records a presigned extrinsic as a `Pending` row.
pub fn insert(
    tx: &Transaction<'_>,
    domain: &DomainId,
    group: Option<&GroupId>,
    extrinsic: &MortalExtrinsic,
) -> rusqlite::Result<DurableTxId> {
    let birth = extrinsic.mortality.birth();
    tx.prepare_cached(INSERT)?.query_row(
        named_params! {
            ":domain": domain.as_str(),
            ":group_id": group.map(GroupId::as_str),
            ":tx_hash": extrinsic.extrinsic.hash().0,
            ":birth_number": birth.number,
            ":birth_hash": birth.hash.0,
            ":period": extrinsic.mortality.period(),
        },
        |row| row.get("id").map(DurableTxId),
    )
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
) -> rusqlite::Result<bool> {
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

const ENTRY: &str = "SELECT * FROM durable_tx WHERE id = :id";

/// The row with `id`.
pub fn entry(conn: &Connection, id: DurableTxId) -> rusqlite::Result<Option<DurableTxEntry>> {
    let mut stmt = conn.prepare_cached(ENTRY)?;
    let mut rows = stmt.query(named_params! { ":id": id.0 })?;
    rows.next()?.map(DurableTxEntry::from_row).transpose()
}

const STATUS: &str = "SELECT status FROM durable_tx WHERE id = :id";

/// The status of the row with `id`.
pub fn status(conn: &Connection, id: DurableTxId) -> rusqlite::Result<Option<DurableTxStatus>> {
    let mut stmt = conn.prepare_cached(STATUS)?;
    let mut rows = stmt.query(named_params! { ":id": id.0 })?;
    rows.next()?.map(|row| row.get("status")).transpose()
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
) -> rusqlite::Result<Vec<DurableTxState>> {
    conn.prepare_cached(GROUP)?
        .query_map(
            named_params! { ":domain": domain.as_str(), ":group_id": group.as_str() },
            DurableTxState::from_row,
        )?
        .collect()
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

const DOMAIN_ENTRIES: &str = "SELECT * FROM durable_tx WHERE domain = :domain ORDER BY id";

/// Every row of one domain, in registration order.
pub fn domain_entries(
    conn: &Connection,
    domain: &DomainId,
) -> rusqlite::Result<Vec<DurableTxEntry>> {
    conn.prepare_cached(DOMAIN_ENTRIES)?
        .query_map(
            named_params! { ":domain": domain.as_str() },
            DurableTxEntry::from_row,
        )?
        .collect()
}

const LIVE_DOMAINS: &str = "SELECT DISTINCT domain FROM durable_tx
    WHERE status IN ('PENDING', 'PENDING_SUCCESS') ORDER BY domain";

/// Every domain with a transaction still awaiting a verdict.
pub fn live_domains(conn: &Connection) -> rusqlite::Result<Vec<DomainId>> {
    conn.prepare_cached(LIVE_DOMAINS)?
        .query_map([], |row| row.get::<_, String>("domain").map(DomainId::new))?
        .collect()
}

const HAS_LIVE: &str = "SELECT EXISTS (SELECT 1 FROM durable_tx
    WHERE status IN ('PENDING', 'PENDING_SUCCESS')) AS live";

/// Whether any transaction still awaits a verdict, re-read after every
/// commit that changes it.
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
            "PENDING" => Ok(Self::Pending),
            "PENDING_SUCCESS" => Ok(Self::PendingSuccess),
            "FINALIZED_SUCCESS" => Ok(Self::FinalizedSuccess),
            "FAILURE" => Ok(Self::Failure),
            other => Err(FromSqlError::Other(
                format!(
                    "unknown durable status {other:?}, expected PENDING, PENDING_SUCCESS, \
                     FINALIZED_SUCCESS or FAILURE"
                )
                .into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use futures::executor::block_on;

    use super::*;
    use crate::durable::model::FailureKind;
    use crate::durable::testing::{block, extrinsic, open_db};

    const DOMAIN: DomainId = DomainId::from_static("test");

    fn register(db: &Db, group: Option<GroupId>, tag: u8) -> DurableTxId {
        block_on(db.write(move |tx| {
            Ok(insert(
                tx,
                &DOMAIN,
                group.as_ref(),
                &extrinsic(tag, 100, 64),
            )?)
        }))
        .unwrap()
    }

    fn read<T: Send + 'static>(
        db: &Db,
        f: impl FnOnce(&Connection) -> rusqlite::Result<T> + Send + 'static,
    ) -> T {
        block_on(db.read(move |conn| Ok(f(conn)?))).unwrap()
    }

    fn write_verdict(db: &Db, observed: DurableTxEntry, verdict: Verdict) -> bool {
        block_on(db.write(move |tx| Ok(compare_and_set(tx, &observed, &verdict)?))).unwrap()
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

        assert_eq!(
            (wrote, read(&db, move |conn| entry(conn, id)).unwrap()),
            (
                true,
                DurableTxEntry {
                    status: DurableTxStatus::PendingSuccess,
                    success_detected_at: Some(at),
                    ..observed
                }
            )
        );
    }

    /// iOS: `updateTxStatus writes only while the observed status still holds`.
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

        assert_eq!(
            (wrote, read(&db, move |conn| status(conn, id))),
            (false, Some(DurableTxStatus::PendingSuccess))
        );
    }

    /// Android: `a verdict is written only against the attempt it was derived from`.
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

    /// iOS: `updateTxStatus does not overwrite a terminal entry`.
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

    /// iOS: `Group entries are scoped to their domain`.
    #[test]
    fn a_group_lists_its_own_domain_in_registration_order() {
        let (_dir, db) = open_db();
        let group_id = GroupId::new("payment");
        let first = register(&db, Some(group_id.clone()), 1);
        let second = register(&db, Some(group_id.clone()), 2);
        register(&db, None, 3);
        let other = group_id.clone();
        block_on(db.write(move |tx| {
            Ok(insert(
                tx,
                &DomainId::from_static("other"),
                Some(&other),
                &extrinsic(4, 100, 64),
            )?)
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
        assert_eq!((next(&mut live), next(&mut grouped)), (false, vec![]));

        let id = register(&db, Some(group_id), 1);
        let mut status = observe_status(&db, id);

        assert_eq!(
            (next(&mut live), next(&mut grouped), next(&mut status)),
            (
                true,
                vec![DurableTxState {
                    id,
                    status: DurableTxStatus::Pending
                }],
                Some(DurableTxStatus::Pending)
            )
        );
    }

    fn next<T>(stream: &mut BoxStream<'static, Result<T, DbError>>) -> T {
        block_on(stream.next()).unwrap().unwrap()
    }

    /// The table carries step 4's status already; until that code exists a
    /// row in it is reported rather than misread.
    #[test]
    fn a_status_the_engine_does_not_model_is_a_read_error() {
        let (_dir, db) = open_db();
        let id = register(&db, None, 1);
        block_on(db.write(move |tx| {
            tx.execute(
                "UPDATE durable_tx SET status = 'PENDING_SUBMISSION' WHERE id = ?1",
                [id.0],
            )?;
            Ok(())
        }))
        .unwrap();

        assert!(block_on(db.read(move |conn| Ok(status(conn, id)?))).is_err());
    }
}
