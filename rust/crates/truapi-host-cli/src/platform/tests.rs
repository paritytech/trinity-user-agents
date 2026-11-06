use super::*;
use crate::chain::ChainBackend;

/// The preset production builds from, so tests exercise the same config.
fn test_network() -> crate::network::Stack {
	crate::network::Network::default().bundled_stack(ChainBackend::Rpc)
}

/// Battery examples that preflight `getChainInfo` resolve the genesis they
/// ask for through this set, so an error here fails every one of them.
///
/// Serving the preset's three roles unblocks the preflight in every example
/// that asks for one: `People` for account-alias, account-proof, ring-VRF
/// registration and both create-transaction variants, and `AssetHub` for the
/// other seventeen.
///
/// `feature_supported` answers from the same set `supported_chains` serves, so
/// the two cannot disagree. The negatives are the malformed inputs, a well-formed
/// hash the host serves no role for, and the all-zero SSO sentinel — which the
/// provider does route, to the People fallback, yet is still not a served role.
#[test]
fn feature_supported_resolves_against_the_served_chain_set() {
	let platform = CliPlatform::new(&test_network(), None, ApprovalPolicy::AutoAccept, None);
	let stack = crate::network::Network::default().bundled_stack(ChainBackend::Rpc);

	let supported =
		|genesis: Vec<u8>| {
			futures::executor::block_on(platform.feature_supported(
				api::HostFeatureSupportedRequest::Chain { genesis_hash: genesis },
			))
			.expect("feature_supported is wired")
			.supported
		};

	// Every role the host serves answers supported. The reverse direction is the
	// `unserved` assertion below, since every served role is now a real chain.
	for entry in stack.host_chain_set().chains {
		assert!(
			supported(entry.genesis_hash.to_vec()),
			"{:?} is served but reported unsupported",
			entry.identifier
		);
	}

	// A well-formed hash the host does not serve is unsupported. Asset Hub used
	// to be this case; without a stand-in, "supported" could degrade to "is 32
	// bytes" and only the all-zero sentinel would notice.
	let unserved = [0xab; 32];
	assert!(
		!stack.host_chain_set().chains.iter().any(|entry| entry.genesis_hash == unserved),
		"the stand-in must not be a served role"
	);
	assert!(!supported(unserved.to_vec()));

	// A malformed genesis is unsupported, never a panic or a truncated match.
	assert!(!supported(Vec::new()));
	assert!(!supported(stack.genesis().people[..31].to_vec()));
	assert!(!supported([stack.genesis().people.as_slice(), &[0u8]].concat()));
	assert!(!supported(vec![0u8; 32]));
}

#[test]
fn supported_chains_answers_the_configured_network() {
	let platform = CliPlatform::new(&test_network(), None, ApprovalPolicy::AutoAccept, None);
	let set = futures::executor::block_on(platform.supported_chains())
		.expect("the CLI host serves the preset's chains");

	let stack = crate::network::Network::default().bundled_stack(ChainBackend::Rpc);
	assert_eq!(set.network, stack.namespace());
	let mut served = set
		.chains
		.iter()
		.map(|entry| (entry.identifier, hex::encode(entry.genesis_hash)))
		.collect::<Vec<_>>();
	served.sort_by_key(|(identifier, _)| format!("{identifier:?}"));
	let mut expected = vec![
		(api::ChainIdentifier::People, hex::encode(stack.genesis().people)),
		(api::ChainIdentifier::Bulletin, hex::encode(stack.genesis().bulletin)),
		(api::ChainIdentifier::AssetHub, hex::encode(stack.genesis().asset_hub)),
	];
	expected.sort_by_key(|(identifier, _)| format!("{identifier:?}"));
	assert_eq!(served, expected);
}
use tempfile::tempdir;

#[test]
fn record_approval_appends_one_line_per_decision() {
	let dir = tempdir().expect("tempdir");
	let path = dir.path().join("approvals.log");
	record_approval(&path, true, "allocate resources");
	record_approval(&path, false, "sign VRF transcript");
	record_approval(&path, true, "sign VRF transcript");
	assert_eq!(
		std::fs::read_to_string(&path).expect("approvals transcript"),
		"approved allocate resources\ndenied sign VRF transcript\napproved sign VRF transcript\n",
	);
}

#[test]
fn record_navigation_appends_one_json_line_per_request() {
	let dir = tempdir().expect("tempdir");
	let path = dir.path().join("navigations.log");
	record_navigation(&path, "polkadot://dim2.dot/recover", 1).expect("first");
	record_navigation(&path, "https://example.com/a?b=\"c\"", 2).expect("second");
	let text = std::fs::read_to_string(&path).expect("navigation transcript");
	let lines: Vec<serde_json::Value> = text
		.lines()
		.map(|line| serde_json::from_str(line).expect("json line"))
		.collect();
	assert_eq!(
		lines,
		vec![
			serde_json::json!({ "url": "polkadot://dim2.dot/recover", "at": 1 }),
			serde_json::json!({ "url": "https://example.com/a?b=\"c\"", "at": 2 }),
		]
	);
}

#[test]
fn record_navigation_reports_an_unwritable_transcript() {
	let dir = tempdir().expect("tempdir");
	let path = dir.path().join("missing").join("navigations.log");
	let error = record_navigation(&path, "polkadot://dim2.dot", 1).unwrap_err();
	let StorageError::RecordNavigation { path: failed_path, source } = &error else {
		panic!("expected the navigation operation and path: {error:?}");
	};
	assert_eq!((failed_path, source.kind()), (&path, std::io::ErrorKind::NotFound));
	let reason = format!("could not record navigation in {}: {source}", path.display());
	let source_kind = std::error::Error::source(&error)
		.and_then(|source| source.downcast_ref::<std::io::Error>())
		.map(std::io::Error::kind);
	assert_eq!(
		(source_kind, api::GenericError::from(error)),
		(Some(std::io::ErrorKind::NotFound), api::GenericError { reason })
	);
}

#[tokio::test]
async fn navigate_to_records_the_url_it_was_handed() {
	let dir = tempdir().expect("tempdir");
	let path = dir.path().join("navigations.log");
	let mut platform = CliPlatform::new(&test_network(), None, ApprovalPolicy::Prompt, None);
	Arc::get_mut(&mut platform).expect("sole owner").navigations_log = Some(path.clone());
	platform
		.navigate_to("polkadot://dim2.dot/recover".to_string())
		.await
		.expect("navigate_to");
	let text = std::fs::read_to_string(&path).expect("navigation transcript");
	let line: serde_json::Value = serde_json::from_str(text.trim_end()).expect("one json line");
	assert_eq!(line["url"], "polkadot://dim2.dot/recover");
	assert!(line["at"].as_u64().expect("unix ms") > 1_700_000_000_000);
}

#[tokio::test]
async fn navigate_to_without_a_transcript_still_succeeds() {
	let platform = CliPlatform::new(&test_network(), None, ApprovalPolicy::Prompt, None);
	platform
		.navigate_to("polkadot://dim2.dot".to_string())
		.await
		.expect("navigate_to");
}

#[tokio::test]
async fn approval_policy_changes_apply_to_future_confirmations() {
	let platform = CliPlatform::new(&test_network(), None, ApprovalPolicy::Prompt, None);

	assert_eq!(platform.approval_policy(), ApprovalPolicy::Prompt);
	platform.set_approval_policy(ApprovalPolicy::AutoAccept);
	assert_eq!(platform.approval_policy(), ApprovalPolicy::AutoAccept);
	assert!(platform.decide("test action", "test detail".to_string()).await);

	platform.set_approval_policy(ApprovalPolicy::Prompt);
	assert_eq!(platform.approval_policy(), ApprovalPolicy::Prompt);
}

/// A user id `validate_name` rejects must not leave the previous identity's
/// storage mounted. `storage_user_id` falls back to `full_username`, a
/// free-form People-chain display name, and `validate_name` rejects uppercase,
/// spaces and non-ASCII — so `"Tarik Gul"` reaches this path.
#[test]
fn a_rejected_user_id_does_not_leave_the_previous_users_storage_mounted() {
	let dir = tempdir().expect("tempdir");
	let network_dir = dir.path().join("paseo");
	std::fs::create_dir_all(&network_dir).expect("network dir");

	let platform = CliPlatform::new(
		&test_network(),
		Some(CliStoragePaths::pairing(network_dir.clone())),
		ApprovalPolicy::AutoAccept,
		None,
	);

	// First identity: a valid lite username.
	platform.switch_pairing_user_storage("alice").expect("switch to alice");
	let alice_dir = platform.state_dir().expect("alice state dir");
	assert!(alice_dir.ends_with("alice_pairing_host"), "unexpected dir: {}", alice_dir.display());

	// Second identity arrives with a display name validate_name rejects. It
	// must still get its own namespace rather than inheriting alice's.
	platform
		.switch_pairing_user_storage("Tarik Gul")
		.expect("an unusable id must still isolate, not fail");

	let after = platform.state_dir().expect("state dir after");
	assert_ne!(
		after,
		alice_dir,
		"the rejected identity is still mounted on alice's storage at {}",
		after.display()
	);

	// The derived name is the hex digest of the raw id, and is itself a legal
	// session name so the persisted pointer survives a restart.
	let expected = format!("{}_pairing_host", pairing_storage_name("Tarik Gul"));
	assert!(after.ends_with(&expected), "expected a derived namespace, got {}", after.display());
	assert!(crate::sessions::validate_name(&pairing_storage_name("Tarik Gul")).is_ok());
}

/// The same thing through the lifecycle the core actually drives:
/// `AuthPresenter::auth_state_changed` with a `Connected` state. That path
/// only logs a `warn!` on failure, so a rejected id used to silently inherit
/// the previous identity's namespace.
///
/// Asserts the isolation itself, not just that the paths differ: a value
/// written while alice is connected must not be readable by the next
/// identity.
#[tokio::test]
async fn auth_state_changed_does_not_inherit_storage_on_a_rejected_username() {
	use truapi::platform::AuthPresenter;

	let dir = tempdir().expect("tempdir");
	let network_dir = dir.path().join("paseo");
	std::fs::create_dir_all(&network_dir).expect("network dir");

	let platform = CliPlatform::new(
		&test_network(),
		Some(CliStoragePaths::pairing(network_dir)),
		ApprovalPolicy::AutoAccept,
		None,
	);

	let connect = |lite: Option<&str>, full: Option<&str>| {
		AuthState::Connected(Box::new(SessionUiInfo {
			lite_username: lite.map(str::to_string),
			full_username: full.map(str::to_string),
			..SessionUiInfo::default()
		}))
	};

	platform.auth_state_changed(connect(Some("alice"), None));
	let alice_dir = platform.state_dir().expect("alice state dir");
	assert!(alice_dir.ends_with("alice_pairing_host"));

	// Alice's product KV. `switch_pairing_user_storage` intentionally carries
	// the transient pairing keys (AuthSession, PairingDeviceIdentity,
	// LastProcessedPairingStatement) across namespaces, because the core
	// writes a session before it knows the username. Product KV is the state
	// the comment there promises stays with its own user, so that is what
	// isolation is asserted on.
	let alice_key = ProductStorageKey::new(ProductId::for_test("demo-product.dot"), "secret");
	platform
		.write(alice_key.encode(), b"alice-only".to_vec())
		.await
		.expect("alice product write");

	// No lite username, so storage_user_id falls back to full_username.
	platform.auth_state_changed(connect(None, Some("Tarik Gul")));

	let after = platform.state_dir().expect("state dir after");
	assert_ne!(
		after,
		alice_dir,
		"second identity is serving out of alice's storage at {}",
		after.display()
	);

	// The isolation that matters: alice's product KV is not visible.
	assert_eq!(
		platform.read(alice_key.encode()).await.expect("read"),
		None,
		"the second identity can read alice's product storage"
	);
}

/// One undecodable value in `core-storage.json` must not cost the host the
/// entries it never touched. Before the file was moved aside, a real
/// `CliPlatform` discarded `PairingDeviceIdentity` and then overwrote it on
/// the next unrelated write.
///
/// Keys on disk are hex of the SCALE-encoded `CoreStorageKey`, so
/// `AuthSession` is `00` and `PairingDeviceIdentity` is `01`.
#[tokio::test]
async fn cli_platform_preserves_unreadable_core_storage() {
	use truapi::platform::CoreStorage;

	let dir = tempdir().expect("tempdir");
	let state_dir = dir.path().join("state");
	let product_dir = dir.path().join("products");
	std::fs::create_dir_all(&state_dir).expect("state dir");
	let core_path = state_dir.join("core-storage.json");

	std::fs::write(&core_path, r#"{"values":{"00":"cafebabe","01":"deadbeef","ff":"zzzz"}}"#)
		.expect("seed core storage");

	let platform = CliPlatform::new(
		&test_network(),
		Some(CliStoragePaths::new(state_dir, product_dir)),
		ApprovalPolicy::AutoAccept,
		None,
	);

	// Nothing loaded: the good entries went with the bad one.
	assert_eq!(
		platform.read_core_storage(CoreStorageKey::AuthSession).await.expect("read"),
		None,
		"AuthSession should have loaded but the file was discarded"
	);
	assert_eq!(
		platform
			.read_core_storage(CoreStorageKey::PairingDeviceIdentity)
			.await
			.expect("read"),
		None
	);

	// The host writes one unrelated key. That persists the whole map.
	platform
		.write_core_storage(CoreStorageKey::AuthSession, vec![0x42])
		.await
		.expect("write");

	let after = std::fs::read_to_string(&core_path).expect("read back");
	assert!(after.contains("42"), "the new AuthSession value should be persisted: {after}");

	// The unreadable original is preserved beside it, so nothing is lost.
	let preserved = std::fs::read_to_string(core_path.with_extension(UNREADABLE_SUFFIX))
		.expect("the unreadable file should have been moved aside");
	assert!(
		preserved.contains("deadbeef") && preserved.contains("cafebabe"),
		"the moved-aside file should still hold the original entries: {preserved}"
	);
}

/// `read_string_map` collects into a `Result`, so one undecodable value fails
/// the whole file and the load yields nothing. That is survivable only
/// because the file is moved aside first — otherwise the next write persists
/// the empty map over it and every intact entry is gone.
#[test]
fn an_unreadable_storage_file_is_moved_aside_before_the_next_write() {
	let dir = tempdir().expect("tempdir");
	let path = dir.path().join("core-storage.json");

	// Two entries the host cares about, plus one whose value is not hex.
	let raw = r#"{"values":{
        "6175746853657373696f6e":"cafebabe",
        "70616972696e674964656e74697479":"deadbeef",
        "62726f6b656e":"zzzz"
    }}"#;
	std::fs::write(&path, raw).expect("seed");

	// The two good entries are readable in isolation, so nothing about them
	// is malformed — they are collateral.
	assert!(raw.contains("cafebabe") && raw.contains("deadbeef"));

	// Load: everything is dropped, not just the bad entry.
	let loaded = load_hex_key_map(&path);
	assert!(
		loaded.is_empty(),
		"expected the whole file to be discarded, got {} entries",
		loaded.len()
	);

	// A later write persists the empty map plus whatever is new. The original
	// is no longer at `path`, so this must not be able to destroy it.
	let mut next: HashMap<Vec<u8>, Vec<u8>> = loaded;
	next.insert(b"fresh".to_vec(), b"\x01".to_vec());
	save_hex_key_map(&path, &next).expect("save");

	let after = std::fs::read_to_string(&path).expect("read back");
	assert!(after.contains(&hex::encode(b"fresh")));

	let preserved = std::fs::read_to_string(path.with_extension(UNREADABLE_SUFFIX))
		.expect("the unreadable file should have been moved aside");
	assert!(
		preserved.contains("cafebabe") && preserved.contains("deadbeef"),
		"the moved-aside file should still hold the original entries: {preserved}"
	);
}

fn test_storage_paths(root: &Path, session: &str) -> CliStoragePaths {
	CliStoragePaths::new(root.to_path_buf(), root.join("storage").join(session))
}

#[test]
fn connected_user_id_prefers_the_full_username() {
	let info = SessionUiInfo {
		lite_username: Some("alice.dot".to_string()),
		full_username: Some("Alice".to_string()),
		..SessionUiInfo::default()
	};
	assert_eq!(connected_user_id(&info), Some("Alice"));

	let info =
		SessionUiInfo { lite_username: Some("alice.dot".to_string()), ..SessionUiInfo::default() };
	assert_eq!(connected_user_id(&info), Some("alice.dot"));
	assert_eq!(connected_user_id(&SessionUiInfo::default()), None);
}

#[test]
fn pairing_storage_switches_with_the_connected_username() {
	let temporary = tempdir().expect("create pairing storage root");
	let network_dir = temporary.path().join("testnet");
	let platform = CliPlatform::new(
		&test_network(),
		Some(CliStoragePaths::pairing(network_dir.clone())),
		ApprovalPolicy::AutoAccept,
		None,
	);
	let product_key = ProductStorageKey::new(ProductId::for_test("product.dot"), "theme");

	platform.switch_pairing_user_storage("alice.dot").expect("select alice");
	futures::executor::block_on(platform.write(product_key.encode(), b"dark".to_vec()))
		.expect("write alice product value");

	platform.switch_pairing_user_storage("bob.dot").expect("select bob");
	assert_eq!(
		futures::executor::block_on(platform.read(product_key.encode()))
			.expect("read bob product value"),
		None
	);

	platform.switch_pairing_user_storage("alice.dot").expect("restore alice");
	assert_eq!(
		futures::executor::block_on(platform.read(product_key.encode()))
			.expect("read alice product value"),
		Some(b"dark".to_vec())
	);
	assert_eq!(
		platform.state_dir().as_deref(),
		Some(network_dir.join("alice.dot_pairing_host").as_path())
	);
	assert_eq!(
		read_current_pairing_user(&network_dir.join("pairing-host")).as_deref(),
		Some("alice.dot")
	);
}

#[test]
fn device_encryption_key_outlives_pairing_user_switches() {
	let temporary = tempdir().expect("create pairing storage root");
	let network_dir = temporary.path().join("testnet");
	let platform = CliPlatform::new(
		&test_network(),
		Some(CliStoragePaths::pairing(network_dir.clone())),
		ApprovalPolicy::AutoAccept,
		None,
	);

	platform.switch_pairing_user_storage("alice.dot").expect("select alice");
	futures::executor::block_on(
		platform.write_core_storage(CoreStorageKey::DeviceEncryptionKey, vec![7; 32]),
	)
	.expect("write device key");

	// Peers address this install by the matching public key, so switching
	// users must not strand them on a regenerated one.
	platform.switch_pairing_user_storage("bob.dot").expect("select bob");
	assert_eq!(
		futures::executor::block_on(
			platform.read_core_storage(CoreStorageKey::DeviceEncryptionKey)
		)
		.expect("read device key as bob"),
		Some(vec![7; 32])
	);

	// A user-scoped slot stays isolated, so the routing is not simply
	// making every slot global.
	futures::executor::block_on(
		platform.write_core_storage(CoreStorageKey::AutoSigningKeys, vec![1, 2, 3]),
	)
	.expect("write bob auto-signing keys");
	platform.switch_pairing_user_storage("alice.dot").expect("restore alice");
	assert_eq!(
		futures::executor::block_on(platform.read_core_storage(CoreStorageKey::AutoSigningKeys))
			.expect("read alice auto-signing keys"),
		None
	);

	// It also survives a fresh process reading the same directories.
	let restarted = CliPlatform::new(
		&test_network(),
		Some(CliStoragePaths::pairing(network_dir)),
		ApprovalPolicy::AutoAccept,
		None,
	);
	assert_eq!(
		futures::executor::block_on(
			restarted.read_core_storage(CoreStorageKey::DeviceEncryptionKey)
		)
		.expect("read device key after restart"),
		Some(vec![7; 32])
	);
}

/// A pairing login writes product KV before its username is known, so the
/// bootstrap directory's products must follow the first resolved user
/// instead of being stranded outside every identity namespace.
#[test]
fn product_storage_written_before_the_username_carries_into_the_resolved_user() {
	let temporary = tempdir().expect("create pairing storage root");
	let network_dir = temporary.path().join("testnet");
	let product_key = ProductStorageKey::new(ProductId::for_test("product.dot"), "theme");
	save_product_storage(
		&network_dir.join("pairing-host/storage"),
		&ProductId::for_test("product.dot"),
		&HashMap::from([("theme".to_string(), b"dark".to_vec())]),
	)
	.expect("write bootstrap pairing product storage");
	let platform = CliPlatform::new(
		&test_network(),
		Some(CliStoragePaths::pairing(network_dir.clone())),
		ApprovalPolicy::AutoAccept,
		None,
	);

	platform
		.switch_pairing_user_storage("alice.dot")
		.expect("resolve the bootstrap storage owner");

	assert_eq!(
		futures::executor::block_on(platform.read(product_key.encode()))
			.expect("read carried product value"),
		Some(b"dark".to_vec())
	);
	// Persisted, not merely carried in memory: a restart must find it too.
	assert_eq!(
		load_product_storage(&network_dir.join("alice.dot_pairing_host/storage")),
		HashMap::from([(
			ProductId::for_test("product.dot"),
			HashMap::from([("theme".to_string(), b"dark".to_vec())]),
		)])
	);
}

/// Both products by name, because a signature made with an account the
/// caller does not own is the thing the user has to be able to see.
#[test]
fn statement_proof_approval_names_both_products_without_dumping_payload() {
	let review = UserConfirmationReview::StatementStoreProductSign(
		truapi::platform::StatementStoreProductSignReview {
			calling_product_id: Some(ProductId::for_test("dim2next.paseo")),
			account: api::ProductAccountId {
				dot_ns_identifier: ProductId::for_test("dim2.paseo"),
				derivation_index: api::DerivationIndex::Index(0),
			},
			payload: vec![0x42; 128],
		},
	);

	let (action, detail) = approval_summary(&review);

	assert_eq!(action, "sign statement proof");
	assert_eq!(
		detail,
		"Product dim2next.paseo requested a Statement Store proof signature for the \
         dim2.paseo account over a 128-byte payload."
	);
	assert!(!detail.contains("[66"));
}

#[test]
fn vrf_approval_names_both_products_without_dumping_transcript_values() {
	let review = UserConfirmationReview::SignVrf(truapi::platform::SignVrfReview {
		calling_product_id: ProductId::for_test("caller.dot"),
		request: truapi::v01::HostAccountSignVrfRequest {
			account: truapi::v01::ProductAccountId {
				dot_ns_identifier: ProductId::for_test("target.dot"),
				derivation_index: truapi::v01::DerivationIndex::Raw([7; 32]),
			},
			transcript_label: b"lottery".to_vec(),
			items: vec![truapi::v01::VrfTranscriptItem {
				label: b"round".to_vec(),
				value: vec![0x42; 128],
			}],
		},
	});

	let (action, detail) = approval_summary(&review);

	assert_eq!(action, "sign VRF transcript");
	assert_eq!(
		detail,
		"Product caller.dot requested a VRF signature for the target.dot account over 1 transcript items."
	);
	assert!(!detail.contains("[66"));
}

#[test]
fn cli_notifications_return_stable_ids_and_cancel_idempotently() {
	let platform = CliPlatform::new(&test_network(), None, ApprovalPolicy::AutoAccept, None);
	let first =
		futures::executor::block_on(platform.push_notification(api::HostPushNotificationRequest {
			text: "Hello".to_string(),
			deeplink: None,
			scheduled_at: None,
		}))
		.expect("immediate notification");
	let second =
		futures::executor::block_on(platform.push_notification(api::HostPushNotificationRequest {
			text: "Again".to_string(),
			deeplink: Some("polkadot://example".to_string()),
			scheduled_at: None,
		}))
		.expect("second notification");

	assert_eq!(first.id, 1);
	assert_eq!(second.id, 2);
	futures::executor::block_on(platform.cancel_notification(first.id))
		.expect("already-fired cancellation is idempotent");
	futures::executor::block_on(platform.cancel_notification(999))
		.expect("unknown cancellation is idempotent");
}

#[test]
fn product_storage_uses_safe_per_product_files() {
	let temporary = tempdir().expect("create product storage root");
	let first = ProductStorageKey::new(ProductId::for_test("first.dot"), "theme");
	let localhost = ProductStorageKey::new(ProductId::for_test("localhost:3000"), "theme");
	let platform = CliPlatform::new(
		&test_network(),
		Some(test_storage_paths(temporary.path(), "test")),
		ApprovalPolicy::AutoAccept,
		None,
	);

	futures::executor::block_on(async {
		platform
			.write(first.encode(), b"dark".to_vec())
			.await
			.expect("write first product");
		platform
			.write(localhost.encode(), b"light".to_vec())
			.await
			.expect("write localhost product");
	});

	let directory = temporary.path().join("storage").join("test");
	let files = fs::read_dir(&directory)
		.expect("list product storage")
		.map(|entry| entry.expect("product storage entry").path())
		.collect::<Vec<_>>();
	assert_eq!(files.len(), 2);
	assert!(files.iter().all(|path| {
		path.parent() == Some(directory.as_path()) &&
			path.extension().and_then(|extension| extension.to_str()) == Some("json") &&
			!path
				.file_name()
				.and_then(|name| name.to_str())
				.is_some_and(|name| name.contains(':'))
	}));

	drop(platform);
	let restored = CliPlatform::new(
		&test_network(),
		Some(test_storage_paths(temporary.path(), "test")),
		ApprovalPolicy::AutoAccept,
		None,
	);
	let (first_value, localhost_value) = futures::executor::block_on(async {
		(
			restored.read(first.encode()).await.expect("read first"),
			restored.read(localhost.encode()).await.expect("read localhost"),
		)
	});
	assert_eq!(first_value, Some(b"dark".to_vec()));
	assert_eq!(localhost_value, Some(b"light".to_vec()));
}

#[test]
fn product_storage_is_isolated_by_session_then_product() {
	let temporary = tempdir().expect("create session storage root");
	let same_product = ProductId::for_test("same.dot");
	let key = ProductStorageKey::new(same_product.clone(), "value");
	let first = CliPlatform::new(
		&test_network(),
		Some(test_storage_paths(temporary.path(), "first")),
		ApprovalPolicy::AutoAccept,
		None,
	);
	let second = CliPlatform::new(
		&test_network(),
		Some(test_storage_paths(temporary.path(), "second")),
		ApprovalPolicy::AutoAccept,
		None,
	);

	futures::executor::block_on(async {
		first.write(key.encode(), b"one".to_vec()).await.expect("write first session");
		second.write(key.encode(), b"two".to_vec()).await.expect("write second session");
	});

	let first_path =
		product_storage_path(&temporary.path().join("storage").join("first"), &same_product);
	let second_path =
		product_storage_path(&temporary.path().join("storage").join("second"), &same_product);
	assert!(first_path.is_file());
	assert!(second_path.is_file());
	assert_ne!(
		fs::read_to_string(first_path).expect("read first session file"),
		fs::read_to_string(second_path).expect("read second session file")
	);
}
