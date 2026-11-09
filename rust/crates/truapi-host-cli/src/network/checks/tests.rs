use std::time::Duration;

use clap::ValueEnum;

use super::{
	super::{
		CHAIN_CHECK_TIMEOUT, Network,
		built_in::BuiltInCopy,
		testing::{serve_rpc, serve_silent},
	},
	*,
};

/// A loopback WebSocket node that answers `chain_getBlockHash` with
/// `genesis` and `state_getStorage` with `network_suffix`, SCALE-encoded,
/// or no value.
async fn serve_chain(genesis: [u8; 32], network_suffix: Option<&'static str>) -> String {
	use parity_scale_codec::Encode;

	let suffix = network_suffix
		.map(|suffix| format!("0x{}", hex::encode(suffix.as_bytes().to_vec().encode())));
	serve_rpc(move |method, _| match method {
		"state_getStorage" => serde_json::json!(suffix),
		_ => serde_json::json!(format!("0x{}", hex::encode(genesis))),
	})
	.await
}

/// Every role on a loopback port nothing listens on.
fn stack_on_closed_ports() -> Stack {
	Network::PaseoNextV2.bundled_stack(ChainBackend::Rpc).with_edit(|descriptor| {
		for (port, chain) in (1..).zip(descriptor["chains"].as_array_mut().unwrap()) {
			chain["rpc"] = serde_json::json!([format!("ws://127.0.0.1:{port}")]);
		}
	})
}

/// The stack on closed ports with People served by `url`.
fn stack_with_people_at(url: &str) -> Stack {
	stack_on_closed_ports()
		.with_chain_edit("People", |chain| chain["rpc"] = serde_json::json!([url]))
}

/// The stack on closed ports with a loopback People that reports its own
/// genesis and `network_suffix`.
async fn stack_with_people_suffix(network_suffix: Option<&'static str>) -> Stack {
	let genesis = stack_on_closed_ports().genesis().people;
	stack_with_people_at(&serve_chain(genesis, network_suffix).await)
}

/// An endpoint that does not answer is not evidence of a wrong stack, so it
/// is left to fail on first use.
#[tokio::test]
async fn test_stack_endpoint_unreachable() {
	let stack = stack_on_closed_ports();
	verify_endpoints(&stack, None).await.expect("unreachable endpoint rejected");
}

/// An endpoint that accepts the connection and then stays silent delays a
/// command by at most the probe deadline.
#[tokio::test]
async fn test_silent_endpoint_deadline() {
	let stack = stack_with_people_at(&serve_silent().await);
	let start = std::time::Instant::now();
	verify_endpoints(&stack, None).await.expect("silent endpoint rejected");
	assert!(start.elapsed() < CHAIN_CHECK_TIMEOUT + Duration::from_secs(2));
}

/// The case the endpoint check exists for: an endpoint that answers as a
/// different chain from the genesis the descriptor declares. Its suffix is
/// not read, so a restart reports the genesis rather than the suffix of
/// the wrong chain.
#[tokio::test]
async fn test_stack_endpoint_genesis_mismatch() {
	let url = serve_chain([0x5a; 32], Some("testnet")).await;
	let stack = stack_with_people_at(&url);
	let error = verify_endpoints(&stack, None).await.expect_err("wrong genesis accepted");
	assert!(error.to_string().contains(&url));
	assert!(error.to_string().contains("reports genesis"));
	assert!(!error.to_string().contains("NetworkSuffix"));
}

/// A built-in stack whose chains restarted fails with an error that
/// reports the restart and points at the descriptor in use.
#[tokio::test]
async fn test_built_in_genesis_mismatch_hint() {
	let stack = stack_with_people_at(&serve_chain([0x5a; 32], None).await);
	let origin = BuiltInOrigin {
		id: "paseo-next-v2".to_owned(),
		url: "https://stack.example/s.json".to_owned(),
		copy: BuiltInCopy::Published,
	};
	let error = verify_endpoints(&stack, Some(&origin))
		.await
		.expect_err("wrong genesis accepted");
	assert!(error.to_string().ends_with(&origin.restart_hint()));
}

/// Every identity derives under the TLD, so a People chain that scopes its
/// contexts with another suffix is an error that prints both.
#[tokio::test]
async fn test_network_suffix_mismatch_error() {
	let stack = stack_with_people_suffix(Some("testnet")).await;
	let error = verify_endpoints(&stack, None).await.expect_err("other suffix accepted");
	let message = error.to_string();
	assert!(message.contains("NetworkSuffix testnet") && message.contains("tld is paseo"));

	let matching = stack_with_people_suffix(Some("paseo")).await;
	verify_endpoints(&matching, None).await.expect("matching suffix rejected");
}

/// A People chain without the storage item, or one that does not answer,
/// is left to fail on first use.
#[tokio::test]
async fn test_network_suffix_absent_or_unreachable() {
	let absent = stack_with_people_suffix(None).await;
	verify_endpoints(&absent, None).await.expect("absent suffix rejected");
	let closed_port_stack = stack_on_closed_ports();
	verify_endpoints(&closed_port_stack, None)
		.await
		.expect("unreachable People rejected");
}

/// The checks run the Polkadot fork comparison on an endpoint with
/// Polkadot's genesis, and its failure stops the command rather than being
/// left to a refetch of the descriptor. The light client, which does not use
/// the RPC endpoints, has none of them checked.
#[tokio::test]
async fn test_polkadot_fork_check_wiring() {
	let polkadot_people = stack_on_closed_ports().with_chain_edit("People", |chain| {
		chain["genesisHash"] =
			"0x67fa177a097bfa18f77ea95ab56e9bcdfeb0e5b8a40e46298bb93e16b6fc5008".into();
	});
	let error = verify_chains(&polkadot_people, None).await.expect_err("fork unproven");
	assert!(error.to_string().contains("cannot tell ws://127.0.0.1:"));

	let unreachable = "http://127.0.0.1:9/spec.json";
	let light_client = Network::PaseoNextV2
		.bundled_stack(ChainBackend::Rpc)
		.with_light_specs_at(unreachable, unreachable);
	verify_chains(&light_client, None)
		.await
		.expect("light-client checks failed")
		.expect("an unreachable light spec is not a wrong one");
}

/// Every built-in network runs on the light client, whether or not
/// `truapi-provider` bundles its specs: the descriptor names them.
#[test]
fn test_built_in_networks_name_their_light_specs() {
	for network in Network::value_variants() {
		require_light_specs(&network.bundled_stack(ChainBackend::LightClient))
			.expect("a built-in descriptor names a light spec for every chain");
	}
}

/// A stack with no light spec for a chain the light client runs fails before
/// the command starts, naming the chain, rather than on every connect.
#[test]
fn test_light_client_without_a_light_spec_error() {
	let stack = Network::PaseoNextV2.bundled_stack(ChainBackend::LightClient).with_chain_edit(
		"Bulletin",
		|chain| {
			chain.as_object_mut().expect("a chain is an object").remove("lightSpec");
		},
	);

	let error = require_light_specs(&stack).unwrap_err().to_string();

	assert!(error.contains("Bulletin") && error.contains("--chain-backend rpc"), "{error}");
	let over_rpc = Stack { chain_backend: ChainBackend::Rpc, ..stack };
	require_light_specs(&over_rpc).expect("RPC needs no light spec");
}

/// A spec that is not the one the descriptor names is a wrong answer, which
/// makes the CLI fetch the descriptor again, while one that cannot be fetched
/// only leaves its chain unavailable.
#[tokio::test]
async fn test_light_spec_wrong_or_unreachable() {
	let (server, _) = crate::network::serve_light_specs().await;
	let (relay, people) = (format!("{server}/relay.json"), format!("{server}/people.json"));
	let bundled = Network::PaseoNextV2.bundled_stack(ChainBackend::Rpc);
	let directory = tempfile::tempdir().expect("temporary directory");
	let light = |stack: &Stack| {
		StackLightClient::new(&stack.stack_id(), stack.descriptor(), directory.path().into())
			.expect("distinct genesis hashes")
	};

	let swapped = bundled.with_light_specs_at(&people, &relay);
	let error = verify_light_specs(&light(&swapped), &swapped).await.unwrap_err().to_string();
	assert!(error.contains("light spec of"), "{error}");

	let missing = bundled.with_light_specs_at(&relay, &format!("{server}/moved.json"));
	verify_light_specs(&light(&missing), &missing)
		.await
		.expect("a spec the server does not have is unreachable, not wrong");
}
