//! Queries over the `durable_tx` ledger, one SQL constant and one function
//! each.

use futures::stream::BoxStream;
use rusqlite::{
	Connection, Row, Transaction, named_params,
	types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Type, ValueRef},
};
use subxt::utils::H256;

use super::model::{
	DomainId, DurableTxEntry, DurableTxId, DurableTxState, DurableTxStatus, FailureKind, GroupId,
	PolicyId, PolicyParams, ScheduledDurableTx, SubmissionPlan, Verdict,
};
use crate::{
	chain::{HashAndNumber, MortalExtrinsic, Mortality},
	store::{Db, DbError},
};

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
    birth_hash = :birth_hash, period = :period, success_number = NULL, success_hash = NULL,
    scanned_number = NULL
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

const ABANDON_SUBMISSION: &str = "UPDATE durable_tx
    SET status = 'FAILURE', failure_kind = 'ABANDONED'
    WHERE id = :id AND status = 'PENDING_SUBMISSION'";

/// Fails a row that is still waiting to be built. Returns whether it was.
pub fn abandon_submission(tx: &Transaction<'_>, id: DurableTxId) -> Result<bool, DbError> {
	let changed = tx.prepare_cached(ABANDON_SUBMISSION)?.execute(named_params! { ":id": id.0 })?;
	Ok(changed == 1)
}

const SUBMISSION_PLAN: &str = "SELECT policy_id, policy_params FROM durable_tx
    WHERE id = :id AND policy_id IS NOT NULL";

/// The plan that rebuilds the row with `id`, when it has one.
pub fn submission_plan(
	conn: &Connection,
	id: DurableTxId,
) -> Result<Option<SubmissionPlan>, DbError> {
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
	db.observe(PENDING_SUBMISSIONS, |stmt| stmt.query_map([], ScheduledDurableTx::from_row))
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
    SET status = :status, success_number = :success_number, success_hash = :success_hash,
        failure_kind = :failure_kind, scanned_number = COALESCE(:scanned_number, scanned_number)
    WHERE id = :id AND status = :expected AND tx_hash = :expected_hash
    AND status IN ('PENDING', 'PENDING_SUCCESS')";

/// Writes `verdict` only while the row still awaits a verdict in the status
/// and attempt `observed` holds. Returns whether it wrote.
pub fn compare_and_set(
	tx: &Transaction<'_>,
	observed: &DurableTxEntry,
	verdict: &Verdict,
) -> Result<bool, DbError> {
	let success = verdict.status.success_block();
	let changed = tx.prepare_cached(COMPARE_AND_SET)?.execute(named_params! {
		":status": status_name(verdict.status),
		":success_number": success.map(|block| block.number),
		":success_hash": success.map(|block| block.hash.0),
		":failure_kind": failure_kind(verdict.status),
		":scanned_number": verdict.scanned_to,
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
	"SELECT id, ",
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

const DOMAIN_ENTRIES: &str =
	concat!("SELECT * FROM durable_tx WHERE domain = :domain AND ", submitted!(), " ORDER BY id");

/// Every row of one domain with a submitted attempt, in registration order.
pub fn domain_entries(
	conn: &Connection,
	domain: &DomainId,
) -> Result<Vec<DurableTxEntry>, DbError> {
	Ok(conn
		.prepare_cached(DOMAIN_ENTRIES)?
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
			status: status_from_row(row)?,
			scanned_to: row.get("scanned_number")?,
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
		Ok(Self { id: DurableTxId(row.get("id")?), status: status_from_row(row)? })
	}
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
fn failure_kind(status: DurableTxStatus) -> Option<FailureKind> {
	match status {
		DurableTxStatus::Failure(kind) => Some(kind),
		_ => None,
	}
}

impl ToSql for FailureKind {
	fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
		Ok(ToSqlOutput::from(match self {
			Self::Expired => "EXPIRED",
			Self::DispatchFailed => "DISPATCH_FAILED",
			Self::Rejected => "REJECTED",
			Self::Abandoned => "ABANDONED",
		}))
	}
}

impl FromSql for FailureKind {
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
mod tests {
	use futures::{FutureExt, StreamExt, executor::block_on};

	use super::*;
	use crate::{
		durable::testing::{
			REBUILD_POLICY, block, extrinsic, insert_retryable, open_db, rebuild_plan,
		},
		test_support::wait_until,
	};

	fn domain() -> DomainId {
		DomainId::new("test")
	}

	fn register(db: &Db, group: Option<GroupId>, tag: u8) -> DurableTxId {
		block_on(
			db.write(move |tx| {
				insert(tx, &domain(), group.as_ref(), &extrinsic(tag, 100, 64), None)
			}),
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

		let wrote =
			write_verdict(&db, observed.clone(), DurableTxStatus::PendingSuccess { at }.into());

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
		write_verdict(
			&db,
			observed.clone(),
			DurableTxStatus::PendingSuccess { at: block(120) }.into(),
		);
		let expired = DurableTxStatus::Failure(FailureKind::Expired);

		let wrote = write_verdict(&db, observed, expired.into());

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

		assert!(!write_verdict(
			&db,
			observed,
			DurableTxStatus::FinalizedSuccess { at: None }.into()
		));
	}

	#[test]
	fn a_terminal_row_is_never_rewritten() {
		let (_dir, db) = open_db();
		let id = register(&db, None, 1);
		let observed = read(&db, move |conn| entry(conn, id)).unwrap();
		write_verdict(&db, observed.clone(), DurableTxStatus::Failure(FailureKind::Expired).into());
		let terminal =
			DurableTxEntry { status: DurableTxStatus::Failure(FailureKind::Expired), ..observed };

		assert!(!write_verdict(
			&db,
			terminal,
			DurableTxStatus::FinalizedSuccess { at: None }.into()
		));
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
			DurableTxStatus::Failure(FailureKind::Expired),
			DurableTxStatus::Failure(FailureKind::DispatchFailed),
			DurableTxStatus::Failure(FailureKind::Rejected),
		];

		let read_back: Vec<_> = (1..)
			.zip(written)
			.map(|(tag, status)| {
				let id = register(&db, None, tag);
				let observed = read(&db, move |conn| entry(conn, id)).unwrap();
				write_verdict(&db, observed, status.into());
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
		let scanned = Verdict { status: DurableTxStatus::Pending, scanned_to: Some(120) };
		write_verdict(&db, observed, scanned);
		let scanned_entry = read(&db, move |conn| entry(conn, id)).unwrap();

		write_verdict(
			&db,
			scanned_entry,
			DurableTxStatus::PendingSuccess { at: block(125) }.into(),
		);

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
			vec![
				DurableTxState { id: first, status: DurableTxStatus::Pending },
				DurableTxState { id: second, status: DurableTxStatus::Pending },
			]
		);
	}

	#[test]
	fn domain_entries_list_every_row_of_the_domain() {
		let (_dir, db) = open_db();
		let ids = [register(&db, None, 1), register(&db, None, 2)];

		let listed = read(&db, |conn| domain_entries(conn, &domain()));

		assert_eq!(listed.iter().map(|entry| entry.id).collect::<Vec<_>>(), ids);
	}

	#[test]
	fn only_domains_with_a_transaction_awaiting_a_verdict_are_live() {
		let (_dir, db) = open_db();
		assert_eq!(read(&db, live_domains), Vec::<DomainId>::new());
		let id = register(&db, None, 1);
		assert_eq!(read(&db, live_domains), vec![domain()]);

		let observed = read(&db, move |conn| entry(conn, id)).unwrap();
		write_verdict(&db, observed, DurableTxStatus::FinalizedSuccess { at: None }.into());

		assert_eq!(read(&db, live_domains), Vec::<DomainId>::new());
	}

	#[test]
	fn observed_queries_follow_commits() {
		let (_dir, db) = open_db();
		let group_id = GroupId::new("payment");
		let mut live = observe_has_live(&db);
		let mut grouped = observe_group(&db, domain(), group_id.clone());
		assert!(!next(&mut live));
		assert_eq!(next(&mut grouped), vec![]);

		let id = register(&db, Some(group_id), 1);
		let mut status = observe_status(&db, id);

		assert!(next(&mut live));
		assert_eq!(
			next(&mut grouped),
			vec![DurableTxState { id, status: DurableTxStatus::Pending }]
		);
		assert_eq!(next(&mut status), Some(DurableTxStatus::Pending));
	}

	fn next<T>(stream: &mut BoxStream<'static, Result<T, DbError>>) -> T {
		block_on(stream.next()).unwrap().unwrap()
	}

	fn schedule_in(db: &Db, domain: DomainId, group: Option<GroupId>) -> DurableTxId {
		let plan = rebuild_plan(vec![1]);
		let write = db.write(move |tx| schedule(tx, &domain, group.as_ref(), &plan));
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
		insert_retryable(db, &domain(), extrinsic(1, 100, 64), rebuild_plan(vec![params]))
	}

	#[test]
	fn a_scheduled_row_waits_for_its_policy_with_no_attempt() {
		let (_dir, db) = open_db();
		let id = schedule_in(&db, domain(), Some(GroupId::new("payment")));

		assert_eq!(
			pending_rows(&db),
			vec![ScheduledDurableTx {
				id,
				domain: domain(),
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
		schedule_in(&db, domain(), None);

		assert!(next(&mut observe_has_live(&db)));
		assert!(read(&db, has_pending_submissions));
		assert_eq!(read(&db, live_domains), Vec::<DomainId>::new());
		assert_eq!(read(&db, |conn| domain_entries(conn, &domain())), vec![]);
	}

	#[test]
	fn starting_an_attempt_gives_a_waiting_row_its_extrinsic() {
		let (_dir, db) = open_db();
		let id = schedule_in(&db, domain(), None);

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
			})
		);
		assert_eq!(pending_rows(&db), vec![]);
	}

	/// Two builds of one row must not both become attempts: only the first
	/// finds it still waiting.
	#[test]
	fn an_attempt_starts_only_on_a_row_still_waiting() {
		let (_dir, db) = open_db();
		let waiting = schedule_in(&db, domain(), None);
		let submitted = register(&db, None, 1);
		start_attempt_tagged(&db, waiting, 9);

		assert!(!start_attempt_tagged(&db, waiting, 10));
		assert!(!start_attempt_tagged(&db, submitted, 10));
		assert_eq!(
			read(&db, move |conn| entry(conn, waiting)).unwrap().tx_hash,
			extrinsic(9, 110, 32).extrinsic.hash()
		);
	}

	/// A rebuilt attempt has its own hash and birth, so the blocks searched
	/// for the previous one prove nothing about it.
	#[test]
	fn a_rebuilt_attempt_starts_with_nothing_searched() {
		let (_dir, db) = open_db();
		let id = schedule_in(&db, domain(), None);
		start_attempt_tagged(&db, id, 9);
		let first = read(&db, move |conn| entry(conn, id)).unwrap();
		let scanned = Verdict { scanned_to: Some(120), ..Verdict::from(DurableTxStatus::Pending) };
		write_verdict(&db, first, scanned);
		let searched = read(&db, move |conn| entry(conn, id)).unwrap();
		write_verdict(&db, searched, DurableTxStatus::PendingSubmission.into());

		start_attempt_tagged(&db, id, 10);

		let rebuilt = read(&db, move |conn| entry(conn, id)).unwrap();
		assert_eq!(rebuilt.scanned_to, None);
	}

	#[test]
	fn only_a_row_still_waiting_can_be_abandoned() {
		let (_dir, db) = open_db();
		let waiting = schedule_in(&db, domain(), None);
		let submitted = register(&db, None, 1);

		assert!(abandon_waiting(&db, waiting));
		assert!(!abandon_waiting(&db, waiting));
		assert!(!abandon_waiting(&db, submitted));
		assert_eq!(
			read(&db, move |conn| status(conn, waiting)),
			Some(DurableTxStatus::Failure(FailureKind::Abandoned))
		);
		assert_eq!(read(&db, move |conn| status(conn, submitted)), Some(DurableTxStatus::Pending));
	}

	/// A proven failure the policy accepts to retry sends the row back to be
	/// built, through the same compare-and-set as any verdict.
	#[test]
	fn a_verdict_can_send_a_submitted_row_back_to_its_policy() {
		let (_dir, db) = open_db();
		let id = insert_rebuilt_by(&db, 3);
		let observed = read(&db, move |conn| entry(conn, id)).unwrap();

		assert!(write_verdict(&db, observed, DurableTxStatus::PendingSubmission.into()));
		assert_eq!(
			pending_rows(&db),
			vec![ScheduledDurableTx {
				id,
				domain: domain(),
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
		assert_eq!(read(&db, move |conn| submission_plan(conn, without)), None);
	}

	/// A bucket builds one domain's rows of one policy and group. A row with
	/// no group is a bucket of its own, not a match for every group.
	#[test]
	fn a_bucket_lists_only_its_domain_policy_and_group() {
		let (_dir, db) = open_db();
		let grouped = schedule_in(&db, domain(), Some(GroupId::new("g1")));
		let ungrouped = schedule_in(&db, domain(), None);
		schedule_in(&db, DomainId::new("other"), Some(GroupId::new("g1")));
		schedule_in(&db, domain(), Some(GroupId::new("g2")));
		let bucket = |group: Option<GroupId>| {
			read(&db, move |conn| {
				bucket_submissions(conn, &domain(), &REBUILD_POLICY, group.as_ref())
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
		let never_built = schedule_in(&db, domain(), None);
		abandon_waiting(&db, never_built);
		let live = register(&db, None, 1);

		let listed = read(&db, |conn| domain_entries(conn, &domain()));

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
		let id = schedule_in(&db, domain(), None);
		assert_eq!(next(&mut waiting).len(), 1);

		start_attempt_tagged(&db, id, 9);
		let attempt = read(&db, move |conn| entry(conn, id)).unwrap();
		write_verdict(&db, attempt, DurableTxStatus::PendingSubmission.into());

		let reported_again = || {
			let emitted = waiting.next().now_or_never();
			matches!(emitted, Some(Some(Ok(rows))) if rows.len() == 1)
		};
		wait_until(reported_again, "the handed-back row is reported");
	}
}
