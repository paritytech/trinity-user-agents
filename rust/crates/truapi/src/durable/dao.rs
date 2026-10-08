//! Queries over the `durable_tx` ledger, one SQL constant and one function
//! each.

use futures::stream::BoxStream;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Type, ValueRef};
use rusqlite::{Connection, Row, Transaction, named_params};
use subxt::utils::H256;

use super::model::{
    DomainId, DurableTxEntry, DurableTxId, DurableTxState, DurableTxStatus, GroupId, PolicyId,
    PolicyParams,
    SubmissionPlan, ScheduledDurableTx, Verdict,
};
use crate::chain::{HashAndNumber, MortalExtrinsic, Mortality};
use crate::store::{Db, DbError};

const INSERT: &str = "INSERT INTO durable_tx
    (domain, group_id, tx_hash, birth_number, birth_hash, period, status, policy_id,
     policy_params)
    VALUES (:domain, :group_id, :tx_hash, :birth_number, :birth_hash, :period, 'PENDING',
     :policy_id, :policy_params)
    RETURNING id";

/// Records a presigned extrinsic as a `Pending` row, rebuilt through `plan`
/// after a failure its policy accepts to retry.
pub fn insert(
    tx: &Transaction<'_>,
    domain: &DomainId,
    group: Option<&GroupId>,
    extrinsic: &MortalExtrinsic,
    plan: Option<&SubmissionPlan>,
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
            ":policy_id": plan.map(|plan| plan.policy.as_str()),
            ":policy_params": plan.map(|plan| &plan.params),
        },
        |row| row.get("id").map(DurableTxId),
    )?;
    Ok(id)
}

const SCHEDULE: &str = "INSERT INTO durable_tx
    (domain, group_id, status, policy_id, policy_params)
    VALUES (:domain, :group_id, 'PENDING_SUBMISSION', :policy_id, :policy_params)
    RETURNING id";

/// Records a transaction built later through `plan`, as a
/// `PendingSubmission` row with no attempt.
pub fn schedule(
    tx: &Transaction<'_>,
    domain: &DomainId,
    group: Option<&GroupId>,
    plan: &SubmissionPlan,
) -> Result<DurableTxId, DbError> {
    let id = tx.prepare_cached(SCHEDULE)?.query_row(
        named_params! {
            ":domain": domain.as_str(),
            ":group_id": group.map(GroupId::as_str),
            ":policy_id": plan.policy.as_str(),
            ":policy_params": plan.params,
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

const SUBMISSION_PLAN: &str = "SELECT policy_id, policy_params FROM durable_tx
    WHERE id = :id AND policy_id IS NOT NULL";

/// The plan that rebuilds the row with `id`, when it has one.
pub fn submission_plan(conn: &Connection, id: DurableTxId) -> Result<Option<SubmissionPlan>, DbError> {
    let mut stmt = conn.prepare_cached(SUBMISSION_PLAN)?;
    let mut rows = stmt.query(named_params! { ":id": id.0 })?;
    Ok(rows.next()?.map(SubmissionPlan::from_row).transpose()?)
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
    let mut stmt = conn.prepare_cached(HAS_PENDING_SUBMISSIONS)?;
    let waiting = stmt.query_row([], |row| row.get("waiting"))?;
    Ok(waiting)
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
    let mut stmt = conn.prepare_cached(BUCKET_SUBMISSIONS)?;
    let params = named_params! {
        ":domain": domain.as_str(),
        ":policy_id": policy.as_str(),
        ":group_id": group.map(GroupId::as_str),
    };
    let rows = stmt.query_map(params, ScheduledDurableTx::from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
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

/// Rows holding a submitted attempt: neither waiting to be built nor given
/// up before they ever were.
macro_rules! submitted {
    () => {
        "status != 'PENDING_SUBMISSION' AND tx_hash IS NOT NULL"
    };
}

const ENTRY: &str = concat!("SELECT * FROM durable_tx WHERE id = :id AND ", submitted!());

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

const DOMAIN_ENTRIES: &str = concat!(
    "SELECT * FROM durable_tx WHERE domain = :domain AND ",
    submitted!(),
    " ORDER BY id"
);

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
            domain: domain_from_row(row)?,
            group: group_from_row(row)?,
            tx_hash: H256(row.get("tx_hash")?),
            mortality: mortality_from_row(row)?,
            status: row.get("status")?,
            success_detected_at: success_from_row(row)?,
        })
    }
}

fn domain_from_row(row: &Row<'_>) -> rusqlite::Result<DomainId> {
    let name: String = row.get("domain")?;
    Ok(DomainId::new(name))
}

fn group_from_row(row: &Row<'_>) -> rusqlite::Result<Option<GroupId>> {
    let name: Option<String> = row.get("group_id")?;
    Ok(name.map(GroupId::new))
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
            domain: domain_from_row(row)?,
            group: group_from_row(row)?,
            plan: SubmissionPlan::from_row(row)?,
        })
    }
}

impl SubmissionPlan {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            policy: PolicyId::new(row.get::<_, String>("policy_id")?),
            params: PolicyParams(row.get("policy_params")?),
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

impl ToSql for PolicyParams {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        self.0.to_sql()
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
mod tests;
