use futures::{channel::oneshot, future, stream};
use tokio::sync::mpsc;

use super::*;
use crate::{
	PublicProfileResolutionOptions, platform::HostInfo, public_profile::PublicProfiles,
	runtime::services::RuntimeServices, store::open_temporary_core_db, test_support::StubPlatform,
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
		let services = RuntimeServices::new(
			platform.clone(),
			HostInfo {
				name: "Profile test".into(),
				icon: None,
				version: None,
				platform: truapi::latest::HostPlatform::Unknown,
			},
			[1; 32],
			[2; 32],
			[3; 32],
			Spawner::for_test(),
		);
		let (_directory, db) = open_temporary_core_db();
		db.write(move |tx| {
			let expires_at = crate::unix_time::current_unix_secs() + if fresh { 3600 } else { 0 };
			tx.execute(
				"INSERT INTO public_profiles (username, public_record, expires_at) VALUES ('alice.01', X'00', ?)",
				[expires_at],
			)?;
			Ok(())
		}).await.unwrap();
		assert!(services.install_core_db(db));
		let (events, mut received) = mpsc::unbounded_channel();
		let subscription = observe_public_profile(
			PublicProfiles::new(services).observe(
				"alice.01".to_owned(),
				PublicProfileResolutionOptions { force_fresh, skip_stale },
			),
			Arc::new(Observer { events }),
			&Spawner::for_test(),
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

struct PendingResolution(Option<oneshot::Sender<()>>);

impl Drop for PendingResolution {
	fn drop(&mut self) {
		self.0.take().unwrap().send(()).unwrap();
	}
}

#[tokio::test]
async fn cancelling_native_observation_drops_an_in_progress_resolution_without_callbacks() {
	let (started, pending) = oneshot::channel();
	let (dropped, cancelled) = oneshot::channel();
	let (events, mut received) = mpsc::unbounded_channel();
	let stream = Subscription::new(stream::once(async move {
		let _resolution = PendingResolution(Some(dropped));
		started.send(()).unwrap();
		future::pending::<Result<PublicProfile, PublicProfileError>>().await
	}));
	let subscription =
		observe_public_profile(stream, Arc::new(Observer { events }), &Spawner::for_test());
	pending.await.unwrap();

	subscription.cancel();
	cancelled.await.unwrap();

	assert!(received.recv().await.is_none());
}
