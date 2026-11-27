use tokio::sync::mpsc;

use super::*;
use crate::{
	PublicProfile, PublicProfileError,
	native::tests::{EventCallbacks, native_host_runtime_config},
	store::open_temporary_core_db,
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

fn profile_runtime(
	callbacks: Arc<EventCallbacks>,
	directory: &std::path::Path,
) -> Arc<NativeTrUApiHostRuntime> {
	let config = HostRuntimeConfig {
		database_directory: directory.to_str().unwrap().into(),
		local_session_secret: None,
		local_session_lite_username: None,
		..native_host_runtime_config()
	};
	NativeTrUApiHostRuntime::with_runtime_config(callbacks, config, no_workers()).unwrap()
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
		let callbacks = Arc::new(EventCallbacks::new());
		let (directory, db) = open_temporary_core_db();
		db.write(move |tx| {
			let expires_at = crate::unix_time::current_unix_secs() + if fresh { 3600 } else { 0 };
			tx.execute(
				"INSERT INTO public_profiles (username, public_record, expires_at) VALUES ('alice.01', X'00', ?)",
				[expires_at],
			)?;
			Ok(())
		}).await.unwrap();
		db.close().await.unwrap();
		let host = profile_runtime(callbacks.clone(), directory.path());
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
				callbacks.chain_connects.lock().unwrap().clone()
			),
			(
				if fresh && !force_fresh { "complete" } else { "offline" },
				if fresh && !force_fresh { vec![] } else { vec![vec![0xcc; 32]] }
			),
		);
		assert!(received.recv().await.is_none());
		drop(subscription);
	}
}

#[tokio::test]
async fn cancelling_native_observation_drops_an_in_progress_resolution_without_callbacks() {
	let callbacks = Arc::new(EventCallbacks::new());
	*callbacks.chain_id.lock().unwrap() = Some(1);
	let directory = tempfile::tempdir().unwrap();
	let host = profile_runtime(callbacks.clone(), directory.path());
	let (events, mut received) = mpsc::unbounded_channel();
	let subscription = host.observe_public_profile(
		"alice.01".to_owned(),
		PublicProfileResolutionOptions { force_fresh: false, skip_stale: false },
		Arc::new(Observer { events }),
	);
	crate::test_support::wait_until(
		|| !callbacks.chain_sends.lock().unwrap().is_empty(),
		"the lookup must reach the chain before cancellation",
	);

	subscription.cancel();

	assert!(received.recv().await.is_none());
}
