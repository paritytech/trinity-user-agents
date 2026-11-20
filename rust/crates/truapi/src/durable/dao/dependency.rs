//! Queries over `durable_tx_dependency`: what each deferred transaction
//! waits for.

use rusqlite::{
	Connection, OptionalExtension, Transaction, named_params,
	types::{ToSql, ToSqlOutput},
};

use super::{domain_from_row, invalid_status};
use crate::{
	durable::model::{
		DependencyStatus, DependencyStatusRecord, DomainId, DurableTxId, OnDependencyFailure,
		TransactionDependency,
	},
	store::DbError,
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

const DEPENDENCY_STATUSES: &str = "SELECT dependency.id, dependency.status
    FROM durable_tx_dependency
    JOIN durable_tx dependency ON dependency.id = durable_tx_dependency.depends_on
    WHERE durable_tx_dependency.tx_id = :tx_id ORDER BY dependency.id";

/// How each dependency of `dependent`, a row no dependency blocks any more,
/// let it go.
pub fn released_dependencies(
	conn: &Connection,
	dependent: DurableTxId,
) -> rusqlite::Result<Vec<DependencyStatusRecord>> {
	let mut stmt = conn.prepare_cached(DEPENDENCY_STATUSES)?;
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
	let status: String = row.get(1)?;
	let status = match status.as_str() {
		"PENDING_SUCCESS" | "FINALIZED_SUCCESS" => DependencyStatus::Success,
		"FAILURE" => DependencyStatus::IgnoredFailure,
		other => {
			return Err(invalid_status(format!(
				"dependency {id:?} of released transaction {dependent:?} is {other:?}, \
                 expected PENDING_SUCCESS, FINALIZED_SUCCESS or FAILURE"
			)));
		},
	};
	Ok(DependencyStatusRecord { id, status })
}

impl ToSql for OnDependencyFailure {
	fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
		Ok(ToSqlOutput::from(match self {
			Self::Ignore => "IGNORE",
			Self::Cascade => "CASCADE",
		}))
	}
}
