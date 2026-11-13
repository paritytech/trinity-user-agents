#[cfg(not(target_arch = "wasm32"))]
mod cache;

use alloy_primitives::Address;

use base64::{Engine, engine::general_purpose::STANDARD};
use parity_scale_codec::Encode;

use super::*;

use crate::{
	chain::EvmContractError,
	chain_runtime::RuntimeFailure,
	host_logic::dotns_gateway::{
		DotnsCallError, DotnsPopController, DotnsRegistry, dispatcher_address_key,
	},
};

#[cfg(not(target_arch = "wasm32"))]
use crate::{Spawner, platform::HostInfo, test_support::StubPlatform};

struct ProfileContracts {
	deployed: bool,
	tld_reverts: bool,
	registry: Address,
	resolver: Address,
	text: Option<Result<String, EvmContractError>>,
	batches: usize,
}

impl ProfileContracts {
	fn new(text: Result<String, EvmContractError>) -> Self {
		Self {
			deployed: true,
			tld_reverts: false,
			registry: Address::repeat_byte(0x33),
			resolver: Address::repeat_byte(0x44),
			text: Some(text),
			batches: 0,
		}
	}
}

#[async_trait::async_trait]
impl DotnsTransport for ProfileContracts {
	async fn storage(&mut self, key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
		assert_eq!(key, dispatcher_address_key());
		Ok(self.deployed.then(|| vec![0x11; 20]))
	}

	async fn views(
		&mut self,
		dest: &[u8; 20],
		inputs: Vec<Vec<u8>>,
	) -> Vec<Result<Vec<u8>, EvmContractError>> {
		self.batches += 1;
		assert_eq!(inputs.len(), 3);
		let mut results = Vec::new();
		for input in inputs {
			results.push(self.view(dest, input).await);
		}
		results
	}

	async fn view(&mut self, dest: &[u8; 20], input: Vec<u8>) -> Result<Vec<u8>, EvmContractError> {
		if dest == &[0x11; 20] &&
			DotnsPopController::protocolRegistryCall::abi_decode(&input).is_ok()
		{
			return Ok(DotnsPopController::protocolRegistryCall::abi_encode_returns(
				&Address::repeat_byte(0x22),
			));
		}
		if dest == &[0x22; 20] {
			if ProtocolRegistry::tldCall::abi_decode(&input).is_ok() {
				if self.tld_reverts {
					return Err(EvmContractError::Reverted {
						detail: "tld view absent".to_string(),
					});
				}
				return Ok(ProtocolRegistry::tldCall::abi_encode_returns(&".paseo".to_string()));
			}
			if let Ok(call) = ProtocolRegistry::getCall::abi_decode(&input) {
				if call.key == B256::right_padding_from(b"registry") {
					return Ok(ProtocolRegistry::getCall::abi_encode_returns(&self.registry));
				}
				assert_eq!(call.key, B256::right_padding_from(b"contentResolver"));
				return Ok(ProtocolRegistry::getCall::abi_encode_returns(&self.resolver));
			}
		}
		let node = username_node(".paseo", "alice.01");
		if dest == &[0x33; 20] {
			let call = DotnsRegistry::resolverCall::abi_decode(&input).unwrap();
			assert_eq!(call.node, B256::from(node));
			return Ok(DotnsRegistry::resolverCall::abi_encode_returns(&Address::repeat_byte(
				0x55,
			)));
		}
		let call = ContentResolver::textCall::abi_decode(&input).unwrap();
		assert_eq!(dest, &[0x44; 20], "profiles use the deployment's content resolver");
		assert_eq!((call.node, call.key), (B256::from(node), PROFILE_RECORD_KEY.to_string()));
		self.text
			.take()
			.expect("profile read once")
			.map(|text| ContentResolver::textCall::abi_encode_returns(&text))
	}
}

#[tokio::test]
async fn a_default_reverse_resolver_does_not_hide_the_public_profile() {
	let record =
		ProfileRecord { entries: vec![subxt::utils::WrapperKeepOpaque::from_encoded(vec![7, 8])] };
	let mut transport = ProfileContracts::new(Ok(STANDARD.encode(record.encode())));

	assert_eq!(PublicProfiles::read_record_from(&mut transport, "alice.01").await.unwrap(), record);
}

#[tokio::test]
async fn a_profile_location_uses_the_deployments_contracts_and_tld() {
	let mut transport = ProfileContracts::new(Ok(String::new()));

	let location = ProfileRecordLocation::resolve(&mut transport, "alice.01").await;
	assert_eq!(
		(location.unwrap(), transport.batches),
		(
			Some(ProfileRecordLocation {
				node: username_node(".paseo", "alice.01"),
				registry: [0x33; 20],
				resolver: [0x44; 20],
			}),
			1
		)
	);
}

#[tokio::test]
async fn missing_profile_contracts_skip_the_tld_fallback() {
	for (registry, resolver) in
		[(Address::ZERO, Address::repeat_byte(0x44)), (Address::repeat_byte(0x33), Address::ZERO)]
	{
		let mut transport = ProfileContracts {
			tld_reverts: true,
			registry,
			resolver,
			..ProfileContracts::new(Ok(String::new()))
		};

		assert_eq!(ProfileRecordLocation::resolve(&mut transport, "alice.01").await.unwrap(), None);
	}
}

#[tokio::test]
async fn an_absent_gateway_or_record_has_an_empty_profile() {
	for deployed in [true, false] {
		let mut transport =
			ProfileContracts { deployed, ..ProfileContracts::new(Ok(String::new())) };

		assert_eq!(
			PublicProfiles::read_record_from(&mut transport, "alice.01").await.unwrap(),
			ProfileRecord::default()
		);
	}
}

#[tokio::test]
async fn profiles_accept_the_usernames_the_identity_reader_returns() {
	for username in ["alice01", "alice-smith", &"a".repeat(63), "alice.01"] {
		let mut transport =
			ProfileContracts { deployed: false, ..ProfileContracts::new(Ok(String::new())) };

		assert_eq!(ProfileRecordLocation::resolve(&mut transport, username).await.unwrap(), None);
	}
}

#[tokio::test]
async fn a_profile_location_rejects_invalid_usernames_even_without_a_gateway() {
	let mut transport =
		ProfileContracts { deployed: false, ..ProfileContracts::new(Ok(String::new())) };

	assert!(matches!(
		ProfileRecordLocation::resolve(&mut transport, "alice.1").await,
		Err(PublicProfileError::InvalidUsername)
	));
}

#[tokio::test]
async fn a_failed_content_read_is_not_an_absent_record() {
	for reverted in [true, false] {
		let failure = if reverted {
			EvmContractError::Reverted { detail: "profile unavailable".to_string() }
		} else {
			EvmContractError::Chain(RuntimeFailure::unavailable_with_reason(
				"profile",
				"RPC disconnected",
			))
		};
		let expected = failure.to_string();
		let mut transport = ProfileContracts::new(Err(failure));
		let location = ProfileRecordLocation::resolve(&mut transport, "alice.01")
			.await
			.unwrap()
			.unwrap();

		let error = location.read(&mut transport).await.unwrap_err();
		let DotnsError::TextRecord(DotnsCallError::View(error)) = error else {
			panic!("unexpected profile error: {error:?}");
		};
		assert_eq!(error.to_string(), expected);
	}
}

#[tokio::test]
async fn a_profile_failure_retains_the_contract_error_source() {
	let mut transport = ProfileContracts::new(Err(EvmContractError::Reverted {
		detail: "profile unavailable".into(),
	}));
	let error = PublicProfiles::read_record_from(&mut transport, "alice.01").await.unwrap_err();
	let source = core::error::Error::source(&error).expect("dotNS error is retained");

	assert!(matches!(
		source.downcast_ref::<Box<DotnsError>>().map(Box::as_ref),
		Some(DotnsError::TextRecord(DotnsCallError::View(EvmContractError::Reverted { detail })))
			if detail == "profile unavailable"
	));
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn a_profile_failure_retains_the_database_error_source() {
	let profiles = PublicProfiles::new(profile_services());
	let error = profiles.resolve("alice.01", false).await.unwrap_err();
	let source = core::error::Error::source(&error).expect("database error is retained");

	assert!(matches!(source.downcast_ref::<DbError>(), Some(DbError::NotConfigured)));
}

#[tokio::test]
async fn raw_profile_text_keeps_malformed_data_visible_to_writers() {
	let mut transport = ProfileContracts::new(Ok("not a profile".to_string()));
	let location = ProfileRecordLocation::resolve(&mut transport, "alice.01")
		.await
		.unwrap()
		.unwrap();

	assert_eq!(location.read(&mut transport).await.unwrap(), Some("not a profile".to_string()));
}

#[cfg(not(target_arch = "wasm32"))]
fn profile_services() -> Arc<RuntimeServices> {
	services_with_platform(Arc::new(StubPlatform {
		chain_connect_error: Some("offline"),
		..Default::default()
	}))
}

#[cfg(not(target_arch = "wasm32"))]
fn services_with_platform(platform: Arc<StubPlatform>) -> Arc<RuntimeServices> {
	RuntimeServices::new(
		platform,
		HostInfo {
			name: "Profile test".into(),
			icon: None,
			version: None,
			platform: truapi::latest::HostPlatform::Unknown,
		},
		crate::test_support::test_stack("paseo", [1; 32], [2; 32], [3; 32]),
		Spawner::for_test(),
	)
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn a_cached_record_survives_an_offline_chain_until_forced() {
	let (_directory, db) = crate::store::open_temporary_core_db();
	let record = PublicProfile { avatar: None };
	let now = current_unix_secs();
	db.write(move |tx| {
		dao::store_record(tx, "alice01", &ProfileRecord::default(), now, now + RECORD_TTL_SECS)
	})
	.await
	.unwrap();
	let services = profile_services();
	assert!(services.install_core_db(db));
	let profiles = PublicProfiles::new(services);

	let cached = profiles.resolve("alice01", false).await;
	let forced = profiles.resolve("alice01", true).await;
	assert_eq!(
		(cached.unwrap(), matches!(forced, Err(PublicProfileError::Dotns(_)))),
		(record, true)
	);
}

#[test]
fn a_lite_username_resolves_part_by_part() {
	let tld = ".paseo";
	let expected = namehash_under(&namehash_under(&tld_node(tld), "01"), "alice");

	assert_eq!(username_node(tld, "alice.01"), expected);
	assert_eq!(username_node(tld, "alicebc"), namehash_under(&tld_node(tld), "alicebc"));
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn cached_metadata_never_loads_bulletin() {
	for entries in [
		vec![subxt::utils::WrapperKeepOpaque::from_value(ProfileEntry::Avatar(ProfileAvatar {
			cid: [9; 32].into(),
			format: crate::ImageFormat::Png,
		}))],
		vec![subxt::utils::WrapperKeepOpaque::from_encoded(vec![0, 1])],
		vec![],
	] {
		let platform = Arc::new(StubPlatform::default());
		let services = services_with_platform(platform.clone());
		let (_directory, db) = crate::store::open_temporary_core_db();
		let now = current_unix_secs();
		let record = ProfileRecord { entries };
		let expected = PublicProfile { avatar: record.avatar() };
		db.write(move |tx| dao::store_record(tx, "alice.01", &record, now, now + RECORD_TTL_SECS))
			.await
			.unwrap();
		assert!(services.install_core_db(db));

		let profile = PublicProfiles::new(services).resolve("alice.01", false).await.unwrap();

		assert_eq!((profile, platform.chain_connects.lock().unwrap().clone()), (expected, vec![]));
	}
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn expired_and_forced_profiles_only_read_asset_hub_and_do_not_cache_errors() {
	for (expires_in, force) in [(0, false), (RECORD_TTL_SECS, true)] {
		let platform =
			Arc::new(StubPlatform { chain_connect_error: Some("offline"), ..Default::default() });
		let services = services_with_platform(platform.clone());
		let (_directory, db) = crate::store::open_temporary_core_db();
		let now = current_unix_secs();
		let record = ProfileRecord {
			entries: vec![subxt::utils::WrapperKeepOpaque::from_value(ProfileEntry::Avatar(
				ProfileAvatar { cid: [9; 32].into(), format: crate::ImageFormat::Png },
			))],
		};
		let stored = record.clone();
		db.write(move |tx| dao::store_record(tx, "alice.01", &stored, now, now + expires_in))
			.await
			.unwrap();
		assert!(services.install_core_db(db.clone()));

		let result = PublicProfiles::new(services).resolve("alice.01", force).await;
		let stored: Vec<u8> = db
			.read(|conn| {
				Ok(conn.query_row(
					"SELECT public_record FROM public_profiles WHERE username = 'alice.01'",
					[],
					|row| row.get(0),
				)?)
			})
			.await
			.unwrap();

		assert_eq!(
			(
				matches!(result, Err(PublicProfileError::Dotns(_))),
				platform.chain_connects.lock().unwrap().clone(),
				stored
			),
			(true, vec![[3; 32]], record.encode())
		);
	}
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn an_invalid_username_does_not_need_storage_or_a_chain() {
	let profiles = PublicProfiles::new(profile_services());

	assert!(matches!(
		profiles.resolve("alice.1", false).await,
		Err(PublicProfileError::InvalidUsername)
	));
}
