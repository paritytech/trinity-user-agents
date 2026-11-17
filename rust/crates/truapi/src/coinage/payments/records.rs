//! J2 · Payment records: layer 2's records of receivables, deposits, refunds,
//! outgoing cheques and RFC-6 operations.
//!
//! Every record that names a layer-1 operation is written before that
//! operation starts, under its key, and a refusal to start it is written
//! against the same key, so after a restart the operation's status is read
//! from layer 1 rather than reconstructed (invariant 2). Writes only add
//! records or set a flag; nothing is ever deleted (invariant 3). J2 keeps no
//! coin secret in the clear: only receivable secrets, which protect no value
//! on their own, and sealed cheques.

use parity_scale_codec::{Decode, Encode};
use rusqlite::{Connection, OptionalExtension, Transaction, named_params};
use truapi::latest::{CoinPaymentCheque, CoinPaymentError};
use zeroize::Zeroizing;

use super::{
	cheque::ChequeHash,
	receivable::{Receivable, ReceivableSecret},
};
use crate::{
	coinage::{PurseId, operation::OpKey},
	store::DbError,
};

/// A layer-1 operation layer 2 started, or tried to: its key, and the
/// refusal when layer 1 refused to start it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationRecord {
	/// The operation's key.
	pub key: OpKey,
	/// Why layer 1 refused to start it, as RFC-17 reports it.
	pub refused: Option<CoinPaymentError>,
}

/// A receivable this user agent holds, with what was deposited under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceivableRecord {
	/// The receivable.
	pub receivable: Receivable,
	/// Its secret.
	pub secret: ReceivableSecret,
	/// The purse everything deposited under it goes to (invariant 5).
	pub purse: PurseId,
	/// Drawn by K1 as a payment's return receivable: what arrives under it is
	/// a refund, which is never refunded.
	pub return_receivable: bool,
	/// The deposits under it, in the order they were recorded.
	pub deposits: Vec<DepositRecord>,
}

/// One cheque deposited under a receivable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepositRecord {
	/// The cheque.
	pub cheque: ChequeHash,
	/// The product that deposited it, whose statement-store key posts its
	/// refunds.
	pub product: String,
	/// The amount it declared, in cents.
	pub amount: u64,
	/// Its claim deadline, in Unix milliseconds.
	pub claim_by: u64,
	/// The import of its coins.
	pub import: OperationRecord,
	/// The payer's return receivable; none on a refund.
	pub return_to: Option<Receivable>,
	/// Its refunds, in the order they were started.
	pub refunds: Vec<RefundRecord>,
	/// Whether its automatic refund was made (K2 invariant 5).
	pub automatic_refund_made: bool,
}

/// One refund of a deposit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefundRecord {
	/// The refund's export.
	pub operation: OperationRecord,
	/// The refund cheque, once sealed.
	pub cheque: Option<ChequeHash>,
}

/// A cheque this user agent sealed: a payment or a refund.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutgoingCheque {
	/// Its identity.
	pub hash: ChequeHash,
	/// The sealed cheque, kept so it can be posted again.
	pub cheque: CoinPaymentCheque,
	/// The purse it was paid from.
	pub source: PurseId,
	/// The receivable it is sealed to.
	pub payee: Receivable,
	/// The export of its coins.
	pub export: OpKey,
	/// The payer's return receivable; none for a refund cheque.
	pub return_to: Option<Receivable>,
	/// Its claim deadline, in Unix milliseconds.
	pub claim_by: u64,
	/// The product it belongs to: the one that paid, or the one a refund was
	/// made for. A refund cheque is posted with its statement-store key.
	pub product: String,
	/// Whether layer 2 posts it: a refund cheque. A payment cheque is handed
	/// over by its product.
	pub post: bool,
	/// Whether a post succeeded.
	pub posted: bool,
	/// Its reclaims, in the order they were started.
	pub reclaims: Vec<OperationRecord>,
}

/// What an RFC-6 call did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub enum Rfc6Kind {
	/// A payment out of a purse.
	Payment,
	/// A top-up into a purse.
	TopUp,
}

/// An RFC-6 payment or top-up, keyed by the id its product chose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rfc6Record {
	/// The product that called.
	pub product: Vec<u8>,
	/// The id it chose.
	pub id: [u8; 32],
	/// A payment or a top-up.
	pub kind: Rfc6Kind,
	/// The amount it asked for, in the payment asset's units.
	pub amount: u128,
	/// For a top-up from an account, the account that pays; the secret of a
	/// private-key source is never stored.
	pub source: Option<[u8; 32]>,
	/// The funding session it serves, when a funding provider's worker
	/// started it (N2 invariant 6).
	pub funding: Option<String>,
	/// The layer-1 operation.
	pub operation: OperationRecord,
}

/// An RFC-6 id that already names an operation of its product.
#[derive(Clone, Copy, Debug, PartialEq, Eq, derive_more::Display, derive_more::Error)]
#[display("the id already names an operation")]
pub struct AlreadyExists;

#[derive(Encode, Decode)]
struct StoredReceivable {
	secret: [u8; 32],
	purse: PurseId,
	return_receivable: bool,
}

#[derive(Encode, Decode)]
struct StoredDeposit {
	product: String,
	amount: u64,
	claim_by: u64,
	import: OpKey,
	return_to: Option<Receivable>,
	refunds: Vec<(OpKey, Option<ChequeHash>)>,
	automatic_refund_made: bool,
}

#[derive(Encode, Decode)]
struct StoredOutgoing {
	cheque: CoinPaymentCheque,
	source: PurseId,
	payee: Receivable,
	export: OpKey,
	return_to: Option<Receivable>,
	claim_by: u64,
	product: String,
	post: bool,
	posted: bool,
	reclaims: Vec<OpKey>,
}

#[derive(Encode, Decode)]
struct StoredRfc6 {
	kind: Rfc6Kind,
	amount: u128,
	source: Option<[u8; 32]>,
	funding: Option<String>,
	operation: OpKey,
}

// ---- Writes, each inside the caller's store transaction ----

/// Records a receivable with no deposits.
pub fn record_receivable(tx: &Transaction<'_>, record: &ReceivableRecord) -> Result<(), DbError> {
	let stored = StoredReceivable {
		secret: *record.secret.0,
		purse: record.purse,
		return_receivable: record.return_receivable,
	};
	tx.execute(
		"INSERT INTO coinage_receivable (receivable, record) VALUES (:receivable, :record)",
		named_params! { ":receivable": record.receivable.encode(), ":record": stored.encode() },
	)?;
	Ok(())
}

/// Records a deposit under `receivable`, before its import starts.
pub fn record_deposit(tx: &Transaction<'_>, receivable: Receivable, deposit: &DepositRecord) -> Result<(), DbError> {
	let stored = StoredDeposit {
		product: deposit.product.clone(),
		amount: deposit.amount,
		claim_by: deposit.claim_by,
		import: deposit.import.key.clone(),
		return_to: deposit.return_to,
		refunds: deposit.refunds.iter().map(|refund| (refund.operation.key.clone(), refund.cheque)).collect(),
		automatic_refund_made: deposit.automatic_refund_made,
	};
	tx.execute(
		"INSERT INTO coinage_deposit (cheque, receivable, record) VALUES (:cheque, :receivable, :record)",
		named_params! {
			":cheque": deposit.cheque.encode(),
			":receivable": receivable.encode(),
			":record": stored.encode(),
		},
	)?;
	Ok(())
}

/// Records a refund of `deposit`, before its export starts.
pub fn record_refund(tx: &Transaction<'_>, deposit: ChequeHash, refund: &RefundRecord) -> Result<(), DbError> {
	update_deposit(tx, deposit, |stored| stored.refunds.push((refund.operation.key.clone(), refund.cheque)))
}

/// Records the sealed cheque of the refund `refund` of `deposit`.
pub fn record_refund_cheque(
	tx: &Transaction<'_>,
	deposit: ChequeHash,
	refund: &OpKey,
	cheque: ChequeHash,
) -> Result<(), DbError> {
	update_deposit(tx, deposit, |stored| {
		if let Some(entry) = stored.refunds.iter_mut().find(|(key, _)| key == refund) {
			entry.1 = Some(cheque);
		}
	})
}

/// Records that `deposit`'s automatic refund was made.
pub fn mark_automatic_refund(tx: &Transaction<'_>, deposit: ChequeHash) -> Result<(), DbError> {
	update_deposit(tx, deposit, |stored| stored.automatic_refund_made = true)
}

/// Records an outgoing cheque, in the store transaction that commits its
/// export.
pub fn record_outgoing(tx: &Transaction<'_>, cheque: &OutgoingCheque) -> Result<(), DbError> {
	let stored = StoredOutgoing {
		cheque: cheque.cheque.clone(),
		source: cheque.source,
		payee: cheque.payee,
		export: cheque.export.clone(),
		return_to: cheque.return_to,
		claim_by: cheque.claim_by,
		product: cheque.product.clone(),
		post: cheque.post,
		posted: cheque.posted,
		reclaims: cheque.reclaims.iter().map(|reclaim| reclaim.key.clone()).collect(),
	};
	tx.execute(
		"INSERT INTO coinage_outgoing (cheque, record) VALUES (:cheque, :record)",
		named_params! { ":cheque": cheque.hash.encode(), ":record": stored.encode() },
	)?;
	Ok(())
}

/// Records a reclaim of `cheque`, before it starts.
pub fn record_reclaim(tx: &Transaction<'_>, cheque: ChequeHash, reclaim: &OpKey) -> Result<(), DbError> {
	update_outgoing(tx, cheque, |stored| stored.reclaims.push(reclaim.clone()))
}

/// Records that a post of `cheque` succeeded.
pub fn mark_posted(tx: &Transaction<'_>, cheque: ChequeHash) -> Result<(), DbError> {
	update_outgoing(tx, cheque, |stored| stored.posted = true)
}

/// Records an RFC-6 operation, or refuses an id its product already used
/// (N2 invariant 3).
pub fn record_rfc6(tx: &Transaction<'_>, record: &Rfc6Record) -> Result<Result<(), AlreadyExists>, DbError> {
	if rfc6(tx, &record.product, record.id)?.is_some() {
		return Ok(Err(AlreadyExists));
	}
	let stored = StoredRfc6 {
		kind: record.kind,
		amount: record.amount,
		source: record.source,
		funding: record.funding.clone(),
		operation: record.operation.key.clone(),
	};
	tx.execute(
		"INSERT INTO coinage_rfc6 (product, id, record) VALUES (:product, :id, :record)",
		named_params! { ":product": record.product, ":id": record.id.as_slice(), ":record": stored.encode() },
	)?;
	Ok(Ok(()))
}

/// Records that layer 1 refused to start the operation `key`; nothing tries
/// to start it again.
pub fn record_refusal(tx: &Transaction<'_>, key: &OpKey, error: &CoinPaymentError) -> Result<(), DbError> {
	tx.execute(
		"INSERT OR REPLACE INTO coinage_refusal (operation, error) VALUES (:operation, :error)",
		named_params! { ":operation": key.encode(), ":error": error.encode() },
	)?;
	Ok(())
}

// ---- Reads ----

/// The receivable record, with its deposits.
pub fn receivable(conn: &Connection, receivable: Receivable) -> Result<Option<ReceivableRecord>, DbError> {
	let stored: Option<Vec<u8>> = conn
		.query_row(
			"SELECT record FROM coinage_receivable WHERE receivable = :receivable",
			named_params! { ":receivable": receivable.encode() },
			|row| row.get(0),
		)
		.optional()?;
	let Some(stored) = stored else { return Ok(None) };
	let stored: StoredReceivable = decode(&stored)?;
	let mut statement = conn.prepare_cached(
		"SELECT cheque, record FROM coinage_deposit WHERE receivable = :receivable ORDER BY rowid",
	)?;
	let rows = statement.query_map(named_params! { ":receivable": receivable.encode() }, |row| {
		Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
	})?;
	let mut deposits = Vec::new();
	for row in rows {
		let (cheque, record) = row?;
		deposits.push(deposit_record(conn, decode(&cheque)?, decode(&record)?)?);
	}
	Ok(Some(ReceivableRecord {
		receivable,
		secret: ReceivableSecret(Zeroizing::new(stored.secret)),
		purse: stored.purse,
		return_receivable: stored.return_receivable,
		deposits,
	}))
}

/// The deposit of `cheque`, with the receivable it was deposited under.
pub fn deposit(conn: &Connection, cheque: ChequeHash) -> Result<Option<(Receivable, DepositRecord)>, DbError> {
	let found: Option<(Vec<u8>, Vec<u8>)> = conn
		.query_row(
			"SELECT receivable, record FROM coinage_deposit WHERE cheque = :cheque",
			named_params! { ":cheque": cheque.encode() },
			|row| Ok((row.get(0)?, row.get(1)?)),
		)
		.optional()?;
	let Some((receivable, record)) = found else { return Ok(None) };
	Ok(Some((decode(&receivable)?, deposit_record(conn, cheque, decode(&record)?)?)))
}

/// Every deposit, with the receivable it was deposited under.
pub fn deposits(conn: &Connection) -> Result<Vec<(Receivable, DepositRecord)>, DbError> {
	let cheques: Vec<Vec<u8>> = column(conn, "SELECT cheque FROM coinage_deposit ORDER BY rowid")?;
	let mut all = Vec::with_capacity(cheques.len());
	for cheque in cheques {
		all.extend(deposit(conn, decode(&cheque)?)?);
	}
	Ok(all)
}

/// The outgoing cheque `cheque`.
pub fn outgoing(conn: &Connection, cheque: ChequeHash) -> Result<Option<OutgoingCheque>, DbError> {
	let stored: Option<Vec<u8>> = conn
		.query_row(
			"SELECT record FROM coinage_outgoing WHERE cheque = :cheque",
			named_params! { ":cheque": cheque.encode() },
			|row| row.get(0),
		)
		.optional()?;
	stored.map(|stored| outgoing_cheque(conn, cheque, decode(&stored)?)).transpose()
}

/// Every outgoing cheque.
pub fn all_outgoing(conn: &Connection) -> Result<Vec<OutgoingCheque>, DbError> {
	let cheques: Vec<Vec<u8>> = column(conn, "SELECT cheque FROM coinage_outgoing ORDER BY rowid")?;
	let mut all = Vec::with_capacity(cheques.len());
	for cheque in cheques {
		all.extend(outgoing(conn, decode(&cheque)?)?);
	}
	Ok(all)
}

/// The outgoing cheques layer 2 posts that no post has reached yet and whose
/// claim deadline has not passed at `now_millis`.
pub fn unposted(conn: &Connection, now_millis: u64) -> Result<Vec<OutgoingCheque>, DbError> {
	Ok(all_outgoing(conn)?
		.into_iter()
		.filter(|cheque| cheque.post && !cheque.posted && cheque.claim_by > now_millis)
		.collect())
}

/// The RFC-6 operation `product` named `id`.
pub fn rfc6(conn: &Connection, product: &[u8], id: [u8; 32]) -> Result<Option<Rfc6Record>, DbError> {
	let stored: Option<Vec<u8>> = conn
		.query_row(
			"SELECT record FROM coinage_rfc6 WHERE product = :product AND id = :id",
			named_params! { ":product": product, ":id": id.as_slice() },
			|row| row.get(0),
		)
		.optional()?;
	let Some(stored) = stored else { return Ok(None) };
	let stored: StoredRfc6 = decode(&stored)?;
	Ok(Some(Rfc6Record {
		product: product.to_vec(),
		id,
		kind: stored.kind,
		amount: stored.amount,
		source: stored.source,
		funding: stored.funding,
		operation: operation_record(conn, stored.operation)?,
	}))
}

/// The RFC-6 top-ups from `source`, for N2's one-top-up-per-source rule.
pub fn top_ups_from(conn: &Connection, source: [u8; 32]) -> Result<Vec<Rfc6Record>, DbError> {
	let rows: Vec<(Vec<u8>, Vec<u8>)> = {
		let mut statement = conn.prepare_cached("SELECT product, id FROM coinage_rfc6")?;
		let mapped = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
		mapped.collect::<Result<_, _>>()?
	};
	let mut found = Vec::new();
	for (product, id) in rows {
		let Ok(id) = <[u8; 32]>::try_from(id.as_slice()) else { continue };
		if let Some(record) = rfc6(conn, &product, id)?
			&& record.kind == Rfc6Kind::TopUp
			&& record.source == Some(source)
		{
			found.push(record);
		}
	}
	Ok(found)
}

fn deposit_record(conn: &Connection, cheque: ChequeHash, stored: StoredDeposit) -> Result<DepositRecord, DbError> {
	let refunds = stored
		.refunds
		.into_iter()
		.map(|(key, cheque)| Ok(RefundRecord { operation: operation_record(conn, key)?, cheque }))
		.collect::<Result<_, DbError>>()?;
	Ok(DepositRecord {
		cheque,
		product: stored.product,
		amount: stored.amount,
		claim_by: stored.claim_by,
		import: operation_record(conn, stored.import)?,
		return_to: stored.return_to,
		refunds,
		automatic_refund_made: stored.automatic_refund_made,
	})
}

fn outgoing_cheque(conn: &Connection, hash: ChequeHash, stored: StoredOutgoing) -> Result<OutgoingCheque, DbError> {
	let reclaims =
		stored.reclaims.into_iter().map(|key| operation_record(conn, key)).collect::<Result<_, DbError>>()?;
	Ok(OutgoingCheque {
		hash,
		cheque: stored.cheque,
		source: stored.source,
		payee: stored.payee,
		export: stored.export,
		return_to: stored.return_to,
		claim_by: stored.claim_by,
		product: stored.product,
		post: stored.post,
		posted: stored.posted,
		reclaims,
	})
}

fn operation_record(conn: &Connection, key: OpKey) -> Result<OperationRecord, DbError> {
	let refused: Option<Vec<u8>> = conn
		.query_row(
			"SELECT error FROM coinage_refusal WHERE operation = :operation",
			named_params! { ":operation": key.encode() },
			|row| row.get(0),
		)
		.optional()?;
	Ok(OperationRecord { key, refused: refused.map(|error| decode(&error)).transpose()? })
}

fn update_deposit(tx: &Transaction<'_>, cheque: ChequeHash, change: impl FnOnce(&mut StoredDeposit)) -> Result<(), DbError> {
	let stored: Option<Vec<u8>> = tx
		.query_row(
			"SELECT record FROM coinage_deposit WHERE cheque = :cheque",
			named_params! { ":cheque": cheque.encode() },
			|row| row.get(0),
		)
		.optional()?;
	let Some(stored) = stored else { return Ok(()) };
	let mut stored: StoredDeposit = decode(&stored)?;
	change(&mut stored);
	tx.execute(
		"UPDATE coinage_deposit SET record = :record WHERE cheque = :cheque",
		named_params! { ":cheque": cheque.encode(), ":record": stored.encode() },
	)?;
	Ok(())
}

fn update_outgoing(
	tx: &Transaction<'_>,
	cheque: ChequeHash,
	change: impl FnOnce(&mut StoredOutgoing),
) -> Result<(), DbError> {
	let stored: Option<Vec<u8>> = tx
		.query_row(
			"SELECT record FROM coinage_outgoing WHERE cheque = :cheque",
			named_params! { ":cheque": cheque.encode() },
			|row| row.get(0),
		)
		.optional()?;
	let Some(stored) = stored else { return Ok(()) };
	let mut stored: StoredOutgoing = decode(&stored)?;
	change(&mut stored);
	tx.execute(
		"UPDATE coinage_outgoing SET record = :record WHERE cheque = :cheque",
		named_params! { ":cheque": cheque.encode(), ":record": stored.encode() },
	)?;
	Ok(())
}

fn column(conn: &Connection, sql: &str) -> Result<Vec<Vec<u8>>, DbError> {
	let mut statement = conn.prepare_cached(sql)?;
	let rows = statement.query_map([], |row| row.get(0))?;
	Ok(rows.collect::<Result<_, _>>()?)
}

fn decode<T: Decode>(bytes: &[u8]) -> Result<T, DbError> {
	T::decode(&mut &bytes[..]).map_err(|error| {
		DbError::Sqlite(rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Blob, Box::new(error)))
	})
}

#[cfg(test)]
mod tests;
