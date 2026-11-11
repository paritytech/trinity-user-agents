use futures::executor::block_on;
use truapi::latest::{FundingDirection, FundingFailure};

use super::*;
use crate::{host_logic::funding::FundingStage, platform::ProductId, store::open_memory_core_db};

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
	block_on(write(&db, &[&session("fs_1"), &session("fs_2")], Vec::new(), &[])).expect("written");
	let assigned =
		FundingSession { provider_id: Some(ProductId::for_test("ramp.dot")), ..session("fs_1") };

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

// A summary stored before whether money moved was kept still loads, read as
// moved when it settled or refunded anything.
#[test]
fn a_summary_of_the_previous_format_still_loads() {
	let packed = summary("fs_old");
	let row = (packed.summary.clone(), packed.acknowledged).encode();
	let read = summary_of_format(2, &mut &row[..]).expect("read");
	assert_eq!(
		(read.money_moved, read.acknowledged, summary_of_format(1, &mut &row[..]).is_none()),
		(false, packed.acknowledged, true)
	);
}
