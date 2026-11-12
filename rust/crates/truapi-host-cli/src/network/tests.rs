use truapi::latest::ChainIdentifier;

use super::*;

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
		for (identifier, genesis_hash) in stack.served_roles() {
			let ws = &stack.endpoint_for_role(identifier).expect("served role has no endpoint").ws;
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
					unreachable.push(format!("{identifier:?} at {ws}: {err}"));
					continue;
				},
			};

			checked += 1;
			// Paseo's People chain calls itself "People" and previewnet's calls
			// itself "Individuality Local", so either name satisfies the role.
			let expected_in_name: &[&str] = match identifier {
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
					identifier
				));
			} else if genesis_hash == reported {
				println!(
					"{} {:?} {} matches {ws} ({chain_name})",
					stack.namespace(),
					identifier,
					hex::encode(genesis_hash)
				);
			} else {
				drifted.push(format!(
					"{} advertises {:?} as {} but {ws} reports {}",
					stack.namespace(),
					identifier,
					hex::encode(genesis_hash),
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
		.map(|network| network.bundled_stack(ChainBackend::Rpc).served_roles().len())
		.sum();
	assert!(expected_checks > 0);
	assert_eq!(checked, expected_checks);
}

/// A Polkadot fork carries Polkadot's genesis hashes, which the core refuses
/// unless its host vouches for them. The CLI runs a fork once the chain checks
/// show it is one, so the core must start on it too.
#[test]
fn a_polkadot_fork_starts_the_core() {
	let polkadot_people = "67fa177a097bfa18f77ea95ab56e9bcdfeb0e5b8a40e46298bb93e16b6fc5008";
	let fork = Network::PaseoNextV2
		.bundled_stack(ChainBackend::Rpc)
		.with_chain_edit("People", |chain| {
			chain["genesisHash"] = format!("0x{polkadot_people}").into()
		});

	assert_eq!(
		fork.core_stack().map(|stack| hex::encode(stack.chains().people)),
		Ok(polkadot_people.to_string())
	);
}
