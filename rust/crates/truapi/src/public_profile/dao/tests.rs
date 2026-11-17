use core::{future::Future, task::Poll, time::Duration};
use std::{
	collections::VecDeque,
	sync::{
		Arc, Mutex,
		atomic::{AtomicBool, Ordering},
	},
};

use futures::future::{BoxFuture, poll_fn};
use subxt::utils::WrapperKeepOpaque;

use super::*;
use crate::{
	ImageFormat, Spawner,
	public_profile::tests::{avatar_record, cached_profiles},
	store::{Db, open_temporary_core_db},
	time::ManualTimer,
};

async fn store(db: &Db, username: &'static str, record: ProfileRecord, expires_at: u64) {
	db.write(move |tx| store_record(tx, username, &record, expires_at))
		.await
		.expect("row written");
}

async fn fresh(db: &Db, username: &'static str, now: u64) -> Option<ProfileRecord> {
	db.read(move |conn| {
		Ok(stored_record(conn, username)?
			.filter(|cached| cached.is_fresh(now))
			.map(|cached| cached.record))
	})
	.await
	.expect("row read")
}

#[tokio::test]
async fn a_record_is_served_until_it_expires() {
	let (_dir, db) = open_temporary_core_db();
	let record = avatar_record(1, ImageFormat::Webp);
	store(&db, "alice.01", record.clone(), 200).await;

	assert_eq!(fresh(&db, "alice.01", 199).await, Some(record));
	assert_eq!(fresh(&db, "alice.01", 200).await, None);
}

#[tokio::test]
async fn a_clock_correction_cannot_extend_a_records_lifetime() {
	let (_dir, db) = open_temporary_core_db();
	let written_at = 10_000;
	store(&db, "alice.01", ProfileRecord::default(), written_at + RECORD_TTL_SECS).await;

	assert_eq!(
		(fresh(&db, "alice.01", written_at - 1).await, fresh(&db, "alice.01", written_at).await,),
		(None, Some(ProfileRecord::default())),
	);
}

#[tokio::test]
async fn undecodable_records_are_cache_misses() {
	let (_dir, db) = open_temporary_core_db();
	db.write(|tx| {
		tx.execute("INSERT INTO public_profiles VALUES ('broken.01', X'FF', 200, 1)", [])?;
		Ok(())
	})
	.await
	.unwrap();

	let missing = db.read(|conn| Ok(stored_record(conn, "broken.01")?.is_none())).await.unwrap();

	assert!(missing);
}

struct Scheduled {
	timer: Arc<ManualTimer>,
	tasks: Arc<Mutex<VecDeque<BoxFuture<'static, ()>>>>,
}

fn control(profiles: &mut Arc<PublicProfilesService>) -> Scheduled {
	let timer = Arc::new(ManualTimer::default());
	let tasks = Arc::new(Mutex::new(VecDeque::new()));
	let queue = tasks.clone();
	let service = Arc::get_mut(profiles).unwrap();
	service.cache.timer = timer.clone();
	service.cache.spawner = Spawner::with_test_scheduler(move |task| {
		queue.lock().unwrap().push_back(task);
	});
	Scheduled { timer, tasks }
}

async fn rows(db: &Db) -> Vec<(String, Vec<u8>, u64, u64)> {
	db.read(|conn| {
		Ok(conn
			.prepare(
				"SELECT username, public_record, expires_at, access_order
				FROM public_profiles ORDER BY access_order",
			)?
			.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?
			.collect::<Result<_, _>>()?)
	})
	.await
	.unwrap()
}

#[tokio::test]
async fn accesses_keep_the_first_deadline_and_retry_without_renewing_metadata() {
	let mut record = avatar_record(1, crate::ImageFormat::Png);
	record.entries.push(WrapperKeepOpaque::from_encoded(vec![99, 4, 5]));
	let (_directory, mut profiles, db, _platform) =
		cached_profiles(Some(record.clone()), &record, true).await;
	let Scheduled { timer, tasks } = control(&mut profiles);
	db.write(|tx| store_record(tx, "bob.02", &ProfileRecord::default(), 200))
		.await
		.unwrap();
	let mut expected = rows(&db).await;
	profiles.stored_record("alice.01").await.unwrap().unwrap();
	timer.advance(Duration::from_millis(400));
	profiles.stored_record("bob.02").await.unwrap().unwrap();
	timer.advance(Duration::from_millis(400));
	profiles.stored_record("alice.01").await.unwrap().unwrap();
	assert_eq!((timer.sleeps(), timer.remaining()), (1, vec![Duration::from_millis(200)]));
	db.write(|tx| {
		tx.execute_batch(
			"CREATE TABLE access_failure (failing INTEGER);
			INSERT INTO access_failure VALUES (1);
			CREATE TRIGGER reject_access BEFORE UPDATE OF access_order
			ON public_profiles WHEN OLD.username = 'alice.01'
			AND EXISTS (SELECT 1 FROM access_failure)
			BEGIN SELECT RAISE(ABORT, 'access update rejected'); END",
		)?;
		Ok(())
	})
	.await
	.unwrap();
	let mut flush = tasks.lock().unwrap().pop_front().unwrap();
	assert!(futures::poll!(&mut flush).is_pending());
	timer.advance(Duration::from_millis(200));
	flush.await;
	assert_eq!(
		(rows(&db).await, timer.sleeps(), timer.remaining(), tasks.lock().unwrap().len()),
		(expected.clone(), 2, vec![Duration::from_secs(1)], 1)
	);
	db.write(|tx| {
		tx.execute("DELETE FROM access_failure", [])?;
		Ok(())
	})
	.await
	.unwrap();
	let mut retry = tasks.lock().unwrap().pop_front().unwrap();
	assert!(futures::poll!(&mut retry).is_pending());
	timer.advance(Duration::from_secs(1));
	retry.await;
	expected[0].3 = 4;
	expected[1].3 = 3;
	expected.swap(0, 1);
	assert_eq!((rows(&db).await, tasks.lock().unwrap().len()), (expected, 0));
}

#[tokio::test]
async fn profile_writes_protect_pending_reads_from_eviction_and_cancel_awakened_timers() {
	let record = ProfileRecord::default();
	let (_directory, mut profiles, db, _platform) = cached_profiles(None, &record, false).await;
	let Scheduled { timer, tasks } = control(&mut profiles);
	db.write(|tx| {
		for index in 0..MAX_ROWS {
			store_record(tx, &format!("user{index}"), &ProfileRecord::default(), 200)?;
		}
		Ok(())
	})
	.await
	.unwrap();
	profiles.stored_record("user0").await.unwrap().unwrap();
	let writes = profiles.cache.writes.lock().await;
	timer.advance(Duration::from_secs(1));
	let mut flush = tasks.lock().unwrap().pop_front().unwrap();
	assert!(futures::poll!(&mut flush).is_pending());
	profiles
		.persist(Some(("alice.01".to_owned(), record.clone(), 400)))
		.await
		.unwrap();
	profiles.stored_record("user2").await.unwrap().unwrap();
	drop(writes);
	flush.await;
	let mut persisted = rows(&db).await;
	let count = persisted.len();
	persisted.retain(|row| ["user0", "user1", "user2", "alice.01"].contains(&row.0.as_str()));
	let capacity = u64::try_from(MAX_ROWS).unwrap();
	assert_eq!(
		(count, persisted),
		(
			MAX_ROWS,
			vec![
				("user2".to_owned(), record.encode(), 200, 3),
				("user0".to_owned(), record.encode(), 200, capacity + 1),
				("alice.01".to_owned(), record.encode(), 400, capacity + 2),
			]
		)
	);
}

#[tokio::test]
async fn canceled_refreshes_finish_acknowledging_without_losing_newer_reads() {
	for retouch in [false, true] {
		let record = avatar_record(1, crate::ImageFormat::Png);
		let (_directory, mut profiles, db, _platform) = cached_profiles(None, &record, false).await;
		let Scheduled { tasks, .. } = control(&mut profiles);
		let armed = Arc::new(AtomicBool::new(false));
		let gate = armed.clone();
		let (entered, started) = futures::channel::oneshot::channel();
		let (release, blocked) = std::sync::mpsc::channel();
		db.write(move |tx| {
			store_record(tx, "bob.02", &ProfileRecord::default(), 200)?;
			let mut entered = Some(entered);
			tx.commit_hook(Some(move || {
				if gate.swap(false, Ordering::SeqCst) {
					entered.take().unwrap().send(()).unwrap();
					blocked.recv().unwrap();
				}
				false
			}))?;
			Ok(())
		})
		.await
		.unwrap();
		profiles.stored_record("bob.02").await.unwrap().unwrap();
		let _timer = tasks.lock().unwrap().pop_front().unwrap();
		armed.store(true, Ordering::SeqCst);
		let mut refresh = Box::pin(profiles.refresh_profile("alice.01"));
		let persistence = tokio::time::timeout(
			Duration::from_secs(5),
			poll_fn(|context| {
				assert!(refresh.as_mut().poll(context).is_pending());
				match tasks.lock().unwrap().pop_front() {
					Some(task) => Poll::Ready(task),
					None => Poll::Pending,
				}
			}),
		)
		.await
		.unwrap();
		let persistence = tokio::spawn(persistence);
		tokio::time::timeout(Duration::from_secs(5), started).await.unwrap().unwrap();
		drop(refresh);
		if retouch {
			profiles.stored_record("bob.02").await.unwrap().unwrap();
		}
		let mut next_write = Box::pin(async {
			let _writes = profiles.cache.writes.lock().await;
			profiles
				.persist(Some(("carol.03".to_owned(), ProfileRecord::default(), 200)))
				.await
		});
		let still_serialized = profiles.cache.writes.try_lock().is_none();
		assert!(futures::poll!(&mut next_write).is_pending());
		release.send(()).unwrap();
		let (persisted, next_result) = tokio::time::timeout(Duration::from_secs(5), async {
			tokio::join!(persistence, next_write)
		})
		.await
		.unwrap();
		persisted.unwrap();
		next_result.unwrap();
		let order: Vec<_> = rows(&db).await.into_iter().map(|row| row.0).collect();
		assert_eq!(
			(still_serialized, order),
			(
				true,
				if retouch {
					vec!["alice.01".to_owned(), "bob.02".to_owned(), "carol.03".to_owned()]
				} else {
					vec!["bob.02".to_owned(), "alice.01".to_owned(), "carol.03".to_owned()]
				}
			)
		);
	}
}
