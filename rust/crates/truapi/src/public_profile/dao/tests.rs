use subxt::utils::WrapperKeepOpaque;

use super::*;
use crate::{
	ImageFormat,
	public_profile::record::{ProfileAvatar, ProfileEntry},
	store::{Db, open_temporary_core_db},
};

fn record_with_avatar() -> ProfileRecord {
	ProfileRecord {
		entries: vec![WrapperKeepOpaque::from_value(ProfileEntry::Avatar(ProfileAvatar {
			cid: [1; 32].into(),
			format: ImageFormat::Webp,
		}))],
	}
}

async fn store(db: &Db, username: &'static str, record: ProfileRecord, expires_at: u64) {
	db.write(move |tx| store_record(tx, username, &record, expires_at))
		.await
		.expect("row written");
}

async fn stored(db: &Db, username: &'static str) -> Option<CachedRecord> {
	db.write(move |tx| stored_record(tx, username)).await.expect("row read")
}

async fn row_count(db: &Db) -> u32 {
	db.read(
		|conn| Ok(conn.query_row("SELECT COUNT(*) FROM public_profiles", [], |row| row.get(0))?),
	)
	.await
	.expect("rows counted")
}

#[tokio::test]
async fn a_record_becomes_stale_at_its_deadline_without_being_removed() {
	let (_dir, db) = open_temporary_core_db();
	store(&db, "alice.01", record_with_avatar(), 200).await;

	let before = stored(&db, "alice.01")
		.await
		.map(|cached| (cached.is_fresh(199), cached.record));
	let after = stored(&db, "alice.01")
		.await
		.map(|cached| (cached.is_fresh(200), cached.record));

	assert_eq!(
		(before, after),
		(Some((true, record_with_avatar())), Some((false, record_with_avatar())))
	);
}

#[tokio::test]
async fn a_name_without_a_record_is_cached_as_an_empty_one() {
	let (_dir, db) = open_temporary_core_db();
	store(&db, "bob.02", ProfileRecord::default(), 200).await;

	assert_eq!(
		stored(&db, "bob.02").await.map(|cached| cached.record),
		Some(ProfileRecord::default())
	);
}

#[tokio::test]
async fn a_clock_correction_cannot_extend_a_records_lifetime() {
	let (_dir, db) = open_temporary_core_db();
	let written_at = 10_000;
	store(&db, "alice.01", ProfileRecord::default(), written_at + RECORD_TTL_SECS).await;

	let cached = stored(&db, "alice.01").await.unwrap();

	assert_eq!((cached.is_fresh(written_at - 1), cached.is_fresh(written_at)), (false, true));
}

#[tokio::test]
async fn stale_metadata_survives_other_profile_writes() {
	let (_dir, db) = open_temporary_core_db();
	store(&db, "alice.01", record_with_avatar(), 200).await;
	store(&db, "bob.02", ProfileRecord::default(), 400).await;

	let stored: Vec<u8> = db
		.read(|conn| {
			Ok(conn.query_row(
				"SELECT public_record FROM public_profiles WHERE username = 'alice.01'",
				[],
				|row| row.get(0),
			)?)
		})
		.await
		.unwrap();

	assert_eq!(stored, record_with_avatar().encode());
}

#[tokio::test]
async fn reading_a_record_protects_it_from_capacity_eviction() {
	let (_dir, db) = open_temporary_core_db();
	db.write(|tx| {
		for index in 0..MAX_ROWS {
			store_record(
				tx,
				&format!("user{index}"),
				&ProfileRecord::default(),
				200 + u64::from(index),
			)?;
		}
		Ok(())
	})
	.await
	.unwrap();

	assert_eq!(
		stored(&db, "user0").await.map(|cached| cached.record),
		Some(ProfileRecord::default())
	);
	store(&db, "newest.01", ProfileRecord::default(), 100 + RECORD_TTL_SECS).await;

	assert_eq!(
		(
			stored(&db, "user0").await.map(|cached| cached.record),
			stored(&db, "user1").await.map(|cached| cached.record),
			row_count(&db).await
		),
		(Some(ProfileRecord::default()), None, MAX_ROWS),
	);
}

#[tokio::test]
async fn stale_reads_update_recency_without_renewing_freshness() {
	let (_dir, db) = open_temporary_core_db();
	store(&db, "alice.01", record_with_avatar(), 200).await;
	store(&db, "bob.02", ProfileRecord::default(), 400).await;

	let cached = db
		.write(|tx| {
			let cached = stored_record(tx, "alice.01")?.unwrap();
			let order: Vec<String> = tx
				.prepare("SELECT username FROM public_profiles ORDER BY access_order")?
				.query_map([], |row| row.get(0))?
				.collect::<Result<_, _>>()?;
			Ok((cached.is_fresh(300), cached.record, order))
		})
		.await
		.unwrap();

	assert_eq!(
		cached,
		(false, record_with_avatar(), vec!["bob.02".to_owned(), "alice.01".to_owned()]),
	);
}

#[tokio::test]
async fn undecodable_records_are_missing_and_do_not_gain_recency() {
	let (_dir, db) = open_temporary_core_db();
	db.write(|tx| {
		tx.execute("INSERT INTO public_profiles VALUES ('broken.01', X'FF', 200, 1)", [])?;
		Ok(())
	})
	.await
	.unwrap();

	let (missing, order) = db
		.write(|tx| {
			let missing = stored_record(tx, "broken.01")?.is_none();
			let order: u64 = tx.query_row(
				"SELECT access_order FROM public_profiles WHERE username = 'broken.01'",
				[],
				|row| row.get(0),
			)?;
			Ok((missing, order))
		})
		.await
		.unwrap();

	assert_eq!((missing, order), (true, 1));
}
