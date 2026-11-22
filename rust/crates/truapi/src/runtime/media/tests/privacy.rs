use super::*;
use crate::platform::{MediaDescription, MediaDescriptionKind, MediaIceCandidate};

#[test]
fn private_ice_and_sdp_never_enter_product_events_or_snapshots() {
	use parity_scale_codec::Encode;
	let host = host(
		released(
			Arc::new(AtomicBool::new(true)),
			Arc::new(AtomicUsize::new(0)),
			advertisement(1, 11),
			Arc::new(Mutex::new(None)),
		),
		Backoff { min: Duration::from_millis(10), max: Duration::from_millis(40) },
	);
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
	))
	.unwrap();
	let wire::HostMediaCreateSessionResponse::V1(created) = created;
	let session_id = created.session.session_id;
	let participant_id = [12; 32];
	let service = host.connection.media.get().unwrap();
	{
		let mut state = service.lock();
		let peer = v::MediaPeer {
			network: v::MediaNetwork { genesis_hash: NETWORK },
			product_id: PRODUCT.into(),
			account: v::MediaAccount::Sr25519([2; 32]),
		};
		let mut participant = state::Participant::new(participant_id, peer);
		participant.visible = true;
		participant.backend_started = true;
		participant.backend_ready = true;
		state
			.sessions
			.get_mut(&session_id)
			.unwrap()
			.participants
			.insert(participant_id, participant);
	}
	let candidate = "candidate:private-host-marker 1 udp 2122260223 192.0.2.123 43210 typ host";
	let sdp =
		"v=0\r\no=private-sdp-marker 1 1 IN IP4 192.0.2.123\r\na=ice-pwd:private-secret-marker\r\n";
	service.backend_event(MediaBackendEvent::IceCandidate {
		session_id,
		participant_id,
		candidate: MediaIceCandidate {
			candidate: candidate.into(),
			mid: Some("private-mid-marker".into()),
			mline_index: Some(0),
		},
	});
	service.backend_event(MediaBackendEvent::Description {
		session_id,
		participant_id,
		description: MediaDescription { kind: MediaDescriptionKind::Offer, sdp: sdp.into() },
	});
	service.backend_event(MediaBackendEvent::PeerStateChanged {
		session_id,
		participant_id,
		state: crate::platform::MediaBackendPeerState::Connected,
	});
	let mut observed_connected = false;
	while let Some(Some(item)) = next_within(&mut subscription, Duration::from_millis(100)) {
		let item = item.unwrap();
		let bytes = item.encode();
		for secret in [
			"192.0.2.123",
			"candidate:",
			"private-host-marker",
			"private-sdp-marker",
			"private-secret-marker",
			"private-mid-marker",
		] {
			assert!(
				!bytes.windows(secret.len()).any(|window| window == secret.as_bytes()),
				"product event disclosed {secret}"
			);
		}
		observed_connected |= format!("{item:?}").contains("Connected");
	}
	assert!(observed_connected, "the real public state transition was delivered");
	host.close_media();
}

struct FailingBackend;
#[async_trait::async_trait]
impl MediaPlatform for FailingBackend {
	async fn media_backend_capabilities(
		&self,
		_: &ProductContext,
	) -> std::result::Result<MediaBackendCapabilities, v::GenericError> {
		Err(v::GenericError {
			reason: "SDP candidate:1 1 udp 1 192.0.2.123 43210 typ host; RTCStats private".into(),
		})
	}
	fn media_backend_events(
		&self,
		_: &ProductContext,
		_: u64,
	) -> BoxStream<'static, std::result::Result<MediaBackendEvent, v::GenericError>> {
		futures::stream::empty().boxed()
	}
	async fn media_backend_command(
		&self,
		_: &ProductContext,
		_: u64,
		_: MediaBackendCommand,
	) -> std::result::Result<MediaBackendResponse, v::GenericError> {
		Err(v::GenericError { reason: "private SDP".into() })
	}
}

#[test]
fn backend_errors_never_disclose_private_transport_details() {
	let (mut host, _) = ProductRuntimeHost::new_pairing_for_tests(
		stub_platform(),
		ProductRuntimeHost::compat_host_config(),
		ProductContext::new(crate::ProductId::for_test(PRODUCT)),
		test_spawner(),
	);
	Arc::get_mut(&mut host.connection).unwrap().media_platform = Some(Arc::new(FailingBackend));
	let error = futures::executor::block_on(
		host.get_capabilities(&CallContext::default(), wire::HostMediaGetCapabilitiesRequest::V1),
	)
	.unwrap_err();
	assert_eq!(error, CallError::HostFailure { reason: "media host failure".into() });
}

struct ExistingDeviceGrantBackend {
	calling_prompts: AtomicUsize,
	device_prompts: AtomicUsize,
}

#[async_trait::async_trait]
impl MediaPlatform for ExistingDeviceGrantBackend {
	async fn media_backend_capabilities(
		&self,
		product: &ProductContext,
	) -> std::result::Result<MediaBackendCapabilities, v::GenericError> {
		Backend.media_backend_capabilities(product).await
	}

	fn media_backend_events(
		&self,
		product: &ProductContext,
		generation: u64,
	) -> BoxStream<'static, std::result::Result<MediaBackendEvent, v::GenericError>> {
		Backend.media_backend_events(product, generation)
	}

	async fn media_backend_command(
		&self,
		product: &ProductContext,
		generation: u64,
		command: MediaBackendCommand,
	) -> std::result::Result<MediaBackendResponse, v::GenericError> {
		if let MediaBackendCommand::RequestConsent { request, .. } = &command {
			match request {
				crate::platform::MediaConsentRequest::Calling { .. } => {
					self.calling_prompts.fetch_add(1, Ordering::SeqCst);
				},
				_ => {
					self.device_prompts.fetch_add(1, Ordering::SeqCst);
					return Ok(MediaBackendResponse::Consent { granted: false });
				},
			}
		}
		Backend.media_backend_command(product, generation, command).await
	}
}

fn microphone_session_with_existing_device_grant(product: &str, allow_once: bool) {
	use crate::{
		api::permissions::Permissions,
		platform::PermissionDecision,
		test_support::StubPlatform,
		versioned::permissions::{HostDevicePermissionRequest, HostDevicePermissionResponse},
	};
	let platform = Arc::new(StubPlatform {
		device_permission_decisions: Mutex::new([PermissionDecision::AllowOnce].into()),
		..Default::default()
	});
	let (mut host, _) = ProductRuntimeHost::new_pairing_for_tests(
		platform.clone(),
		ProductRuntimeHost::compat_host_config(),
		ProductContext::new(crate::ProductId::for_test(product)),
		test_spawner(),
	);
	host.test_session_state().set_session(session_info());
	let backend = Arc::new(ExistingDeviceGrantBackend {
		calling_prompts: AtomicUsize::new(0),
		device_prompts: AtomicUsize::new(0),
	});
	Arc::get_mut(&mut host.connection).unwrap().media_platform = Some(backend.clone());
	assert!(
		host.connection
			.media
			.set(MediaService::with_signaling(
				&host.connection,
				backend.clone(),
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
	let mut subscription = subscribe(&host);
	assert!(matches!(
		event(next_within(&mut subscription, Duration::from_secs(1))),
		v::MediaEvent::Snapshot { .. }
	));
	futures::executor::block_on(async {
		let cx = CallContext::default();
		if allow_once {
			assert_eq!(
				host.request_device_permission(
					&cx,
					HostDevicePermissionRequest::V1(v::HostDevicePermissionRequest::Microphone),
				)
				.await
				.unwrap(),
				HostDevicePermissionResponse::V1(v::HostDevicePermissionResponse { granted: true }),
			);
		}
		let mut tracks = state::off_tracks();
		tracks.microphone = true;
		host.create_session(
			&cx,
			wire::HostMediaCreateSessionRequest::V1(v::HostMediaCreateSessionRequest {
				operation_id: [9; 32],
				tracks,
			}),
		)
		.await
		.unwrap();
	});
	assert_eq!(backend.device_prompts.load(Ordering::SeqCst), 0);
	assert_eq!(
		backend.calling_prompts.load(Ordering::SeqCst),
		1,
		"trusted products and device grants never bypass scoped Calling consent"
	);
	assert_eq!(platform.device_permission_requests.lock().unwrap().len(), usize::from(allow_once));
	host.close_media();
}

#[test]
fn trusted_product_microphone_session_does_not_repeat_device_consent() {
	microphone_session_with_existing_device_grant("peopl.dot", false);
}

#[test]
fn allow_once_microphone_grant_is_honored_by_create_session() {
	microphone_session_with_existing_device_grant(PRODUCT, true);
}
