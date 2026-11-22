use super::*;
use crate::{
	host_logic::session::encode_persisted_session,
	runtime::{authority::AuthorityError, pairing_host::PairingHost},
	test_support::StubPlatform,
	versioned::account::HostAccountConnectionStatusSubscribeItem as ConnectionEvent,
};

fn persisted_media_host() -> (ProductRuntimeHost, Arc<PairingHost>) {
	let session = session_info();
	let platform = Arc::new(StubPlatform {
		session_blob: Some(encode_persisted_session(&session)),
		..Default::default()
	});
	let (mut host, pairing) = ProductRuntimeHost::new_pairing_for_tests(
		platform,
		ProductRuntimeHost::compat_host_config(),
		ProductContext::new(crate::ProductId::for_test(PRODUCT)),
		test_spawner(),
	);
	host.test_session_state().set_session(session);
	let backend: Arc<dyn MediaPlatform> = Arc::new(Backend);
	Arc::get_mut(&mut host.connection).unwrap().media_platform = Some(backend.clone());
	assert!(
		host.connection
			.media
			.set(MediaService::with_signaling(
				&host.connection,
				backend,
				released(
					Arc::new(AtomicBool::new(true)),
					Arc::new(AtomicUsize::new(0)),
					advertisement(1, 11),
					Arc::new(Mutex::new(None)),
				),
				Backoff::DEFAULT,
			))
			.is_ok()
	);
	(host, pairing)
}

#[test]
fn unchanged_storage_notification_preserves_authority_and_live_media() {
	let (host, pairing) = persisted_media_host();
	let service = pairing.session();
	let mut connection_events = host.test_session_state().subscribe();
	futures::executor::block_on(connection_events.next()).unwrap();
	let mut subscription = subscribe(&host);
	assert!(matches!(
		event(next_within(&mut subscription, Duration::from_secs(1))),
		v::MediaEvent::Snapshot { .. }
	));
	let created = futures::executor::block_on(host.create_session(
		&CallContext::default(),
		wire::HostMediaCreateSessionRequest::V1(v::HostMediaCreateSessionRequest {
			operation_id: [9; 32],
			tracks: state::off_tracks(),
		}),
	));
	assert!(created.is_ok(), "{created:?}");
	let snapshot = service.current_session().unwrap();
	let media = host.connection.media.get().unwrap();
	let session_id = *media.lock().sessions.keys().next().unwrap();
	service.clone().start_session_store_sync_for_tests(test_spawner());
	let ticks = service.session_store_change_ticks_for_tests();
	service.notify_session_store_changed();
	assert!(
		service.with_current_session(&snapshot, |_| Ok(())).is_ok(),
		"the storage hint alone must not revoke authority"
	);
	wait_until(
		|| service.session_store_change_ticks_for_tests() > ticks,
		"storage reconciliation did not finish",
	);
	assert_eq!(service.current_session().unwrap().validation_id, snapshot.validation_id);
	assert!(service.with_current_session(&snapshot, |_| Ok(())).is_ok());
	assert!(
		connection_events.next().now_or_never().is_none(),
		"unchanged storage must not emit a connection transition"
	);
	assert!(media.lock().sessions.contains_key(&session_id));
	assert!(media.lock().signaling.is_some());
	host.close_media();
}

#[test]
fn explicit_same_id_external_activation_still_invalidates_authority() {
	let (host, pairing) = persisted_media_host();
	let service = pairing.session();
	let mut events = host.test_session_state().subscribe();
	futures::executor::block_on(events.next()).unwrap();
	let snapshot = service.current_session().unwrap();
	futures::executor::block_on(
		service.activate_external_session(&encode_persisted_session(&session_info())),
	)
	.unwrap();
	assert_ne!(service.current_session().unwrap().validation_id, snapshot.validation_id);
	assert_eq!(
		service.with_current_session(&snapshot, |_| Ok(())),
		Err(AuthorityError::Disconnected)
	);
	let disconnected = futures::executor::block_on(events.next()).unwrap();
	let connected = futures::executor::block_on(events.next()).unwrap();
	assert_eq!(
		disconnected,
		ConnectionEvent::V1(v::HostAccountConnectionStatusSubscribeItem::Disconnected)
	);
	assert_eq!(
		connected,
		ConnectionEvent::V1(v::HostAccountConnectionStatusSubscribeItem::Connected)
	);
	assert_eq!(host.test_session_state().current(), Some(session_info()));
	host.close_media();
}
