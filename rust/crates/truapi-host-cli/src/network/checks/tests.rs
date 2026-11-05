use std::time::Duration;

use clap::ValueEnum;
use trinity_stack::Descriptor;

use super::{
	super::{
		Network,
		fixtures::edit_stack,
		overrides::{ChainOverrides, apply_chain_overrides},
		stack::stack_network_config,
	},
	*,
};

/// A loopback WebSocket node that answers `chain_getBlockHash` with
/// `genesis` and `state_getStorage` with `network_suffix`, SCALE-encoded,
/// or no value.
async fn serve_chain(genesis: [u8; 32], network_suffix: Option<&'static str>) -> String {
	use futures_util::{SinkExt, StreamExt};
	use parity_scale_codec::Encode;
	use tokio_tungstenite::tungstenite::Message;

	let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
		.await
		.expect("loopback bind failed");
	let url = format!("ws://{}", listener.local_addr().expect("no listener address"));
	let suffix = network_suffix
		.map(|suffix| format!("0x{}", hex::encode(suffix.as_bytes().to_vec().encode())));
	tokio::spawn(async move {
		loop {
			let (socket, _) = listener.accept().await.expect("accept failed");
			let suffix = suffix.clone();
			tokio::spawn(async move {
				let mut socket =
					tokio_tungstenite::accept_async(socket).await.expect("handshake failed");
				while let Some(Ok(Message::Text(request))) = socket.next().await {
					let request: serde_json::Value =
						serde_json::from_str(&request).expect("request is not JSON");
					let result = match request["method"].as_str() {
						Some("state_getStorage") => serde_json::json!(suffix),
						_ => serde_json::json!(format!("0x{}", hex::encode(genesis))),
					};
					let response = serde_json::json!({ "jsonrpc": "2.0", "id": request["id"], "result": result });
					if socket.send(Message::text(response.to_string())).await.is_err() {
						break;
					}
				}
			});
		}
	});
	url
}

/// Every role on a loopback port nothing listens on.
fn stack_on_closed_ports() -> Descriptor {
	edit_stack(|descriptor| {
		for (port, chain) in (1..).zip(descriptor["chains"].as_array_mut().unwrap()) {
			chain["rpc"] = serde_json::json!([format!("ws://127.0.0.1:{port}")]);
		}
	})
}

/// The stack on closed ports with People served by `url`.
fn stack_with_people_at(url: &str) -> NetworkConfig {
	let mut descriptor = stack_on_closed_ports();
	descriptor
		.chains
		.iter_mut()
		.find(|chain| chain.role == "People")
		.expect("People missing")
		.rpc = vec![url.to_owned()];
	stack_network_config(&descriptor).unwrap()
}

/// The stack on closed ports with a loopback People that reports its own
/// genesis and `network_suffix`.
async fn stack_with_people_suffix(network_suffix: Option<&'static str>) -> NetworkConfig {
	let genesis = stack_network_config(&stack_on_closed_ports()).unwrap().people_genesis;
	stack_with_people_at(&serve_chain(genesis, network_suffix).await)
}

async fn verify_stack_endpoints(
	config: &NetworkConfig,
	unanswered: Unanswered,
	built_in: Option<&BuiltInOrigin>,
) -> Result<()> {
	verify_endpoints(
		config,
		&config.live_chain_endpoints,
		unanswered,
		built_in,
		Some(config.network_suffix),
	)
	.await
}

/// An endpoint that does not answer is not evidence of a wrong stack, so it
/// is left to fail on first use.
#[tokio::test]
async fn test_stack_endpoint_unreachable() {
	let config = stack_network_config(&stack_on_closed_ports()).unwrap();
	verify_stack_endpoints(&config, Unanswered::Warn, None)
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
	verify_stack_endpoints(&config, Unanswered::Warn, None)
		.await
		.expect("silent endpoint rejected");
	assert!(start.elapsed() < BUILT_IN_REFRESH_TIMEOUT + Duration::from_secs(2));
}

/// An override is the user's own reroute, so an endpoint it points to that
/// cannot answer is an error before anything runs.
#[tokio::test]
async fn test_rerouted_endpoint_unreachable_error() {
	let config = stack_network_config(&stack_on_closed_ports()).unwrap();
	let error = verify_stack_endpoints(&config, Unanswered::Fail, None)
		.await
		.expect_err("unreachable rerouted endpoint accepted");
	assert!(error.to_string().contains("ws://127.0.0.1:"));
}

/// The case the endpoint check exists for: an endpoint that answers as a
/// different chain from the genesis the descriptor declares. Its suffix is
/// not read, so a restart reports the genesis rather than the suffix of
/// the wrong chain.
#[tokio::test]
async fn test_stack_endpoint_genesis_mismatch() {
	let url = serve_chain([0x5a; 32], Some("testnet")).await;
	let config = stack_with_people_at(&url);
	let error = verify_stack_endpoints(&config, Unanswered::Warn, None)
		.await
		.expect_err("wrong genesis accepted");
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
		id: "paseo-next-v2",
		url: "https://stack.example/s.json".to_owned(),
		copy: BuiltInCopy::Published,
	};
	let error = verify_stack_endpoints(&config, Unanswered::Warn, Some(&origin))
		.await
		.expect_err("wrong genesis accepted");
	assert!(error.to_string().ends_with(&format_restart_hint(&origin)));
}

/// The hint tells the user which copy is in use and why, so a restart on
/// the bundled copy points at the published descriptor that could not be
/// fetched or was rejected.
#[test]
fn test_restart_hint_by_copy() {
	let url = "https://stack.example/s.json".to_owned();
	let published = BuiltInOrigin { id: "previewnet", url, copy: BuiltInCopy::Published };
	assert_eq!(
		format_restart_hint(&published),
		"the previewnet stack restarted from genesis after the published descriptor in use \
		 (https://stack.example/s.json) was published"
	);
	let unreachable = BuiltInOrigin { copy: BuiltInCopy::Unreachable, ..published };
	assert_eq!(
		format_restart_hint(&unreachable),
		"the previewnet stack restarted from genesis after the bundled descriptor in use was \
		 built, and the published copy at https://stack.example/s.json could not be fetched"
	);
	let rejected = BuiltInOrigin { copy: BuiltInCopy::Rejected, ..unreachable };
	assert_eq!(
		format_restart_hint(&rejected),
		"the previewnet stack restarted from genesis after the bundled descriptor in use was \
		 built, and the published copy at https://stack.example/s.json was rejected, as the \
		 warning above reports"
	);
}

/// Every identity derives under the TLD, so a People chain that scopes its
/// contexts with another suffix is an error that prints both.
#[tokio::test]
async fn test_network_suffix_mismatch_error() {
	let config = stack_with_people_suffix(Some("testnet")).await;
	let error = verify_stack_endpoints(&config, Unanswered::Warn, None)
		.await
		.expect_err("other suffix accepted");
	let message = error.to_string();
	assert!(message.contains("NetworkSuffix testnet") && message.contains("tld is paseo"));

	let matching = stack_with_people_suffix(Some("paseo")).await;
	verify_stack_endpoints(&matching, Unanswered::Warn, None)
		.await
		.expect("matching suffix rejected");
}

/// A People chain without the storage item, or one that does not answer,
/// is left to fail on first use.
#[tokio::test]
async fn test_network_suffix_absent_or_unreachable() {
	let absent = stack_with_people_suffix(None).await;
	verify_stack_endpoints(&absent, Unanswered::Warn, None)
		.await
		.expect("absent suffix rejected");
	let closed_port_config = stack_network_config(&stack_on_closed_ports()).unwrap();
	verify_stack_endpoints(&closed_port_config, Unanswered::Warn, None)
		.await
		.expect("unreachable People rejected");
}

/// Built-in stacks have every endpoint and the suffix checked, as a
/// `--stack` does; Polkadot's pinned values, a local network and the light
/// client, which does not use the RPC endpoints, do not, beyond what an
/// override reroutes.
#[test]
fn test_chain_checks_by_source() {
	let paseo = Network::PaseoNextV2.config();
	let built_in = plan_chain_checks(&paseo, &paseo, false, None);
	assert_eq!((built_in.rerouted.len(), built_in.from_stack.len()), (0, 3));
	assert_eq!(built_in.expected_suffix, Some("paseo"));

	let polkadot = Network::Polkadot.config();
	let polkadot_checks = plan_chain_checks(&polkadot, &polkadot, false, None);
	assert_eq!((polkadot_checks.rerouted.len(), polkadot_checks.from_stack.len()), (0, 0));
	assert_eq!(polkadot_checks.expected_suffix, None);

	let light_client = NetworkConfig { chain_backend: ChainBackend::LightClient, ..paseo.clone() };
	let light_client = plan_chain_checks(&light_client, &paseo, false, None);
	assert_eq!((light_client.rerouted.len(), light_client.from_stack.len()), (0, 0));
	assert_eq!(light_client.expected_suffix, None);

	let overrides =
		ChainOverrides::parse([Some("ws://127.0.0.1:9".to_string()), None, None]).unwrap();
	let local =
		plan_chain_checks(&apply_chain_overrides(paseo.clone(), overrides), &paseo, true, None);
	assert_eq!((local.rerouted.len(), local.from_stack.len()), (1, 0));
	assert_eq!(local.expected_suffix, None);
}

/// Only the endpoints an override reroutes are checked as such, even one
/// set to another role's URL, which only its genesis tells apart.
#[test]
fn test_split_rerouted() {
	let base = Network::PaseoNextV2.config();
	let overrides = ChainOverrides::parse([Some(base.bulletin_ws.clone()), None, None]).unwrap();
	let config = apply_chain_overrides(base.clone(), overrides);
	let (rerouted, untouched) = split_rerouted(&config, &base);
	assert_eq!(rerouted.len(), 1);
	assert_eq!((rerouted[0].genesis, &rerouted[0].ws), (base.people_genesis, &base.bulletin_ws));
	assert_eq!(untouched.len(), 2);
	assert!(untouched.iter().all(|endpoint| endpoint.genesis != base.people_genesis));
}

/// A built-in stack that restarted from genesis after this build has chains
/// the light client's bundled specs cannot sync.
#[test]
fn test_light_client_after_restart_error() {
	let restarted = NetworkConfig {
		chain_backend: ChainBackend::LightClient,
		people_genesis: [0x5a; 32],
		..Network::PaseoNextV2.config()
	};
	let error = refuse_light_client(&restarted).unwrap_err();
	assert!(error.to_string().contains("--chain-backend rpc"));
}

#[test]
fn test_light_client_unknown_network_error() {
	let polkadot =
		NetworkConfig { chain_backend: ChainBackend::LightClient, ..Network::Polkadot.config() };
	let error = refuse_light_client(&polkadot).unwrap_err();
	assert!(error.to_string().contains("no chain specs for polkadot"));
}

/// The light client finds each chain in `truapi-provider`'s bundled catalog
/// by the bundled descriptor's genesis hash, so a network reset applied to
/// only one of the two copies leaves it unable to connect.
#[test]
fn test_light_client_catalog_match() {
	let bundled: Vec<_> = Network::value_variants()
		.iter()
		.map(|network| network.config())
		.filter(|config| truapi_provider::known_networks().any(|known| known == config.id))
		.collect();
	assert!(!bundled.is_empty());
	for config in bundled {
		let light = NetworkConfig { chain_backend: ChainBackend::LightClient, ..config };
		refuse_light_client(&light).expect("light-client catalog differs");
	}
}
