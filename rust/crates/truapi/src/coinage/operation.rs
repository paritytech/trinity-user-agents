//! B4 · Operation model: each operation's identity and kind, the arguments
//! it was started with, a requested cancellation, an export's coins and
//! release, an offload's stored wait, and the stored end of an operation
//! that has no transactions to read it from.
//!
//! Everything else about an operation is read from its transactions'
//! records. An operation is a group of the durability layer's records,
//! registered under [`OpKey::group`].

use parity_scale_codec::{Decode, Encode};
use rusqlite::{Connection, OptionalExtension, Transaction, named_params};
use sp_crypto_hashing::blake2_256;

use super::{Asset, Exponent, PurseId};
use crate::{durable::GroupId, store::DbError};

/// Names one operation: an identifier the caller chose, within a scope that
/// keeps the caller's own callers apart. The layer gives the scope no
/// meaning; its own operations use the empty scope.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
#[derive(uniffi::Record)]
pub struct OpKey {
	/// The caller's scope, such as the product.
	pub scope: Vec<u8>,
	/// The identifier the caller chose and stored before calling.
	pub id: [u8; 32],
}

impl OpKey {
	/// One of the layer's own operations, under the empty scope.
	pub fn own(id: [u8; 32]) -> Self {
		Self { scope: Vec::new(), id }
	}

	/// Whether the layer started this operation itself.
	pub fn is_own(&self) -> bool {
		self.scope.is_empty()
	}

	/// The durability layer's group the operation's records belong to.
	pub fn group(&self) -> GroupId {
		let digest = blake2_256(&(&self.scope, self.id).encode());
		GroupId::new(format!("coinage-op:{}", hex::encode(digest)))
	}
}

/// When a failed attempt of an operation's transaction is built again (E3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
#[derive(uniffi::Enum)]
pub enum Retry {
	/// Until this time, in Unix seconds.
	Until(u64),
	/// After an attempt expired unincluded; never after a dispatch failure or
	/// a rejection.
	ExpiryOnly,
	/// Never.
	Never,
}

/// What an operation does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub enum OpKind {
	/// Moves value between two purses.
	Rebalance,
	/// Drains a purse and closes it.
	Delete,
	/// Loads entries from an external account.
	TopUp,
	/// Moves value out into the external asset.
	Offload,
	/// Hands coin secrets to a payee.
	Export,
	/// Takes back the coins of an export.
	Reclaim,
	/// Claims coins a peer handed over.
	Import,
	/// Finds the assets of earlier installations.
	Reconstruct,
	/// Unloads entries whose ring is about to expire.
	Rescue,
	/// Recycles a coin into entries.
	Recycle,
	/// Registers the current installation on chain.
	RegisterInstallation,
}

/// How an offload or a reconstruction ended, which its records alone do not
/// say.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub enum OperationEnd {
	/// An offload's plan reached the amount, or a reconstruction's scan
	/// finished.
	Done,
	/// Nothing could cover the rest of an offload.
	Failed,
	/// An offload's cancellation was requested and its records are stable.
	Cancelled,
}

/// What [`begin`] found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Begun {
	/// The key was new, and the operation is recorded.
	New,
	/// The key names an operation with the same kind and arguments.
	Existing,
}

/// Why an operation request was refused. Nothing was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum OperationError {
	/// The key names an operation with a different kind or arguments.
	#[display("the key names an operation with different arguments")]
	KeyReused,
	/// An export, or one of the layer's own operations, cannot be cancelled.
	#[display("the operation cannot be cancelled")]
	NotCancellable,
	/// No operation has this key.
	#[display("no operation has this key")]
	UnknownOperation,
}

/// One operation as B4 stores it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationRecord {
	/// Its key.
	pub key: OpKey,
	/// What it does.
	pub kind: OpKind,
	/// The SCALE encoding of the arguments it was started with.
	pub arguments: Vec<u8>,
	/// Whether a cancellation was requested.
	pub cancel_requested: bool,
	/// For an export, whether it was released.
	pub released: bool,
	/// For a waiting offload, its stored wait: until a time in Unix seconds,
	/// or, when none, until the next `run_due`.
	pub offload_wait: Option<Option<u64>>,
	/// For an offload or a reconstruction, its stored end.
	pub end: Option<OperationEnd>,
	/// For an export, the coins it hands to its payee, with their
	/// denominations, so a joined export returns the same coins.
	pub exported: Vec<(Asset, Exponent)>,
}

/// What a delete is started with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub struct DeleteArguments {
	/// The purse it deletes.
	pub purse: PurseId,
	/// The purse its value drains into.
	pub drain_into: PurseId,
}

/// What an import is started with: never the coins' secrets, which B2
/// keeps apart until each claim is final.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct ImportArguments {
	/// The purse that receives the coins.
	pub into: PurseId,
	/// The accounts the coins are in.
	pub accounts: Vec<[u8; 32]>,
}

/// What a reclaim is started with.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct ReclaimArguments {
	/// The export whose coins it takes back.
	pub export: OpKey,
	/// The purse that receives them.
	pub into: PurseId,
}

impl OperationRecord {
	/// The arguments the operation was started with, when they decode as a
	/// `T`.
	pub fn arguments_as<T: Decode>(&self) -> Option<T> {
		T::decode(&mut self.arguments.as_slice()).ok()
	}
}

/// Records a new operation, or finds the one `key` already names. The first
/// step of every primitive that starts an operation, so a repeated call
/// joins it instead of starting a second one (invariant 5).
pub fn begin(
	tx: &Transaction<'_>,
	key: &OpKey,
	kind: OpKind,
	arguments: &[u8],
) -> Result<Result<Begun, OperationError>, DbError> {
	if let Some(existing) = read(tx, key)? {
		let same = existing.kind == kind && existing.arguments == arguments;
		return Ok(if same { Ok(Begun::Existing) } else { Err(OperationError::KeyReused) });
	}
	tx.execute(
		"INSERT INTO coinage_operation (scope, id, kind, arguments) VALUES (:scope, :id, :kind, :arguments)",
		named_params! {
			":scope": key.scope,
			":id": key.id.as_slice(),
			":kind": kind.encode(),
			":arguments": arguments,
		},
	)?;
	Ok(Ok(Begun::New))
}

/// Records a cancellation request (invariant 1). A request for an operation
/// that has already ended changes nothing that matters and succeeds.
pub fn request_cancel(tx: &Transaction<'_>, key: &OpKey) -> Result<Result<(), OperationError>, DbError> {
	let Some(existing) = read(tx, key)? else {
		return Ok(Err(OperationError::UnknownOperation));
	};
	if existing.kind == OpKind::Export || key.is_own() {
		return Ok(Err(OperationError::NotCancellable));
	}
	update(tx, key, "cancel_requested = 1", None)?;
	Ok(Ok(()))
}

/// Records that an export was released, or given up at start.
pub fn record_release(tx: &Transaction<'_>, export: &OpKey) -> Result<(), DbError> {
	update(tx, export, "released = 1", None)
}

/// Stores the wait F3's plan decided: until `until`, or, when none, until the
/// next `run_due`.
pub fn record_offload_wait(tx: &Transaction<'_>, offload: &OpKey, until: Option<u64>) -> Result<(), DbError> {
	update(tx, offload, "offload_wait = :value", Some(until.encode()))
}

/// Clears an offload's stored wait, when its next plan runs.
pub fn clear_offload_wait(tx: &Transaction<'_>, offload: &OpKey) -> Result<(), DbError> {
	update(tx, offload, "offload_wait = NULL", None)
}

/// Stores an offload's or a reconstruction's end. An offload's plan never
/// runs after it.
pub fn record_end(tx: &Transaction<'_>, key: &OpKey, end: OperationEnd) -> Result<(), DbError> {
	update(tx, key, "ended = :value", Some(end.encode()))
}

/// Stores the coins an export hands to its payee.
pub fn record_exported(tx: &Transaction<'_>, export: &OpKey, coins: &[(Asset, Exponent)]) -> Result<(), DbError> {
	update(tx, export, "exported = :value", Some(coins.encode()))
}

/// Every operation, for a snapshot.
pub fn all(conn: &Connection) -> Result<Vec<OperationRecord>, DbError> {
	let mut statement = conn.prepare_cached(&format!("{SELECT} FROM coinage_operation"))?;
	let rows = statement.query_map([], record)?;
	Ok(rows.collect::<Result<_, _>>()?)
}

const SELECT: &str =
	"SELECT scope, id, kind, arguments, cancel_requested, released, offload_wait, ended, exported";

/// The operation `key` names, if one was begun.
pub fn read(conn: &Connection, key: &OpKey) -> Result<Option<OperationRecord>, DbError> {
	let found = conn
		.query_row(
			&format!("{SELECT} FROM coinage_operation WHERE scope = :scope AND id = :id"),
			named_params! { ":scope": key.scope, ":id": key.id.as_slice() },
			record,
		)
		.optional()?;
	Ok(found)
}

fn update(tx: &Transaction<'_>, key: &OpKey, assignment: &str, value: Option<Vec<u8>>) -> Result<(), DbError> {
	let sql = format!("UPDATE coinage_operation SET {assignment} WHERE scope = :scope AND id = :id");
	match value {
		Some(value) => tx.execute(&sql, named_params! { ":scope": key.scope, ":id": key.id.as_slice(), ":value": value })?,
		None => tx.execute(&sql, named_params! { ":scope": key.scope, ":id": key.id.as_slice() })?,
	};
	Ok(())
}

fn record(row: &rusqlite::Row<'_>) -> rusqlite::Result<OperationRecord> {
	let id: Vec<u8> = row.get(1)?;
	let id = <[u8; 32]>::try_from(id.as_slice()).map_err(|_| unreadable(1, "an operation id is 32 bytes".into()))?;
	Ok(OperationRecord {
		key: OpKey { scope: row.get(0)?, id },
		kind: decoded(row, 2)?,
		arguments: row.get(3)?,
		cancel_requested: row.get(4)?,
		released: row.get(5)?,
		offload_wait: optional_decoded(row, 6)?,
		end: optional_decoded(row, 7)?,
		exported: optional_decoded(row, 8)?.unwrap_or_default(),
	})
}

fn decoded<T: Decode>(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<T> {
	let bytes: Vec<u8> = row.get(index)?;
	T::decode(&mut bytes.as_slice()).map_err(|error| unreadable(index, Box::new(error)))
}

fn optional_decoded<T: Decode>(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Option<T>> {
	let bytes: Option<Vec<u8>> = row.get(index)?;
	bytes
		.map(|bytes| T::decode(&mut bytes.as_slice()).map_err(|error| unreadable(index, Box::new(error))))
		.transpose()
}

fn unreadable(index: usize, reason: Box<dyn std::error::Error + Send + Sync>) -> rusqlite::Error {
	rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Blob, reason)
}

#[cfg(test)]
mod tests;
