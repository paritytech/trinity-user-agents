use clap::ValueEnum;
use truapi::{
	latest::ChainIdentifier,
	platform::{ChainExtensionVersion, ExtensionVersionMap, HostChainEntry, HostChainSet},
};

use crate::chain::ChainBackend;

/// Supported live network presets for the headless hosts.
///
/// The CLI account store keeps BIP-39 mnemonics in plaintext (`accounts.rs`),
/// which is only acceptable for disposable test identities. A preset that sets
/// [`NetworkConfig::disposable_identities`] may use that store; any other
/// preset only signs with a mnemonic supplied per process, never writes one,
/// and never approves a confirmation without a prompt.
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
	/// Polkadot mainnet (People, Bulletin, Asset Hub). Identities are real:
	/// pass the signer with --mnemonic; no stored accounts, no auto-accept
	#[value(name = "polkadot")]
	Polkadot,
}

/// Env var overriding the identity backend base URL for every command. The URL
/// includes `/api/v1`, for instance a local backend at
/// `http://localhost:8080/api/v1`.
pub const IDENTITY_BACKEND_BASE_ENV: &str = "HOST_CLI_IDENTITY_BACKEND_BASE";

/// Env vars overriding one chain's WebSocket URL, for a fork of the preset's
/// network. The genesis hashes stay the preset's, which a fork keeps.
pub const PEOPLE_WS_ENV: &str = "HOST_CLI_PEOPLE_WS";
pub const ASSET_HUB_WS_ENV: &str = "HOST_CLI_ASSET_HUB_WS";
pub const BULLETIN_WS_ENV: &str = "HOST_CLI_BULLETIN_WS";

/// Chain URL overrides; `None` keeps the preset's URL.
#[derive(Debug, Default)]
struct ChainOverrides {
	people_ws: Option<String>,
	asset_hub_ws: Option<String>,
	bulletin_ws: Option<String>,
}

impl ChainOverrides {
	fn from_env() -> anyhow::Result<Self> {
		Self::parse(
			[PEOPLE_WS_ENV, ASSET_HUB_WS_ENV, BULLETIN_WS_ENV].map(|name| std::env::var(name).ok()),
		)
	}

	/// Blank values keep the preset. Anything else must be a `ws` or `wss` URL.
	fn parse([people_ws, asset_hub_ws, bulletin_ws]: [Option<String>; 3]) -> anyhow::Result<Self> {
		let url = |name: &str, value: Option<String>| -> anyhow::Result<Option<String>> {
			let Some(value) = value.map(|value| value.trim().to_string()).filter(|v| !v.is_empty())
			else {
				return Ok(None);
			};
			let url = reqwest::Url::parse(&value)
				.map_err(|err| anyhow::anyhow!("{name} is not a URL ({err}): {value}"))?;
			anyhow::ensure!(
				matches!(url.scheme(), "ws" | "wss"),
				"{name} must be a ws:// or wss:// URL: {value}"
			);
			Ok(Some(value))
		};
		Ok(Self {
			people_ws: url(PEOPLE_WS_ENV, people_ws)?,
			asset_hub_ws: url(ASSET_HUB_WS_ENV, asset_hub_ws)?,
			bulletin_ws: url(BULLETIN_WS_ENV, bulletin_ws)?,
		})
	}

	/// The env vars behind the overrides that are set.
	fn set_names(&self) -> Vec<&'static str> {
		[
			(PEOPLE_WS_ENV, &self.people_ws),
			(ASSET_HUB_WS_ENV, &self.asset_hub_ws),
			(BULLETIN_WS_ENV, &self.bulletin_ws),
		]
		.into_iter()
		.filter_map(|(name, value)| value.is_some().then_some(name))
		.collect()
	}
}

impl Network {
	/// CLI resolution: the preset with the backend and chain URL overrides, or
	/// the opt-in disposable local-network configuration, which cannot be
	/// combined with the chain URL overrides. `chain_backend` selects how the
	/// chains are reached; the light client cannot be combined with either.
	///
	/// A rerouted chain must still be the chain the preset names: each endpoint
	/// that differs from the preset's is asked for its genesis before anything
	/// runs against it, and a disposable configuration may never carry the
	/// genesis of a preset whose identities are real.
	pub async fn cli_config(self, chain_backend: ChainBackend) -> anyhow::Result<NetworkConfig> {
		let overrides = ChainOverrides::from_env()?;
		let set = overrides.set_names();
		let mut base = apply_chain_overrides(self.with_backend_override(), overrides);
		base.chain_backend = chain_backend;
		refuse_light_client(&base, &set)?;
		let config = crate::local_network::resolve(base, &set)?;
		refuse_real_identity_genesis(&config)?;
		verify_rerouted_endpoints(&config, &self.preset()).await?;
		Ok(config)
	}

	/// [`Self::cli_config`] without the local-network file, for tests.
	#[cfg(test)]
	pub fn config(self) -> NetworkConfig {
		let overrides = ChainOverrides::from_env().expect("valid chain URL overrides");
		apply_chain_overrides(self.with_backend_override(), overrides)
	}

	fn with_backend_override(self) -> NetworkConfig {
		apply_backend_override(self.preset(), std::env::var(IDENTITY_BACKEND_BASE_ENV).ok())
	}

	/// The unmodified preset values.
	fn preset(self) -> NetworkConfig {
		match self {
			Self::PaseoNextV2 => NetworkConfig {
				id: "paseo-next-v2",
				network_suffix: "paseo",
				identity_backend_base: Some("https://identity.dotspark.app/api/v1"),
				people_ws: PASEO_PEOPLE.ws,
				bulletin_ws: PASEO_BULLETIN.ws,
				asset_hub_ws: PASEO_ASSET_HUB.ws,
				people_genesis: PASEO_PEOPLE.genesis,
				bulletin_genesis: PASEO_BULLETIN.genesis,
				asset_hub_genesis: PASEO_ASSET_HUB.genesis,
				live_chain_endpoints: PASEO_NEXT_V2_CHAIN_ENDPOINTS,
				dotns_pop_controller: None,
				disposable_identities: true,
				chain_backend: ChainBackend::Rpc,
			},
			Self::Previewnet => NetworkConfig {
				id: "previewnet",
				network_suffix: "testnet",
				identity_backend_base: Some("https://identity-previewnet.dotspark.app/api/v1"),
				people_ws: PREVIEWNET_PEOPLE.ws,
				bulletin_ws: PREVIEWNET_BULLETIN.ws,
				asset_hub_ws: PREVIEWNET_ASSET_HUB.ws,
				people_genesis: PREVIEWNET_PEOPLE.genesis,
				bulletin_genesis: PREVIEWNET_BULLETIN.genesis,
				asset_hub_genesis: PREVIEWNET_ASSET_HUB.genesis,
				live_chain_endpoints: PREVIEWNET_CHAIN_ENDPOINTS,
				dotns_pop_controller: None,
				disposable_identities: true,
				chain_backend: ChainBackend::Rpc,
			},
			Self::Polkadot => NetworkConfig {
				id: "polkadot",
				network_suffix: "dot",
				identity_backend_base: None,
				people_ws: POLKADOT_PEOPLE.ws,
				bulletin_ws: POLKADOT_BULLETIN.ws,
				asset_hub_ws: POLKADOT_ASSET_HUB.ws,
				people_genesis: POLKADOT_PEOPLE.genesis,
				bulletin_genesis: POLKADOT_BULLETIN.genesis,
				asset_hub_genesis: POLKADOT_ASSET_HUB.genesis,
				live_chain_endpoints: POLKADOT_CHAIN_ENDPOINTS,
				dotns_pop_controller: Some(POLKADOT_DOTNS_POP_CONTROLLER),
				disposable_identities: false,
				chain_backend: ChainBackend::Rpc,
			},
		}
	}
}

/// The light client knows only the networks `truapi-provider` bundles, by
/// genesis hash, so it can serve neither another network nor a fork.
fn refuse_light_client(config: &NetworkConfig, chain_overrides: &[&str]) -> anyhow::Result<()> {
	if config.chain_backend != ChainBackend::LightClient {
		return Ok(());
	}
	anyhow::ensure!(
		truapi_provider::known_networks().any(|network| network == config.id),
		"--chain-backend light-client has no chain specs for {}; it bundles {}",
		config.id,
		truapi_provider::known_networks().collect::<Vec<_>>().join(", ")
	);
	anyhow::ensure!(
		chain_overrides.is_empty(),
		"{} and --chain-backend light-client cannot be used together: the light client \
         syncs the live chains by genesis hash; unset one of them",
		chain_overrides.join(", ")
	);
	Ok(())
}

/// Replaces the preset's identity backend base with `base` when it carries a
/// non-empty URL. Trailing slashes are stripped so path joins stay clean. The
/// override string is leaked; the CLI reads it a handful of times per process.
fn apply_backend_override(mut config: NetworkConfig, base: Option<String>) -> NetworkConfig {
	if let Some(base) = base {
		let trimmed = base.trim().trim_end_matches('/');
		if !trimmed.is_empty() {
			config.identity_backend_base = Some(Box::leak(trimmed.to_string().into_boxed_str()));
		}
	}
	config
}

/// Refuses a disposable-identity configuration whose chains are a real-identity
/// preset's. A fork or local copy of mainnet keeps mainnet's genesis, and a
/// signature made against it is valid on mainnet itself.
fn refuse_real_identity_genesis(config: &NetworkConfig) -> anyhow::Result<()> {
	if !config.disposable_identities {
		return Ok(());
	}
	for real in Network::value_variants().iter().map(|network| network.preset()) {
		if real.disposable_identities {
			continue;
		}
		for endpoint in config.live_chain_endpoints {
			anyhow::ensure!(
				!real.live_chain_endpoints.iter().any(|chain| chain.genesis == endpoint.genesis),
				"{} would route to {}'s chain at {}, whose identities are real; point it at a \
				 test network",
				config.id,
				real.id,
				endpoint.ws
			);
		}
	}
	Ok(())
}

/// Asks every endpoint that differs from the preset's for its genesis, and
/// refuses one that is not the chain it is routed as.
async fn verify_rerouted_endpoints(
	config: &NetworkConfig,
	preset: &NetworkConfig,
) -> anyhow::Result<()> {
	use truapi::statement_allowance as alloc;

	for endpoint in config.live_chain_endpoints {
		if preset.live_chain_endpoints.iter().any(|chain| chain.ws == endpoint.ws) {
			continue;
		}
		let rpc = alloc::rpc::RpcClient::connect(endpoint.ws).await.map_err(|err| {
			anyhow::anyhow!("cannot reach {} to check its genesis: {err}", endpoint.ws)
		})?;
		let reported = alloc::fetch_genesis_hash(&rpc)
			.await
			.map_err(|err| anyhow::anyhow!("cannot read the genesis of {}: {err}", endpoint.ws))?;
		anyhow::ensure!(
			reported == endpoint.genesis,
			"{} reports genesis 0x{}, but {} routes it as the chain with genesis 0x{}",
			endpoint.ws,
			hex::encode(reported),
			config.id,
			hex::encode(endpoint.genesis)
		);
	}
	Ok(())
}

/// Points each overridden role at its new URL, in both the preset field and the
/// genesis-keyed routing table, so the two cannot disagree. Overridden values
/// are leaked like the backend override.
fn apply_chain_overrides(mut config: NetworkConfig, overrides: ChainOverrides) -> NetworkConfig {
	let leak = |value: Option<String>| value.map(|url| &*Box::leak(url.into_boxed_str()));
	let routes = [
		(leak(overrides.people_ws), config.people_genesis),
		(leak(overrides.asset_hub_ws), config.asset_hub_genesis),
		(leak(overrides.bulletin_ws), config.bulletin_genesis),
	];
	if routes.iter().all(|(url, _)| url.is_none()) {
		return config;
	}
	let [(people, _), (asset_hub, _), (bulletin, _)] = routes;
	config.people_ws = people.unwrap_or(config.people_ws);
	config.asset_hub_ws = asset_hub.unwrap_or(config.asset_hub_ws);
	config.bulletin_ws = bulletin.unwrap_or(config.bulletin_ws);
	let endpoints: Vec<ChainEndpoint> = config
		.live_chain_endpoints
		.iter()
		.map(|endpoint| {
			let ws = routes
				.iter()
				.find_map(|(url, genesis)| (*genesis == endpoint.genesis).then_some(*url).flatten())
				.unwrap_or(endpoint.ws);
			ChainEndpoint { ws, ..*endpoint }
		})
		.collect();
	config.live_chain_endpoints = Box::leak(endpoints.into_boxed_slice());
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

const POLKADOT_ASSET_HUB: ChainEndpoint = ChainEndpoint {
	genesis: hex_literal_genesis(
		"68d56f15f85d3136970ec16946040bc1752654e906147f7e43e9d539d7c3de2f",
	),
	ws: "wss://polkadot-asset-hub-rpc.polkadot.io",
	required_for_host: true,
};

const POLKADOT_PEOPLE: ChainEndpoint = ChainEndpoint {
	genesis: hex_literal_genesis(
		"67fa177a097bfa18f77ea95ab56e9bcdfeb0e5b8a40e46298bb93e16b6fc5008",
	),
	ws: "wss://polkadot-people-rpc.polkadot.io",
	required_for_host: true,
};

const POLKADOT_BULLETIN: ChainEndpoint = ChainEndpoint {
	genesis: hex_literal_genesis(
		"2761c95259d59e55ae3daf756c1413b46e45a5a2987299f8ef8e5d8e4776cbc4",
	),
	ws: "wss://bulletin-rpc.polkadot.io",
	required_for_host: true,
};

/// The dotNS deployment on Polkadot Asset Hub, copied verbatim from
/// `deployments/polkadot/420420419.json` in paritytech/dotns at
/// `c3b291d287a28874f14f906ed61fb43162fc7caa` (paritytech/dotns#340). Replace
/// the file rather than editing it.
const POLKADOT_DOTNS_DEPLOYMENT: &str = include_str!("../deployments/polkadot-420420419.json");

/// `DotnsPopController`, read out of [`POLKADOT_DOTNS_DEPLOYMENT`] at compile
/// time. The preset carries it because Polkadot Asset Hub can leave
/// `DotnsGateway.DispatcherAddress` unset, and discovery then finds nothing.
const POLKADOT_DOTNS_POP_CONTROLLER: [u8; 20] =
	manifest_address(POLKADOT_DOTNS_DEPLOYMENT, "DotnsPopController");

const PASEO_NEXT_V2_CHAIN_ENDPOINTS: &[ChainEndpoint] =
	&[PASEO_ASSET_HUB, PASEO_PEOPLE, PASEO_BULLETIN];

const PREVIEWNET_CHAIN_ENDPOINTS: &[ChainEndpoint] =
	&[PREVIEWNET_ASSET_HUB, PREVIEWNET_PEOPLE, PREVIEWNET_BULLETIN];

const POLKADOT_CHAIN_ENDPOINTS: &[ChainEndpoint] =
	&[POLKADOT_ASSET_HUB, POLKADOT_PEOPLE, POLKADOT_BULLETIN];

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
	/// Identity backend base URL including `/api/v1`. `None` on a preset with
	/// no published backend, where [`IDENTITY_BACKEND_BASE_ENV`] must supply it.
	pub identity_backend_base: Option<&'static str>,
	pub people_ws: &'static str,
	pub bulletin_ws: &'static str,
	/// Asset Hub RPC, where PGAS allowances are claimed.
	pub asset_hub_ws: &'static str,
	pub people_genesis: [u8; 32],
	pub bulletin_genesis: [u8; 32],
	/// Asset Hub genesis hash, both the role's identity in
	/// [`NetworkConfig::host_chain_set`] and its routing key, and the genesis the
	/// CLI's Asset Hub extrinsics sign in `CheckGenesis`.
	///
	/// Wrong here is worse than stale elsewhere: routing matches on it, so a value
	/// the chain does not report sends Asset Hub traffic to the fallback chain.
	pub asset_hub_genesis: [u8; 32],
	pub live_chain_endpoints: &'static [ChainEndpoint],
	/// `DotnsPopController` to use when on-chain discovery finds none, for a
	/// network whose gateway dispatcher is not set yet. Discovery wins when it
	/// answers.
	pub dotns_pop_controller: Option<[u8; 20]>,
	/// Whether identities on this network are disposable test identities the
	/// plaintext account store may keep and auto-accept may sign for. Only
	/// test networks set it.
	pub disposable_identities: bool,
	/// How this run reaches the chains, from `--chain-backend`.
	pub chain_backend: ChainBackend,
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
	/// The identity backend base URL, or an error naming the override when the
	/// preset has none.
	pub fn identity_backend(&self) -> anyhow::Result<&'static str> {
		self.identity_backend_base.ok_or_else(|| {
			anyhow::anyhow!(
				"{} has no published identity backend; set {IDENTITY_BACKEND_BASE_ENV} to its \
				 base URL, including /api/v1",
				self.id
			)
		})
	}

	/// Refuses to write a mnemonic for this network to the plaintext account
	/// store unless its identities are disposable.
	pub fn ensure_disposable_identities(&self) -> anyhow::Result<()> {
		anyhow::ensure!(
			self.disposable_identities,
			"{} holds real identities, which the plaintext account store must not keep; \
			 pass the signer with --mnemonic or HOST_CLI_SIGNER_MNEMONIC",
			self.id
		);
		Ok(())
	}

	/// The chain set this preset serves, for `Features::supported_chains`.
	///
	/// One entry per role this struct names a genesis hash for. `ChainEndpoint`
	/// carries no role, so `live_chain_endpoints` cannot supply one — an endpoint's
	/// identifier is not recoverable from it, which is why a served role needs its
	/// own field here rather than a lookup into that list.
	///
	/// Every role listed must also be routable: `CliChainProvider` answers an
	/// unmapped genesis with People, so a role the routing filter drops would
	/// connect to the wrong chain while the host claimed to serve it.
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

	/// The chains this network builds as V5 general transactions: People and
	/// Asset Hub, both on pipeline 1, the one that declares their authorizers.
	/// The mobile apps read the same from remote config.
	pub fn extension_versions(&self) -> ExtensionVersionMap {
		ExtensionVersionMap {
			chains: vec![
				ChainExtensionVersion { genesis_hash: self.people_genesis, extension_version: 1 },
				ChainExtensionVersion {
					genesis_hash: self.asset_hub_genesis,
					extension_version: 1,
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

/// The H160 a dotNS deployment manifest records under `name`, decoded at
/// compile time from its `"<name>":"0x<40 hex>"` entry. Accepts the EIP-55
/// mixed case the manifests use, and fails the build when the entry is missing.
const fn manifest_address(manifest: &str, name: &str) -> [u8; 20] {
	let json = manifest.as_bytes();
	let name = name.as_bytes();
	let mut start = 0;
	while start + name.len() + 46 <= json.len() {
		let mut matches = json[start] == b'"';
		let mut i = 0;
		while matches && i < name.len() {
			matches = json[start + 1 + i] == name[i];
			i += 1;
		}
		let value = start + 1 + name.len();
		if matches &&
			json[value] == b'"' &&
			json[value + 1] == b':' &&
			json[value + 2] == b'"' &&
			json[value + 3] == b'0' &&
			json[value + 4] == b'x'
		{
			let hex = value + 5;
			let mut out = [0u8; 20];
			let mut byte = 0;
			while byte < 20 {
				out[byte] =
					hex_nibble(json[hex + byte * 2]) << 4 | hex_nibble(json[hex + byte * 2 + 1]);
				byte += 1;
			}
			return out;
		}
		start += 1;
	}
	panic!("the deployment manifest has no entry with that name");
}

const fn hex_nibble(c: u8) -> u8 {
	match c {
		b'0'..=b'9' => c - b'0',
		b'a'..=b'f' => c - b'a' + 10,
		b'A'..=b'F' => c - b'A' + 10,
		_ => panic!("invalid hex digit in a compile-time hex literal"),
	}
}

#[cfg(test)]
mod tests {
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
			ChainOverrides {
				people_ws: Some("ws://127.0.0.1:9".to_string()),
				..Default::default()
			},
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
			ChainOverrides {
				people_ws: Some(POLKADOT_PEOPLE.ws.to_string()),
				..Default::default()
			},
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
		let polkadot = NetworkConfig {
			chain_backend: ChainBackend::LightClient,
			..Network::Polkadot.preset()
		};
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
		assert_eq!(
			overridden.asset_hub_ws, preset.asset_hub_ws,
			"a blank override keeps the preset"
		);
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
}
