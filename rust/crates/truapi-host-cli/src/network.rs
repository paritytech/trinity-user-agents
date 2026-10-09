use clap::ValueEnum;
use truapi::latest::ChainIdentifier;
use truapi::platform::{HostChainEntry, HostChainSet};

/// Supported live network presets for the headless hosts.
///
/// Every preset must be a test network. The CLI account store keeps BIP-39
/// mnemonics in plaintext (`accounts.rs`), which is only acceptable for
/// disposable test identities, so adding a production preset means reworking
/// that storage first. The `every_preset_is_a_test_network` test enforces it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum Network {
    #[value(name = "paseo-next-v2")]
    #[default]
    PaseoNextV2,
    /// Named for the id the playground and dotli already use for this network
    /// (`VITE_NETWORKS=paseo-next-v2,previewnet`), so `HostChainSet::network`
    /// reads the same string a product sees everywhere else.
    #[value(name = "previewnet")]
    Previewnet,
}

/// Env var overriding the identity backend base URL for every command. The URL
/// includes `/api/v1`, for instance a local backend at
/// `http://localhost:8080/api/v1`. Chain endpoints stay on the preset.
pub const IDENTITY_BACKEND_BASE_ENV: &str = "HOST_CLI_IDENTITY_BACKEND_BASE";

impl Network {
    /// Preset resolved with any environment overrides applied.
    pub fn config(self) -> NetworkConfig {
        apply_backend_override(self.preset(), std::env::var(IDENTITY_BACKEND_BASE_ENV).ok())
    }

    /// The unmodified preset values.
    fn preset(self) -> NetworkConfig {
        match self {
            Self::PaseoNextV2 => {
                NetworkConfig {
                    id: "paseo-next-v2",
                    network_suffix: "paseo",
                    identity_backend_base: "https://identity.dotspark.app/api/v1",
                    people_ws: PASEO_PEOPLE.ws,
                    bulletin_ws: PASEO_BULLETIN.ws,
                    asset_hub_ws: PASEO_ASSET_HUB.ws,
                    people_genesis: PASEO_PEOPLE.genesis,
                    bulletin_genesis: PASEO_BULLETIN.genesis,
                    asset_hub_genesis: PASEO_ASSET_HUB.genesis,
                    live_chain_endpoints: PASEO_NEXT_V2_CHAIN_ENDPOINTS,
                }
            }
            Self::Previewnet => {
                NetworkConfig {
                    id: "previewnet",
                    network_suffix: "testnet",
                    identity_backend_base: "https://identity-previewnet.dotspark.app/api/v1",
                    people_ws: PREVIEWNET_PEOPLE.ws,
                    bulletin_ws: PREVIEWNET_BULLETIN.ws,
                    asset_hub_ws: PREVIEWNET_ASSET_HUB.ws,
                    people_genesis: PREVIEWNET_PEOPLE.genesis,
                    bulletin_genesis: PREVIEWNET_BULLETIN.genesis,
                    asset_hub_genesis: PREVIEWNET_ASSET_HUB.genesis,
                    live_chain_endpoints: PREVIEWNET_CHAIN_ENDPOINTS,
                }
            }
        }
    }
}

/// Replaces the preset's identity backend base with `base` when it carries a
/// non-empty URL. Trailing slashes are stripped so path joins stay clean. The
/// override string is leaked; the CLI reads it a handful of times per process.
fn apply_backend_override(mut config: NetworkConfig, base: Option<String>) -> NetworkConfig {
    if let Some(base) = base {
        let trimmed = base.trim().trim_end_matches('/');
        if !trimmed.is_empty() {
            config.identity_backend_base = Box::leak(trimmed.to_string().into_boxed_str());
        }
    }
    config
}

// Each chain is declared once and reused by both the preset fields and the
// endpoint table. A genesis hash can therefore never drift from the URL it
// routes to. Every route below is host-required: session identity (dotNS
// usernames) resolves through Asset Hub, SSO through People, preimages through
// Bulletin.

pub const PASEO_ASSET_HUB: ChainEndpoint = ChainEndpoint {
    genesis: hex_literal_genesis(
        "4349b00e54897e21196fd331015fc5be0f14e118beb0375ed2bb1793737bb57a",
    ),
    ws: "wss://paseo-asset-hub-next-rpc.polkadot.io",
    required_for_host: true,
};

const PASEO_PEOPLE: ChainEndpoint = ChainEndpoint {
    genesis: hex_literal_genesis(
        "4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48",
    ),
    ws: "wss://paseo-people-next-system-rpc.polkadot.io",
    required_for_host: true,
};

const PASEO_BULLETIN: ChainEndpoint = ChainEndpoint {
    genesis: hex_literal_genesis(
        "8cfe6717dc4becfda2e13c488a1e2061ff2dfee96e7d031157f72d36716c0a22",
    ),
    ws: "wss://paseo-bulletin-next-rpc.polkadot.io",
    required_for_host: true,
};

const PREVIEWNET_ASSET_HUB: ChainEndpoint = ChainEndpoint {
    genesis: hex_literal_genesis(
        "bac97e23fc8f4bccae72a98f8aeb2bcab20bf755862304e4b46ad6473456e896",
    ),
    ws: "wss://previewnet.substrate.dev/asset-hub",
    required_for_host: true,
};

const PREVIEWNET_PEOPLE: ChainEndpoint = ChainEndpoint {
    genesis: hex_literal_genesis(
        "55e3e689ecfa9d2fffcf7d309b8011956671493982230bfd0420c683542249e9",
    ),
    ws: "wss://previewnet.substrate.dev/people",
    required_for_host: true,
};

const PREVIEWNET_BULLETIN: ChainEndpoint = ChainEndpoint {
    genesis: hex_literal_genesis(
        "a081192b90c1f6a3f8e9ce7b2a8246f41af805c66456c84e05fd97c2b3502425",
    ),
    ws: "wss://previewnet.substrate.dev/bulletin",
    required_for_host: true,
};

const PASEO_NEXT_V2_CHAIN_ENDPOINTS: &[ChainEndpoint] =
    &[PASEO_ASSET_HUB, PASEO_PEOPLE, PASEO_BULLETIN];

const PREVIEWNET_CHAIN_ENDPOINTS: &[ChainEndpoint] =
    &[PREVIEWNET_ASSET_HUB, PREVIEWNET_PEOPLE, PREVIEWNET_BULLETIN];

/// Resolved RPC/backend/genesis values for one network preset.
#[derive(Debug, Clone, Copy)]
pub struct NetworkConfig {
    pub id: &'static str,
    /// The network's dotNS TLD without the dot, as its runtimes report it in
    /// `NetworkSuffix.NetworkSuffix`. Every reserved RFC-0022 identity the CLI
    /// derives ends in it (`uid.<suffix>`, `peopl.<suffix>`), so a person the
    /// CLI creates here is the same person a phone derives from that seed on
    /// this network. `live_people_chain::network_suffix_matches_the_preset`
    /// holds it against the chain.
    pub network_suffix: &'static str,
    pub identity_backend_base: &'static str,
    pub people_ws: &'static str,
    // Read only by tests; production routing goes through `live_chain_endpoints`.
    #[allow(dead_code)]
    pub bulletin_ws: &'static str,
    /// Asset Hub RPC, where PGAS allowances are claimed.
    pub asset_hub_ws: &'static str,
    pub people_genesis: [u8; 32],
    pub bulletin_genesis: [u8; 32],
    /// Asset Hub genesis hash, both the role's identity in
    /// [`NetworkConfig::host_chain_set`] and its routing key. The chain it connects
    /// to is authoritative for `CheckGenesis`.
    ///
    /// Wrong here is worse than stale elsewhere: routing matches on it, so a value
    /// the chain does not report sends Asset Hub traffic to the fallback chain.
    pub asset_hub_genesis: [u8; 32],
    pub live_chain_endpoints: &'static [ChainEndpoint],
}

#[derive(Debug, Clone, Copy)]
pub struct ChainEndpoint {
    pub genesis: [u8; 32],
    pub ws: &'static str,
    /// Whether host internals require this route even when optional product
    /// Chain calls are disabled.
    pub required_for_host: bool,
}

impl NetworkConfig {
    /// The chain set this preset serves, for `Features::supported_chains`.
    ///
    /// One entry per role this struct names a genesis hash for. `ChainEndpoint`
    /// carries no role, so `live_chain_endpoints` cannot supply one — an endpoint's
    /// identifier is not recoverable from it, which is why a served role needs its
    /// own field here rather than a lookup into that list.
    ///
    /// Every role listed must also be routable: `WsChainProvider` answers an
    /// unmapped genesis with its fallback URL, so a role the routing filter drops
    /// would connect to the wrong chain while the host claimed to serve it.
    /// `chain::tests::every_served_role_routes_to_its_own_chain` holds that.
    pub fn host_chain_set(&self) -> HostChainSet {
        HostChainSet {
            network: self.id.to_string(),
            chains: vec![
                HostChainEntry {
                    identifier: ChainIdentifier::People,
                    genesis_hash: self.people_genesis,
                },
                HostChainEntry {
                    identifier: ChainIdentifier::Bulletin,
                    genesis_hash: self.bulletin_genesis,
                },
                HostChainEntry {
                    identifier: ChainIdentifier::AssetHub,
                    genesis_hash: self.asset_hub_genesis,
                },
            ],
        }
    }

    /// The preset's own URL for a served role, independent of the genesis-keyed
    /// routing table. Every genesis test resolves the role this way so that a
    /// drifted hash cannot silently move them onto the fallback URL.
    ///
    /// Matched exhaustively on purpose: adding a role to [`ChainIdentifier`]
    /// should stop this compiling rather than reach a `None` that only a test
    /// run notices, and one of those tests is `#[ignore]`d.
    #[cfg(test)]
    pub fn url_for_role(&self, role: ChainIdentifier) -> Option<&'static str> {
        match role {
            ChainIdentifier::People => Some(self.people_ws),
            ChainIdentifier::Bulletin => Some(self.bulletin_ws),
            ChainIdentifier::AssetHub => Some(self.asset_hub_ws),
            ChainIdentifier::Relay => None,
        }
    }
}

/// Decode a 64-char hex genesis at compile time.
const fn hex_literal_genesis(hex: &str) -> [u8; 32] {
    let bytes = hex.as_bytes();
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = hex_nibble(bytes[i * 2]) << 4 | hex_nibble(bytes[i * 2 + 1]);
        i += 1;
    }
    out
}

const fn hex_nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => panic!("invalid hex digit in genesis literal"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every served role must match an endpoint that actually routes it, at the
    /// URL the preset names for that role.
    ///
    /// `WsChainProvider::url_for` falls back to `people_ws` for an unrecognised
    /// genesis, so a drifted `people_genesis` still resolves to a working URL
    /// and satisfies every routing assertion. Pinning against
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
                    panic!(
                        "{} serves {:?} with no preset URL",
                        config.id, entry.identifier
                    )
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
            let served: Vec<ChainIdentifier> = config
                .host_chain_set()
                .chains
                .iter()
                .map(|entry| entry.identifier)
                .collect();

            assert_eq!(
                served,
                vec![
                    ChainIdentifier::People,
                    ChainIdentifier::Bulletin,
                    ChainIdentifier::AssetHub,
                ],
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
        const SPEC: &str = include_str!("../SPEC.md");

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

    /// Guards the invariant documented on [`Network`]: the plaintext mnemonic
    /// store is only safe for disposable identities, so no preset may point at a
    /// production network. If this fails because a real network was added,
    /// rework the account store rather than relaxing the assertion.
    /// Hosts a preset may route to. Every entry is a disposable test deployment:
    /// `paseo`/`testnet` name the Paseo testnets,
    /// `previewnet.substrate.dev` is the previewnet parachain set, and the two
    /// exact dotSpark hosts are those test presets' identity backends.
    ///
    /// An allowlist rather than a substring rule, because the rule this test
    /// exists for is "no production network", and a production host can contain
    /// any substring. Adding a preset means adding its hosts here on purpose.
    const TEST_NETWORK_MARKERS: &[&str] = &[
        "paseo",
        "testnet",
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

    #[test]
    fn every_preset_is_a_test_network() {
        for network in Network::value_variants() {
            // Reading the raw preset. An exported backend override cannot leak in.
            let config = network.preset();
            let mut routes = vec![
                config.identity_backend_base,
                config.people_ws,
                config.bulletin_ws,
                config.asset_hub_ws,
            ];
            routes.extend(config.live_chain_endpoints.iter().map(|chain| chain.ws));

            for route in routes {
                assert!(
                    TEST_NETWORK_MARKERS
                        .iter()
                        .any(|marker| route.contains(marker)),
                    "preset `{}` routes to a host that is not a recognised test \
                     network: {route}",
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
    /// A drifted hash does not fail loudly on its own: `url_for` answers an
    /// unrecognised genesis with `people_ws`, so every role still resolves to
    /// some working URL, and products read the advertised hash back out of
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
                    }
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
                if !expected_in_name
                    .iter()
                    .any(|expected| chain_name.contains(expected))
                {
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
        assert!(
            expected_checks > 0,
            "no preset serves any chain, so this test proved nothing"
        );
        assert_eq!(
            checked, expected_checks,
            "every served role must be probed; anything else means the loop \
             skipped one and this test stopped covering it"
        );
    }

    #[test]
    fn backend_override_replaces_only_the_backend_base() {
        let preset = Network::PaseoNextV2.preset();
        let overridden =
            apply_backend_override(preset, Some("http://localhost:8080/api/v1/".to_string()));

        assert_eq!(
            overridden.identity_backend_base, "http://localhost:8080/api/v1",
            "trailing slash is stripped"
        );
        assert_eq!(overridden.asset_hub_ws, preset.asset_hub_ws);
        assert_eq!(
            apply_backend_override(preset, Some("  ".to_string())).identity_backend_base,
            preset.identity_backend_base,
            "a blank override keeps the preset"
        );
    }
}
