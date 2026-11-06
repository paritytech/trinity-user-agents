use std::time::Duration;

use clap::ValueEnum;

use super::{
	super::{
		Network,
		testing::{edit_stack, serve_rpc},
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
	let descriptor = edit_stack(|descriptor| {
		for (port, chain) in (1..).zip(descriptor["chains"].as_array_mut().unwrap()) {
			chain["rpc"] = serde_json::json!([format!("ws://127.0.0.1:{port}")]);
		}
	});
	Stack::new(descriptor, ChainBackend::Rpc).unwrap()
}

/// The stack on closed ports with People served by `url`.
fn stack_with_people_at(url: &str) -> Stack {
	stack_on_closed_ports().edited(|descriptor| {
		for chain in descriptor["chains"].as_array_mut().unwrap() {
			if chain["role"] == "People" {
				chain["rpc"] = serde_json::json!([url]);
			}
		}
	})
}

/// The stack on closed ports with a loopback People that reports its own
/// genesis and `network_suffix`.
async fn stack_with_people_suffix(network_suffix: Option<&'static str>) -> Stack {
	let genesis = stack_on_closed_ports().genesis().people;
	stack_with_people_at(&serve_chain(genesis, network_suffix).await)
}

async fn verify_stack_endpoints(config: &Stack, built_in: Option<&BuiltInOrigin>) -> Result<()> {
	let Roles { people, asset_hub, bulletin } = config.chains();
	verify_endpoints(config, &[people, asset_hub, bulletin], built_in).await
}

/// An endpoint that does not answer is not evidence of a wrong stack, so it
/// is left to fail on first use.
#[tokio::test]
async fn test_stack_endpoint_unreachable() {
	let config = stack_on_closed_ports();
	verify_stack_endpoints(&config, None)
		.await
		.expect("unreachable endpoint rejected");
}

/// An endpoint that accepts the connection and then stays silent delays a
/// command by at most the probe deadline.
#[tokio::test]
async fn test_silent_endpoint_deadline() {
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let url = format!("ws://{}", listener.local_addr().unwrap());
	tokio::spawn(async move {
		let mut sockets = Vec::new();
		while let Ok((socket, _)) = listener.accept().await {
			sockets.push(socket);
		}
	});
	let config = stack_with_people_at(&url);
	let start = std::time::Instant::now();
	verify_stack_endpoints(&config, None).await.expect("silent endpoint rejected");
	assert!(start.elapsed() < BUILT_IN_REFRESH_TIMEOUT + Duration::from_secs(2));
}

/// The case the endpoint check exists for: an endpoint that answers as a
/// different chain from the genesis the descriptor declares. Its suffix is
/// not read, so a restart reports the genesis rather than the suffix of
/// the wrong chain.
#[tokio::test]
async fn test_stack_endpoint_genesis_mismatch() {
	let url = serve_chain([0x5a; 32], Some("testnet")).await;
	let config = stack_with_people_at(&url);
	let error = verify_stack_endpoints(&config, None).await.expect_err("wrong genesis accepted");
	assert!(error.to_string().contains(&url));
	assert!(error.to_string().contains("reports genesis"));
	assert!(!error.to_string().contains("NetworkSuffix"));
}

/// A built-in stack whose chains restarted fails with an error that
/// reports the restart and points at the descriptor in use.
#[tokio::test]
async fn test_built_in_genesis_mismatch_hint() {
	let config = stack_with_people_at(&serve_chain([0x5a; 32], None).await);
	let origin = BuiltInOrigin {
		id: "paseo-next-v2".to_owned(),
		url: "https://stack.example/s.json".to_owned(),
		copy: BuiltInCopy::Published,
	};
	let error = verify_stack_endpoints(&config, Some(&origin))
		.await
		.expect_err("wrong genesis accepted");
	assert!(error.to_string().ends_with(&origin.restart_hint()));
}

/// Every identity derives under the TLD, so a People chain that scopes its
/// contexts with another suffix is an error that prints both.
#[tokio::test]
async fn test_network_suffix_mismatch_error() {
	let config = stack_with_people_suffix(Some("testnet")).await;
	let error = verify_stack_endpoints(&config, None).await.expect_err("other suffix accepted");
	let message = error.to_string();
	assert!(message.contains("NetworkSuffix testnet") && message.contains("tld is paseo"));

	let matching = stack_with_people_suffix(Some("paseo")).await;
	verify_stack_endpoints(&matching, None).await.expect("matching suffix rejected");
}

/// A People chain without the storage item, or one that does not answer,
/// is left to fail on first use.
#[tokio::test]
async fn test_network_suffix_absent_or_unreachable() {
	let absent = stack_with_people_suffix(None).await;
	verify_stack_endpoints(&absent, None).await.expect("absent suffix rejected");
	let closed_port_config = stack_on_closed_ports();
	verify_stack_endpoints(&closed_port_config, None)
		.await
		.expect("unreachable People rejected");
}

/// A network reached over RPC has every endpoint and the suffix checked; the
/// light client, which does not use the RPC endpoints, has none.
#[test]
fn test_chain_checks_by_backend() {
	let paseo = Network::PaseoNextV2.bundled_stack(ChainBackend::Rpc);
	assert_eq!(ChainChecks::plan(&paseo, None).endpoints.len(), 3);
	let light_client = Network::PaseoNextV2.bundled_stack(ChainBackend::LightClient);
	assert!(ChainChecks::plan(&light_client, None).endpoints.is_empty());
}

/// A built-in stack that restarted from genesis after this build has chains
/// the light client's bundled specs cannot sync.
#[test]
fn test_light_client_after_restart_error() {
	let restarted =
		Network::PaseoNextV2
			.bundled_stack(ChainBackend::LightClient)
			.edited(|descriptor| {
				for chain in descriptor["chains"].as_array_mut().unwrap() {
					if chain["role"] == "People" {
						chain["genesisHash"] = format!("0x{}", "5a".repeat(32)).into();
					}
				}
			});
	let error = refuse_light_client(&restarted).unwrap_err();
	assert!(error.to_string().contains("--chain-backend rpc"));
}

#[test]
fn test_light_client_unknown_network_error() {
	let pcf_devnet = Network::PcfDevnet.bundled_stack(ChainBackend::LightClient);
	let error = refuse_light_client(&pcf_devnet).unwrap_err();
	assert!(error.to_string().contains("no chain specs for pcf-devnet"));
}

/// The light client finds each chain in `truapi-provider`'s bundled catalog
/// by the bundled descriptor's genesis hash, so a network reset applied to
/// only one of the two copies leaves it unable to connect.
#[test]
fn test_light_client_catalog_match() {
	let bundled: Vec<_> = Network::value_variants()
		.iter()
		.map(|network| network.bundled_stack(ChainBackend::LightClient))
		.filter(|config| truapi_provider::known_networks().any(|known| known == config.namespace()))
		.collect();
	assert!(!bundled.is_empty());
	for config in bundled {
		refuse_light_client(&config).expect("light-client catalog differs");
	}
}
