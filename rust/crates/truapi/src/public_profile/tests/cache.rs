use std::sync::Mutex;

use futures::FutureExt;
use parity_scale_codec::Encode;
use subxt::utils::WrapperKeepOpaque;

use super::*;

fn profile_responder(record: &ProfileRecord) -> crate::test_support::RpcResponder {
	let contracts = Mutex::new(ProfileContracts::new(Ok(STANDARD.encode(record.encode()))));
	Arc::new(move |request| {
		crate::test_support::legacy_asset_hub_frames(
			request,
			|_| Some([0x11; 20].to_vec()),
			|dest, input| {
				let data = contracts
					.lock()
					.unwrap()
					.view(dest, input.to_vec())
					.now_or_never()
					.unwrap()
					.unwrap();
				crate::test_support::view_output(data, false)
			},
		)
	})
}

#[tokio::test]
async fn refreshing_an_unchanged_avatar_renews_only_the_profile_cache() {
	for (expires_in, force) in [(0, false), (RECORD_TTL_SECS, true)] {
		let record = ProfileRecord {
			entries: vec![WrapperKeepOpaque::from_value(ProfileEntry::Avatar(ProfileAvatar {
				cid: [9; 32].into(),
				format: crate::ImageFormat::Png,
			}))],
		};
		let platform = Arc::new(StubPlatform {
			rpc_responder: Some(profile_responder(&record)),
			..Default::default()
		});
		let services = services_with_platform(platform.clone());
		let (_directory, db) = crate::store::open_temporary_core_db();
		let now = current_unix_secs();
		let stored = record.clone();
		db.write(move |tx| dao::store_record(tx, "alice.01", &stored, now + expires_in))
			.await
			.unwrap();
		assert!(services.install_core_db(db.clone()).is_some());

		let profile = services.public_profiles.resolve("alice.01", force).await.unwrap();
		let (cached, expires_at) = db
			.write(move |tx| {
				let cached = dao::stored_record(tx, "alice.01")?
					.filter(|cached| cached.is_fresh(now))
					.map(|cached| cached.record);
				let expires_at: u64 = tx.query_row(
					"SELECT expires_at FROM public_profiles WHERE username = 'alice.01'",
					[],
					|row| row.get(0),
				)?;
				Ok((cached, expires_at))
			})
			.await
			.unwrap();

		assert_eq!(
			(profile, cached, platform.chain_connects.lock().unwrap().clone()),
			(PublicProfile { avatar: record.avatar() }, Some(record), vec![[3; 32]])
		);
		assert!(
			(now + RECORD_TTL_SECS..=current_unix_secs() + RECORD_TTL_SECS).contains(&expires_at)
		);
	}
}

/// A native profile service with configured storage and a deterministic chain response.
pub async fn cached_profiles(
	stored: Option<ProfileRecord>,
	resolved: &ProfileRecord,
	fresh: bool,
) -> (tempfile::TempDir, Arc<PublicProfilesService>, crate::store::Db, Arc<StubPlatform>) {
	let platform = Arc::new(StubPlatform {
		rpc_responder: Some(profile_responder(resolved)),
		..Default::default()
	});
	let services = services_with_platform(platform.clone());
	let (directory, db) = crate::store::open_temporary_core_db();
	if let Some(stored) = stored {
		let now = current_unix_secs();
		let expires_at = if fresh { now + RECORD_TTL_SECS / 2 } else { now };
		db.write(move |tx| dao::store_record(tx, "alice.01", &stored, expires_at))
			.await
			.unwrap();
	}
	assert!(services.install_core_db(db.clone()).is_some());
	(directory, services.public_profiles.clone(), db, platform)
}

/// Distinct avatar records for cache and stream assertions.
pub fn avatar_record(cid: u8, format: crate::ImageFormat) -> ProfileRecord {
	ProfileRecord {
		entries: vec![WrapperKeepOpaque::from_value(ProfileEntry::Avatar(ProfileAvatar {
			cid: [cid; 32].into(),
			format,
		}))],
	}
}

#[tokio::test]
async fn cached_profiles_do_not_wait_for_the_database_writer() {
	let record = avatar_record(1, crate::ImageFormat::Png);
	let expected = PublicProfile { avatar: record.avatar() };
	let (_directory, profiles, db, _platform) =
		cached_profiles(Some(record.clone()), &record, true).await;
	let (entered, started) = futures::channel::oneshot::channel();
	let (release, blocked) = std::sync::mpsc::channel();
	let writer = tokio::spawn(async move {
		db.write(move |_| {
			entered.send(()).unwrap();
			blocked.recv().unwrap();
			Ok(())
		})
		.await
		.unwrap();
	});
	started.await.unwrap();
	let cached = tokio::time::timeout(
		core::time::Duration::from_secs(1),
		profiles.resolve("alice.01", false),
	)
	.await;
	release.send(()).unwrap();
	writer.await.unwrap();

	assert_eq!(cached.expect("cached reads use the reader pool").unwrap(), expected);
}

#[tokio::test]
async fn skipping_stale_metadata_emits_the_resolved_profile_even_when_unchanged() {
	use futures::TryStreamExt;

	let avatar = avatar_record(1, crate::ImageFormat::Png);
	for force_fresh in [false, true] {
		for (stored, mut resolved) in [
			(avatar.clone(), avatar_record(2, crate::ImageFormat::Png)),
			(avatar.clone(), avatar.clone()),
			(avatar.clone(), ProfileRecord::default()),
			(ProfileRecord::default(), ProfileRecord::default()),
			(ProfileRecord::default(), avatar.clone()),
		] {
			resolved.entries.push(WrapperKeepOpaque::from_encoded(vec![99, 5, 6]));
			let expected = PublicProfile { avatar: resolved.avatar() };
			let (_directory, profiles, db, platform) =
				cached_profiles(Some(stored), &resolved, false).await;

			let emitted: Vec<_> = profiles
				.observe(
					"alice.01".to_owned(),
					PublicProfileResolutionOptions { force_fresh, skip_stale: true },
				)
				.try_collect()
				.await
				.unwrap();
			let retained = db
				.write(|tx| {
					Ok(dao::stored_record(tx, "alice.01")?.map(|cached| {
						(cached.is_fresh(current_unix_secs() + RECORD_TTL_SECS / 2), cached.record)
					}))
				})
				.await
				.unwrap();

			assert_eq!(
				(emitted, retained, platform.chain_connects.lock().unwrap().clone()),
				(vec![expected], Some((true, resolved)), vec![[3; 32]]),
			);
		}
	}
}

#[tokio::test]
async fn a_fresh_profile_stream_completes_without_network_access() {
	use futures::TryStreamExt;

	for skip_stale in [false, true] {
		for record in [ProfileRecord::default(), avatar_record(1, crate::ImageFormat::Png)] {
			let expected = PublicProfile { avatar: record.avatar() };
			let (_directory, profiles, _db, platform) =
				cached_profiles(Some(record.clone()), &record, true).await;

			let emitted: Vec<_> = profiles
				.observe(
					"alice.01".to_owned(),
					PublicProfileResolutionOptions { force_fresh: false, skip_stale },
				)
				.try_collect()
				.await
				.unwrap();

			assert_eq!(
				(emitted, platform.chain_connects.lock().unwrap().clone()),
				(vec![expected], vec![])
			);
		}
	}
}

#[tokio::test]
async fn forcing_fresh_metadata_emits_the_cached_profile_before_looking_up_changes() {
	use futures::StreamExt;

	for skip_stale in [false, true] {
		for (stored, resolved) in [
			(avatar_record(1, crate::ImageFormat::Png), avatar_record(2, crate::ImageFormat::Png)),
			(avatar_record(1, crate::ImageFormat::Png), avatar_record(1, crate::ImageFormat::Jpeg)),
			(avatar_record(1, crate::ImageFormat::Png), ProfileRecord::default()),
			(ProfileRecord::default(), avatar_record(2, crate::ImageFormat::Png)),
		] {
			let expected_stored = PublicProfile { avatar: stored.avatar() };
			let expected_resolved = PublicProfile { avatar: resolved.avatar() };
			let (_directory, profiles, db, platform) =
				cached_profiles(Some(stored), &resolved, true).await;
			let mut stream = profiles.observe(
				"alice.01".to_owned(),
				PublicProfileResolutionOptions { force_fresh: true, skip_stale },
			);

			let initial = stream.next().await.unwrap().unwrap();
			assert_eq!(
				(initial, platform.chain_connects.lock().unwrap().clone()),
				(expected_stored, vec![]),
			);
			let updated = stream.next().await.unwrap().unwrap();
			let complete = stream.next().await.is_none();
			let retained = db
				.write(|tx| Ok(dao::stored_record(tx, "alice.01")?.map(|cached| cached.record)))
				.await
				.unwrap();
			assert_eq!(
				(updated, complete, retained, platform.chain_connects.lock().unwrap().clone()),
				(expected_resolved, true, Some(resolved), vec![[3; 32]]),
			);
		}
	}
}

#[tokio::test]
async fn stale_metadata_is_emitted_before_the_chain_is_contacted() {
	use futures::StreamExt;

	let stored = avatar_record(1, crate::ImageFormat::Png);
	for resolved in [
		avatar_record(2, crate::ImageFormat::Png),
		avatar_record(1, crate::ImageFormat::Jpeg),
		ProfileRecord::default(),
	] {
		let expected_stored = PublicProfile { avatar: stored.avatar() };
		let expected_resolved = PublicProfile { avatar: resolved.avatar() };
		let (_directory, profiles, _db, platform) =
			cached_profiles(Some(stored.clone()), &resolved, false).await;
		let mut stream = profiles.observe(
			"alice.01".to_owned(),
			PublicProfileResolutionOptions { force_fresh: false, skip_stale: false },
		);

		let initial = stream.next().await.unwrap().unwrap();
		assert_eq!(
			(initial, platform.chain_connects.lock().unwrap().clone()),
			(expected_stored, vec![]),
		);
		let updated = stream.next().await.unwrap().unwrap();
		assert_eq!(
			(
				updated,
				stream.next().await.is_none(),
				platform.chain_connects.lock().unwrap().clone()
			),
			(expected_resolved, true, vec![[3; 32]]),
		);
	}
}

#[tokio::test]
async fn absent_metadata_resolves_once_and_persists_the_full_record() {
	use futures::TryStreamExt;

	for skip_stale in [false, true] {
		for force_fresh in [false, true] {
			let mut record = avatar_record(2, crate::ImageFormat::Png);
			record.entries.push(WrapperKeepOpaque::from_encoded(vec![99, 1, 2]));
			let expected = PublicProfile { avatar: record.avatar() };
			let (_directory, profiles, db, platform) = cached_profiles(None, &record, false).await;

			let emitted: Vec<_> = profiles
				.observe(
					"alice.01".to_owned(),
					PublicProfileResolutionOptions { force_fresh, skip_stale },
				)
				.try_collect()
				.await
				.unwrap();
			let stored = db
				.write(|tx| Ok(dao::stored_record(tx, "alice.01")?.map(|cached| cached.record)))
				.await
				.unwrap();

			assert_eq!(
				(emitted, stored, platform.chain_connects.lock().unwrap().clone()),
				(vec![expected], Some(record), vec![[3; 32]]),
			);
		}
	}
}

#[tokio::test]
async fn unchanged_profiles_renew_freshness_without_a_duplicate_emission() {
	use futures::TryStreamExt;

	for (fresh, force_fresh, skip_stale) in
		[(false, false, false), (true, true, false), (true, true, true)]
	{
		for record in [ProfileRecord::default(), avatar_record(1, crate::ImageFormat::Png)] {
			let mut resolved = record.clone();
			resolved.entries.push(WrapperKeepOpaque::from_encoded(vec![99, 3, 4]));
			let expected = PublicProfile { avatar: record.avatar() };
			let (_directory, profiles, db, platform) =
				cached_profiles(Some(record), &resolved, fresh).await;

			let emitted: Vec<_> = profiles
				.observe(
					"alice.01".to_owned(),
					PublicProfileResolutionOptions { force_fresh, skip_stale },
				)
				.try_collect()
				.await
				.unwrap();
			let stored = db
				.write(|tx| {
					Ok(dao::stored_record(tx, "alice.01")?.map(|cached| {
						(cached.is_fresh(current_unix_secs() + RECORD_TTL_SECS / 2), cached.record)
					}))
				})
				.await
				.unwrap();

			assert_eq!(
				(emitted, stored, platform.chain_connects.lock().unwrap().clone()),
				(vec![expected], Some((true, resolved)), vec![[3; 32]]),
			);
		}
	}
}

#[tokio::test]
async fn a_failed_refresh_preserves_cached_metadata_and_respects_stale_delivery() {
	use futures::StreamExt;

	for (fresh, force_fresh, skip_stale) in [
		(false, false, false),
		(true, true, false),
		(false, false, true),
		(false, true, true),
		(true, true, true),
	] {
		let record = avatar_record(1, crate::ImageFormat::Png);
		let expected = PublicProfile { avatar: record.avatar() };
		let (_directory, profiles, db, _platform) =
			cached_profiles(Some(record.clone()), &record, fresh).await;
		let expires_before: u64 = db
			.write(|tx| {
				tx.query_row(
					"SELECT expires_at FROM public_profiles WHERE username = 'alice.01'",
					[],
					|row| row.get(0),
				)
				.map_err(Into::into)
			})
			.await
			.unwrap();
		let platform =
			Arc::new(StubPlatform { chain_connect_error: Some("offline"), ..Default::default() });
		let services = services_with_platform(platform.clone());
		assert!(services.install_core_db(db.clone()).is_some());

		drop(profiles);
		let mut stream = services.public_profiles.clone().observe(
			"alice.01".to_owned(),
			PublicProfileResolutionOptions { force_fresh, skip_stale },
		);

		let initial =
			if skip_stale && !fresh { None } else { Some(stream.next().await.unwrap().unwrap()) };
		let failed = matches!(stream.next().await, Some(Err(PublicProfileError::Dotns(_))));
		let complete = stream.next().await.is_none();
		let retained = db
			.write(|tx| {
				let record = dao::stored_record(tx, "alice.01")?.map(|cached| cached.record);
				let expires_at: u64 = tx.query_row(
					"SELECT expires_at FROM public_profiles WHERE username = 'alice.01'",
					[],
					|row| row.get(0),
				)?;
				Ok((record, expires_at))
			})
			.await
			.unwrap();

		assert_eq!(
			(initial, failed, complete, retained, platform.chain_connects.lock().unwrap().clone()),
			(
				if skip_stale && !fresh { None } else { Some(expected) },
				true,
				true,
				(Some(record), expires_before),
				vec![[3; 32]],
			),
		);
	}
}

#[tokio::test]
async fn dropping_after_the_cached_profile_does_not_start_a_lookup() {
	use futures::StreamExt;

	for (fresh, force_fresh, skip_stale) in
		[(false, false, false), (true, true, false), (true, true, true)]
	{
		let record = avatar_record(1, crate::ImageFormat::Png);
		let (_directory, profiles, _db, platform) =
			cached_profiles(Some(record.clone()), &record, fresh).await;
		let mut stream = profiles.observe(
			"alice.01".to_owned(),
			PublicProfileResolutionOptions { force_fresh, skip_stale },
		);
		let initial = stream.next().await.unwrap().unwrap();

		drop(stream);

		assert_eq!(
			(initial, platform.chain_connects.lock().unwrap().clone()),
			(PublicProfile { avatar: record.avatar() }, vec![]),
		);
	}
}

#[tokio::test]
async fn invalid_profile_streams_fail_before_accessing_storage() {
	use futures::StreamExt;

	let mut stream = profile_services().public_profiles.clone().observe(
		"alice.1".to_owned(),
		PublicProfileResolutionOptions { force_fresh: false, skip_stale: false },
	);

	assert!(matches!(stream.next().await, Some(Err(PublicProfileError::InvalidUsername))));
	assert!(stream.next().await.is_none());
}
