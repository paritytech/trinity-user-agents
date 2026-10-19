use futures::executor::block_on;

use super::*;
use crate::test_support::{StubPlatform, runtime_config, stub_platform, test_spawner};

const PRODUCT: &str = "chat.dot";
const OTHER: &str = "other.dot";
const NOW: u64 = 1_800_000_000_000;

/// A relay that keeps each registration, or refuses all of them.
#[derive(Default)]
struct RecordingRelay {
	registrations: Mutex<Vec<WakeRelayRegistration>>,
	refuse: bool,
}

#[async_trait]
impl WakeRelay for RecordingRelay {
	async fn register_wake_topics(
		&self,
		registration: WakeRelayRegistration,
	) -> Result<(), latest::GenericError> {
		if self.refuse {
			return Err(latest::GenericError { reason: "relay is down".to_string() });
		}
		self.registrations.lock().unwrap().push(registration);
		Ok(())
	}
}

impl RecordingRelay {
	fn last(&self) -> WakeRelayRegistration {
		self.registrations.lock().unwrap().last().cloned().expect("a registration")
	}
}

fn services_on(platform: Arc<StubPlatform>) -> Arc<RuntimeServices> {
	let (config, _) = runtime_config(PRODUCT);
	RuntimeServices::new(
		platform,
		config.host.host_info.clone(),
		config.people_chain_genesis_hash,
		config.bulletin_chain_genesis_hash,
		config.asset_hub_chain_genesis_hash,
		test_spawner(),
	)
}

fn watch(id: &str, room_id: &str, topics: &[u8]) -> latest::WorkerWakeWatch {
	latest::WorkerWakeWatch {
		id: id.to_string(),
		room_id: room_id.to_string(),
		topics: topics.iter().map(|byte| [*byte; 32]).collect(),
	}
}

/// The key the core keeps in core storage.
fn stored_key(services: &RuntimeServices) -> Bytes32 {
	let mut guard = block_on(services.push_wakes.registry.lock());
	block_on(services.stored_registry(&mut guard)).unwrap().expect("a registry").key
}

fn push(key: &Bytes32, topic: u8, statement: u8, sent_at_ms: u64) -> String {
	WakePush::sign(key, [topic; 32], [statement; 32], sent_at_ms)
		.to_json()
		.to_string()
}

/// [`RuntimeServices::handle_wake_push_at`] at [`NOW`], with a wake that
/// holds until 30 s later and records each reason.
fn handle(
	services: &Arc<RuntimeServices>,
	payload: &str,
	wakes: &Mutex<Vec<(String, ChatWakeReason)>>,
) -> Result<Vec<PushWakeDelivery>, PushWakeRejection> {
	block_on(services.handle_wake_push_at(payload, NOW, |_, product_id, reason| {
		wakes.lock().unwrap().push((product_id.to_string(), reason));
		Ok(ChatWakeOutcome::Held { until_ms: NOW + 30_000 })
	}))
}

// The relay learns each topic once, sorted, for every product of the device,
// and the product learns that a push now reaches it.
#[test]
fn the_relay_gets_the_topics_of_every_product() {
	let services = services_on(stub_platform());
	let relay = Arc::new(RecordingRelay::default());
	block_on(services.set_wake_relay(Some(relay.clone()))).unwrap();

	let first = block_on(services.set_wake_watches(
		PRODUCT,
		vec![watch("inbox", "home", &[3, 1]), watch("bob", "dm-1", &[2, 3])],
	));
	let second = block_on(services.set_wake_watches(OTHER, vec![watch("w", "main", &[9])]));

	assert_eq!(first, Ok(latest::WorkerWakeDelivery::Push));
	assert_eq!(second, Ok(latest::WorkerWakeDelivery::Push));
	assert_eq!(
		relay.last(),
		WakeRelayRegistration {
			key: stored_key(&services),
			topics: vec![[1; 32], [2; 32], [3; 32], [9; 32]],
		}
	);
}

// Without a relay the core still keeps the watches, and says so, so a
// product can tell the user that a closed app learns nothing.
#[test]
fn a_host_without_a_relay_keeps_the_watches_and_says_so() {
	let services = services_on(stub_platform());

	let delivery = block_on(services.set_wake_watches(PRODUCT, vec![watch("w", "main", &[1])]));

	assert_eq!(
		delivery,
		Ok(latest::WorkerWakeDelivery::Unavailable {
			reason: "this host has no push relay".to_string()
		})
	);
	let wakes = Mutex::default();
	let key = stored_key(&services);
	assert_eq!(handle(&services, &push(&key, 1, 7, NOW), &wakes).map(|found| found.len()), Ok(1));
}

#[test]
fn a_refusing_relay_leaves_the_watches_without_push() {
	let services = services_on(stub_platform());
	let relay = Arc::new(RecordingRelay { refuse: true, ..Default::default() });
	block_on(services.set_wake_relay(Some(relay))).unwrap();

	let delivery = block_on(services.set_wake_watches(PRODUCT, vec![watch("w", "main", &[1])]));

	assert_eq!(
		delivery,
		Ok(latest::WorkerWakeDelivery::Unavailable { reason: "relay is down".to_string() })
	);
}

// The watches outlive the worker and the app: a new runtime over the same
// core storage reads them and the key, so a push the relay signed before the
// restart still wakes the product, and a relay installed at start gets the
// stored topics at once.
#[test]
fn the_watches_and_the_key_survive_a_restart() {
	let platform = stub_platform();
	let before = services_on(platform.clone());
	block_on(before.set_wake_watches(PRODUCT, vec![watch("inbox", "home", &[1])])).unwrap();
	let key = stored_key(&before);
	drop(before);

	let after = services_on(platform);
	let relay = Arc::new(RecordingRelay::default());
	block_on(after.set_wake_relay(Some(relay.clone()))).unwrap();
	let wakes = Mutex::default();
	let deliveries = handle(&after, &push(&key, 1, 7, NOW - 1_000), &wakes);

	assert_eq!(relay.last(), WakeRelayRegistration { key, topics: vec![[1; 32]] });
	assert_eq!(
		deliveries,
		Ok(vec![PushWakeDelivery {
			product_id: PRODUCT.to_string(),
			room_id: "home".to_string(),
			watch_id: "inbox".to_string(),
			outcome: PushWakeOutcome::Held { until_ms: NOW + 30_000 },
		}])
	);
	assert_eq!(
		*wakes.lock().unwrap(),
		vec![(
			PRODUCT.to_string(),
			ChatWakeReason::Notification {
				room_id: "home".to_string(),
				watch_id: "inbox".to_string()
			}
		)]
	);
}

// An empty list removes the product, and the relay forgets its topics.
#[test]
fn an_empty_list_removes_the_watches_of_the_product() {
	let services = services_on(stub_platform());
	let relay = Arc::new(RecordingRelay::default());
	block_on(services.set_wake_relay(Some(relay.clone()))).unwrap();
	block_on(services.set_wake_watches(PRODUCT, vec![watch("w", "main", &[1])])).unwrap();
	let key = stored_key(&services);

	block_on(services.set_wake_watches(PRODUCT, Vec::new())).unwrap();

	assert_eq!(relay.last().topics, Vec::<Bytes32>::new());
	assert_eq!(
		handle(&services, &push(&key, 1, 7, NOW), &Mutex::default()),
		Err(PushWakeRejection::UnknownTopic)
	);
}

// Only a push from the relay of this device, about a watched topic, fresh,
// and the first one for its statement, wakes anything.
#[test]
fn the_core_refuses_every_push_it_cannot_trust() {
	let services = services_on(stub_platform());
	let wakes = Mutex::default();
	assert_eq!(
		handle(&services, &push(&[0; 32], 1, 7, NOW), &wakes),
		Err(PushWakeRejection::NotRegistered)
	);
	block_on(services.set_wake_watches(PRODUCT, vec![watch("w", "main", &[1])])).unwrap();
	let key = stored_key(&services);

	let cases = [
		("{}".to_string(), PushWakeRejection::Malformed { reason: "no version".to_string() }),
		(push(&[5; 32], 1, 7, NOW), PushWakeRejection::BadSignature),
		(push(&key, 1, 7, NOW - WAKE_PUSH_MAX_AGE_MS - 1), PushWakeRejection::Stale),
		(push(&key, 1, 7, NOW + WAKE_PUSH_MAX_SKEW_MS + 1), PushWakeRejection::Stale),
		(push(&key, 2, 7, NOW), PushWakeRejection::UnknownTopic),
	];
	for (payload, rejection) in cases {
		assert_eq!(handle(&services, &payload, &wakes), Err(rejection), "{payload}");
	}
	assert!(wakes.lock().unwrap().is_empty(), "a refused push wakes nothing");

	let first = push(&key, 1, 7, NOW);
	assert!(handle(&services, &first, &wakes).is_ok());
	assert_eq!(handle(&services, &first, &wakes), Err(PushWakeRejection::Replayed));
	assert_eq!(wakes.lock().unwrap().len(), 1);
}

// A changed field breaks the MAC, so the relay cannot be impersonated by a
// push that moves a signed wake to another topic or time.
#[test]
fn the_mac_covers_every_field() {
	let key = [4; 32];
	let mac = wake_push_mac(&key, &[1; 32], &[2; 32], NOW);
	assert_ne!(mac, wake_push_mac(&[5; 32], &[1; 32], &[2; 32], NOW));
	assert_ne!(mac, wake_push_mac(&key, &[9; 32], &[2; 32], NOW));
	assert_ne!(mac, wake_push_mac(&key, &[1; 32], &[9; 32], NOW));
	assert_ne!(mac, wake_push_mac(&key, &[1; 32], &[2; 32], NOW + 1));
	let signed = WakePush::sign(&key, [1; 32], [2; 32], NOW);
	assert_eq!(WakePush::parse(&signed.to_json().to_string()), Ok(signed));
}

// A product whose foreground worker runs reads the statement itself, so the
// push spends none of its hourly budget.
#[test]
fn a_running_foreground_worker_spends_no_wake() {
	let services = services_on(stub_platform());
	block_on(services.set_wake_watches(PRODUCT, vec![watch("w", "main", &[1])])).unwrap();
	services
		.worker_ledger
		.set_keep_alive(PRODUCT, crate::WorkerKeepAlive::Foreground);
	services.worker_ledger.set_app_foreground(true);
	let wakes = Mutex::default();

	let deliveries = handle(&services, &push(&stored_key(&services), 1, 7, NOW), &wakes);

	assert_eq!(deliveries.map(|found| found[0].outcome), Ok(PushWakeOutcome::Running));
	assert!(wakes.lock().unwrap().is_empty());
}

// The real wake holds the worker and tells the room which watch woke it.
#[test]
fn a_push_wakes_the_product_and_tells_the_room() {
	let services = services_on(stub_platform());
	block_on(services.set_wake_watches(PRODUCT, vec![watch("inbox", "home", &[1])])).unwrap();
	let payload = push(&stored_key(&services), 1, 7, current_unix_millis());

	let deliveries = block_on(services.handle_wake_push(&payload)).unwrap();

	assert!(matches!(deliveries[0].outcome, PushWakeOutcome::Held { .. }));
	assert_eq!(services.worker_ledger.count(PRODUCT), 1);
	let mut events =
		services.chat_room_events.subscribe::<latest::GenericError>(PRODUCT, Vec::new());
	assert_eq!(
		futures::StreamExt::next(&mut events)
			.now_or_never()
			.flatten()
			.map(|item| item.ok()),
		Some(Some(latest::HostChatRoomEventsSubscribeItem {
			room_id: "home".to_string(),
			event: latest::ChatRoomEvent::Woken { watch_id: "inbox".to_string() },
		}))
	);
}

#[test]
fn watches_are_checked_against_the_limits() {
	use latest::HostWorkerSetWakeWatchesError as Error;
	let services = services_on(stub_platform());
	let set = |watches| block_on(services.set_wake_watches(PRODUCT, watches));
	let many_topics = latest::WorkerWakeWatch {
		id: "w".to_string(),
		room_id: "main".to_string(),
		topics: (0..=MAX_WAKE_WATCH_TOPICS as u32)
			.map(|index| {
				let mut topic = [0u8; 32];
				topic[..4].copy_from_slice(&index.to_le_bytes());
				topic
			})
			.collect(),
	};

	assert!(matches!(set(vec![many_topics]), Err(Error::TooMany { .. })));
	assert!(matches!(
		set((0..=MAX_WAKE_WATCHES)
			.map(|index| watch(&format!("w{index}"), "main", &[1]))
			.collect()),
		Err(Error::TooMany { .. })
	));
	assert_eq!(
		set(vec![watch("w", "main", &[1]), watch("w", "other", &[2])]),
		Err(Error::InvalidWatch { reason: "two watches have the id w".to_string() })
	);
	assert_eq!(
		set(vec![watch("w", "main", &[])]),
		Err(Error::InvalidWatch { reason: "watch w has no topic".to_string() })
	);
	assert!(matches!(set(vec![watch("", "main", &[1])]), Err(Error::InvalidWatch { .. })));
}

// A worker sets its watches through `worker.setWakeWatches`, answered in the
// version of its request, and the watches reach the shared runtime services.
#[test]
fn a_worker_sets_its_watches_through_the_protocol() {
	use truapi::versioned::worker::{
		HostWorkerSetWakeWatchesError, HostWorkerSetWakeWatchesRequest,
		HostWorkerSetWakeWatchesResponse,
	};
	let (config, _) = runtime_config(PRODUCT);
	let product = crate::platform::ProductContext::new_with_execution(
		PRODUCT.to_string(),
		crate::platform::ProductExecutionKind::Worker,
	)
	.unwrap();
	let host =
		crate::runtime::ProductRuntimeHost::new(stub_platform(), (config, product), test_spawner());
	let set = |watches| {
		block_on(truapi::api::Worker::set_wake_watches(
			&host,
			&truapi::CallContext::default(),
			HostWorkerSetWakeWatchesRequest::V1(latest::HostWorkerSetWakeWatchesRequest {
				watches,
			}),
		))
	};

	assert_eq!(
		set(vec![watch("inbox", "home", &[1])]),
		Ok(HostWorkerSetWakeWatchesResponse::V1(latest::HostWorkerSetWakeWatchesResponse {
			delivery: latest::WorkerWakeDelivery::Unavailable {
				reason: "this host has no push relay".to_string()
			},
		}))
	);
	assert_eq!(
		set(vec![watch("inbox", "home", &[])]),
		Err(truapi::CallError::Domain(HostWorkerSetWakeWatchesError::V1(
			latest::HostWorkerSetWakeWatchesError::InvalidWatch {
				reason: "watch inbox has no topic".to_string()
			}
		)))
	);
	let services = host.connection.services();
	assert_eq!(
		block_on(services.push_wakes.registry.lock())
			.as_ref()
			.and_then(|registry| registry.products.get(PRODUCT).cloned()),
		Some(vec![watch("inbox", "home", &[1])])
	);
}
