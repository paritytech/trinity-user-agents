use super::*;

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

/// The source each lookup with nothing cached reports, for a host with no
/// Asset Hub, a chain that cannot be reached, and one that never answers.
#[test]
fn an_uncached_lookup_reports_why_it_read_no_manifest() {
	let lookup = |platform: crate::test_support::StubPlatform,
	              asset_hub_chain_genesis_hash: [u8; 32],
	              ceiling: Duration| {
		let platform = std::sync::Arc::new(platform);
		let services = RuntimeServices::new(
			platform.clone(),
			crate::platform::HostInfo {
				name: "Polkadot Mobile".to_string(),
				icon: None,
				version: None,
				platform: truapi::latest::HostPlatform::Unknown,
			},
			[0; 32],
			[0xbb; 32],
			asset_hub_chain_genesis_hash,
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
			lookup(Default::default(), [0; 32], MANIFEST_RESOLUTION_CEILING),
			lookup(
				crate::test_support::StubPlatform {
					chain_connect_error: Some("fixture serves no chain"),
					..Default::default()
				},
				[0xcc; 32],
				MANIFEST_RESOLUTION_CEILING,
			),
			lookup(
				crate::test_support::StubPlatform {
					chain_connect_pending: true,
					..Default::default()
				},
				[0xcc; 32],
				Duration::from_millis(10),
			),
		],
		[
			(None, ManifestSource::NoAssetHub),
			(
				None,
				ManifestSource::LookupFailed {
					reason: "Asset Hub follow stopped before initialization".to_string(),
				},
			),
			(None, ManifestSource::TimedOut),
		]
	);
}

/// Lookups for one product that start while none has finished share one
/// lookup, so a burst of them holds one Asset Hub follow, not one each.
#[test]
fn concurrent_lookups_for_one_product_open_one_follow() {
	let platform = std::sync::Arc::new(crate::test_support::StubPlatform::default());
	let services = RuntimeServices::new(
		platform.clone(),
		crate::platform::HostInfo {
			name: "Polkadot Mobile".to_string(),
			icon: None,
			version: None,
			platform: truapi::latest::HostPlatform::Unknown,
		},
		[0; 32],
		[0xbb; 32],
		[0xcc; 32],
		crate::Spawner::for_test(),
	);
	let product = ProductId::for_test("peopl.dot");

	futures::executor::block_on(futures::future::join_all(
		(0..4).map(|_| root_manifest(&services, platform.as_ref(), &product)),
	));

	assert_eq!(
		crate::test_support::recorded_rpc_method_count(&platform.sent_rpc, "chainHead_v1_follow"),
		1
	);
}
