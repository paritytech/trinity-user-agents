//! Queries over the `durable_tx` ledger, one SQL constant and one function
//! each.

use futures::stream::BoxStream;
use rusqlite::{
	Connection, Row, Transaction, named_params,
	types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Type, ValueRef},
};
use subxt::utils::H256;

use super::model::{
	DomainId, DurableTxEntry, DurableTxFailureKind, DurableTxId, DurableTxState, DurableTxStatus,
	GroupId, PolicyParams, ScheduledDurableTx, SubmissionPlan,
};
use crate::{
	chain::{HashAndNumber, MortalExtrinsic, Mortality},
	store::{Db, DbError},
};

const INSERT: &str = "INSERT INTO durable_tx
    (domain, group_id, tx_hash, birth_number, birth_hash, period, status, policy_params)
    VALUES (:domain, :group_id, :tx_hash, :birth_number, :birth_hash, :period, 'PENDING',
     :policy_params)
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
			":policy_params": plan.map(|plan| &plan.params),
		},
		|row| row.get("id").map(DurableTxId),
	)?;
	Ok(id)
}

const SCHEDULE: &str = "INSERT INTO durable_tx
    (domain, group_id, status, policy_params)
    VALUES (:domain, :group_id, 'PENDING_SUBMISSION', :policy_params)
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
			":policy_params": plan.params,
		},
		|row| row.get("id").map(DurableTxId),
	)?;
	Ok(id)
}

const START_ATTEMPT: &str = "UPDATE durable_tx
    SET status = 'PENDING', tx_hash = :tx_hash, birth_number = :birth_number,
    birth_hash = :birth_hash, period = :period, success_number = NULL, success_hash = NULL,
    scanned_number = NULL, attempts = attempts + 1,
    policy_params = COALESCE(:policy_params, policy_params)
    WHERE id = :id AND status = 'PENDING_SUBMISSION'";

/// Gives a waiting row the attempt `extrinsic` makes, and `params` in place of
/// its policy params when given. Returns whether the row was still waiting.
pub fn start_attempt(
	tx: &Transaction<'_>,
	id: DurableTxId,
	extrinsic: &MortalExtrinsic,
	params: Option<&PolicyParams>,
) -> Result<bool, DbError> {
	let birth = extrinsic.mortality.birth();
	let changed = tx.prepare_cached(START_ATTEMPT)?.execute(named_params! {
		":tx_hash": extrinsic.extrinsic.hash().0,
		":birth_number": birth.number,
		":birth_hash": birth.hash.0,
		":period": extrinsic.mortality.period(),
		":policy_params": params,
		":id": id.0,
	})?;
	Ok(changed == 1)
}

const ABANDON_SUBMISSION: &str = "UPDATE durable_tx
    SET status = 'FAILURE', failure_kind = 'ABANDONED'
    WHERE id = :id AND status = 'PENDING_SUBMISSION'";

/// Fails a row that is still waiting to be built. Returns whether it was.
pub fn abandon_submission(tx: &Transaction<'_>, id: DurableTxId) -> Result<bool, DbError> {
	let changed = tx.prepare_cached(ABANDON_SUBMISSION)?.execute(named_params! { ":id": id.0 })?;
	Ok(changed == 1)
}

const SUBMISSION_PLAN: &str = "SELECT policy_params FROM durable_tx
    WHERE id = :id AND policy_params IS NOT NULL";

/// The plan that rebuilds the row with `id`, when it has one.
pub fn submission_plan(
	conn: &Connection,
	id: DurableTxId,
) -> Result<Option<SubmissionPlan>, DbError> {
	let mut stmt = conn.prepare_cached(SUBMISSION_PLAN)?;
	let mut rows = stmt.query(named_params! { ":id": id.0 })?;
	Ok(rows.next()?.map(SubmissionPlan::from_row).transpose()?)
}

const PENDING_SUBMISSIONS: &str = "SELECT id, domain, group_id, policy_params, attempts
    FROM durable_tx WHERE status = 'PENDING_SUBMISSION' ORDER BY id";

/// Every row waiting to be built, in registration order, re-read after every
/// commit that changes them.
pub fn observe_pending_submissions(
	db: &Db,
) -> BoxStream<'static, Result<Vec<ScheduledDurableTx>, DbError>> {
	db.observe(PENDING_SUBMISSIONS, |stmt| stmt.query_map([], ScheduledDurableTx::from_row))
}

const WAITING_DOMAINS: &str = "SELECT DISTINCT domain FROM durable_tx
    WHERE status = 'PENDING_SUBMISSION'";

/// The domain of every row waiting to be built.
pub fn waiting_domains(conn: &Connection) -> Result<Vec<DomainId>, DbError> {
	let mut stmt = conn.prepare_cached(WAITING_DOMAINS)?;
	let domains = stmt.query_map([], domain_from_row)?;
	Ok(domains.collect::<rusqlite::Result<_>>()?)
}

/// [`waiting_domains`], re-read after every commit that changes it.
pub fn observe_waiting_domains(db: &Db) -> BoxStream<'static, Result<Vec<DomainId>, DbError>> {
	db.observe(WAITING_DOMAINS, |stmt| stmt.query_map([], domain_from_row))
}

const BUCKET_SUBMISSIONS: &str = "SELECT id, domain, group_id, policy_params, attempts
    FROM durable_tx WHERE status = 'PENDING_SUBMISSION' AND domain = :domain
    AND group_id IS :group_id ORDER BY id";

/// The rows of one domain and group waiting to be built, in registration
/// order.
pub fn bucket_submissions(
	conn: &Connection,
	domain: &DomainId,
	group: Option<&GroupId>,
) -> Result<Vec<ScheduledDurableTx>, DbError> {
	let mut stmt = conn.prepare_cached(BUCKET_SUBMISSIONS)?;
	let params = named_params! {
		":domain": domain.as_str(),
		":group_id": group.map(GroupId::as_str),
	};
	let rows = stmt.query_map(params, ScheduledDurableTx::from_row)?;
	Ok(rows.collect::<rusqlite::Result<_>>()?)
}

const COMPARE_AND_SET: &str = "UPDATE durable_tx
    SET status = :status, success_number = :success_number, success_hash = :success_hash,
        failure_kind = :failure_kind, scanned_number = COALESCE(:scanned_number, scanned_number)
    WHERE id = :id AND status = :expected AND tx_hash = :expected_hash
    AND status IN ('PENDING', 'PENDING_SUCCESS')";

/// Writes `status`, and `scanned_to` unless it is `None`, only while the row
/// still awaits a verdict in the status and attempt `observed` holds.
/// Returns whether it wrote.
pub fn compare_and_set(
	tx: &Transaction<'_>,
	observed: &DurableTxEntry,
	status: DurableTxStatus,
	scanned_to: Option<u64>,
) -> Result<bool, DbError> {
	let success = status.success_block();
	let changed = tx.prepare_cached(COMPARE_AND_SET)?.execute(named_params! {
		":status": status_name(status),
		":success_number": success.map(|block| block.number),
		":success_hash": success.map(|block| block.hash.0),
		":failure_kind": failure_kind(status),
		":scanned_number": scanned_to,
		":id": observed.id.0,
		":expected": status_name(observed.status),
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

/// The columns [`status_from_row`] reads.
macro_rules! status_columns {
	() => {
		"status, success_number, success_hash, failure_kind"
	};
}

const STATUS: &str = concat!("SELECT ", status_columns!(), " FROM durable_tx WHERE id = :id");

/// The status of the row with `id`.
pub fn status(conn: &Connection, id: DurableTxId) -> Result<Option<DurableTxStatus>, DbError> {
	let mut stmt = conn.prepare_cached(STATUS)?;
	let mut rows = stmt.query(named_params! { ":id": id.0 })?;
	Ok(rows.next()?.map(status_from_row).transpose()?)
}

/// [`status`], re-read after every commit that changes it.
pub fn observe_status(
	db: &Db,
	id: DurableTxId,
) -> BoxStream<'static, Result<Option<DurableTxStatus>, DbError>> {
	db.observe(STATUS, move |stmt| {
		stmt.query_optional(named_params! { ":id": id.0 }, status_from_row)
	})
}

const GROUP: &str = concat!(
	"SELECT id, tx_hash, policy_params, ",
	status_columns!(),
	" FROM durable_tx WHERE domain = :domain AND group_id = :group_id ORDER BY id"
);

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

const GROUP_EXISTS: &str =
	"SELECT EXISTS (SELECT 1 FROM durable_tx WHERE domain = :domain AND group_id = :group_id)";

/// Whether any transaction of `domain` belongs to `group`.
pub fn group_exists(
	conn: &Connection,
	domain: &DomainId,
	group: &GroupId,
) -> Result<bool, DbError> {
	let params = named_params! { ":domain": domain.as_str(), ":group_id": group.as_str() };
	Ok(conn.prepare_cached(GROUP_EXISTS)?.query_row(params, |row| row.get(0))?)
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

const UNDECIDED_ENTRIES: &str = "SELECT * FROM durable_tx
    WHERE domain = :domain AND status IN ('PENDING', 'PENDING_SUCCESS') ORDER BY id";

/// Every row of one domain awaiting a verdict, in registration order.
pub fn undecided_entries(
	conn: &Connection,
	domain: &DomainId,
) -> Result<Vec<DurableTxEntry>, DbError> {
	Ok(conn
		.prepare_cached(UNDECIDED_ENTRIES)?
		.query_map(named_params! { ":domain": domain.as_str() }, DurableTxEntry::from_row)?
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

/// The statuses of a live transaction, as an SQL list: one still waiting to
/// be built or awaiting a verdict. Other domains' queries use it to treat a
/// transaction as live exactly as the ledger does.
macro_rules! live_statuses {
	() => {
		"('PENDING_SUBMISSION', 'PENDING', 'PENDING_SUCCESS')"
	};
}

const LIVE_TRANSACTION_DOMAINS: &str =
	concat!("SELECT DISTINCT domain FROM durable_tx WHERE status IN ", live_statuses!());

/// The domain of every live transaction, re-read after every commit that
/// changes them.
pub fn observe_live_transaction_domains(
	db: &Db,
) -> BoxStream<'static, Result<Vec<DomainId>, DbError>> {
	db.observe(LIVE_TRANSACTION_DOMAINS, |stmt| stmt.query_map([], domain_from_row))
}

impl DurableTxEntry {
	fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
		Ok(Self {
			id: DurableTxId(row.get("id")?),
			domain: domain_from_row(row)?,
			group: group_from_row(row)?,
			tx_hash: H256(row.get("tx_hash")?),
			mortality: mortality_from_row(row)?,
			status: status_from_row(row)?,
			scanned_to: row.get("scanned_number")?,
			plan: plan_from_row(row)?,
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
	let birth =
		HashAndNumber { hash: H256(row.get("birth_hash")?), number: row.get("birth_number")? };
	Mortality::new(birth, row.get("period")?).map_err(|error| {
		rusqlite::Error::FromSqlConversionFailure(0, Type::Integer, Box::new(error))
	})
}

/// The block the row's success was recorded at, when it has one.
fn success_from_row(row: &Row<'_>) -> rusqlite::Result<Option<HashAndNumber>> {
	let number: Option<u64> = row.get("success_number")?;
	let hash: Option<[u8; 32]> = row.get("success_hash")?;
	Ok(number
		.zip(hash)
		.map(|(number, hash)| HashAndNumber { hash: H256(hash), number }))
}

impl ScheduledDurableTx {
	fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
		Ok(Self {
			id: DurableTxId(row.get("id")?),
			domain: domain_from_row(row)?,
			group: group_from_row(row)?,
			plan: SubmissionPlan::from_row(row)?,
			attempts: row.get("attempts")?,
		})
	}
}

impl SubmissionPlan {
	fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
		Ok(Self { params: PolicyParams(row.get("policy_params")?) })
	}
}

impl DurableTxState {
	fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
		let tx_hash: Option<[u8; 32]> = row.get("tx_hash")?;
		Ok(Self {
			id: DurableTxId(row.get("id")?),
			status: status_from_row(row)?,
			tx_hash: tx_hash.map(H256),
			plan: plan_from_row(row)?,
		})
	}
}

/// The row's plan, when a policy builds it.
fn plan_from_row(row: &Row<'_>) -> rusqlite::Result<Option<SubmissionPlan>> {
	let params: Option<Vec<u8>> = row.get("policy_params")?;
	Ok(params.map(|params| SubmissionPlan { params: PolicyParams(params) }))
}

impl ToSql for PolicyParams {
	fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
		self.0.to_sql()
	}
}

/// The row's status, with the evidence its columns hold for it.
fn status_from_row(row: &Row<'_>) -> rusqlite::Result<DurableTxStatus> {
	let name: String = row.get("status")?;
	match name.as_str() {
		"PENDING_SUBMISSION" => Ok(DurableTxStatus::PendingSubmission),
		"PENDING" => Ok(DurableTxStatus::Pending),
		"PENDING_SUCCESS" => pending_success_from_row(row),
		"FINALIZED_SUCCESS" => Ok(DurableTxStatus::FinalizedSuccess { at: success_from_row(row)? }),
		"FAILURE" => Ok(DurableTxStatus::Failure(row.get("failure_kind")?)),
		other => Err(invalid_status(format!(
			"unknown durable status {other:?}, expected PENDING_SUBMISSION, PENDING, \
             PENDING_SUCCESS, FINALIZED_SUCCESS or FAILURE"
		))),
	}
}

/// A `PENDING_SUCCESS` row, which must name the block it was seen at.
fn pending_success_from_row(row: &Row<'_>) -> rusqlite::Result<DurableTxStatus> {
	let at = success_from_row(row)?.ok_or_else(|| {
		invalid_status("PENDING_SUCCESS row without success_number and success_hash".into())
	})?;
	Ok(DurableTxStatus::PendingSuccess { at })
}

fn invalid_status(message: String) -> rusqlite::Error {
	rusqlite::Error::FromSqlConversionFailure(0, Type::Text, message.into())
}

/// The `status` column's value for `status`.
fn status_name(status: DurableTxStatus) -> &'static str {
	match status {
		DurableTxStatus::PendingSubmission => "PENDING_SUBMISSION",
		DurableTxStatus::Pending => "PENDING",
		DurableTxStatus::PendingSuccess { .. } => "PENDING_SUCCESS",
		DurableTxStatus::FinalizedSuccess { .. } => "FINALIZED_SUCCESS",
		DurableTxStatus::Failure(_) => "FAILURE",
	}
}

/// The `failure_kind` column's value for `status`.
fn failure_kind(status: DurableTxStatus) -> Option<DurableTxFailureKind> {
	match status {
		DurableTxStatus::Failure(kind) => Some(kind),
		_ => None,
	}
}

impl ToSql for DurableTxFailureKind {
	fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
		Ok(ToSqlOutput::from(match self {
			Self::Expired => "EXPIRED",
			Self::DispatchFailed => "DISPATCH_FAILED",
			Self::Rejected => "REJECTED",
			Self::Abandoned => "ABANDONED",
		}))
	}
}

impl FromSql for DurableTxFailureKind {
	fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
		match value.as_str()? {
			"EXPIRED" => Ok(Self::Expired),
			"DISPATCH_FAILED" => Ok(Self::DispatchFailed),
			"REJECTED" => Ok(Self::Rejected),
			"ABANDONED" => Ok(Self::Abandoned),
			other => Err(FromSqlError::Other(
				format!(
					"unknown durable failure kind {other:?}, expected EXPIRED, \
                     DISPATCH_FAILED, REJECTED or ABANDONED"
				)
				.into(),
			)),
		}
	}
}

#[cfg(test)]
mod tests;
