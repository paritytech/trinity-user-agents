use tokio::sync::mpsc;

use super::*;
use crate::{
	PublicProfile, PublicProfileError,
	native::tests::{native_host_runtime_config, native_host_runtime_no_session},
	store::{Db, open_temporary_core_db},
	test_support::StubPlatform,
};

struct Observer {
	events: mpsc::UnboundedSender<Result<Option<PublicProfile>, PublicProfileError>>,
}

impl NativePublicProfileObserver for Observer {
	fn on_profile(&self, profile: PublicProfile) {
		self.events.send(Ok(Some(profile))).unwrap();
	}

	fn on_complete(&self) {
		self.events.send(Ok(None)).unwrap();
	}

	fn on_error(&self, error: PublicProfileError) {
		self.events.send(Err(error)).unwrap();
	}
}

fn profile_runtime(platform: Arc<StubPlatform>, db: Db) -> Arc<NativeTrUApiHostRuntime> {
	let mut config =
		NativeResolvedHostRuntimeConfig::try_from(native_host_runtime_config()).unwrap();
	config.signing.stack =
		crate::test_support::test_stack("paseo", [1; 32], [2; 32], [3; 32]);
	let core = SigningHostRuntime::new(platform, config.signing, Spawner::for_test());
	assert!(core.set_core_db(db));
	let mut host = native_host_runtime_no_session();
	Arc::get_mut(&mut host).unwrap().runtime = Arc::new(core);
	host
}

#[tokio::test]
async fn native_profile_observation_reports_eligible_profiles_before_its_terminal_callback() {
	for (fresh, force_fresh, skip_stale) in [
		(false, false, false),
		(true, false, false),
		(true, true, false),
		(false, false, true),
		(false, true, true),
		(true, false, true),
		(true, true, true),
	] {
		let platform =
			Arc::new(StubPlatform { chain_connect_error: Some("offline"), ..Default::default() });
		let (_directory, db) = open_temporary_core_db();
		db.write(move |tx| {
			let expires_at = crate::unix_time::current_unix_secs() + if fresh { 3600 } else { 0 };
			tx.execute(
				"INSERT INTO public_profiles (username, public_record, expires_at) VALUES ('alice.01', X'00', ?)",
				[expires_at],
			)?;
			Ok(())
		}).await.unwrap();
		let host = profile_runtime(platform.clone(), db);
		let (events, mut received) = mpsc::unbounded_channel();
		let subscription = host.observe_public_profile(
			"alice.01".to_owned(),
			PublicProfileResolutionOptions { force_fresh, skip_stale },
			Arc::new(Observer { events }),
		);

		if !skip_stale || fresh {
			assert_eq!(
				received.recv().await.unwrap().unwrap(),
				Some(PublicProfile { avatar: None })
			);
		}
		let terminal = received.recv().await.unwrap();
		assert_eq!(
			(
				match terminal {
					Ok(None) => "complete",
					Err(PublicProfileError::Dotns(_)) => "offline",
					other => panic!("unexpected terminal callback: {other:?}"),
				},
				platform.chain_connects.lock().unwrap().clone()
			),
			(
				if fresh && !force_fresh { "complete" } else { "offline" },
				if fresh && !force_fresh { vec![] } else { vec![[3; 32]] }
			),
		);
		assert!(received.recv().await.is_none());
		drop(subscription);
	}
}

#[tokio::test]
async fn cancelling_native_observation_drops_an_in_progress_resolution_without_callbacks() {
	let platform = Arc::new(StubPlatform { chain_connect_pending: true, ..Default::default() });
	let (_directory, db) = open_temporary_core_db();
	let host = profile_runtime(platform.clone(), db);
	let (events, mut received) = mpsc::unbounded_channel();
	let subscription = host.observe_public_profile(
		"alice.01".to_owned(),
		PublicProfileResolutionOptions { force_fresh: false, skip_stale: false },
		Arc::new(Observer { events }),
	);
	while platform.chain_connects.lock().unwrap().is_empty() {
		tokio::task::yield_now().await;
	}

	subscription.cancel();

	assert!(received.recv().await.is_none());
}
