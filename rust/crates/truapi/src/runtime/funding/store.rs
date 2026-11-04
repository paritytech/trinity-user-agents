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

pub(super) use crate::store::Db;
use crate::{
	host_logic::funding::{FundingSession, FundingSessionError, PackedSession},
	store::DbError,
};

/// Format of a stored session. The stored enums pin their SCALE indexes, so
/// variants added later decode alongside the ones a row was written with; a
/// field added to a stored struct or variant takes a new format.
const SESSION_FORMAT: u16 = 5;
/// Format of a stored summary.
const SUMMARY_FORMAT: u16 = 2;

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
		let sessions = scan(tx, &SESSIONS)?;
		let summaries = scan(tx, &SUMMARIES)?;
		if let Some(format) = sessions.newer.max(summaries.newer) {
			return Ok(Err(FundingSessionError::NewerFormat { format }));
		}
		Ok(Ok((sessions.records, summaries.records)))
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

fn scan<T: Decode>(tx: &Transaction<'_>, table: &Table) -> Result<Scanned<T>, DbError> {
	let mut scanned = Scanned { records: Vec::new(), newer: None };
	let mut statement = tx.prepare_cached(table.select)?;
	let mut rows = statement.query([])?;
	while let Some(row) = rows.next()? {
		let intent: String = row.get("intent")?;
		let record: Vec<u8> = row.get("record")?;
		let input = &mut &record[..];
		match u16::decode(input) {
			Ok(stored) if stored > table.format => scanned.newer = scanned.newer.max(Some(stored)),
			Ok(stored) if stored < table.format => {
				error!(
					table = table.name,
					%intent,
					stored,
					format = table.format,
					"funding record of an older format; leaving it in place"
				);
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
mod tests {
	use futures::executor::block_on;
	use truapi::latest::{FundingDirection, FundingFailure};

	use super::*;
	use crate::{
		host_logic::funding::FundingStage, platform::ProductId, store::open_memory_core_db,
	};

	const NOW: u64 = 1_700_000_000_000;

	fn session(intent: &str) -> FundingSession {
		FundingSession::new(
			intent.to_string(),
			Some(ProductId::for_test("wallet.dot")),
			FundingDirection::In,
			Some(100),
			NOW,
		)
	}

	fn summary(intent: &str) -> PackedSession {
		PackedSession::from(&FundingSession {
			stage: FundingStage::Failed { reason: FundingFailure::Expired, settled_at_ms: NOW },
			..session(intent)
		})
	}

	fn stored(db: &Db) -> (Vec<FundingSession>, Vec<PackedSession>) {
		block_on(load(db)).expect("loaded")
	}

	#[test]
	fn sessions_and_summaries_round_trip() {
		let db = open_memory_core_db();

		block_on(write(&db, &[&session("fs_1")], Vec::new(), &[summary("fs_2")])).expect("written");

		assert_eq!(stored(&db), (vec![session("fs_1")], vec![summary("fs_2")]));
	}

	// A write names the rows it changes; the others stay as they were.
	#[test]
	fn a_write_replaces_and_deletes_only_the_rows_it_names() {
		let db = open_memory_core_db();
		block_on(write(&db, &[&session("fs_1"), &session("fs_2")], Vec::new(), &[]))
			.expect("written");
		let assigned = FundingSession {
			provider_id: Some(ProductId::for_test("ramp.dot")),
			..session("fs_1")
		};

		block_on(write(&db, &[&assigned], vec!["fs_2".to_string()], &[])).expect("written");

		assert_eq!(stored(&db).0, vec![assigned]);
	}

	// A compacted session leaves its row in the same transaction that stores
	// its summary, so a failed write loses neither.
	#[test]
	fn a_compaction_whose_summary_fails_keeps_the_session() {
		let db = open_memory_core_db();
		block_on(write(&db, &[&session("fs_1")], Vec::new(), &[])).expect("written");
		block_on(db.write(|tx| {
			Ok(tx.execute_batch(
				"CREATE TRIGGER fail BEFORE INSERT ON funding_summary
					BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
			)?)
		}))
		.expect("trigger created");

		let compacted = block_on(write(&db, &[], vec!["fs_1".to_string()], &[summary("fs_1")]));

		assert!(matches!(compacted, Err(FundingSessionError::Storage { .. })));
		assert_eq!(stored(&db), (vec![session("fs_1")], Vec::new()));
	}

	// A row of an older format may hold a session with money in flight, so it
	// is skipped and kept, never deleted.
	#[test]
	fn a_row_of_an_older_format_is_skipped_and_kept() {
		let db = open_memory_core_db();
		block_on(put_session_row(&db, "fs_old", (SESSION_FORMAT - 1, "anything").encode()));
		block_on(write(&db, &[&session("fs_1")], Vec::new(), &[])).expect("written");

		assert_eq!(
			(stored(&db).0, block_on(rows(&db, "funding_session")).len()),
			(vec![session("fs_1")], 2)
		);
	}

	// An app downgraded below the one that stored its sessions cannot read
	// them, and must not delete anything: the newer app reads them again once
	// it is back.
	#[test]
	fn a_row_of_a_newer_format_fails_the_load_and_nothing_is_deleted() {
		let db = open_memory_core_db();
		block_on(put_session_row(&db, "fs_new", (SESSION_FORMAT + 1, session("fs_new")).encode()));
		block_on(put_session_row(&db, "fs_old", (SESSION_FORMAT - 1, "anything").encode()));
		let before = block_on(rows(&db, "funding_session"));

		assert_eq!(
			(block_on(load(&db)), block_on(rows(&db, "funding_session"))),
			(Err(FundingSessionError::NewerFormat { format: SESSION_FORMAT + 1 }), before)
		);
	}

	// One bad row must not disable funding, nor be lost: it is skipped and
	// kept for diagnosis.
	#[test]
	fn a_row_that_does_not_decode_is_skipped_and_kept() {
		let db = open_memory_core_db();
		let mut corrupt = (SESSION_FORMAT, session("fs_bad")).encode();
		corrupt.push(0xff);
		block_on(put_session_row(&db, "fs_bad", corrupt.clone()));
		block_on(write(&db, &[&session("fs_1")], Vec::new(), &[])).expect("written");

		assert_eq!(
			(stored(&db).0, block_on(rows(&db, "funding_session"))),
			(
				vec![session("fs_1")],
				vec![
					("fs_1".to_string(), (SESSION_FORMAT, session("fs_1")).encode()),
					("fs_bad".to_string(), corrupt),
				]
			)
		);
	}
}
