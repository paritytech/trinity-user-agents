use super::*;

/// Every served role must match an endpoint that actually routes it, at the
/// URL the preset names for that role.
///
/// `CliChainProvider` falls back to People for an unrecognised genesis, so a
/// drifted `people_genesis` still resolves to a working URL and satisfies
/// every routing assertion. Pinning against
/// `live_chain_endpoints` checks the constants themselves rather than the
/// plumbing that reads them.
///
/// This catches the two copies disagreeing, which is what an edit to one of
/// them causes. It cannot catch both being wrong in the same way; only a
/// live connection distinguishes that.
#[test]
fn served_chain_genesis_hashes_match_the_endpoint_routes() {
	for network in Network::value_variants() {
		let config = network.config();
		for entry in config.host_chain_set().chains {
			let expected_ws = config.url_for_role(entry.identifier).unwrap_or_else(|| {
				panic!("{} serves {:?} with no preset URL", config.id, entry.identifier)
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
			assert_eq!(
				endpoint.ws,
				expected_ws,
				"{} routes {:?} (genesis {}) to {}, but the preset names {}",
				config.id,
				entry.identifier,
				hex::encode(entry.genesis_hash),
				endpoint.ws,
				expected_ws
			);
		}
	}
}

/// Anchors the served set itself. Every other test that reads it iterates, so
/// all of them pass on an empty set; this is what notices a dropped role, and it
/// covers each preset rather than only the default.
#[test]
fn every_preset_serves_exactly_the_expected_roles() {
	for network in Network::value_variants() {
		let config = network.config();
		let served: Vec<ChainIdentifier> =
			config.host_chain_set().chains.iter().map(|entry| entry.identifier).collect();

		assert_eq!(
			served,
			vec![ChainIdentifier::People, ChainIdentifier::Bulletin, ChainIdentifier::AssetHub,],
			"{} serves an unexpected role set",
			config.id
		);
	}
}

/// SPEC.md §14.1 is the second hand-maintained copy of these hashes, and the
/// only one outside this module that pins them at build time.
///
/// Compiled in with `include_str!` so a moved table breaks the build rather
/// than drifting quietly.
#[test]
fn the_spec_genesis_table_matches_the_preset() {
	const SPEC: &str = include_str!("../../SPEC.md");

	for network in Network::value_variants() {
		let config = network.config();
		for (row, expected) in [
			("People genesis", config.people_genesis),
			("Bulletin genesis", config.bulletin_genesis),
			("Asset Hub genesis", config.asset_hub_genesis),
		] {
			let expected_row = format!("| {row} | `0x{}` |", hex::encode(expected));
			assert!(
				SPEC.contains(&expected_row),
				"SPEC.md is missing the row `{expected_row}` for {}; the table and \
                 the preset have drifted",
				config.id
			);
		}
	}
}

/// Hosts a disposable-identity preset may route to, matched exactly: the
/// paseo-next-v2 chains, the previewnet parachain set, and those two
/// presets' dotSpark identity backends. Adding a preset means adding its
/// hosts here on purpose.
const TEST_NETWORK_HOSTS: &[&str] = &[
	"paseo-asset-hub-next-rpc.polkadot.io",
	"paseo-people-next-system-rpc.polkadot.io",
	"paseo-bulletin-next-rpc.polkadot.io",
	"previewnet.substrate.dev",
	"identity.dotspark.app",
	"identity-previewnet.dotspark.app",
];

#[test]
fn every_preset_names_a_known_dotns_tld() {
	for network in Network::value_variants() {
		let config = network.preset();
		assert!(
			truapi::platform::DOTNS_TLDS.contains(&config.network_suffix),
			"preset `{}` derives reserved identities under `.{}`, a TLD navigation does not \
             accept",
			config.id,
			config.network_suffix,
		);
	}
}

/// Guards the invariant documented on [`Network`]: a preset that lets the
/// plaintext account store keep its identities must route only to test
/// networks. If this fails because a real network was added, clear its
/// `disposable_identities` rather than extending the allowlist.
#[test]
fn every_disposable_preset_routes_to_a_test_network() {
	for network in Network::value_variants() {
		// Reading the raw preset. An exported backend override cannot leak in.
		let config = network.preset();
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
			assert!(
				TEST_NETWORK_HOSTS.contains(&host.as_str()),
				"preset `{}` routes to a host that is not a recognised test network: {route}",
				config.id,
			);
		}
	}
}

/// The chains a host advertises must be the chains it reaches.
///
/// [`served_chain_genesis_hashes_match_the_endpoint_routes`] pins the two
/// constants against each other and cannot see them agreeing on a wrong
/// value, which is the shape a wiped testnet leaves behind. Only a live
/// connection distinguishes that, so this asks each endpoint for its own
/// genesis.
///
/// A drifted hash does not fail loudly on its own: `CliChainProvider` answers
/// an unrecognised genesis with People, so every role still resolves to some
/// working chain, and products read the advertised hash back out of
/// `get_chain_info` and sign `CheckGenesis` over it.
///
/// A genesis alone does not prove the role is right: pointing People at the
/// Bulletin endpoint and its genesis satisfies every constant-versus-constant
/// assertion, and the endpoint then truthfully reports the hash it was given.
/// The chain's own name is what ties the role to the chain behind it.
///
/// Unreachable endpoints and mismatches are both collected before failing,
/// because a wipe drifts more than one role at a time and takes endpoints
/// down with it; returning early would report only the first, and a
/// connection error would be indistinguishable from drift. The checked count
/// is asserted at the end: `--ignored` runs this test *without*
/// [`every_preset_serves_exactly_the_expected_roles`], so nothing else is
/// holding the served set non-empty in that invocation.
///
/// Ignored by default; needs network access to the preset's chains.
///
/// ```sh
/// cargo +nightly test -p truapi-host-cli --bin truapi-host \
///     advertised_genesis -- --ignored --nocapture
/// ```
#[tokio::test]
#[ignore = "needs network access to the preset's chains"]
async fn the_advertised_genesis_matches_what_each_chain_reports() {
	use truapi::statement_allowance as alloc;

	let mut checked = 0usize;
	let mut drifted = Vec::new();
	let mut unreachable = Vec::new();

	for network in Network::value_variants() {
		let config = network.config();
		for entry in config.host_chain_set().chains {
			let ws = config
				.url_for_role(entry.identifier)
				.expect("every served role names a preset URL");
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
			// The chain's own name has to name the role, or a role pointed at
			// the wrong preset chain passes every other assertion here.
			// Both spellings the People role answers to: Paseo's chain calls
			// itself "People" and previewnet's calls itself "Individuality
			// Local", so either name satisfies the role.
			let expected_in_name: &[&str] = match entry.identifier {
				ChainIdentifier::People => &["People", "Individuality"],
				ChainIdentifier::Bulletin => &["Bulletin"],
				ChainIdentifier::AssetHub => &["Asset Hub"],
				ChainIdentifier::Relay => &["Relay"],
			};
			if !expected_in_name.iter().any(|expected| chain_name.contains(expected)) {
				drifted.push(format!(
					"{} serves {:?} from {ws}, which calls itself \
                     {chain_name:?} and names none of {expected_in_name:?}",
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

	// Reported together, drift first. Asserting them separately meant one
	// unreachable endpoint discarded every drift the answering endpoints had
	// already proven, and claimed drift was unproven when it was not.
	let mut failures = Vec::new();
	if !drifted.is_empty() {
		failures.push(format!(
			"drifted, so refresh the preset and SPEC.md together:\n{}",
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
	// Derived rather than hardcoded, so adding a role widens this instead of
	// failing it. Non-emptiness is asserted separately: `--ignored` runs this
	// without the anchor test, and an empty served set would otherwise make
	// both the loop and its count trivially agree on zero.
	assert!(expected_checks > 0, "no preset serves any chain, so this test proved nothing");
	assert_eq!(
		checked, expected_checks,
		"every served role must be probed; anything else means the loop \
         skipped one and this test stopped covering it"
	);
}

/// The compile-time reader must agree with a JSON parser on every entry,
/// so the preset's controller is exactly what the manifest records.
#[test]
fn manifest_addresses_decode_as_a_json_parser_reads_them() {
	let manifest: serde_json::Map<String, serde_json::Value> =
		serde_json::from_str(POLKADOT_DOTNS_DEPLOYMENT).expect("manifest is a JSON object");
	for (name, address) in &manifest {
		let address = address.as_str().expect("every entry is an address string");
		assert_eq!(
			hex::encode(manifest_address(POLKADOT_DOTNS_DEPLOYMENT, name)),
			address.trim_start_matches("0x").to_lowercase(),
			"{name}"
		);
	}
	assert_eq!(
		Network::Polkadot.preset().dotns_pop_controller,
		Some(manifest_address(POLKADOT_DOTNS_DEPLOYMENT, "DotnsPopController"))
	);
}

/// A fork keeps its source's genesis, so a test preset routed at a copy of
/// mainnet would sign mainnet-valid transactions with auto-approval.
#[test]
fn a_test_preset_never_carries_a_real_identity_genesis() {
	assert!(refuse_real_identity_genesis(&Network::PaseoNextV2.preset()).is_ok());

	let mainnet_people = POLKADOT_PEOPLE;
	let mut forked = Network::PaseoNextV2.preset();
	forked.live_chain_endpoints = Box::leak(vec![mainnet_people].into_boxed_slice());
	let error = refuse_real_identity_genesis(&forked).expect_err("mainnet genesis is refused");
	assert!(error.to_string().contains("polkadot"), "{error}");
}

/// A rerouted endpoint is checked before use, so one that cannot answer is
/// refused rather than trusted.
#[tokio::test]
async fn an_unreachable_rerouted_endpoint_is_refused() {
	let preset = Network::PaseoNextV2.preset();
	let rerouted = apply_chain_overrides(
		preset,
		ChainOverrides { people_ws: Some("ws://127.0.0.1:9".to_string()), ..Default::default() },
	);
	let error = verify_rerouted_endpoints(&rerouted, &preset)
		.await
		.expect_err("an endpoint that cannot report its genesis is refused");
	assert!(error.to_string().contains("ws://127.0.0.1:9"), "{error}");
	assert!(verify_rerouted_endpoints(&preset, &preset).await.is_ok());
}

/// The case the override check exists for: a test preset pointed at a real
/// network's endpoint.
///
/// Ignored by default; needs network access to Polkadot People.
#[tokio::test]
#[ignore = "needs network access to Polkadot People"]
async fn a_test_preset_pointed_at_mainnet_is_refused() {
	let preset = Network::PaseoNextV2.preset();
	let rerouted = apply_chain_overrides(
		preset,
		ChainOverrides { people_ws: Some(POLKADOT_PEOPLE.ws.to_string()), ..Default::default() },
	);
	let error = verify_rerouted_endpoints(&rerouted, &preset)
		.await
		.expect_err("mainnet People is not paseo-next-v2's People");
	assert!(error.to_string().contains("reports genesis"), "{error}");
}

/// Mainnet identities are real: they must never reach the plaintext account
/// store or auto-accept.
#[test]
fn polkadot_holds_real_identities() {
	assert!(Network::Polkadot.preset().ensure_disposable_identities().is_err());
}

#[test]
fn a_preset_without_a_backend_names_the_override() {
	let preset = Network::Polkadot.preset();
	let error = preset.identity_backend().expect_err("polkadot ships no backend");
	assert!(error.to_string().contains(IDENTITY_BACKEND_BASE_ENV), "{error}");

	let overridden =
		apply_backend_override(preset, Some("http://localhost:8080/api/v1".to_string()));
	assert_eq!(overridden.identity_backend().ok(), Some("http://localhost:8080/api/v1"));
}

#[test]
fn backend_override_replaces_only_the_backend_base() {
	let preset = Network::PaseoNextV2.preset();
	let overridden =
		apply_backend_override(preset, Some("http://localhost:8080/api/v1/".to_string()));

	assert_eq!(
		overridden.identity_backend_base,
		Some("http://localhost:8080/api/v1"),
		"trailing slash is stripped"
	);
	assert_eq!(overridden.asset_hub_ws, preset.asset_hub_ws);
	assert_eq!(
		apply_backend_override(preset, Some("  ".to_string())).identity_backend_base,
		preset.identity_backend_base,
		"a blank override keeps the preset"
	);
}

#[test]
fn no_chain_override_keeps_the_preset() {
	for network in Network::value_variants() {
		let preset = network.preset();
		let overrides =
			ChainOverrides::parse([None, Some(String::new()), Some("  ".to_string())]).unwrap();
		assert!(overrides.set_names().is_empty());
		let resolved = apply_chain_overrides(preset, overrides);
		assert_eq!(resolved.people_ws, preset.people_ws);
		assert_eq!(resolved.asset_hub_ws, preset.asset_hub_ws);
		assert_eq!(resolved.bulletin_ws, preset.bulletin_ws);
		assert!(std::ptr::eq(resolved.live_chain_endpoints, preset.live_chain_endpoints));
	}
}

#[test]
fn chain_overrides_are_refused_with_the_light_client() {
	let rpc = Network::PaseoNextV2.preset();
	let light = NetworkConfig { chain_backend: ChainBackend::LightClient, ..rpc };
	let error = refuse_light_client(&light, &[PEOPLE_WS_ENV]).unwrap_err();
	assert!(error.to_string().contains(PEOPLE_WS_ENV), "{error}");
	assert!(refuse_light_client(&light, &[]).is_ok());
	assert!(refuse_light_client(&rpc, &[PEOPLE_WS_ENV]).is_ok());
}

/// Accepting a network without bundled specs would fail every connect
/// instead of once, before the command starts.
#[test]
fn the_light_client_is_refused_for_a_network_it_has_no_specs_for() {
	let polkadot =
		NetworkConfig { chain_backend: ChainBackend::LightClient, ..Network::Polkadot.preset() };
	let error = refuse_light_client(&polkadot, &[]).unwrap_err();
	assert!(error.to_string().contains("no chain specs for polkadot"), "{error}");
}

/// The light client finds each chain in `truapi-provider`'s bundled catalog
/// by the preset's genesis hash, so a network reset applied to only one of
/// the two copies leaves it unable to connect.
#[test]
fn every_preset_the_light_client_bundles_matches_its_catalog() {
	let bundled: Vec<_> = Network::value_variants()
		.iter()
		.map(|network| network.config())
		.filter(|config| truapi_provider::known_networks().any(|known| known == config.id))
		.collect();
	assert!(!bundled.is_empty(), "no preset is bundled, so this test would pass vacuously");
	for config in bundled {
		let (_, catalog) = truapi_provider::EmbeddedChainProviderBuilder::new()
			.add_network(config.id)
			.expect("a known network registers");
		assert_eq!(
			(catalog.people, catalog.assethub, catalog.bulletin),
			(config.people_genesis, config.asset_hub_genesis, config.bulletin_genesis),
			"{} genesis hashes differ from the light-client catalog",
			config.id
		);
	}
}

#[test]
fn chain_overrides_must_be_websocket_urls() {
	for value in ["127.0.0.1:10010", "http://127.0.0.1:10010"] {
		let error = ChainOverrides::parse([None, None, Some(value.to_string())])
			.expect_err("a non-ws override is refused up front");
		assert!(error.to_string().contains(BULLETIN_WS_ENV), "{error}");
	}
}

#[test]
fn chain_overrides_move_the_field_and_the_route_together() {
	let preset = Network::Previewnet.preset();
	let overrides = ChainOverrides::parse([
		Some(" ws://127.0.0.1:10010 ".to_string()),
		Some(" ".to_string()),
		None,
	])
	.unwrap();
	assert_eq!(overrides.set_names(), [PEOPLE_WS_ENV]);
	let overridden = apply_chain_overrides(preset, overrides);

	assert_eq!(overridden.people_ws, "ws://127.0.0.1:10010");
	assert_eq!(overridden.asset_hub_ws, preset.asset_hub_ws, "a blank override keeps the preset");
	assert_eq!(overridden.bulletin_ws, preset.bulletin_ws);
	assert_eq!(overridden.people_genesis, preset.people_genesis, "a fork keeps the genesis");
	for endpoint in overridden.live_chain_endpoints {
		let expected = if endpoint.genesis == preset.people_genesis {
			"ws://127.0.0.1:10010"
		} else {
			preset
				.live_chain_endpoints
				.iter()
				.find(|original| original.genesis == endpoint.genesis)
				.expect("every route survives the override")
				.ws
		};
		assert_eq!(endpoint.ws, expected);
	}
}
