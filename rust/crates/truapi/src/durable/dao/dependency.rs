//! Queries over `durable_tx_dependency`: what each deferred transaction
//! waits for.

use futures::stream::BoxStream;
use rusqlite::{
	Connection, OptionalExtension, Transaction, named_params,
	types::{ToSql, ToSqlOutput},
};

use super::{domain_from_row, invalid_status, status_from_row};
use crate::{
	durable::model::{
		DeferredDurableTx, DependencyStatus, DependencyStatusRecord, DomainId, DurableTxId,
		DurableTxStatus, OnDependencyFailure, TransactionDependency,
	},
	store::{Db, DbError},
};

/// SQL that holds for a `durable_tx` row some dependency still blocks: one
/// that has not taken effect yet, or one that failed with `CASCADE`.
macro_rules! blocked_by_dependency {
	() => {
		"EXISTS (SELECT 1 FROM durable_tx_dependency
            JOIN durable_tx dependency ON dependency.id = durable_tx_dependency.depends_on
            WHERE durable_tx_dependency.tx_id = durable_tx.id
            AND (dependency.status IN ('PENDING_SUBMISSION', 'PENDING')
                OR (dependency.status = 'FAILURE' AND durable_tx_dependency.on_failure = 'CASCADE')))"
	};
}

const RELEASED_DOMAINS: &str = concat!(
	"SELECT id, attempts, domain FROM durable_tx WHERE status = 'PENDING_SUBMISSION' AND NOT ",
	blocked_by_dependency!(),
	" ORDER BY id"
);

/// The domain of every row waiting to be built that no dependency blocks,
/// one per row in registration order. It is read again after every commit
/// that changes such a row, so a row built and handed back is reported again
/// even when the domains it lists stay the same.
pub fn observe_released_domains(db: &Db) -> BoxStream<'static, Result<Vec<DomainId>, DbError>> {
	db.observe(RELEASED_DOMAINS, |stmt| stmt.query_map([], domain_from_row))
}

const RELEASED_SUBMISSIONS: &str = concat!(
	"SELECT id, domain, group_id, policy_params, attempts FROM durable_tx
    WHERE status = 'PENDING_SUBMISSION' AND domain = :domain AND NOT ",
	blocked_by_dependency!(),
	" ORDER BY id"
);

/// The rows of one domain waiting to be built that no dependency blocks, of
/// every group, in registration order, each with how its dependencies let
/// it go.
pub fn released_submissions(
	conn: &Connection,
	domain: &DomainId,
) -> Result<Vec<DeferredDurableTx>, DbError> {
	let mut stmt = conn.prepare_cached(RELEASED_SUBMISSIONS)?;
	let params = named_params! { ":domain": domain.as_str() };
	let rows = stmt.query_map(params, |row| DeferredDurableTx::from_row(row, conn))?;
	Ok(rows.collect::<rusqlite::Result<_>>()?)
}

const INSERT_DEPENDENCY: &str = "INSERT INTO durable_tx_dependency
    (tx_id, depends_on, on_failure) VALUES (:tx_id, :depends_on, :on_failure)";

/// Records that `dependent` waits for `dependency`.
pub fn insert_dependency(
	tx: &Transaction<'_>,
	dependent: DurableTxId,
	dependency: &TransactionDependency,
) -> Result<(), DbError> {
	tx.prepare_cached(INSERT_DEPENDENCY)?.execute(named_params! {
		":tx_id": dependent.0,
		":depends_on": dependency.id.0,
		":on_failure": dependency.on_failure,
	})?;
	Ok(())
}

const REGISTERED_DOMAIN: &str = "SELECT domain FROM durable_tx WHERE id = :id";

/// The domain of transaction `id`, or `None` when no such transaction is
/// registered.
pub fn registered_domain(conn: &Connection, id: DurableTxId) -> Result<Option<DomainId>, DbError> {
	let mut stmt = conn.prepare_cached(REGISTERED_DOMAIN)?;
	let domain = stmt.query_row(named_params! { ":id": id.0 }, domain_from_row).optional()?;
	Ok(domain)
}

const RELEASED_DEPENDENCIES: &str = "SELECT dependency.id, dependency.status,
    dependency.success_number, dependency.success_hash, dependency.failure_kind
    FROM durable_tx_dependency
    JOIN durable_tx dependency ON dependency.id = durable_tx_dependency.depends_on
    WHERE durable_tx_dependency.tx_id = :tx_id ORDER BY dependency.id";

/// How each dependency of `dependent`, a row no dependency blocks any more,
/// released it.
pub fn released_dependencies(
	conn: &Connection,
	dependent: DurableTxId,
) -> rusqlite::Result<Vec<DependencyStatusRecord>> {
	let mut stmt = conn.prepare_cached(RELEASED_DEPENDENCIES)?;
	let params = named_params! { ":tx_id": dependent.0 };
	let records = stmt.query_map(params, |row| dependency_record_from_row(row, dependent))?;
	records.collect()
}

/// One dependency of `dependent` as its policy sees it. A row no dependency
/// blocks only has dependencies that succeeded or failed with `IGNORE`.
fn dependency_record_from_row(
	row: &rusqlite::Row<'_>,
	dependent: DurableTxId,
) -> rusqlite::Result<DependencyStatusRecord> {
	let id = DurableTxId(row.get(0)?);
	let status = match status_from_row(row)? {
		DurableTxStatus::PendingSuccess { .. } | DurableTxStatus::FinalizedSuccess { .. } => {
			DependencyStatus::Success
		},
		DurableTxStatus::Failure(_) => DependencyStatus::IgnoredFailure,
		unsettled => return Err(invalid_status(unreleased_message(id, dependent, unsettled))),
	};
	Ok(DependencyStatusRecord { id, status })
}

fn unreleased_message(id: DurableTxId, dependent: DurableTxId, status: DurableTxStatus) -> String {
	format!(
		"dependency {id:?} of released transaction {dependent:?} is {status:?}, \
         expected PendingSuccess, FinalizedSuccess or Failure"
	)
}

const HAS_UNSETTLED_DEPENDENCY: &str = "SELECT EXISTS (SELECT 1 FROM durable_tx_dependency
    JOIN durable_tx dependency ON dependency.id = durable_tx_dependency.depends_on
    WHERE durable_tx_dependency.tx_id = :tx_id
    AND dependency.status IN ('PENDING_SUBMISSION', 'PENDING'))";

/// Whether some dependency of `dependent` has not taken effect yet: it
/// waits to be built, or was submitted with no verdict.
pub fn has_unsettled_dependency(
	conn: &Connection,
	dependent: DurableTxId,
) -> Result<bool, DbError> {
	let mut stmt = conn.prepare_cached(HAS_UNSETTLED_DEPENDENCY)?;
	let unsettled = stmt.query_row(named_params! { ":tx_id": dependent.0 }, |row| row.get(0))?;
	Ok(unsettled)
}

/// An unfinished row that a failed dependency marked `CASCADE` fails with
/// it.
pub struct CascadingFailure {
	/// The unfinished row.
	pub dependent: DurableTxId,
	/// The dependency that failed.
	pub dependency: DurableTxId,
}

const CASCADING_FAILURES: &str =
	"SELECT durable_tx_dependency.tx_id, durable_tx_dependency.depends_on
    FROM durable_tx_dependency
    JOIN durable_tx dependent ON dependent.id = durable_tx_dependency.tx_id
    JOIN durable_tx dependency ON dependency.id = durable_tx_dependency.depends_on
    WHERE dependent.status IN ('PENDING_SUBMISSION', 'PENDING', 'PENDING_SUCCESS')
    AND durable_tx_dependency.on_failure = 'CASCADE' AND dependency.status = 'FAILURE'
    ORDER BY durable_tx_dependency.tx_id";

/// Every unfinished row with a failed dependency marked `CASCADE`, once per
/// such dependency. A row is unfinished while it waits to be built, is
/// submitted, or took effect at a block that is not finalized.
pub fn cascading_failures(conn: &Connection) -> Result<Vec<CascadingFailure>, DbError> {
	let mut stmt = conn.prepare_cached(CASCADING_FAILURES)?;
	let failures = stmt.query_map([], |row| {
		Ok(CascadingFailure {
			dependent: DurableTxId(row.get(0)?),
			dependency: DurableTxId(row.get(1)?),
		})
	})?;
	Ok(failures.collect::<rusqlite::Result<_>>()?)
}

/// Fails every unfinished row whose dependency marked `CASCADE` failed,
/// then the rows that cascade from those, until none is left. Returns each
/// row it failed, with the dependency that failed it.
pub fn abandon_cascading(tx: &Transaction<'_>) -> Result<Vec<CascadingFailure>, DbError> {
	let mut applied = Vec::new();
	loop {
		let level = cascading_failures(tx)?;
		if level.is_empty() {
			return Ok(applied);
		}
		applied.extend(abandon_each(tx, level)?);
	}
}

/// Fails the unfinished row of each of `failures`. Returns those it failed,
/// once per row: a row with two failed dependencies is failed by the first.
fn abandon_each(
	tx: &Transaction<'_>,
	failures: Vec<CascadingFailure>,
) -> Result<Vec<CascadingFailure>, DbError> {
	let mut abandoned = Vec::new();
	for failure in failures {
		if abandon_unfinished(tx, failure.dependent)? {
			abandoned.push(failure);
		}
	}
	Ok(abandoned)
}

const ABANDON_UNFINISHED: &str = "UPDATE durable_tx
    SET status = 'FAILURE', failure_kind = 'ABANDONED', success_number = NULL, success_hash = NULL
    WHERE id = :id AND status IN ('PENDING_SUBMISSION', 'PENDING', 'PENDING_SUCCESS')";

/// Fails row `id` while it is unfinished, dropping a success recorded at a
/// block that is not finalized. Returns whether it was unfinished.
fn abandon_unfinished(tx: &Transaction<'_>, id: DurableTxId) -> Result<bool, DbError> {
	let changed = tx.prepare_cached(ABANDON_UNFINISHED)?.execute(named_params! { ":id": id.0 })?;
	Ok(changed == 1)
}

impl ToSql for OnDependencyFailure {
	fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
		Ok(ToSqlOutput::from(match self {
			Self::Ignore => "IGNORE",
			Self::Cascade => "CASCADE",
		}))
	}
}
