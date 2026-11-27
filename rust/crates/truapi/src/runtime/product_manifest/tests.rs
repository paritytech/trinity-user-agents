use super::*;
use crate::host_logic::dotns_gateway::{
	DotnsPopController, ProtocolRegistry, dispatcher_address_key,
};
use alloy_primitives::{Address, B256};
use alloy_sol_types::SolCall;

struct ManifestContracts;

#[async_trait::async_trait]
impl DotnsTransport for ManifestContracts {
	async fn storage(&mut self, key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
		assert_eq!(key, dispatcher_address_key());
		Ok(Some(vec![0x11; 20]))
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
				return Ok(ProtocolRegistry::tldCall::abi_encode_returns(&".paseo".to_string()));
			}
			let call = ProtocolRegistry::getCall::abi_decode(&input).unwrap();
			assert_eq!(call.key, B256::right_padding_from(b"registry"));
			return Ok(ProtocolRegistry::getCall::abi_encode_returns(&Address::repeat_byte(0x33)));
		}
		let node = node_on(".paseo", "shop.paseo");
		if dest == &[0x33; 20] {
			let call = DotnsRegistry::resolverCall::abi_decode(&input).unwrap();
			assert_eq!(call.node.0, node);
			return Ok(DotnsRegistry::resolverCall::abi_encode_returns(&Address::repeat_byte(
				0x55,
			)));
		}
		let call = ContentResolver::textCall::abi_decode(&input).unwrap();
		assert_eq!((*dest, call.node.0, call.key.as_str()), ([0x55; 20], node, "manifest"));
		Err(EvmContractError::Reverted { detail: "node resolver has no text records".into() })
	}

	async fn views(
		&mut self,
		_dest: &[u8; 20],
		_inputs: Vec<Vec<u8>>,
	) -> Vec<Result<Vec<u8>, EvmContractError>> {
		unreachable!("this lookup uses individual views")
	}
}

#[test]
fn a_manifest_uses_the_nodes_resolver_and_treats_a_revert_as_absent() {
	assert_eq!(
		futures::executor::block_on(read_text_record(
			&mut ManifestContracts,
			&ProductId::for_test("shop.paseo"),
			None,
			MANIFEST_RECORD_KEY,
		))
		.unwrap(),
		None
	);
}

/// The node a product id resolves to on a network serving `tld`.
fn node_on(tld: &str, product_id: &str) -> [u8; 32] {
	namehash_under(&tld_node(tld), ProductId::for_test(product_id).bare_label())
}

#[test]
fn a_node_matches_the_chain_derivation() {
	// Pinned against paseo-v2, where `ProtocolRegistry.tldNode()` reads
	// 0x096b43… and the dotNS SDK derives `browse.paseo` as 0x185056….
	assert_eq!(
		hex::encode(tld_node(".paseo")),
		"096b436ee9a398429fe33ad4b359bad4398dd74b412ec1dd043c93dfbf581874"
	);
	assert_eq!(
		hex::encode(node_on(".paseo", "browse.paseo")),
		"1850561ffded63ac23dac8fd5e793fca1f349729ed6ade91c45f44a9f7b6b781"
	);
}

#[test]
fn the_tld_an_identifier_carries_does_not_change_the_node_it_resolves_to() {
	// A product id minted on `.dot` has to resolve against a `.paseo`
	// deployment; the suffix it was written with names no node of its own.
	let expected = namehash_under(&tld_node(".paseo"), "dim2");
	assert_eq!(node_on(".paseo", "dim2.dot"), expected);
	assert_eq!(node_on(".paseo", "dim2.paseo"), expected);
}

#[test]
fn one_identifier_resolves_differently_on_two_networks() {
	// The other half of the same property: the network's TLD is what
	// separates deployments, so the same id must not collide across them.
	assert_ne!(node_on(".paseo", "dim2.dot"), node_on(".dot", "dim2.dot"));
}

// The worker subname is the dotNS name `worker.<label>.<tld>`, whichever
// spelling of the product id the caller holds.
#[test]
fn a_worker_subname_is_the_namehash_of_its_full_name() {
	let namehash = |name: &str| {
		name.rsplit('.').fold([0u8; 32], |parent, label| namehash_under(&parent, label))
	};

	assert_eq!(
		[
			record_node(".paseo", &ProductId::for_test("browse.dot"), Some(WORKER_SUBNAME)),
			record_node(".paseo", &ProductId::for_test("worker.browse.dot"), Some(WORKER_SUBNAME)),
		],
		[namehash("worker.browse.paseo"); 2]
	);
}

/// The source each lookup with nothing cached reports, for a chain that cannot
/// be reached and one that never answers.
#[test]
fn an_uncached_lookup_reports_why_it_read_no_manifest() {
	let lookup = |platform: crate::test_support::StubPlatform, ceiling: Duration| {
		let platform = std::sync::Arc::new(platform);
		let services = RuntimeServices::new(
			platform.clone(),
			crate::platform::HostInfo {
				name: "Polkadot Mobile".to_string(),
				icon: None,
				version: None,
				platform: truapi::latest::HostPlatform::Unknown,
			},
			crate::test_support::dot_test_stack(),
			crate::Spawner::for_test(),
		);
		futures::executor::block_on(root_manifest_within(
			ceiling,
			&services,
			platform.as_ref(),
			&ProductId::for_test("peopl.dot"),
		))
	};
	assert_eq!(
		[
			lookup(
				crate::test_support::StubPlatform {
					chain_connect_error: Some("fixture serves no chain"),
					..Default::default()
				},
				MANIFEST_RESOLUTION_CEILING,
			),
			lookup(
				crate::test_support::StubPlatform {
					chain_connect_pending: true,
					..Default::default()
				},
				Duration::from_millis(10),
			),
		],
		[
			(
				None,
				ManifestSource::LookupFailed {
					reason: "Asset Hub connection: legacy_connection: remote_chain_connect: Host { \
					         reason: \"fixture serves no chain\" }"
						.to_string(),
				},
			),
			(None, ManifestSource::TimedOut),
		]
	);
}

/// Lookups for one product that start while none has finished share one
/// read of Asset Hub, not one each.
#[test]
fn concurrent_lookups_for_one_product_read_asset_hub_once() {
	let platform = std::sync::Arc::new(crate::test_support::StubPlatform::default());
	let services = RuntimeServices::new(
		platform.clone(),
		crate::platform::HostInfo {
			name: "Polkadot Mobile".to_string(),
			icon: None,
			version: None,
			platform: truapi::latest::HostPlatform::Unknown,
		},
		crate::test_support::dot_test_stack(),
		crate::Spawner::for_test(),
	);
	let product = ProductId::for_test("peopl.dot");

	futures::executor::block_on(futures::future::join_all(
		(0..4).map(|_| root_manifest(&services, platform.as_ref(), &product)),
	));

	assert_eq!(
		crate::test_support::recorded_rpc_method_count(&platform.sent_rpc, "chain_getBlockHash"),
		1
	);
}

/// A live read goes to dotNS past a fresh cached entry and replaces it, so a
/// funding provider republished within the cache's lifetime is seen at once;
/// a cached read still answers from the entry, and then from the new one.
#[test]
fn a_live_read_replaces_a_fresh_cached_entry() {
	use crate::platform::CoreStorage;
	let platform = crate::test_support::StubPlatform::default();
	let key = worker_manifest_cache_key(&ProductId::for_test("fundtest.dot"));
	let read = |policy: CachePolicy| {
		cached_dotns_json(&platform, key.clone(), policy, || async {
			Ok::<_, String>(Some("republished".to_string()))
		})
	};

	let (before, live, after) = futures::executor::block_on(async {
		platform
			.write_core_storage(
				key.clone(),
				encode_cached_root_manifest(Some("published"), current_unix_secs()),
			)
			.await
			.expect("seeded");
		let before = read(CachePolicy::UseCache).await.expect("cached");
		let live = read(CachePolicy::Refresh).await.expect("live");
		let after = read(CachePolicy::UseCache).await.expect("cached");
		(before, live, after)
	});

	assert_eq!(
		[
			(before.0, matches!(before.1, DotnsRead::Cached { .. })),
			(live.0, matches!(live.1, DotnsRead::Fetched)),
			(after.0, matches!(after.1, DotnsRead::Cached { .. })),
		],
		[
			(Some("published".to_string()), true),
			(Some("republished".to_string()), true),
			(Some("republished".to_string()), true),
		]
	);
}
