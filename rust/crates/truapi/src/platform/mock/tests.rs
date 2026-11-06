use super::*;
use futures::{FutureExt, executor::block_on};

/// Decode a lowercase hex string into bytes.
fn hex_bytes(hex: &str) -> Vec<u8> {
	(0..hex.len())
		.step_by(2)
		.map(|index| {
			u8::from_str_radix(&hex[index..index + 2], 16).expect("test vector is valid hex")
		})
		.collect()
}

fn resource_review() -> UserConfirmationReview {
	UserConfirmationReview::ResourceAllocation(crate::platform::ResourceAllocationReview {
		calling_product_id: crate::platform::ProductId::for_test("mock.dot"),
		resources: vec![],
	})
}

#[test]
fn implements_platform() {
	fn assert_platform<P: crate::platform::Platform>(_: &P) {}
	assert_platform(&MockPlatform::new());
}

#[test]
fn product_storage_round_trips() {
	// Namespacing is `core_and_product_keys_do_not_collide`'s subject; this
	// is the write/read/clear cycle on its own.
	let p = MockPlatform::new();
	block_on(p.write("k".into(), vec![1, 2, 3])).unwrap();
	assert_eq!(block_on(p.read("k".into())).unwrap(), Some(vec![1, 2, 3]));
	block_on(p.clear("k".into())).unwrap();
	assert_eq!(block_on(p.read("k".into())).unwrap(), None);
}

#[test]
fn core_storage_round_trips() {
	let p = MockPlatform::new();
	block_on(p.write_core_storage(CoreStorageKey::AuthSession, vec![7])).unwrap();
	assert_eq!(block_on(p.read_core_storage(CoreStorageKey::AuthSession)).unwrap(), Some(vec![7]));
	block_on(p.clear_core_storage(CoreStorageKey::AuthSession)).unwrap();
	assert_eq!(block_on(p.read_core_storage(CoreStorageKey::AuthSession)).unwrap(), None);
}

#[test]
fn core_and_product_keys_do_not_collide() {
	let p = MockPlatform::new();
	block_on(p.write_core_storage(CoreStorageKey::AuthSession, vec![1])).unwrap();
	// Reading the same logical name as a product key must miss the core slot.
	assert_eq!(block_on(p.read("auth-session".into())).unwrap(), None);
	assert_eq!(block_on(p.read("core:auth-session".into())).unwrap(), None);
	// ...and a product key must not be visible through core storage.
	block_on(p.write("x".into(), vec![2])).unwrap();
	assert_eq!(block_on(p.read_core_storage(CoreStorageKey::PairingDeviceIdentity)).unwrap(), None);
}

#[test]
fn permissions_deny_all_denies_device_and_remote() {
	let p = MockPlatform::with_config(MockConfig {
		device_permissions: PermissionPolicy::DenyAll,
		remote_permissions: PermissionPolicy::DenyAll,
		..Default::default()
	});
	assert_eq!(
		block_on(p.device_permission(
			&mock_product(),
			latest::HostDevicePermissionRequest::Notifications
		))
		.unwrap(),
		PermissionDecision::Deny
	);
	assert_eq!(
		block_on(p.remote_permission(
			&mock_product(),
			latest::RemotePermissionRequest { permission: latest::RemotePermission::WebRtc }
		))
		.unwrap(),
		PermissionDecision::Deny
	);
}

#[test]
fn permissions_split_allows_device_denies_remote() {
	let p = MockPlatform::with_config(MockConfig {
		device_permissions: PermissionPolicy::AllowAll,
		remote_permissions: PermissionPolicy::DenyAll,
		..Default::default()
	});
	assert_eq!(
		block_on(p.device_permission(
			&mock_product(),
			latest::HostDevicePermissionRequest::Notifications
		))
		.unwrap(),
		PermissionDecision::AllowAlways
	);
	assert_eq!(
		block_on(p.remote_permission(
			&mock_product(),
			latest::RemotePermissionRequest { permission: latest::RemotePermission::WebRtc }
		))
		.unwrap(),
		PermissionDecision::Deny
	);
}

#[test]
fn remote_permission_granted_when_allowed() {
	let p = MockPlatform::with_config(MockConfig {
		remote_permissions: PermissionPolicy::AllowAll,
		..Default::default()
	});
	assert_eq!(
		block_on(p.remote_permission(
			&mock_product(),
			latest::RemotePermissionRequest { permission: latest::RemotePermission::WebRtc }
		))
		.unwrap(),
		PermissionDecision::AllowAlways
	);
}

#[test]
fn navigation_records_and_can_error() {
	let p = MockPlatform::new();
	block_on(p.navigate_to("a".into())).unwrap();
	block_on(p.navigate_to("b".into())).unwrap();
	assert_eq!(p.navigations(), vec!["a".to_string(), "b".to_string()]);

	let p2 = MockPlatform::with_config(MockConfig {
		faults: MockFaults { navigate_error: Some("blocked".into()), ..Default::default() },
		..Default::default()
	});
	assert!(block_on(p2.navigate_to("c".into())).is_err());
}

#[test]
fn notifications_record_order_with_unique_ids() {
	let p = MockPlatform::new();
	let make = |text: &str| latest::HostPushNotificationRequest {
		text: text.to_string(),
		deeplink: None,
		scheduled_at: None,
	};
	let id0 = block_on(p.push_notification(make("one"))).unwrap().id;
	let id1 = block_on(p.push_notification(make("two"))).unwrap().id;
	// From 1, not 0: a product that reads 0 as "no id" cannot cancel the
	// first notification it schedules, and the JS mock starts at 1 too.
	assert_eq!((id0, id1), (1, 2));
	assert_eq!(p.pushed_notifications().len(), 2);
	block_on(p.cancel_notification(id1)).unwrap();
	assert_eq!(p.cancelled_notifications(), vec![id1]);
}

#[test]
fn notification_error_injected() {
	let p = MockPlatform::with_config(MockConfig {
		faults: MockFaults { notification_error: Some("denied".into()), ..Default::default() },
		..Default::default()
	});
	let request = latest::HostPushNotificationRequest {
		text: "x".into(),
		deeplink: None,
		scheduled_at: None,
	};
	assert!(block_on(p.push_notification(request)).is_err());
}

#[test]
fn a_storage_subscription_sees_the_current_value_and_later_writes() {
	let p = MockPlatform::new();
	block_on(p.write("k".into(), vec![1])).expect("seed write succeeds");

	// `now_or_never`, not `block_on`: the fan-out is a synchronous send, so
	// an item that is not already there is never coming, and awaiting one
	// would hang this test instead of failing it.
	let mut stream = p.subscribe_storage("k".into());
	assert_eq!(next_value(&mut stream), Some(Some(vec![1])), "seeded value");

	block_on(p.write("k".into(), vec![2])).expect("write succeeds");
	assert_eq!(next_value(&mut stream), Some(Some(vec![2])), "write reaches");

	block_on(p.clear("k".into())).expect("clear succeeds");
	assert_eq!(next_value(&mut stream), Some(None), "clear reaches");
}

/// The next item already queued on a storage subscription, or `None` when
/// nothing is queued. The outer `Option` is arrival, the inner is the value.
fn next_value(
	stream: &mut BoxStream<
		'static,
		Result<latest::HostLocalStorageChangeItem, latest::GenericError>,
	>,
) -> Option<Option<Vec<u8>>> {
	stream
		.next()
		.now_or_never()
		.flatten()
		.map(|item| item.expect("a mock subscription never errors").value)
}

#[test]
fn a_storage_subscription_hears_nothing_from_another_key() {
	// Per-key, not a single broadcast: a subscriber woken by every write
	// would make a test asserting "no change" pass for the wrong reason.
	let p = MockPlatform::new();
	let mut stream = p.subscribe_storage("watched".into());
	assert_eq!(next_value(&mut stream), Some(None), "seeded value");

	block_on(p.write("other".into(), vec![9])).expect("write succeeds");
	assert_eq!(next_value(&mut stream), None, "a write to another key woke this subscription");
}

#[test]
fn an_operation_is_open_until_its_own_product_ends_it() {
	let p = MockPlatform::new();
	let mine = mock_product();
	let theirs = ProductContext::new(ProductId::for_test("other.dot"));

	let id = block_on(p.begin_operation(&mine, "sync".into())).expect("begin succeeds").id;
	assert_eq!(
		p.open_operations(),
		vec![OpenOperation {
			product_id: crate::platform::ProductId::for_test("mock.dot"),
			id,
			label: "sync".to_string(),
		}]
	);

	// Ending is per product: another product holding the same id must not
	// drop this one's demand.
	block_on(p.end_operation(&theirs, id)).expect("a foreign end is Ok");
	assert_eq!(p.open_operations().len(), 1);

	block_on(p.end_operation(&mine, id)).expect("end succeeds");
	assert!(p.open_operations().is_empty());
	// Idempotent by contract.
	block_on(p.end_operation(&mine, id)).expect("a repeat end is Ok");
}

#[test]
fn storage_error_injected() {
	let p = MockPlatform::with_config(MockConfig {
		faults: MockFaults { storage_error: Some("disk".into()), ..Default::default() },
		..Default::default()
	});
	assert!(block_on(p.read("k".into())).is_err());
	assert!(block_on(p.read_core_storage(CoreStorageKey::AuthSession)).is_err());
}

#[test]
fn confirm_records_kind_and_answers() {
	let p = MockPlatform::new();
	assert!(block_on(p.confirm_user_action(resource_review())).unwrap());
	assert_eq!(p.confirmations(), vec![ConfirmKind::ResourceAllocation]);

	let p2 =
		MockPlatform::with_config(MockConfig { confirm_user_actions: false, ..Default::default() });
	assert!(!block_on(p2.confirm_user_action(resource_review())).unwrap());
}

#[test]
fn set_confirm_user_actions_answers_until_reset() {
	let p = MockPlatform::new();
	p.set_confirm_user_actions(false);
	assert!(!block_on(p.confirm_user_action(resource_review())).unwrap());
	p.set_confirm_user_actions(true);
	assert!(block_on(p.confirm_user_action(resource_review())).unwrap());
	p.set_confirm_user_actions(false);
	p.reset();
	assert!(block_on(p.confirm_user_action(resource_review())).unwrap());

	let declining =
		MockPlatform::with_config(MockConfig { confirm_user_actions: false, ..Default::default() });
	declining.set_confirm_user_actions(true);
	declining.reset();
	assert!(!block_on(declining.confirm_user_action(resource_review())).unwrap());
}

#[test]
fn feature_supported_reflects_config() {
	let p =
		MockPlatform::with_config(MockConfig { feature_supported: false, ..Default::default() });
	let response = block_on(p.feature_supported(latest::HostFeatureSupportedRequest::Chain {
		genesis_hash: vec![0; 32],
	}))
	.unwrap();
	assert!(!response.supported);
}

#[test]
fn feature_supported_reports_unserved_chains_only_when_asked() {
	let supported = |p: &MockPlatform, genesis_hash: Vec<u8>| {
		block_on(p.feature_supported(latest::HostFeatureSupportedRequest::Chain { genesis_hash }))
			.unwrap()
			.supported
	};
	let unserved = vec![0xab; 32];
	// The default keeps answering every chain as supported.
	assert!(supported(&MockPlatform::new(), unserved.clone()));
	let strict =
		MockPlatform::with_config(MockConfig { only_served_chains: true, ..Default::default() });
	let served = strict.config.served_chains[0].to_vec();
	assert!(supported(&strict, served));
	assert!(!supported(&strict, unserved));
}

#[test]
fn theme_emits_configured_variant_then_stays_open() {
	let p = MockPlatform::new();
	let mut stream = p.subscribe_theme();
	assert_eq!(block_on(stream.next()).unwrap().unwrap().variant, latest::ThemeVariant::Dark);
	// A live subscription does not end after the current value.
	assert!(stream.next().now_or_never().is_none());
}

#[test]
fn theme_emits_configured_light() {
	let p = MockPlatform::with_config(MockConfig {
		theme: latest::ThemeVariant::Light,
		..Default::default()
	});
	assert_eq!(
		block_on(p.subscribe_theme().next()).unwrap().unwrap().variant,
		latest::ThemeVariant::Light
	);
}

#[test]
fn preimage_insert_then_lookup_round_trips() {
	let p = MockPlatform::new();
	// The core owns Bulletin submission on main; the host only retrieves
	// content, so tests seed the content-addressed store directly.
	let key = p.insert_preimage(vec![1, 2, 3]);
	// The key is the content address the core asks for, not an arbitrary
	// digest: the core recomputes blake2b-256 over whatever comes back and
	// reports a mismatch as a miss, so a key derived any other way makes
	// every seeded preimage unreachable through the core. The expected
	// value is the published blake2b-256 of `[1, 2, 3]`, so this fails if
	// the algorithm changes even where both sides change together.
	assert_eq!(key, hex_bytes("11c0e79b71c3976ccd0c02d1310e2516c08edc9d8b6f57ccd680d63a4d8e72da"));
	let key = key.try_into().expect("a blake2b-256 key is 32 bytes");
	let found = block_on(p.lookup_preimage(key).next()).unwrap().unwrap();
	assert_eq!(found, Some(vec![1, 2, 3]));
	// An unknown key misses.
	let miss = block_on(p.lookup_preimage([9; 32]).next()).unwrap().unwrap();
	assert_eq!(miss, None);
}

#[test]
fn preimage_lookup_receives_a_later_insert() {
	let p = MockPlatform::new();
	let key = MockPlatform::new().insert_preimage(vec![1, 2, 3]);
	let mut waiting = p.lookup_preimage(key.try_into().expect("a blake2b-256 key is 32 bytes"));
	let mut other = p.lookup_preimage([9; 32]);
	assert_eq!(block_on(waiting.next()).unwrap().unwrap(), None);
	assert_eq!(block_on(other.next()).unwrap().unwrap(), None);

	p.insert_preimage(vec![1, 2, 3]);

	assert_eq!(waiting.next().now_or_never().unwrap().unwrap().unwrap(), Some(vec![1, 2, 3]));
	// An insert reaches only the lookups waiting on its own key.
	assert!(other.next().now_or_never().is_none());
}

#[test]
fn a_dropped_preimage_lookup_leaves_no_subscriber_behind() {
	let p = MockPlatform::new();
	let first = p.lookup_preimage([9; 32]);
	let second = p.lookup_preimage([9; 32]);

	drop(first);
	assert_eq!(p.preimage_subscribers.lock().unwrap()[&vec![9; 32]].len(), 1);
	drop(second);

	assert!(p.preimage_subscribers.lock().unwrap().is_empty());
}

#[test]
fn reinserting_a_held_preimage_does_not_deliver_it_again() {
	let p = MockPlatform::new();
	let key = p.insert_preimage(vec![1, 2, 3]);
	let mut lookup = p.lookup_preimage(key.try_into().expect("a blake2b-256 key is 32 bytes"));
	assert_eq!(block_on(lookup.next()).unwrap().unwrap(), Some(vec![1, 2, 3]));

	p.insert_preimage(vec![1, 2, 3]);

	assert!(lookup.next().now_or_never().is_none());
}

#[test]
fn a_dropped_lookup_is_pruned_when_the_next_one_registers() {
	let p = MockPlatform::new();
	drop(p.lookup_preimage([9; 32]));
	let _open = p.lookup_preimage([9; 32]);
	let subscribers = p.preimage_subscribers.lock().unwrap();
	assert_eq!(subscribers.get(&vec![9; 32]).map(Vec::len), Some(1));
}

#[test]
fn write_product_storage_is_read_back_and_notifies() {
	let p = MockPlatform::new();
	let mut updates = p.subscribe_storage("k".to_string());
	assert_eq!(block_on(updates.next()).unwrap().unwrap().value, None);

	p.write_product_storage("k".to_string(), vec![7]);

	assert_eq!(block_on(ProductStorage::read(&p, "k".to_string())).unwrap(), Some(vec![7]));
	assert_eq!(updates.next().now_or_never().unwrap().unwrap().unwrap().value, Some(vec![7]));
}

#[test]
fn chain_silent_records_sends_and_parks() {
	let p = MockPlatform::new();
	let conn = block_on(p.connect([0u8; 32])).unwrap();
	conn.send("req-1".to_string());
	assert_eq!(p.sent_rpc(), vec!["req-1".to_string()]);
	// Silent: the response stream never yields (parks rather than ends).
	assert!(conn.responses().next().now_or_never().is_none());
}

#[test]
fn chain_scripted_replays_frames() {
	let p = MockPlatform::with_config(MockConfig {
		chain: ChainBehavior::Scripted(vec!["frame-1".into(), "frame-2".into()]),
		..Default::default()
	});
	let conn = block_on(p.connect([0u8; 32])).unwrap();
	let frames: Vec<String> = block_on(conn.responses().collect());
	assert_eq!(frames, vec!["frame-1".to_string(), "frame-2".to_string()]);
}

#[test]
fn chain_closed_ends_stream_immediately() {
	let p = MockPlatform::with_config(MockConfig {
		chain: ChainBehavior::Closed,
		..Default::default()
	});
	let conn = block_on(p.connect([0u8; 32])).unwrap();
	// Closed ends at once (None), so disconnect paths fail fast instead of
	// parking like Silent.
	assert!(block_on(conn.responses().next()).is_none());
}

#[test]
fn chain_connect_error() {
	let p = MockPlatform::with_config(MockConfig {
		chain: ChainBehavior::ConnectError("offline".into()),
		..Default::default()
	});
	assert!(block_on(p.connect([0u8; 32])).is_err());
}

#[test]
fn auth_states_record_in_order() {
	let p = MockPlatform::new();
	p.auth_state_changed(AuthState::Disconnected);
	p.auth_state_changed(AuthState::Pairing { deeplink: "dl".into() });
	assert_eq!(p.auth_states().len(), 2);
}

#[test]
fn clone_shares_recordings() {
	let p = MockPlatform::new();
	let clone = p.clone();
	block_on(clone.navigate_to("z".into())).unwrap();
	assert_eq!(p.navigations(), vec!["z".to_string()]);
}

fn allocation_review(product: &str) -> UserConfirmationReview {
	UserConfirmationReview::ResourceAllocation(crate::platform::ResourceAllocationReview {
		calling_product_id: crate::platform::ProductId::for_test(product),
		resources: vec![],
	})
}

fn device_request(p: &MockPlatform, request: latest::HostDevicePermissionRequest) -> bool {
	block_on(p.device_permission(&mock_product(), request)).expect("device permission answers") ==
		PermissionDecision::AllowAlways
}

fn remote_request(p: &MockPlatform, permission: latest::RemotePermission) -> bool {
	block_on(p.remote_permission(&mock_product(), latest::RemotePermissionRequest { permission }))
		.expect("remote permission answers") ==
		PermissionDecision::AllowAlways
}

#[test]
fn reviews_carry_the_payload_not_just_the_kind() {
	// Two reviews of the SAME kind with different payloads: a kind-only
	// recording cannot tell these apart, which is the whole reason the
	// payload log exists.
	let p = MockPlatform::new();
	block_on(p.confirm_user_action(allocation_review("first.dot"))).unwrap();
	block_on(p.confirm_user_action(allocation_review("second.dot"))).unwrap();

	assert_eq!(
		p.confirmations(),
		vec![ConfirmKind::ResourceAllocation; 2],
		"the kind view should see two identical kinds",
	);
	let products: Vec<crate::platform::ProductId> = p
		.reviews()
		.into_iter()
		.map(|review| match review {
			UserConfirmationReview::ResourceAllocation(inner) => inner.calling_product_id,
			other => panic!("unexpected review {other:?}"),
		})
		.collect();
	assert_eq!(
		products,
		vec![
			crate::platform::ProductId::for_test("first.dot"),
			crate::platform::ProductId::for_test("second.dot")
		]
	);
}

#[test]
fn an_explicit_grant_overrides_a_deny_all_policy() {
	let p = MockPlatform::with_config(MockConfig {
		device_permissions: PermissionPolicy::DenyAll,
		..MockConfig::default()
	});
	assert!(!device_request(&p, latest::HostDevicePermissionRequest::Camera));
	p.grant_permission("Camera");
	assert!(device_request(&p, latest::HostDevicePermissionRequest::Camera));
	// The grant is per permission, not a policy flip.
	assert!(!device_request(&p, latest::HostDevicePermissionRequest::Microphone));
	assert_eq!(p.granted_permissions(), vec!["Camera".to_string()]);
}

#[test]
fn an_explicit_revoke_overrides_an_allow_all_policy() {
	let p = MockPlatform::new();
	assert!(remote_request(&p, latest::RemotePermission::ChainSubmit));
	p.revoke_permission("ChainSubmit");
	assert!(!remote_request(&p, latest::RemotePermission::ChainSubmit));
	assert!(p.granted_permissions().is_empty());
	// Dropping the override restores the policy rather than leaving a denial.
	p.reset_permission("ChainSubmit");
	assert!(remote_request(&p, latest::RemotePermission::ChainSubmit));
}

#[test]
fn enforcing_denies_whatever_was_not_explicitly_granted() {
	let p = MockPlatform::new();
	p.set_enforce_permissions(true);
	assert!(
		!device_request(&p, latest::HostDevicePermissionRequest::Camera),
		"enforcing must not fall back to the allow-all policy",
	);
	p.grant_permission("Camera");
	assert!(device_request(&p, latest::HostDevicePermissionRequest::Camera));
	assert!(!device_request(&p, latest::HostDevicePermissionRequest::Location));
}

#[test]
fn the_permission_log_records_surface_key_and_answer() {
	// The keys are spelled out rather than derived from the permission,
	// because a test that builds them the same way the mock does would
	// agree with any key space the mock happened to pick.
	let p = MockPlatform::new();
	p.revoke_permission("Camera");
	device_request(&p, latest::HostDevicePermissionRequest::Camera);
	remote_request(&p, latest::RemotePermission::ChainSubmit);

	assert_eq!(
		p.permission_log(),
		vec![
			PermissionLogEntry {
				tag: "Camera".to_string(),
				approved: false,
				kind: PermissionKind::Device,
			},
			PermissionLogEntry {
				tag: "ChainSubmit".to_string(),
				approved: true,
				kind: PermissionKind::Remote,
			},
		]
	);
}

#[test]
fn permissions_are_keyed_by_variant_tag_not_display_prose() {
	// The TypeScript mock keys on the SCALE tag, so a suite written once
	// against either mock has to reach the same decision here. `Display`
	// renders "camera" for a person to read, and keying on that would make
	// the grant below silently miss.
	let p = MockPlatform::new();
	p.revoke_permission("Camera");
	assert!(!device_request(&p, latest::HostDevicePermissionRequest::Camera));

	p.grant_permission("Camera");
	assert!(device_request(&p, latest::HostDevicePermissionRequest::Camera));

	// A grant that keyed on `Display` would read "access to a.example",
	// so it could never be issued before knowing the domains asked for,
	// and would not cover a second request naming different ones.
	p.revoke_permission("Remote");
	assert!(!remote_request(
		&p,
		latest::RemotePermission::Remote { domains: vec!["a.example".to_string()] }
	));
	assert!(!remote_request(
		&p,
		latest::RemotePermission::Remote {
			domains: vec!["b.example".to_string(), "c.example".to_string()],
		}
	));
}

fn mock_product() -> ProductContext {
	ProductContext::new(ProductId::for_test("mock.dot"))
}

fn create_room(p: &MockPlatform, room_id: &str) -> latest::ChatRoomRegistrationStatus {
	block_on(p.create_chat_room(
		&mock_product(),
		latest::HostChatCreateRoomRequest {
			room_id: room_id.to_string(),
			name: format!("{room_id} room"),
			icon: "https://example.invalid/i.png".to_string(),
		},
	))
	.expect("room registration succeeds")
	.status
}

fn post_text(
	p: &MockPlatform,
	room_id: &str,
	body: &str,
) -> Result<latest::HostChatPostMessageResponse, latest::HostChatPostMessageError> {
	block_on(p.post_chat_message(
		&mock_product(),
		latest::HostChatPostMessageRequest {
			room_id: room_id.to_string(),
			payload: latest::ChatMessageContent::Text { text: body.to_string() },
			alt: None,
		},
	))
}

#[test]
fn registering_a_room_twice_reports_the_second_as_existing() {
	let p = MockPlatform::new();
	assert_eq!(create_room(&p, "lobby"), latest::ChatRoomRegistrationStatus::New);
	assert_eq!(create_room(&p, "lobby"), latest::ChatRoomRegistrationStatus::Exists);
	// The repeat must not duplicate the room.
	assert_eq!(p.chat_rooms().len(), 1);
	assert_eq!(p.chat_rooms()[0].room_id, "lobby");
}

#[test]
fn posting_records_the_message_against_the_id_the_product_was_given() {
	let p = MockPlatform::new();
	create_room(&p, "lobby");
	let first = post_text(&p, "lobby", "hello").expect("post succeeds");
	let second = post_text(&p, "lobby", "again").expect("post succeeds");
	assert_ne!(first.message_id, second.message_id);

	let posted = p.posted_chat_messages();
	assert_eq!(posted.len(), 2);
	assert_eq!(posted[0].message_id, first.message_id);
	assert_eq!(posted[0].payload, latest::ChatMessageContent::Text { text: "hello".to_string() });
	assert_eq!(posted[1].message_id, second.message_id);
}

#[test]
fn posting_to_an_unregistered_room_is_an_error() {
	// A mock that accepted this would hide a product bug rather than
	// surface it.
	let p = MockPlatform::new();
	let err = post_text(&p, "never-created", "hi").expect_err("unknown room is rejected");
	assert!(
		matches!(err, latest::HostChatPostMessageError::Unknown { reason } if reason.contains("never-created")),
	);
	assert!(p.posted_chat_messages().is_empty());
}

#[test]
fn an_injected_chat_error_refuses_every_chat_entry_point() {
	let p = MockPlatform::with_config(MockConfig {
		faults: MockFaults { chat_error: Some("chat down".into()), ..Default::default() },
		..Default::default()
	});
	let product = mock_product();

	let room = block_on(p.create_chat_room(
		&product,
		latest::HostChatCreateRoomRequest {
			room_id: "lobby".to_string(),
			name: "lobby room".to_string(),
			icon: "https://example.invalid/i.png".to_string(),
		},
	))
	.expect_err("room creation carries the injected reason");
	assert!(
		matches!(room, latest::HostChatCreateRoomError::Unknown { reason } if reason == "chat down"),
	);

	let bot = block_on(p.register_chat_bot(
		&product,
		latest::HostChatRegisterBotRequest {
			bot_id: "greeter".to_string(),
			name: "Greeter".to_string(),
			icon: "https://example.invalid/i.png".to_string(),
		},
	))
	.expect_err("bot registration carries the injected reason");
	assert!(
		matches!(bot, latest::HostChatRegisterBotError::Unknown { reason } if reason == "chat down"),
	);

	// Posting reports the injected reason rather than the unknown-room
	// rejection it would otherwise hit, so the guard is what refused it.
	let message = post_text(&p, "lobby", "hi").expect_err("posting carries the injected reason");
	assert!(
		matches!(message, latest::HostChatPostMessageError::Unknown { reason } if reason == "chat down"),
	);

	// The subscription is an entry point too. Handing back a stream that
	// looks healthy and simply never carries a room is the one answer a
	// failing host must not give, because it is indistinguishable from a
	// host with nothing to list.
	let mut subscription = p.subscribe_chat_rooms(&product);
	let subscribed = subscription
		.next()
		.now_or_never()
		.expect("the refusal is ready immediately")
		.expect("an item")
		.expect_err("the subscription carries the injected reason");
	assert_eq!(subscribed.reason, "chat down");
	// And ends there. A stream that kept the consumer waiting after
	// refusing is the stalled stream this guard exists to avoid.
	assert!(
		subscription
			.next()
			.now_or_never()
			.expect("the end is ready immediately")
			.is_none()
	);

	assert!(p.chat_rooms().is_empty());
	assert!(p.posted_chat_messages().is_empty());
}

#[test]
fn a_room_subscriber_sees_the_current_list_and_later_replacements() {
	let p = MockPlatform::new();
	create_room(&p, "first");
	let mut rooms = p.subscribe_chat_rooms(&mock_product());

	// Both items are already queued on an unbounded channel by the time
	// they are asserted, so take them without blocking: a subscription
	// that never delivers must fail this test rather than hang it.
	let seeded = rooms
		.next()
		.now_or_never()
		.expect("the seeded item is ready immediately")
		.expect("a seeded item")
		.expect("chat rooms stream is infallible here");
	assert_eq!(seeded.rooms.len(), 1);

	// A later registration is delivered as a replacement list, which is
	// what the trait's "and later replacements" contract requires.
	create_room(&p, "second");
	let replacement = rooms
		.next()
		.now_or_never()
		.expect("registering a room must deliver a replacement list")
		.expect("a replacement item")
		.expect("chat rooms stream is infallible here");
	let ids: Vec<String> = replacement.rooms.into_iter().map(|room| room.room_id).collect();
	assert_eq!(ids, vec!["first".to_string(), "second".to_string()]);
}

#[test]
fn clearing_chat_state_empties_rooms_bots_and_messages() {
	let p = MockPlatform::new();
	create_room(&p, "lobby");
	post_text(&p, "lobby", "hi").expect("post succeeds");
	block_on(p.register_chat_bot(
		&mock_product(),
		latest::HostChatRegisterBotRequest {
			bot_id: "helper".to_string(),
			name: "Helper".to_string(),
			icon: "https://example.invalid/b.png".to_string(),
		},
	))
	.expect("bot registration succeeds");
	assert_eq!(p.chat_bots().len(), 1);

	p.clear_chat_state();

	assert!(p.chat_rooms().is_empty());
	assert!(p.chat_bots().is_empty());
	assert!(p.posted_chat_messages().is_empty());
}

#[test]
fn a_theme_subscriber_sees_the_current_theme_and_later_changes() {
	let p = MockPlatform::new();
	let mut themes = p.subscribe_theme();

	let seeded = themes
		.next()
		.now_or_never()
		.expect("the seeded theme is ready immediately")
		.expect("a seeded item")
		.expect("theme stream is infallible here");
	assert_eq!(seeded.variant, latest::ThemeVariant::Dark);

	p.set_theme(latest::ThemeVariant::Light);
	let changed = themes
		.next()
		.now_or_never()
		.expect("set_theme must deliver to live subscribers")
		.expect("a changed item")
		.expect("theme stream is infallible here");
	assert_eq!(changed.variant, latest::ThemeVariant::Light);
	assert_eq!(p.theme(), latest::ThemeVariant::Light);
}

#[test]
fn a_simulated_disconnect_ends_the_response_stream_and_blocks_reconnect() {
	let p = MockPlatform::with_config(MockConfig {
		// Silent responses stay pending forever, so if the disconnect is
		// not observed this assertion fails rather than passing by luck.
		chain: ChainBehavior::Silent,
		..MockConfig::default()
	});
	let connection = block_on(p.connect([0; 32])).expect("first connect succeeds");
	assert_eq!(p.chain_status(), ChainStatus::Connected);
	let mut responses = connection.responses();
	assert!(
		responses.next().now_or_never().is_none(),
		"a silent connection has nothing to deliver yet",
	);

	p.simulate_disconnect();

	assert_eq!(p.chain_status(), ChainStatus::Disconnected);
	assert!(
		matches!(responses.next().now_or_never(), Some(None)),
		"the disconnect must end the stream, not leave it pending",
	);
	assert!(
		block_on(p.connect([0; 32])).is_err(),
		"a disconnected chain must refuse new connections",
	);

	p.simulate_reconnect();
	assert!(block_on(p.connect([0; 32])).is_ok());
}

#[test]
fn seeded_preimages_are_readable_in_key_order() {
	let p = MockPlatform::new();
	let first = p.insert_preimage(vec![1, 2, 3]);
	let second = p.insert_preimage(vec![4, 5, 6]);
	let mut expected = vec![(first, vec![1, 2, 3]), (second, vec![4, 5, 6])];
	expected.sort_by(|(left, _), (right, _)| left.cmp(right));
	assert_eq!(p.preimages(), expected);

	p.clear_preimages();
	assert!(p.preimages().is_empty());
}

#[test]
fn an_injected_confirmation_fault_is_not_a_refusal() {
	// A host that could not ask is not a user who said no, and the mock
	// has to be able to express the difference.
	let p = MockPlatform::with_config(MockConfig {
		faults: MockFaults {
			confirmation_error: Some("no UI available".to_string()),
			..MockFaults::default()
		},
		..MockConfig::default()
	});
	let err = block_on(p.confirm_user_action(allocation_review("mock.dot")))
		.expect_err("the confirmation fails");
	assert_eq!(err.reason, "no UI available");
	// It still records what was asked, so a test can assert the prompt fired.
	assert_eq!(p.confirmations(), vec![ConfirmKind::ResourceAllocation]);
}

#[test]
fn injected_permission_and_feature_faults_surface_as_errors() {
	let p = MockPlatform::with_config(MockConfig {
		faults: MockFaults {
			permission_error: Some("permission backend down".to_string()),
			feature_error: Some("feature backend down".to_string()),
			..MockFaults::default()
		},
		..MockConfig::default()
	});
	assert!(
		block_on(p.device_permission(&mock_product(), latest::HostDevicePermissionRequest::Camera))
			.is_err()
	);
	assert!(
		block_on(p.remote_permission(
			&mock_product(),
			latest::RemotePermissionRequest { permission: latest::RemotePermission::ChainSubmit }
		))
		.is_err()
	);
	// A failed prompt is not a recorded decision.
	assert!(p.permission_log().is_empty());
}

#[test]
fn the_mock_stack_names_the_chains_the_mock_serves() {
	// Spelled out rather than compared against `mock_genesis`, because the
	// TypeScript mock declares these same three and a suite written against
	// either has to be answered the same way.
	let stack = mock_genesis::stack();
	assert_eq!(
		(stack.network(), stack.tld(), stack.chains().clone()),
		(
			"mock",
			"paseo",
			crate::platform::StackChains {
				relay: None,
				asset_hub: Some([0x33; 32]),
				people: [0x11; 32],
				bulletin: [0x22; 32],
			}
		)
	);
	assert_eq!(MockConfig::default().served_chains, vec![[0x11; 32], [0x22; 32], [0x33; 32]]);
}

#[test]
fn reset_clears_recordings_and_restores_policy_fallback() {
	let p = MockPlatform::new();
	block_on(p.navigate_to("u".into())).unwrap();
	block_on(p.confirm_user_action(allocation_review("mock.dot"))).unwrap();
	block_on(p.write("k".into(), vec![1])).unwrap();
	p.insert_preimage(vec![9]);
	p.auth_state_changed(AuthState::Disconnected);
	p.set_enforce_permissions(true);
	p.revoke_permission(latest::HostDevicePermissionRequest::Camera.to_string());
	device_request(&p, latest::HostDevicePermissionRequest::Camera);

	p.reset();

	assert!(p.navigations().is_empty());
	assert!(p.reviews().is_empty());
	assert!(p.confirmations().is_empty());
	assert!(p.auth_states().is_empty());
	assert!(p.permission_log().is_empty());
	assert!(p.granted_permissions().is_empty());
	assert_eq!(block_on(p.read("k".into())).unwrap(), None);
	// Enforcement and the explicit denial are both gone, so the allow-all
	// policy answers again.
	assert!(device_request(&p, latest::HostDevicePermissionRequest::Camera));
}
