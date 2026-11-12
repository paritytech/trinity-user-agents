//! Funding sessions in the core database: one `funding_session` row per
//! stored session and one `funding_summary` row per compacted one, keyed by
//! intent.
//!
//! Each record is a SCALE value led by its format as a `u16`. Loading fails on
//! a row of a newer format, which a newer app left before a downgrade, and
//! skips one of an older format or one that does not decode. A write touches
//! only the rows of the sessions it names, so a skipped row stays in place: a
//! session with money in flight is never lost to a format change, and a later
//! core that can read the old format still finds it.

use parity_scale_codec::{Decode, Encode};
use rusqlite::{Transaction, named_params};
use tracing::error;

use super::PackedSession;
pub(super) use crate::store::Db;
use crate::{
	host_logic::funding::{FundingSession, FundingSessionError, FundingSessionSummary},
	store::DbError,
};

/// Format of a stored session. The stored enums pin their SCALE indexes, so
/// variants added later decode alongside the ones a row was written with; a
/// field added to a stored struct or variant takes a new format.
const SESSION_FORMAT: u16 = 5;
/// Format of a stored summary. Format 2 lacked whether money moved, which
/// is read as moved when the summary settled or refunded anything.
const SUMMARY_FORMAT: u16 = 3;

/// A table of records keyed by intent, with its queries.
struct Table {
	name: &'static str,
	format: u16,
	select: &'static str,
	upsert: &'static str,
	delete: &'static str,
}

const SESSIONS: Table = Table {
	name: "funding_session",
	format: SESSION_FORMAT,
	select: "SELECT intent, record FROM funding_session",
	upsert: "INSERT INTO funding_session (intent, record) VALUES (:intent, :record)
		ON CONFLICT (intent) DO UPDATE SET record = excluded.record",
	delete: "DELETE FROM funding_session WHERE intent = :intent",
};

const SUMMARIES: Table = Table {
	name: "funding_summary",
	format: SUMMARY_FORMAT,
	select: "SELECT intent, record FROM funding_summary",
	upsert: "INSERT INTO funding_summary (intent, record) VALUES (:intent, :record)
		ON CONFLICT (intent) DO UPDATE SET record = excluded.record",
	delete: "DELETE FROM funding_summary WHERE intent = :intent",
};

impl From<DbError> for FundingSessionError {
	fn from(error: DbError) -> Self {
		Self::Storage { reason: error.to_string() }
	}
}

/// Read every stored session and summary, in no particular order. A row of a
/// newer format fails the load with [`FundingSessionError::NewerFormat`], so
/// the registry, which writes only after a load, leaves every row as it is.
pub(super) async fn load(
	db: &Db,
) -> Result<(Vec<FundingSession>, Vec<PackedSession>), FundingSessionError> {
	db.write(|tx| {
		let sessions = scan(tx, &SESSIONS, |_, _| None)?;
		let summaries = scan(tx, &SUMMARIES, summary_of_format)?;
		if let Some(format) = sessions.newer.max(summaries.newer) {
			return Ok(Err(FundingSessionError::NewerFormat { format }));
		}
		let summaries = summaries
			.records
			.into_iter()
			.map(|packed: PackedSession| PackedSession {
				summary: FundingSessionSummary {
					money_moved: packed.money_moved,
					..packed.summary
				}
				.with_ended(),
				..packed
			})
			.collect();
		Ok(Ok((sessions.records, summaries)))
	})
	.await?
}

/// Store `sessions` and `summaries` over their rows and delete the rows of
/// `removed`, in one transaction.
pub(super) async fn write(
	db: &Db,
	sessions: &[&FundingSession],
	removed: Vec<String>,
	summaries: &[PackedSession],
) -> Result<(), FundingSessionError> {
	let sessions: Vec<_> = sessions
		.iter()
		.map(|session| (session.intent.clone(), (SESSION_FORMAT, session).encode()))
		.collect();
	let summaries: Vec<_> = summaries
		.iter()
		.map(|packed| (packed.summary.intent.clone(), (SUMMARY_FORMAT, packed).encode()))
		.collect();
	db.write(move |tx| {
		upsert(tx, &SESSIONS, &sessions)?;
		delete(tx, &SESSIONS, &removed)?;
		upsert(tx, &SUMMARIES, &summaries)
	})
	.await?;
	Ok(())
}

/// The rows of one table, by what this core makes of them.
struct Scanned<T> {
	records: Vec<T>,
	/// The newest format above this core's, when a row has one.
	newer: Option<u16>,
}

/// A summary row of an older format this core still reads.
fn summary_of_format(format: u16, input: &mut &[u8]) -> Option<PackedSession> {
	if format != 2 {
		return None;
	}
	let (summary, acknowledged) = <(FundingSessionSummary, bool)>::decode(input).ok()?;
	let money_moved = summary.refunded.is_some() || summary.stage.settled_amount().is_some();
	Some(PackedSession { summary, acknowledged, money_moved })
}

/// The rows of `table`, reading a row of an older format through `older`
/// when it can.
fn scan<T: Decode>(
	tx: &Transaction<'_>,
	table: &Table,
	older: impl Fn(u16, &mut &[u8]) -> Option<T>,
) -> Result<Scanned<T>, DbError> {
	let mut scanned = Scanned { records: Vec::new(), newer: None };
	let mut statement = tx.prepare_cached(table.select)?;
	let mut rows = statement.query([])?;
	while let Some(row) = rows.next()? {
		let intent: String = row.get("intent")?;
		let record: Vec<u8> = row.get("record")?;
		let input = &mut &record[..];
		match u16::decode(input) {
			Ok(stored) if stored > table.format => scanned.newer = scanned.newer.max(Some(stored)),
			Ok(stored) if stored < table.format => match older(stored, input) {
				Some(value) if input.is_empty() => scanned.records.push(value),
				_ => error!(
					table = table.name,
					%intent,
					stored,
					format = table.format,
					"funding record of an older format; leaving it in place"
				),
			},
			Ok(_) => match T::decode(input) {
				Ok(value) if input.is_empty() => scanned.records.push(value),
				_ => unreadable(table, &intent, &record),
			},
			Err(_) => unreadable(table, &intent, &record),
		}
	}
	Ok(scanned)
}

fn unreadable(table: &Table, intent: &str, record: &[u8]) {
	error!(
		table = table.name,
		%intent,
		bytes = record.len(),
		"funding record does not decode; leaving it in place"
	);
}

fn upsert(tx: &Transaction<'_>, table: &Table, rows: &[(String, Vec<u8>)]) -> Result<(), DbError> {
	let mut statement = tx.prepare_cached(table.upsert)?;
	for (intent, record) in rows {
		statement.execute(named_params! { ":intent": intent, ":record": record })?;
	}
	Ok(())
}

fn delete(tx: &Transaction<'_>, table: &Table, intents: &[String]) -> Result<(), DbError> {
	let mut statement = tx.prepare_cached(table.delete)?;
	for intent in intents {
		statement.execute(named_params! { ":intent": intent })?;
	}
	Ok(())
}

/// Make every write to the funding tables fail while `failing`, as a full
/// disk would. The triggers stay and check a switch row, as the writer's
/// authorizer turns a `DROP TRIGGER` into a no-op.
#[cfg(test)]
pub(crate) async fn fail_writes(db: &Db, failing: bool) {
	const TRIGGERS: &str = "
		CREATE TABLE IF NOT EXISTS funding_write_failure (failing INTEGER);
		CREATE TRIGGER IF NOT EXISTS funding_session_insert BEFORE INSERT ON funding_session
			WHEN EXISTS (SELECT 1 FROM funding_write_failure)
			BEGIN SELECT RAISE(ABORT, 'disk full'); END;
		CREATE TRIGGER IF NOT EXISTS funding_session_delete BEFORE DELETE ON funding_session
			WHEN EXISTS (SELECT 1 FROM funding_write_failure)
			BEGIN SELECT RAISE(ABORT, 'disk full'); END;
		CREATE TRIGGER IF NOT EXISTS funding_summary_insert BEFORE INSERT ON funding_summary
			WHEN EXISTS (SELECT 1 FROM funding_write_failure)
			BEGIN SELECT RAISE(ABORT, 'disk full'); END;
		DELETE FROM funding_write_failure;";
	db.write(move |tx| {
		tx.execute_batch(TRIGGERS)?;
		if failing {
			tx.execute("INSERT INTO funding_write_failure (failing) VALUES (1)", [])?;
		}
		Ok(())
	})
	.await
	.expect("funding writes switched");
}

/// Every session stored in `db`, by intent.
#[cfg(test)]
pub(crate) async fn stored_sessions(db: &Db) -> Vec<FundingSession> {
	let (mut sessions, _) = load(db).await.expect("funding sessions load");
	sessions.sort_by(|a, b| a.intent.cmp(&b.intent));
	sessions
}

/// The raw rows of `table`, by intent.
#[cfg(test)]
pub(super) async fn rows(db: &Db, table: &'static str) -> Vec<(String, Vec<u8>)> {
	db.read(move |conn| {
		let mut statement =
			conn.prepare(&format!("SELECT intent, record FROM {table} ORDER BY intent"))?;
		let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
		Ok(rows.collect::<Result<_, _>>()?)
	})
	.await
	.expect("funding rows read")
}

/// Store `record` as session `intent`'s row, as it is.
#[cfg(test)]
pub(crate) async fn put_session_row(db: &Db, intent: &str, record: Vec<u8>) {
	let rows = vec![(intent.to_string(), record)];
	db.write(move |tx| upsert(tx, &SESSIONS, &rows))
		.await
		.expect("funding row written");
}

#[cfg(test)]
mod tests;
