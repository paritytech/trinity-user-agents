//! C1 · Registration: the rows that tie each of the durability layer's
//! records to this layer's assets: the inputs its transaction consumes, the
//! outputs it creates, its operation, the unload tokens it presents, and the
//! records it depends on.
//!
//! These rows are the only place an asset's use is recorded. They are written
//! in the registration callback of the durability layer's registration, so
//! the rows and the record commit together or not at all (invariant 1), and
//! they are never deleted.

use std::collections::{BTreeMap, HashMap, HashSet};

use parity_scale_codec::{Decode, Encode};
use rusqlite::{Connection, OptionalExtension, Transaction, named_params, types::Type};

use super::{
	Asset,
	ledger::{self, TokenId},
	operation::OpKey,
};
use crate::{
	durable::{self, DomainId, DurableTxId, DurableTxState, DurableTxStatus},
	store::DbError,
};

/// The domain of every coinage transaction except the installation
/// registry's append.
pub fn coinage_domain() -> DomainId {
	DomainId::new("coinage")
}

/// The domain of the installation registry's append, on Asset Hub.
pub fn registry_domain() -> DomainId {
	DomainId::new("coinage-registry")
}

/// What one record's transaction consumes, creates and presents.
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
pub struct Rows {
	/// The assets it consumes.
	pub inputs: Vec<Asset>,
	/// The fresh assets it creates; never a peer's coin.
	pub outputs: Vec<Asset>,
	/// The unload tokens it presents.
	pub tokens: Vec<TokenId>,
}

/// Which inputs with a disclosure mark a registration may take
/// (invariant 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
	/// None.
	Ordinary,
	/// A coin this layer exported, claimed like any payee would.
	Import,
	/// A marked coin still in its account, moved to a fresh account.
	Reclaim,
}

/// Why a registration was refused. Nothing was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum RegisterError {
	/// The asset is an input of a transaction that has not failed
	/// (invariant 2).
	#[display("the asset is already an input of another transaction")]
	InputTaken(#[error(not(source))] Asset),
	/// The token is presented by a transaction that has not failed
	/// (invariant 2).
	#[display("the unload token is already presented by another transaction")]
	TokenTaken(#[error(not(source))] TokenId),
	/// The output is already recorded, or is a peer's coin (invariant 3).
	#[display("the output is not fresh")]
	OutputNotFresh(#[error(not(source))] Asset),
	/// The input's secret may have been disclosed (invariant 4).
	#[display("the input's secret may have been disclosed")]
	Disclosed(#[error(not(source))] Asset),
}

/// Checks every row of `records` and writes them all, or returns the first
/// failure and writes nothing.
///
/// Runs in the registration callback, inside the durability layer's
/// registration transaction, after the records are inserted; `records` pairs
/// each record with its rows in the order they were planned. The dependency
/// on the record that mints an input is computed here (invariant 5).
pub fn register(
	tx: &Transaction<'_>,
	domain: &DomainId,
	operation: &OpKey,
	admission: Admission,
	records: &[(DurableTxId, Rows)],
) -> Result<Result<(), RegisterError>, DbError> {
	let mut inputs = HashSet::new();
	let mut tokens = HashSet::new();
	let mut outputs = HashSet::new();
	for (_, rows) in records {
		for input in &rows.inputs {
			if !inputs.insert(*input) || held(tx, input)? {
				return Ok(Err(RegisterError::InputTaken(*input)));
			}
			let marked = ledger::mark(tx, input)?.is_some();
			if marked && admission == Admission::Ordinary {
				return Ok(Err(RegisterError::Disclosed(*input)));
			}
		}
		for token in &rows.tokens {
			if !tokens.insert(*token) || presented(tx, token)? {
				return Ok(Err(RegisterError::TokenTaken(*token)));
			}
		}
		for output in &rows.outputs {
			let fresh = !matches!(output, Asset::PeerCoin(_)) && creator(tx, output)?.is_none();
			if !outputs.insert(*output) || !fresh {
				return Ok(Err(RegisterError::OutputNotFresh(*output)));
			}
		}
	}

	for (record, rows) in records {
		tx.execute(
			"INSERT INTO coinage_record (record, domain, operation) VALUES (:record, :domain, :operation)",
			named_params! { ":record": record.0, ":domain": domain.as_str(), ":operation": operation.encode() },
		)?;
		for input in &rows.inputs {
			if let Some(minter) = creator(tx, input)? {
				tx.execute(
					"INSERT OR IGNORE INTO coinage_dependency (record, depends_on) VALUES (:record, :minter)",
					named_params! { ":record": record.0, ":minter": minter.0 },
				)?;
			}
			tx.execute(
				"INSERT INTO coinage_input (asset, record) VALUES (:asset, :record)",
				named_params! { ":asset": input.encode(), ":record": record.0 },
			)?;
		}
		for output in &rows.outputs {
			tx.execute(
				"INSERT INTO coinage_output (asset, record) VALUES (:asset, :record)",
				named_params! { ":asset": output.encode(), ":record": record.0 },
			)?;
		}
		for token in &rows.tokens {
			tx.execute(
				"INSERT INTO coinage_token (token, record) VALUES (:token, :record)",
				named_params! { ":token": token.encode(), ":record": record.0 },
			)?;
		}
	}
	Ok(Ok(()))
}

/// Whether `asset` is an input of a transaction that has not failed: the
/// layer's one form of mutual exclusion (invariant 2).
pub fn held(conn: &Connection, asset: &Asset) -> Result<bool, DbError> {
	let records = ids(conn, "SELECT record FROM coinage_input WHERE asset = :key", &asset.encode())?;
	any_not_failed(conn, &records)
}

/// The one record that creates `asset` (invariant 3).
pub fn creator(conn: &Connection, asset: &Asset) -> Result<Option<DurableTxId>, DbError> {
	let record: Option<i64> = conn
		.query_row(
			"SELECT record FROM coinage_output WHERE asset = :asset",
			named_params! { ":asset": asset.encode() },
			|row| row.get(0),
		)
		.optional()?;
	Ok(record.map(DurableTxId))
}

fn presented(conn: &Connection, token: &TokenId) -> Result<bool, DbError> {
	let records = ids(conn, "SELECT record FROM coinage_token WHERE token = :key", &token.encode())?;
	any_not_failed(conn, &records)
}

fn ids(conn: &Connection, sql: &str, key: &[u8]) -> Result<Vec<DurableTxId>, DbError> {
	let mut statement = conn.prepare_cached(sql)?;
	let rows = statement.query_map(named_params! { ":key": key }, |row| row.get::<_, i64>(0).map(DurableTxId))?;
	Ok(rows.collect::<Result<_, _>>()?)
}

/// Whether any of `records` has not failed, by the durability layer's
/// status.
fn any_not_failed(conn: &Connection, records: &[DurableTxId]) -> Result<bool, DbError> {
	for record in records {
		let failed = matches!(state(conn, *record)?, Some(state) if matches!(state.status, DurableTxStatus::Failure(_)));
		if !failed {
			return Ok(true);
		}
	}
	Ok(false)
}

/// The durability layer's state of one of this layer's records.
fn state(conn: &Connection, record: DurableTxId) -> Result<Option<DurableTxState>, DbError> {
	let stored: Option<(String, Vec<u8>)> = conn
		.query_row(
			"SELECT domain, operation FROM coinage_record WHERE record = :record",
			named_params! { ":record": record.0 },
			|row| Ok((row.get(0)?, row.get(1)?)),
		)
		.optional()?;
	let Some((domain, operation)) = stored else { return Ok(None) };
	let operation = decode::<OpKey>(&operation)?;
	let states = durable::group(conn, &DomainId::new(domain), &operation.group())?;
	Ok(states.into_iter().find(|state| state.id == record))
}

/// One of this layer's records: its operation, its rows, the records it
/// depends on, and the durability layer's state of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordFacts {
	/// The record's identity.
	pub record: DurableTxId,
	/// The domain it was registered in.
	pub domain: DomainId,
	/// The operation it belongs to.
	pub operation: OpKey,
	/// What it consumes, creates and presents.
	pub rows: Rows,
	/// The records that mint one of its inputs.
	pub depends_on: Vec<DurableTxId>,
	/// The durability layer's state of it.
	pub state: DurableTxState,
}

/// Every record of this layer, with the indexes B1 and D1 read: who consumes
/// an asset, who creates it, and who presents a token.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Registrations {
	records: BTreeMap<DurableTxId, RecordFacts>,
	consumers: HashMap<Asset, Vec<DurableTxId>>,
	creators: HashMap<Asset, DurableTxId>,
	presenters: HashMap<TokenId, Vec<DurableTxId>>,
}

impl Registrations {
	/// One record.
	pub fn record(&self, record: DurableTxId) -> Option<&RecordFacts> {
		self.records.get(&record)
	}

	/// Every record, in registration order.
	pub fn records(&self) -> impl Iterator<Item = &RecordFacts> {
		self.records.values()
	}

	/// The records of one operation, in registration order.
	pub fn of_operation<'a>(&'a self, operation: &'a OpKey) -> impl Iterator<Item = &'a RecordFacts> {
		self.records.values().filter(move |facts| &facts.operation == operation)
	}

	/// Every record that has `asset` as an input.
	pub fn consumers(&self, asset: &Asset) -> impl Iterator<Item = &RecordFacts> {
		self.consumers.get(asset).into_iter().flatten().filter_map(|record| self.records.get(record))
	}

	/// The one record that creates `asset`.
	pub fn creator(&self, asset: &Asset) -> Option<&RecordFacts> {
		self.creators.get(asset).and_then(|record| self.records.get(record))
	}

	/// Every record that presents `token`.
	pub fn presenters(&self, token: &TokenId) -> impl Iterator<Item = &RecordFacts> {
		self.presenters.get(token).into_iter().flatten().filter_map(|record| self.records.get(record))
	}
}

/// Every record of this layer with its rows and state, read from `conn`.
pub fn load(conn: &Connection) -> Result<Registrations, DbError> {
	let headers = all(conn, "SELECT record, domain, operation FROM coinage_record ORDER BY record", |row| {
		Ok((DurableTxId(row.get(0)?), DomainId::new(row.get::<_, String>(1)?), row.get::<_, Vec<u8>>(2)?))
	})?;
	let mut states = HashMap::new();
	let mut operations = HashMap::new();
	for (_, domain, operation) in &headers {
		if operations.contains_key(operation) {
			continue;
		}
		let key = decode::<OpKey>(operation)?;
		for state in durable::group(conn, domain, &key.group())? {
			states.insert(state.id, state);
		}
		operations.insert(operation.clone(), key);
	}
	let mut rows: HashMap<DurableTxId, Rows> = HashMap::new();
	let mut depends_on: HashMap<DurableTxId, Vec<DurableTxId>> = HashMap::new();
	let mut consumers: HashMap<Asset, Vec<DurableTxId>> = HashMap::new();
	let mut creators = HashMap::new();
	let mut presenters: HashMap<TokenId, Vec<DurableTxId>> = HashMap::new();
	for (asset, record) in pairs::<Asset>(conn, "SELECT asset, record FROM coinage_input ORDER BY record")? {
		rows.entry(record).or_default().inputs.push(asset);
		consumers.entry(asset).or_default().push(record);
	}
	for (asset, record) in pairs::<Asset>(conn, "SELECT asset, record FROM coinage_output ORDER BY record")? {
		rows.entry(record).or_default().outputs.push(asset);
		creators.insert(asset, record);
	}
	for (token, record) in pairs::<TokenId>(conn, "SELECT token, record FROM coinage_token ORDER BY record")? {
		rows.entry(record).or_default().tokens.push(token);
		presenters.entry(token).or_default().push(record);
	}
	let dependencies = all(conn, "SELECT record, depends_on FROM coinage_dependency", |row| {
		Ok((DurableTxId(row.get(0)?), DurableTxId(row.get(1)?)))
	})?;
	for (record, minter) in dependencies {
		depends_on.entry(record).or_default().push(minter);
	}
	let records = headers
		.into_iter()
		.filter_map(|(record, domain, operation)| {
			let state = states.remove(&record)?;
			let facts = RecordFacts {
				record,
				domain,
				operation: operations.get(&operation)?.clone(),
				rows: rows.remove(&record).unwrap_or_default(),
				depends_on: depends_on.remove(&record).unwrap_or_default(),
				state,
			};
			Some((record, facts))
		})
		.collect();
	Ok(Registrations { records, consumers, creators, presenters })
}

fn pairs<T: Decode>(conn: &Connection, sql: &str) -> Result<Vec<(T, DurableTxId)>, DbError> {
	all(conn, sql, |row| {
		let bytes: Vec<u8> = row.get(0)?;
		let value = T::decode(&mut bytes.as_slice())
			.map_err(|error| rusqlite::Error::FromSqlConversionFailure(0, Type::Blob, Box::new(error)))?;
		Ok((value, DurableTxId(row.get(1)?)))
	})
}

fn all<T>(
	conn: &Connection,
	sql: &str,
	map: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> Result<Vec<T>, DbError> {
	let mut statement = conn.prepare_cached(sql)?;
	let rows = statement.query_map([], map)?;
	Ok(rows.collect::<Result<_, _>>()?)
}

fn decode<T: Decode>(bytes: &[u8]) -> Result<T, DbError> {
	T::decode(&mut &bytes[..])
		.map_err(|error| DbError::Sqlite(rusqlite::Error::FromSqlConversionFailure(0, Type::Blob, Box::new(error))))
}

#[cfg(test)]
mod tests;
