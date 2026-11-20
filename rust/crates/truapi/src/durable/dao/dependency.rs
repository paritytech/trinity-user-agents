//! Queries over `durable_tx_dependency`: what each deferred transaction
//! waits for.

use rusqlite::{
	Connection, OptionalExtension, Transaction, named_params,
	types::{ToSql, ToSqlOutput},
};

use super::domain_from_row;
use crate::{
	durable::model::{DomainId, DurableTxId, OnDependencyFailure, TransactionDependency},
	store::DbError,
};

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

impl ToSql for OnDependencyFailure {
	fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
		Ok(ToSqlOutput::from(match self {
			Self::Ignore => "IGNORE",
			Self::Cascade => "CASCADE",
		}))
	}
}
