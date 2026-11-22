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
