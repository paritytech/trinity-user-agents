use super::*;

/// Anchors the served set itself. Every other test that reads it iterates, so
/// all of them pass on an empty set; this is what notices a dropped role, and it
/// covers each network rather than only the default.
#[test]
fn test_network_served_roles() {
	for network in Network::value_variants() {
		let stack = network.bundled_stack(ChainBackend::Rpc);
		let served: Vec<ChainIdentifier> =
			stack.host_chain_set().chains.iter().map(|entry| entry.identifier).collect();

		assert_eq!(
			served,
			vec![ChainIdentifier::People, ChainIdentifier::Bulletin, ChainIdentifier::AssetHub],
			"{} serves an unexpected role set",
			stack.namespace()
		);
	}
}

/// Hosts a built-in may route to, matched exactly: the paseo-next-v2 chains
/// and identity backend, the previewnet stack, and the pcf-devnet chains and
/// identity backend. A refreshed bundled copy that routes elsewhere fails here
/// on purpose.
const TEST_NETWORK_HOSTS: &[&str] = &[
	"paseo-asset-hub-next-rpc.polkadot.io",
	"paseo-people-next-system-rpc.polkadot.io",
	"paseo-bulletin-next-rpc.polkadot.io",
	"previewnet.substrate.dev",
	"identity.dotspark.app",
	"asset-hub-paseo-rpc.n.dwellir.com",
	"people-devnet-rpc.api.polkadotcommunity.foundation",
	"bullet.sik.rocks",
	"polkadot-app.api.polkadotcommunity.foundation",
];

/// Guards the invariant documented on [`Network`]: the plaintext account store
/// saves every network's identities, so a built-in must route only to test
/// networks.
#[test]
fn test_built_in_network_hosts() {
	for network in Network::value_variants() {
		let stack = network.bundled_stack(ChainBackend::Rpc);
		let mut routes = vec![stack.identity_backend()];
		routes.extend(stack.chains().iter().map(|chain| chain.ws.as_str()));

		for route in routes {
			let host = reqwest::Url::parse(route)
				.ok()
				.and_then(|url| url.host_str().map(str::to_owned))
				.unwrap_or_default();
			assert!(TEST_NETWORK_HOSTS.contains(&host.as_str()));
		}
	}
}

/// The chains a host advertises must be the chains it reaches.
///
/// A genesis alone does not prove the role is right: pointing People at the
/// Bulletin endpoint and its genesis satisfies every value-versus-value
/// assertion, so the chain's own name ties the role to the chain behind it.
///
/// Unreachable endpoints and mismatches are both collected before failing,
/// because a wipe drifts more than one role at a time and takes endpoints
/// down with it. The checked count is asserted at the end: `--ignored` runs
/// this test without [`test_network_served_roles`].
///
/// Ignored by default; needs network access to each network's chains.
///
/// ```sh
/// cargo +nightly test -p truapi-host-cli --bin truapi-host \
///     advertised_genesis -- --ignored --nocapture
/// ```
#[tokio::test]
#[ignore = "needs network access to each network's chains"]
async fn the_advertised_genesis_matches_what_each_chain_reports() {
	use truapi::statement_allowance as alloc;

	let mut checked = 0usize;
	let mut drifted = Vec::new();
	let mut unreachable = Vec::new();

	for network in Network::value_variants() {
		let stack = network.bundled_stack(ChainBackend::Rpc);
		for entry in stack.host_chain_set().chains {
			let ws = &stack
				.endpoint_for_role(entry.identifier)
				.expect("served role has no endpoint")
				.ws;
			let probe = async {
				let rpc = alloc::rpc::RpcClient::connect(ws)
					.await
					.map_err(|err| format!("connect: {err}"))?;
				let reported = alloc::fetch_genesis_hash(&rpc)
					.await
					.map_err(|err| format!("genesis hash: {err}"))?;
				let name = rpc
					.call("system_chain", serde_json::json!([]))
					.await
					.map_err(|err| format!("system_chain: {err}"))?;
				Ok::<_, String>((reported, name.as_str().unwrap_or_default().to_string()))
			};
			let (reported, chain_name) = match probe.await {
				Ok(values) => values,
				Err(err) => {
					unreachable.push(format!("{:?} at {ws}: {err}", entry.identifier));
					continue;
				},
			};

			checked += 1;
			// Paseo's People chain calls itself "People" and previewnet's calls
			// itself "Individuality Local", so either name satisfies the role.
			let expected_in_name: &[&str] = match entry.identifier {
				ChainIdentifier::People => &["People", "Individuality"],
				ChainIdentifier::Bulletin => &["Bulletin"],
				ChainIdentifier::AssetHub => &["Asset Hub"],
				ChainIdentifier::Relay => &["Relay"],
			};
			if !expected_in_name.iter().any(|expected| chain_name.contains(expected)) {
				drifted.push(format!(
					"{} serves {:?} from {ws}, which calls itself {chain_name:?} and \
					 contains none of {expected_in_name:?}",
					stack.namespace(),
					entry.identifier
				));
			} else if entry.genesis_hash == reported {
				println!(
					"{} {:?} {} matches {ws} ({chain_name})",
					stack.namespace(),
					entry.identifier,
					hex::encode(entry.genesis_hash)
				);
			} else {
				drifted.push(format!(
					"{} advertises {:?} as {} but {ws} reports {}",
					stack.namespace(),
					entry.identifier,
					hex::encode(entry.genesis_hash),
					hex::encode(reported)
				));
			}
		}
	}

	let mut failures = Vec::new();
	if !drifted.is_empty() {
		failures.push(format!(
			"drifted, so refresh the bundled stacks (scripts/update-stacks.sh):\n{}",
			drifted.join("\n")
		));
	}
	if !unreachable.is_empty() {
		failures.push(format!(
			"unreachable, so drift is unproven for these roles only:\n{}",
			unreachable.join("\n")
		));
	}
	assert!(failures.is_empty(), "{}", failures.join("\n\n"));
	let expected_checks: usize = Network::value_variants()
		.iter()
		.map(|network| network.bundled_stack(ChainBackend::Rpc).host_chain_set().chains.len())
		.sum();
	assert!(expected_checks > 0);
	assert_eq!(checked, expected_checks);
}

#[test]
fn hop_pins_are_not_inferred_from_bulletin_rpc_or_stack_namespace() {
	let live = Network::PaseoNextV2.bundled_stack(ChainBackend::Rpc);
	assert_eq!(live.hop_endpoints().len(), 2);
	assert!(
		live.hop_endpoints()
			.iter()
			.all(|endpoint| endpoint.bulletin_genesis == live.genesis().bulletin)
	);
	assert_eq!(
		hex::encode(live.hop_endpoints()[0].bulletin_genesis),
		"8cfe6717dc4becfda2e13c488a1e2061ff2dfee96e7d031157f72d36716c0a22",
	);
	let fork = live.with_edit(|descriptor| descriptor["id"] = "paseo-next-v2-fork".into());
	assert_eq!(fork.namespace(), live.namespace());
	assert!(fork.hop_endpoints().is_empty());

	let rerouted = live.with_chain_edit("Bulletin", |chain| {
		chain["rpc"] = serde_json::json!(["ws://127.0.0.1:10012"]);
	});
	assert_eq!(rerouted.genesis().bulletin, live.genesis().bulletin);
	assert!(rerouted.hop_endpoints().is_empty());
	let reset = live.with_chain_edit("Bulletin", |chain| {
		chain["genesisHash"] = format!("0x{}", "11".repeat(32)).into();
	});
	assert!(reset.hop_endpoints().is_empty());
	assert!(Network::PcfDevnet.bundled_stack(ChainBackend::Rpc).hop_endpoints().is_empty());
}

#[test]
fn preview_hop_requires_its_exact_pinned_bulletin_identity() {
	let stack = Network::Previewnet.bundled_stack(ChainBackend::Rpc);
	assert!(stack.hop_endpoints().is_empty());
	let matching = stack.with_chain_edit("Bulletin", |chain| {
		chain["genesisHash"] =
			"0x1144acd27f0e5b2c88da7dc12c111e396983dec036ccfb42da5bbb0dd7104e89".into();
	});
	assert_eq!(matching.hop_endpoints().len(), 1);
	assert_eq!(matching.hop_endpoints()[0].bulletin_genesis, matching.genesis().bulletin);
	assert_eq!(matching.hop_endpoints()[0].ws, "wss://previewnet.substrate.dev/bulletin");
}
