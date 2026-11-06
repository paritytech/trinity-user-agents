use trinity_stack::Document;

use super::*;

/// Every served role must match an endpoint that actually routes it, at the
/// URL the network declares for that role.
///
/// `CliChainProvider` falls back to People for an unrecognised genesis, so a
/// drifted `people_genesis` still resolves to a working URL and satisfies
/// every routing assertion. Pinning against `live_chain_endpoints` checks
/// the values themselves rather than the plumbing that reads them.
#[test]
fn served_chain_genesis_hashes_match_the_endpoint_routes() {
	for network in Network::value_variants() {
		let config = network.config();
		for entry in config.host_chain_set().chains {
			let expected_ws = config.url_for_role(entry.identifier).unwrap_or_else(|| {
				panic!("{} serves {:?} with no URL", config.id, entry.identifier)
			});
			let endpoint = config
				.live_chain_endpoints
				.iter()
				.find(|endpoint| endpoint.genesis == entry.genesis_hash)
				.unwrap_or_else(|| {
					panic!(
						"{} serves {:?} at genesis {} which no endpoint routes",
						config.id,
						entry.identifier,
						hex::encode(entry.genesis_hash)
					)
				});
			assert_eq!(endpoint.ws, expected_ws);
		}
	}
}

/// Anchors the served set itself. Every other test that reads it iterates, so
/// all of them pass on an empty set; this is what notices a dropped role, and it
/// covers each network rather than only the default.
#[test]
fn test_network_served_roles() {
	for network in Network::value_variants() {
		let config = network.config();
		let served: Vec<ChainIdentifier> =
			config.host_chain_set().chains.iter().map(|entry| entry.identifier).collect();

		assert_eq!(
			served,
			vec![ChainIdentifier::People, ChainIdentifier::Bulletin, ChainIdentifier::AssetHub],
			"{} serves an unexpected role set",
			config.id
		);
	}
}

/// A bundled copy that no longer opens with its pointer would leave the
/// built-in network with nothing to start from when the published copy is
/// unreachable.
#[test]
fn test_bundled_stack_pointers() {
	for network in Network::value_variants() {
		let Some(built_in) = network.built_in_stack() else { continue };
		let bundled = trinity_stack::open(built_in.bundle, &built_in.pointer())
			.unwrap_or_else(|error| panic!("{}: {error}", network.id()));
		assert!(bundled.signed);
		assert_eq!(bundled.descriptor.id, network.id());
	}
}

/// SPEC.md pins Polkadot's genesis hashes, the values a build reserves for
/// real identities. Compiled in with `include_str!` so a moved table breaks
/// the build rather than drifting quietly.
#[test]
fn test_spec_genesis_table_polkadot() {
	const SPEC: &str = include_str!("../../SPEC.md");

	let config = Network::Polkadot.config();
	for (row, expected) in [
		("People genesis", config.people_genesis),
		("Bulletin genesis", config.bulletin_genesis),
		("Asset Hub genesis", config.asset_hub_genesis),
	] {
		let expected_row = format!("| {row} | `0x{}` |", hex::encode(expected));
		assert!(SPEC.contains(&expected_row));
	}
}

/// Hosts a disposable-identity built-in may route to, matched exactly: the
/// paseo-next-v2 chains and identity backend, and the previewnet stack.
/// A refreshed bundled copy that routes elsewhere fails here on purpose.
const TEST_NETWORK_HOSTS: &[&str] = &[
	"paseo-asset-hub-next-rpc.polkadot.io",
	"paseo-people-next-system-rpc.polkadot.io",
	"paseo-bulletin-next-rpc.polkadot.io",
	"previewnet.substrate.dev",
	"identity.dotspark.app",
];

#[test]
fn test_polkadot_served_tld() {
	assert!(DOTNS_TLDS.contains(&Network::Polkadot.config().network_suffix));
}

/// Guards the invariant documented on [`Network`]: a network that lets the
/// plaintext account store save its identities must route only to test
/// networks.
#[test]
fn test_disposable_network_hosts() {
	for network in Network::value_variants() {
		let config = network.config();
		if !config.disposable_identities {
			continue;
		}
		let mut routes: Vec<&str> = config.identity_backend_base.into_iter().collect();
		routes.extend([config.people_ws, config.bulletin_ws, config.asset_hub_ws]);
		routes.extend(config.live_chain_endpoints.iter().map(|chain| chain.ws));

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
		let config = network.config();
		for entry in config.host_chain_set().chains {
			let ws = config.url_for_role(entry.identifier).expect("every served role has a URL");
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
					config.id, entry.identifier
				));
			} else if entry.genesis_hash == reported {
				println!(
					"{} {:?} {} matches {ws} ({chain_name})",
					config.id,
					entry.identifier,
					hex::encode(entry.genesis_hash)
				);
			} else {
				drifted.push(format!(
					"{} advertises {:?} as {} but {ws} reports {}",
					config.id,
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
			"drifted, so refresh the bundled stacks (scripts/update-stacks.sh) or Polkadot's \
			 pinned values:\n{}",
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
		.map(|network| network.config().host_chain_set().chains.len())
		.sum();
	assert!(expected_checks > 0);
	assert_eq!(checked, expected_checks);
}

/// The compile-time reader must agree with a JSON parser on every entry,
/// so Polkadot's controller is exactly what the manifest records.
#[test]
fn manifest_addresses_decode_as_a_json_parser_reads_them() {
	let manifest: serde_json::Map<String, serde_json::Value> =
		serde_json::from_str(POLKADOT_DOTNS_DEPLOYMENT).expect("invalid manifest JSON");
	for (name, address) in &manifest {
		let address = address.as_str().expect("manifest entry is not a string");
		assert_eq!(
			hex::encode(manifest_address(POLKADOT_DOTNS_DEPLOYMENT, name)),
			address.trim_start_matches("0x").to_lowercase(),
			"{name}"
		);
	}
	assert_eq!(
		Network::Polkadot.config().dotns_pop_controller,
		Some(manifest_address(POLKADOT_DOTNS_DEPLOYMENT, "DotnsPopController"))
	);
}

const TEST_SECRET: [u8; 32] = [7; 32];

/// The bundled paseo-next-v2 descriptor, edited, as plain JSON bytes.
fn edit_descriptor(edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
	let Ok(Document::Envelope { payload, .. }) = Document::parse(PASEO_NEXT_V2_STACK.bundle) else {
		panic!("bundled paseo-next-v2 stack is not an envelope");
	};
	let mut descriptor: serde_json::Value =
		serde_json::from_slice(&payload).expect("invalid bundled payload");
	edit(&mut descriptor);
	serde_json::to_vec(&descriptor).expect("JSON value does not serialise")
}

/// The bundled paseo-next-v2 descriptor, edited.
fn edit_stack(edit: impl FnOnce(&mut serde_json::Value)) -> Descriptor {
	let plain = edit_descriptor(edit);
	Document::parse(&plain).expect("invalid test descriptor").descriptor().clone()
}

/// The bundled paseo-next-v2 descriptor with `role`'s chain edited.
fn edit_chain(role: &str, edit: impl FnOnce(&mut serde_json::Value)) -> Descriptor {
	edit_stack(|descriptor| {
		let chains = descriptor["chains"].as_array_mut().expect("chains is not an array");
		edit(chains.iter_mut().find(|chain| chain["role"] == role).expect("role missing"));
	})
}

fn test_pointer() -> Pointer {
	let key = format!("0x{}", hex::encode(trinity_stack::public_key(&TEST_SECRET)));
	Pointer::new("https://stack.example/trinity-stack.json", Some(&key)).unwrap()
}

fn sign_copy(sequence: u32, generation: &str) -> Vec<u8> {
	sign_edited(|descriptor| {
		descriptor["sequence"] = sequence.into();
		descriptor["generation"] = generation.into();
	})
}

fn sign_edited(edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
	trinity_stack::sign(&edit_descriptor(edit), &TEST_SECRET).unwrap()
}

fn refresh(bundled: &[u8], fetched: Fetched) -> Result<StackRefresh> {
	refresh_stack("test", &test_pointer(), bundled, fetched, NOW)
}

const NOW: i64 = 1_800_000_000;

/// A descriptor carries everything a network needs: the routes and genesis of
/// each role, the identity backend, the TLD the CLI derives identities under,
/// and the dotNS controller.
#[test]
fn test_stack_config_from_descriptor() {
	let people_genesis = format!("0x{}", "11".repeat(32));
	let descriptor = edit_stack(|descriptor| {
		descriptor["id"] = "staging-fork".into();
		descriptor["tld"] = "testnet".into();
		descriptor["services"] = serde_json::json!([
			{ "kind": "dub", "urls": ["https://dub.example/"] },
		]);
		descriptor["dotns"] =
			serde_json::json!({ "popController": format!("0x{}", "ab".repeat(20)) });
		for chain in descriptor["chains"].as_array_mut().unwrap() {
			if chain["role"] == "People" {
				chain["rpc"] = serde_json::json!(["ws://127.0.0.1:9944", "wss://b.example"]);
				chain["genesisHash"] = people_genesis.clone().into();
			}
		}
	});

	let config = stack_network_config(&descriptor).unwrap();

	assert_eq!(config.id, "staging");
	assert_eq!(config.network_suffix, "testnet");
	assert_eq!(config.identity_backend_base, Some("https://dub.example/api/v1"));
	assert_eq!(config.dotns_pop_controller, Some([0xab; 20]));
	assert!(config.disposable_identities);
	assert_eq!(config.people_ws, "ws://127.0.0.1:9944");
	assert_eq!(config.people_genesis, [0x11; 32]);
	let paseo = Network::PaseoNextV2.config();
	assert_eq!(
		(config.asset_hub_ws, config.asset_hub_genesis),
		(paseo.asset_hub_ws, paseo.asset_hub_genesis)
	);
	assert_eq!(
		(config.bulletin_ws, config.bulletin_genesis),
		(paseo.bulletin_ws, paseo.bulletin_genesis)
	);
}

/// Reserved identities derive under the TLD, and navigation accepts only the
/// TLDs the core serves.
#[test]
fn test_stack_unserved_tld_error() {
	let staging = edit_stack(|descriptor| descriptor["tld"] = "staging".into());
	let error = stack_network_config(&staging).unwrap_err();
	assert!(error.to_string().contains("staging"));
}

/// The host routes each role by its genesis, so two roles on one genesis
/// would send one role's traffic to the other's chain.
#[test]
fn test_stack_shared_genesis_error() {
	let paseo = Network::PaseoNextV2.config();
	let shared = format!("0x{}", hex::encode(paseo.people_genesis));
	let descriptor = edit_chain("Bulletin", |chain| chain["genesisHash"] = shared.into());
	assert!(stack_network_config(&descriptor).is_err());
}

/// A stack id becomes a state directory name, so one that is not a plain
/// label is not used as the namespace.
#[test]
fn test_stack_id_not_label() {
	let descriptor = edit_stack(|descriptor| descriptor["id"] = "../elsewhere".into());
	assert!(stack_network_config(&descriptor).is_err());
}

#[test]
fn test_stack_missing_role_error() {
	let descriptor = edit_stack(|descriptor| {
		descriptor["chains"]
			.as_array_mut()
			.unwrap()
			.retain(|chain| chain["role"] != "Bulletin");
	});
	let error = stack_network_config(&descriptor).unwrap_err();
	assert!(error.to_string().contains("no Bulletin chain"));
}

/// A fork has its source's genesis, so a stack routed at a copy of mainnet
/// would sign mainnet-valid transactions with auto-approval; and a `dot`
/// name is a mainnet name.
#[test]
fn test_stack_polkadot_values_error() {
	assert!(ensure_no_real_identity_values(&Network::PaseoNextV2.config()).is_ok());
	assert!(ensure_no_real_identity_values(&Network::Polkadot.config()).is_ok());

	let polkadot_people = format!("0x{}", hex::encode(POLKADOT_PEOPLE.genesis));
	let forked = edit_chain("People", |chain| chain["genesisHash"] = polkadot_people.into());
	let error = ensure_no_real_identity_values(&stack_network_config(&forked).unwrap())
		.expect_err("mainnet genesis accepted");
	assert!(error.to_string().contains("polkadot"));

	let dot = edit_stack(|descriptor| descriptor["tld"] = "dot".into());
	let error = ensure_no_real_identity_values(&stack_network_config(&dot).unwrap())
		.expect_err("dot TLD accepted");
	assert!(error.to_string().contains(".dot"));
}

/// Polkadot's namespace contains its real-identity sessions and accounts, so
/// a disposable stack never runs under it.
#[test]
fn test_stack_polkadot_namespace_error() {
	for id in ["polkadot", "polkadot-fork"] {
		let descriptor = edit_stack(|descriptor| descriptor["id"] = id.into());
		let config = stack_network_config(&descriptor).unwrap();
		assert!(ensure_no_real_identity_values(&config).is_err());
	}
}

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
	let endpoints: Vec<_> = config.live_chain_endpoints.iter().collect();
	verify_endpoints(config, &endpoints, unanswered, built_in, Some(config.network_suffix)).await
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

	let light_client = NetworkConfig { chain_backend: ChainBackend::LightClient, ..paseo };
	let light_client = plan_chain_checks(&light_client, &paseo, false, None);
	assert_eq!((light_client.rerouted.len(), light_client.from_stack.len()), (0, 0));
	assert_eq!(light_client.expected_suffix, None);

	let overrides =
		ChainOverrides::parse([Some("ws://127.0.0.1:9".to_string()), None, None]).unwrap();
	let local = plan_chain_checks(&apply_chain_overrides(paseo, overrides), &paseo, true, None);
	assert_eq!((local.rerouted.len(), local.from_stack.len()), (1, 0));
	assert_eq!(local.expected_suffix, None);
}

/// Only the endpoints an override reroutes are checked as such, even one
/// set to another role's URL, which only its genesis tells apart.
#[test]
fn test_split_rerouted() {
	let base = Network::PaseoNextV2.config();
	let overrides =
		ChainOverrides::parse([Some(base.bulletin_ws.to_string()), None, None]).unwrap();
	let config = apply_chain_overrides(base, overrides);
	let (rerouted, untouched) = split_rerouted(&config, &base);
	assert_eq!(rerouted.len(), 1);
	assert_eq!((rerouted[0].genesis, rerouted[0].ws), (base.people_genesis, base.bulletin_ws));
	assert_eq!(untouched.len(), 2);
	assert!(untouched.iter().all(|endpoint| endpoint.genesis != base.people_genesis));
}

/// Mainnet identities are real: they must never reach the plaintext account
/// store or auto-accept.
#[test]
fn polkadot_holds_real_identities() {
	assert!(Network::Polkadot.config().ensure_disposable_identities().is_err());
}

#[test]
fn test_backend_override_hint() {
	let polkadot = Network::Polkadot.config();
	let error = polkadot.identity_backend().expect_err("polkadot ships no backend");
	assert!(error.to_string().contains(IDENTITY_BACKEND_BASE_ENV));

	let overridden =
		apply_backend_override(polkadot, Some("http://localhost:8080/api/v1".to_string()));
	assert_eq!(overridden.identity_backend().ok(), Some("http://localhost:8080/api/v1"));
}

#[test]
fn backend_override_replaces_only_the_backend_base() {
	let paseo = Network::PaseoNextV2.config();
	let overridden =
		apply_backend_override(paseo, Some("http://localhost:8080/api/v1/".to_string()));

	assert_eq!(overridden.identity_backend_base, Some("http://localhost:8080/api/v1"));
	assert_eq!(overridden.asset_hub_ws, paseo.asset_hub_ws);
	assert_eq!(
		apply_backend_override(paseo, Some("  ".to_string())).identity_backend_base,
		paseo.identity_backend_base,
	);
}

fn stack_arg(source: &'static str) -> StackArg {
	StackArg { value: "https://stack.example/s.json".to_string(), source }
}

/// `--network` picks a built-in network and a stack any other one, and the
/// error prints where the stack came from.
#[test]
fn test_network_with_stack_error() {
	for source in ["--stack", STACK_ENV] {
		let stack = stack_arg(source);
		let error = ensure_sources_compatible(
			Some(&stack),
			Some(Network::Previewnet),
			&[],
			false,
			ChainBackend::Rpc,
		)
		.expect_err("--network accepted with a stack");
		assert!(error.to_string().contains(source));
	}
}

/// The light client has chain specs only for the networks truapi-provider
/// bundles, so a stack, whatever its namespace, fails before the command
/// starts rather than on every connect.
#[test]
fn test_light_client_with_stack_error() {
	let stack = stack_arg(STACK_ENV);
	let error =
		ensure_sources_compatible(Some(&stack), None, &[], false, ChainBackend::LightClient)
			.expect_err("light client accepted with a stack");
	assert!(error.to_string().contains(STACK_ENV));
	assert!(ensure_sources_compatible(Some(&stack), None, &[], false, ChainBackend::Rpc).is_ok());
}

/// The light client syncs the chains by genesis from its bundled specs, so
/// it cannot follow a chain rerouted to another URL.
#[test]
fn test_light_client_with_overrides_error() {
	let overrides = [PEOPLE_WS_ENV];
	let error = ensure_sources_compatible(None, None, &overrides, false, ChainBackend::LightClient)
		.expect_err("light client accepted with a chain URL override");
	assert!(error.to_string().contains(PEOPLE_WS_ENV));
	assert!(ensure_sources_compatible(None, None, &overrides, false, ChainBackend::Rpc).is_ok());
}

#[test]
fn test_light_client_with_local_network_error() {
	let error = ensure_sources_compatible(None, None, &[], true, ChainBackend::LightClient)
		.expect_err("light client accepted with a local network");
	let message = error.to_string();
	assert!(message.contains(local_network::CONFIG_ENV));
	assert!(message.contains("--chain-backend light-client"));
}

/// The overrides point a network at a fork with the same genesis, and the
/// local-network file replaces the genesis: mixing them would route roles
/// to two networks.
#[test]
fn test_chain_overrides_with_local_network_error() {
	let overrides = [PEOPLE_WS_ENV, BULLETIN_WS_ENV];
	let error = ensure_sources_compatible(None, None, &overrides, true, ChainBackend::Rpc)
		.expect_err("chain URL override accepted with a local network");
	let message = error.to_string();
	assert!(message.contains(local_network::CONFIG_ENV));
	assert!(message.contains(PEOPLE_WS_ENV) && message.contains(BULLETIN_WS_ENV));

	let stack = stack_arg("--stack");
	let error = ensure_sources_compatible(Some(&stack), None, &[], true, ChainBackend::Rpc)
		.expect_err("stack accepted with a local network");
	assert!(error.to_string().contains("--stack"));
}

#[test]
fn test_chain_overrides_blank_unset() {
	let overrides =
		ChainOverrides::parse([None, Some(String::new()), Some("  ".to_string())]).unwrap();
	assert!(overrides.env_names_in_use().is_empty());
	let paseo = Network::PaseoNextV2.config();
	let resolved = apply_chain_overrides(paseo, overrides);
	assert!(std::ptr::eq(resolved.live_chain_endpoints, paseo.live_chain_endpoints));
}

#[test]
fn test_chain_overrides_websocket_only() {
	for value in ["127.0.0.1:10010", "http://127.0.0.1:10010"] {
		let error = ChainOverrides::parse([None, None, Some(value.to_string())])
			.expect_err("non-ws override accepted");
		assert!(error.to_string().contains(BULLETIN_WS_ENV));
	}
}

/// An override reroutes one role of a built-in network or a stack, in both
/// its field and the genesis-keyed routing table, and the genesis stays the
/// network's, as a fork has the same one.
#[test]
fn test_chain_overrides_field_and_route() {
	let stack = stack_network_config(&edit_stack(|_| {})).unwrap();
	for network in [Network::Previewnet.config(), stack] {
		let overrides = ChainOverrides::parse([
			Some(" ws://127.0.0.1:10010 ".to_string()),
			Some(" ".to_string()),
			None,
		])
		.unwrap();
		assert_eq!(overrides.env_names_in_use(), [PEOPLE_WS_ENV]);
		let overridden = apply_chain_overrides(network, overrides);
		let route = |genesis: [u8; 32]| {
			overridden
				.live_chain_endpoints
				.iter()
				.find(|endpoint| endpoint.genesis == genesis)
				.map(|endpoint| endpoint.ws)
		};

		assert_eq!(overridden.people_ws, "ws://127.0.0.1:10010");
		assert_eq!(overridden.asset_hub_ws, network.asset_hub_ws);
		assert_eq!(overridden.bulletin_ws, network.bulletin_ws);
		assert_eq!(overridden.people_genesis, network.people_genesis);
		assert_eq!(route(network.people_genesis), Some("ws://127.0.0.1:10010"));
		assert_eq!(route(network.asset_hub_genesis), Some(network.asset_hub_ws));
		assert_eq!(route(network.bulletin_genesis), Some(network.bulletin_ws));
	}
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

/// A newer published copy that changes a field the CLI reads is what the
/// network runs on, and the user learns that it is in use ahead of the
/// release that ships it.
#[test]
fn test_refresh_newer_notice() {
	let remote_copy = sign_edited(|descriptor| {
		descriptor["sequence"] = 11.into();
		descriptor["generation"] = "g1".into();
		let people = descriptor["chains"]
			.as_array_mut()
			.unwrap()
			.iter_mut()
			.find(|chain| chain["role"] == "People")
			.unwrap();
		people["rpc"] = serde_json::json!(["wss://people.example"]);
	});
	let refresh = refresh(&sign_copy(10, "g1"), Fetched::Document(&remote_copy)).unwrap();
	assert_eq!(refresh.descriptor.sequence, 11);
	assert_eq!(refresh.copy, BuiltInCopy::Published);
	let notice = refresh.notice.expect("newer copy not reported");
	assert_eq!((notice.bundle_sequence, notice.remote_sequence), (10, 11));
	assert!(!notice.new_generation);
	let text = notice.to_string();
	assert!(text.contains("10") && text.contains("11") && text.contains("in use"));
}

/// A republish that changes only fields the CLI does not read, such as a
/// higher sequence, is used without a notice.
#[test]
fn test_refresh_sequence_only_silent() {
	let remote_copy = sign_copy(11, "g1");
	let refresh = refresh(&sign_copy(10, "g1"), Fetched::Document(&remote_copy)).unwrap();
	assert_eq!(refresh.descriptor.sequence, 11);
	assert_eq!(refresh.copy, BuiltInCopy::Published);
	assert!(refresh.notice.is_none());
}

/// The chains behind the bundled copy are gone after a restart, which the
/// notice reports beyond a routine update.
#[test]
fn test_refresh_new_generation_notice() {
	let published = sign_copy(11, "g2");
	let notice = refresh(&sign_copy(10, "g1"), Fetched::Document(&published))
		.unwrap()
		.notice
		.expect("new generation not reported");
	assert!(notice.new_generation);
	assert!(notice.to_string().contains("restarted from genesis"));
}

/// Accounts derive under the TLD, so a refresh that changes it would swap
/// every account under the user.
#[test]
fn test_refresh_tld_change_bundle() {
	let other_tld_copy = sign_edited(|descriptor| {
		descriptor["sequence"] = 11.into();
		descriptor["generation"] = "g1".into();
		descriptor["tld"] = "testnet".into();
	});
	let refresh = refresh(&sign_copy(10, "g1"), Fetched::Document(&other_tld_copy)).unwrap();
	assert_eq!(refresh.descriptor.sequence, 10);
	assert_eq!(refresh.descriptor.tld, "paseo");
	assert!(refresh.notice.is_none());
	assert_eq!(refresh.copy, BuiltInCopy::Rejected);
}

/// A signed copy with a lower sequence is a replay of an older descriptor.
#[test]
fn test_refresh_rollback_bundle() {
	let older = sign_copy(9, "g0");
	let refresh = refresh(&sign_copy(10, "g1"), Fetched::Document(&older)).unwrap();
	assert_eq!(refresh.descriptor.sequence, 10);
	assert!(refresh.notice.is_none());
	assert_eq!(refresh.copy, BuiltInCopy::Rejected);
}

#[test]
fn test_refresh_unreachable_bundle() {
	let refresh = refresh(&sign_copy(10, "g1"), Fetched::Unreachable).unwrap();
	assert_eq!(refresh.descriptor.sequence, 10);
	assert!(refresh.notice.is_none());
	assert_eq!(refresh.copy, BuiltInCopy::Unreachable);
}

/// An expired stack has shut down, from either copy.
#[test]
fn test_refresh_expired_error() {
	let expired = |sequence: u32| {
		sign_edited(|descriptor| {
			descriptor["sequence"] = sequence.into();
			descriptor["expiresAt"] = (NOW as u32 - 1).into();
		})
	};
	let error = refresh(&expired(10), Fetched::Unreachable)
		.err()
		.expect("expired stack accepted");
	assert!(error.to_string().contains("expiresAt"));

	let published = expired(11);
	assert!(refresh(&sign_copy(10, "g1"), Fetched::Document(&published)).is_err());
}
