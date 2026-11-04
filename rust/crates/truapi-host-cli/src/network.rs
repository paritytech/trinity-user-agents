use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail, ensure};
use clap::ValueEnum;
use trinity_stack::{Descriptor, Fetched, Pointer, Status, accept};
use truapi::{
	latest::ChainIdentifier,
	platform::{
		ChainExtensionVersion, DOTNS_TLDS, ExtensionVersionMap, HostChainEntry, HostChainSet,
	},
};

use crate::chain::ChainBackend;

/// Networks the headless hosts know by name.
///
/// The CLI account store keeps BIP-39 mnemonics in plaintext (`accounts.rs`),
/// which is only acceptable for disposable test identities. A network that sets
/// [`NetworkConfig::disposable_identities`] may use that store; any other
/// network only signs with a mnemonic supplied per process, never writes one,
/// and never approves a confirmation without a prompt.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum Network {
	/// Built-in stack, read from the descriptor its operator publishes.
	#[value(name = "paseo-next-v2")]
	#[default]
	PaseoNextV2,
	/// Built-in stack, read from the descriptor its operator publishes. Its id is
	/// the id the playground and dotli already use for this network
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

/// Env var supplying `--stack` when the flag is absent; a blank value is unset.
pub const STACK_ENV: &str = "HOST_CLI_STACK";

/// Env vars overriding one chain's WebSocket URL, for a fork of the resolved
/// network. The genesis hashes stay the network's, as a fork has the same ones.
pub const PEOPLE_WS_ENV: &str = "HOST_CLI_PEOPLE_WS";
pub const ASSET_HUB_WS_ENV: &str = "HOST_CLI_ASSET_HUB_WS";
pub const BULLETIN_WS_ENV: &str = "HOST_CLI_BULLETIN_WS";

/// Chain URL overrides; `None` keeps the network's URL.
#[derive(Debug, Default)]
struct ChainOverrides {
	people_ws: Option<String>,
	asset_hub_ws: Option<String>,
	bulletin_ws: Option<String>,
}

impl ChainOverrides {
	fn from_env() -> Result<Self> {
		Self::parse(
			[PEOPLE_WS_ENV, ASSET_HUB_WS_ENV, BULLETIN_WS_ENV].map(|name| std::env::var(name).ok()),
		)
	}

	/// Blank values keep the network's URL. Anything else must be a `ws` or
	/// `wss` URL.
	fn parse([people_ws, asset_hub_ws, bulletin_ws]: [Option<String>; 3]) -> Result<Self> {
		let url = |name: &str, value: Option<String>| -> Result<Option<String>> {
			let Some(value) = value.map(|value| value.trim().to_string()).filter(|v| !v.is_empty())
			else {
				return Ok(None);
			};
			let url = reqwest::Url::parse(&value)
				.map_err(|err| anyhow!("{name} is not a URL ({err}): {value}"))?;
			ensure!(
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

/// How long a built-in stack's published descriptor may take before the
/// bundled copy is used instead.
const BUILT_IN_REFRESH_TIMEOUT: Duration = Duration::from_secs(3);

/// A stack this build ships a signed copy of, so a command starts without the
/// published descriptor when it is unreachable. `scripts/update-stacks.sh`
/// refreshes `stacks/<name>.json` from the link in `stacks/<name>.link`.
struct BuiltInStack {
	link: &'static str,
	bundle: &'static [u8],
}

impl BuiltInStack {
	fn pointer(&self) -> Pointer {
		Pointer::from_link(self.link.trim()).expect("invalid built-in stack link")
	}
}

const PASEO_NEXT_V2_STACK: BuiltInStack = BuiltInStack {
	link: include_str!("../stacks/paseo-next-v2.link"),
	bundle: include_bytes!("../stacks/paseo-next-v2.json"),
};

const PREVIEWNET_STACK: BuiltInStack = BuiltInStack {
	link: include_str!("../stacks/previewnet.link"),
	bundle: include_bytes!("../stacks/previewnet.json"),
};

/// Where a command's network comes from: `--chain-backend`, and `--network`
/// or `--stack`.
#[derive(Debug, Clone)]
pub struct NetworkSource {
	pub chain_backend: ChainBackend,
	pub selection: NetworkSelection,
}

/// A built-in network by name, or any other stack.
#[derive(Debug, Clone)]
pub enum NetworkSelection {
	BuiltIn(Network),
	/// A descriptor URL, a stack link or a local descriptor file.
	Stack(String),
}

impl NetworkSource {
	/// The network a command runs against: the built-in network or the stack,
	/// then `HOST_CLI_*_WS` or `HOST_CLI_LOCAL_NETWORK_CONFIG` on top.
	///
	/// Fails when a `--stack` descriptor is unreachable, when the descriptor in
	/// use is expired or lacks a role, when a disposable-identity network
	/// carries Polkadot's genesis hashes, TLD or namespace, when the overrides
	/// cannot be combined, when the light client cannot serve the network, and
	/// when an endpoint of a `--stack` or an override reports a genesis other
	/// than the one it is routed as. A built-in network whose published
	/// descriptor is unreachable runs on its bundled copy.
	pub async fn cli_config(&self) -> Result<NetworkConfig> {
		let (config, unverified) = self.resolve_routes().await?;
		verify_endpoint_genesis(&config, &unverified).await?;
		Ok(config)
	}

	/// [`Self::cli_config`] without contacting any chain; only a descriptor is
	/// fetched. An unreachable descriptor fails only for `--stack`.
	pub async fn resolve(&self) -> Result<NetworkConfig> {
		Ok(self.resolve_routes().await?.0)
	}

	/// The network and the endpoints no signed source vouches for: every
	/// endpoint of a `--stack`, or those an override reroutes.
	async fn resolve_routes(&self) -> Result<(NetworkConfig, Vec<&'static ChainEndpoint>)> {
		let overrides = ChainOverrides::from_env()?;
		let set = overrides.set_names();
		let (base, stack) = match &self.selection {
			NetworkSelection::Stack(stack) => {
				ensure!(
					crate::local_network::config_path().is_none(),
					"{} and --stack cannot be used together: both route every chain; unset one \
					 of them",
					crate::local_network::CONFIG_ENV
				);
				let descriptor = crate::stack::load_descriptor(stack).await?;
				(stack_network_config(&descriptor)?, Some(stack.as_str()))
			},
			NetworkSelection::BuiltIn(network) => (network.resolve_built_in().await?, None),
		};
		let mut config = apply_chain_overrides(
			apply_backend_override(base, std::env::var(IDENTITY_BACKEND_BASE_ENV).ok()),
			overrides,
		);
		config.chain_backend = self.chain_backend;
		refuse_light_client(&config, stack, &set)?;
		let config = crate::local_network::resolve(config, &set)?;
		ensure_no_real_identity_values(&config)?;
		let unverified = config
			.live_chain_endpoints
			.iter()
			.filter(|endpoint| {
				stack.is_some() ||
					!base.live_chain_endpoints.iter().any(|chain| chain.ws == endpoint.ws)
			})
			.collect();
		Ok((config, unverified))
	}
}

impl Network {
	/// The product namespace this network runs under.
	pub fn id(self) -> &'static str {
		match self {
			Self::PaseoNextV2 => "paseo-next-v2",
			Self::Previewnet => "previewnet",
			Self::Polkadot => POLKADOT.id,
		}
	}

	fn built_in_stack(self) -> Option<BuiltInStack> {
		match self {
			Self::PaseoNextV2 => Some(PASEO_NEXT_V2_STACK),
			Self::Previewnet => Some(PREVIEWNET_STACK),
			Self::Polkadot => None,
		}
	}

	/// The values compiled into this build, without fetching anything: a
	/// built-in stack's bundled copy, or Polkadot's pinned values.
	#[cfg(test)]
	pub fn config(self) -> NetworkConfig {
		let Some(built_in) = self.built_in_stack() else { return POLKADOT };
		let bundled = trinity_stack::open(built_in.bundle, &built_in.pointer())
			.expect("invalid bundled stack");
		stack_config(&bundled.descriptor, self.id()).expect("invalid bundled stack")
	}

	/// For a built-in stack, the published descriptor when it is newer than the
	/// bundled copy, otherwise the bundled copy; Polkadot's pinned values
	/// otherwise. Logs a notice when the published descriptor is newer.
	async fn resolve_built_in(self) -> Result<NetworkConfig> {
		let Some(built_in) = self.built_in_stack() else { return Ok(POLKADOT) };
		let pointer = built_in.pointer();
		let fetched = crate::stack::fetch(pointer.url().as_str(), BUILT_IN_REFRESH_TIMEOUT).await;
		if let Err(cause) = &fetched {
			tracing::debug!("{} stack descriptor is unreachable: {cause}", self.id());
		}
		let fetched = fetched.as_deref().map_or(Fetched::Unreachable, Fetched::Document);
		let now = crate::stack::unix_now() as i64;
		let refresh = refresh_stack(self.id(), &pointer, built_in.bundle, fetched, now)?;
		match &refresh.notice {
			Some(notice) if notice.new_generation => tracing::warn!("{notice}"),
			Some(notice) => tracing::info!("{notice}"),
			None => {},
		}
		stack_config(&refresh.descriptor, self.id())
	}
}

/// The descriptor a built-in stack runs on, and the notice to print about it.
struct StackRefresh {
	descriptor: Descriptor,
	notice: Option<StackNotice>,
}

/// A published descriptor newer than the copy this build ships.
struct StackNotice {
	id: String,
	bundle_sequence: u32,
	remote_sequence: u32,
	/// The stack restarted its chains from genesis since the bundled copy.
	new_generation: bool,
}

impl std::fmt::Display for StackNotice {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let Self { id, bundle_sequence, remote_sequence, .. } = self;
		if self.new_generation {
			write!(
				formatter,
				"the {id} stack restarted from genesis: its published descriptor (sequence \
				 {remote_sequence}) is in use instead of the copy this truapi-host ships \
				 (sequence {bundle_sequence}), whose chains are gone; a later truapi-host \
				 release ships the new descriptor"
			)
		} else {
			write!(
				formatter,
				"the published {id} stack descriptor (sequence {remote_sequence}) is newer than \
				 the copy this truapi-host ships (sequence {bundle_sequence}) and is in use; a \
				 later truapi-host release ships it"
			)
		}
	}
}

/// The descriptor a built-in stack runs on: `fetched` when [`accept`] takes it
/// over `bundled` and it has the bundled copy's `tld`, otherwise `bundled`,
/// with a notice when `fetched` is in use. Fails when that descriptor is past
/// `expiresAt`.
fn refresh_stack(
	id: &str,
	pointer: &Pointer,
	bundled: &[u8],
	fetched: Fetched,
	now_unix_seconds: i64,
) -> Result<StackRefresh> {
	let bundled_copy = accept(pointer, None, Fetched::Document(bundled), now_unix_seconds)
		.map_err(|error| anyhow!("the bundled {id} stack does not open: {error}"))?;
	let accepted = accept(pointer, Some(bundled), fetched, now_unix_seconds)
		.map_err(|error| anyhow!("the {id} stack has no usable descriptor: {error}"))?;
	match &accepted.status {
		Status::Updated | Status::Unchanged | Status::Unreachable => {},
		Status::Refused(error) => tracing::warn!(
			"the published {id} stack descriptor is refused ({}): {error}; using the bundled copy",
			error.code()
		),
		status => tracing::warn!(
			"the published {id} stack descriptor is a {}; using the bundled copy",
			status.code()
		),
	}
	let bundle_sequence = bundled_copy.stack.descriptor.sequence;
	let accepted = if accepted.stack.descriptor.tld == bundled_copy.stack.descriptor.tld {
		accepted
	} else {
		tracing::warn!(
			"the published {id} stack descriptor changes the TLD from {} to {}, which would \
			 change every account derived under it; using the bundled copy",
			bundled_copy.stack.descriptor.tld,
			accepted.stack.descriptor.tld
		);
		bundled_copy
	};
	ensure!(!accepted.expired, "the {id} stack is past expiresAt; it has shut down");
	if accepted.stale {
		tracing::warn!("the {id} stack descriptor in use is past validUntil");
	}
	let descriptor = accepted.stack.descriptor;
	let notice = (descriptor.sequence != bundle_sequence).then(|| StackNotice {
		id: id.to_owned(),
		bundle_sequence,
		remote_sequence: descriptor.sequence,
		new_generation: accepted.new_generation,
	});
	Ok(StackRefresh { descriptor, notice })
}

/// A `--stack` descriptor's network, under the product namespace of the
/// descriptor's `id` without a trailing `-fork`.
fn stack_network_config(descriptor: &Descriptor) -> Result<NetworkConfig> {
	let id = descriptor.id.strip_suffix("-fork").unwrap_or(&descriptor.id);
	ensure!(
		truapi::platform::is_dns_label(id),
		"the stack id {:?} is not a lowercase label to use as the product namespace",
		descriptor.id
	);
	stack_config(descriptor, leak_str(id))
}

/// A stack's People, Asset Hub and Bulletin chains, identity backend, TLD and
/// dotNS `popController` under the product namespace `id`, with disposable
/// identities. Fails when a role is missing, when two roles share a genesis
/// hash, and when the TLD is not one the core serves.
fn stack_config(descriptor: &Descriptor, id: &'static str) -> Result<NetworkConfig> {
	let endpoint = |role: &str| -> Result<ChainEndpoint> {
		let chain = descriptor
			.chain(role)
			.with_context(|| format!("the {} stack has no {role} chain", descriptor.id))?;
		let mut genesis = [0u8; 32];
		hex::decode_to_slice(chain.genesis_hash.trim_start_matches("0x"), &mut genesis)
			.expect("invalid genesisHash in a validated descriptor");
		Ok(ChainEndpoint { genesis, ws: leak_str(&chain.rpc[0]), required_for_host: true })
	};
	let (asset_hub, people, bulletin) =
		(endpoint("AssetHub")?, endpoint("People")?, endpoint("Bulletin")?);
	ensure!(
		asset_hub.genesis != people.genesis &&
			asset_hub.genesis != bulletin.genesis &&
			people.genesis != bulletin.genesis,
		"the {} stack gives two of People, Asset Hub and Bulletin the same genesis hash; the \
		 host routes each role by its genesis",
		descriptor.id
	);
	let network_suffix =
		DOTNS_TLDS.iter().find(|tld| **tld == descriptor.tld).with_context(|| {
			format!(
				"the {} stack's tld {} is not one the core serves ({})",
				descriptor.id,
				descriptor.tld,
				DOTNS_TLDS.join(", ")
			)
		})?;
	let identity_backend_base = descriptor
		.services
		.iter()
		.find(|service| service.kind == "dub")
		.and_then(|service| service.urls.first())
		.map(|url| leak_str(&format!("{}/api/v1", url.trim_end_matches('/'))));
	let dotns_pop_controller = descriptor
		.dotns
		.as_ref()
		.and_then(|dotns| dotns.get("popController"))
		.map(|address| {
			let mut bytes = [0u8; 20];
			hex::decode_to_slice(address.trim_start_matches("0x"), &mut bytes)
				.expect("invalid dotns address in a validated descriptor");
			bytes
		});
	Ok(NetworkConfig {
		id,
		network_suffix,
		identity_backend_base,
		people_ws: people.ws,
		bulletin_ws: bulletin.ws,
		asset_hub_ws: asset_hub.ws,
		people_genesis: people.genesis,
		bulletin_genesis: bulletin.genesis,
		asset_hub_genesis: asset_hub.genesis,
		live_chain_endpoints: Box::leak(Box::new([asset_hub, people, bulletin])),
		dotns_pop_controller,
		disposable_identities: true,
		chain_backend: ChainBackend::Rpc,
	})
}

/// `NetworkConfig` is `Copy` with process-lifetime fields; the CLI resolves a
/// network a handful of times per process.
fn leak_str(value: &str) -> &'static str {
	Box::leak(value.to_owned().into_boxed_str())
}

/// The light client knows only the networks `truapi-provider` bundles, by
/// genesis hash, so it can serve neither another network nor a `--stack`, nor
/// a rerouted chain, nor a built-in stack that restarted after this build's
/// chain specs.
fn refuse_light_client(
	config: &NetworkConfig,
	stack: Option<&str>,
	chain_overrides: &[&str],
) -> Result<()> {
	if config.chain_backend != ChainBackend::LightClient {
		return Ok(());
	}
	if let Some(stack) = stack {
		let bundled = truapi_provider::known_networks().collect::<Vec<_>>().join(", ");
		bail!(
			"--chain-backend light-client cannot run --stack {stack}: the light client syncs only \
			 the networks truapi-provider bundles ({bundled}); use --chain-backend rpc"
		);
	}
	ensure!(
		chain_overrides.is_empty(),
		"{} and --chain-backend light-client cannot be used together: the light client syncs \
		 the live chains by genesis hash; unset one of them",
		chain_overrides.join(", ")
	);
	let (_, catalog) = truapi_provider::EmbeddedChainProviderBuilder::new()
		.add_network(config.id)
		.map_err(|error| {
			anyhow!("--chain-backend light-client has no chain specs for {}: {error}", config.id)
		})?;
	ensure!(
		(catalog.people, catalog.assethub, catalog.bulletin) ==
			(config.people_genesis, config.asset_hub_genesis, config.bulletin_genesis),
		"this build's light-client chain specs for {} predate the stack's restart from genesis; \
		 use --chain-backend rpc, or `truapi-host update` once a release ships the new specs",
		config.id
	);
	Ok(())
}

/// Replaces the identity backend base with `base` when it carries a non-empty
/// URL. Trailing slashes are stripped so path joins stay clean.
fn apply_backend_override(mut config: NetworkConfig, base: Option<String>) -> NetworkConfig {
	if let Some(base) = base {
		let trimmed = base.trim().trim_end_matches('/');
		if !trimmed.is_empty() {
			config.identity_backend_base = Some(leak_str(trimmed));
		}
	}
	config
}

/// Points each overridden role at its new URL, in both the role field and the
/// genesis-keyed routing table, so the two cannot disagree.
fn apply_chain_overrides(mut config: NetworkConfig, overrides: ChainOverrides) -> NetworkConfig {
	let leak = |value: Option<String>| value.map(|url| leak_str(&url));
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

/// Fails for a disposable-identity network that carries Polkadot's genesis
/// hashes, its `dot` TLD or its product namespace. A fork of mainnet has
/// mainnet's genesis, and a signature made against it is valid on mainnet
/// itself; mainnet's namespace contains its real-identity sessions.
fn ensure_no_real_identity_values(config: &NetworkConfig) -> Result<()> {
	if !config.disposable_identities {
		return Ok(());
	}
	ensure!(
		config.id != POLKADOT.id,
		"a stack cannot run under the {} namespace, whose identities are real; give the \
		 stack another id",
		POLKADOT.id
	);
	ensure!(
		config.network_suffix != POLKADOT.network_suffix,
		"{} uses Polkadot's TLD .{}, whose identities are real; point it at a test network",
		config.id,
		config.network_suffix
	);
	for endpoint in config.live_chain_endpoints {
		ensure!(
			!POLKADOT_CHAIN_ENDPOINTS.iter().any(|chain| chain.genesis == endpoint.genesis),
			"{} would route to {}'s chain at {}, whose identities are real; point it at a test \
			 network",
			config.id,
			POLKADOT.id,
			endpoint.ws
		);
	}
	Ok(())
}

/// Fails when one of `endpoints` reports a genesis other than the one it is
/// routed as; an endpoint that does not answer is logged and left to fail on
/// first use.
async fn verify_endpoint_genesis(
	config: &NetworkConfig,
	endpoints: &[&'static ChainEndpoint],
) -> Result<()> {
	use truapi::statement_allowance as alloc;

	let checks = endpoints.iter().map(|&endpoint| async move {
		let provider = crate::chain::CliChainProvider::for_endpoint(config, endpoint);
		let reported = async {
			let client =
				provider.rpc_client(endpoint.genesis).await.map_err(|error| error.to_string())?;
			alloc::fetch_genesis_hash(&alloc::rpc::RpcClient::new(client))
				.await
				.map_err(|error| error.to_string())
		}
		.await
		.inspect_err(|error| {
			tracing::warn!("cannot check the genesis of {}: {error}", endpoint.ws)
		});
		let Ok(reported) = reported else { return Ok(()) };
		ensure!(
			reported == endpoint.genesis,
			"{} reports genesis 0x{}, but {} routes it as the chain with genesis 0x{}",
			endpoint.ws,
			hex::encode(reported),
			config.id,
			hex::encode(endpoint.genesis)
		);
		Ok(())
	});
	futures::future::try_join_all(checks).await?;
	Ok(())
}

// Each Polkadot chain is declared once and reused by both the network fields
// and the endpoint table, so a genesis hash cannot drift from the URL it
// routes to. Every route below is host-required: session identity (dotNS
// usernames) resolves through Asset Hub, SSO through People, preimages through
// Bulletin.

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
/// time. Polkadot carries it because its Asset Hub can leave
/// `DotnsGateway.DispatcherAddress` unset, and discovery then finds nothing.
const POLKADOT_DOTNS_POP_CONTROLLER: [u8; 20] =
	manifest_address(POLKADOT_DOTNS_DEPLOYMENT, "DotnsPopController");

const POLKADOT_CHAIN_ENDPOINTS: &[ChainEndpoint] =
	&[POLKADOT_ASSET_HUB, POLKADOT_PEOPLE, POLKADOT_BULLETIN];

const POLKADOT: NetworkConfig = NetworkConfig {
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
};

/// Resolved RPC/backend/genesis values for one network.
#[derive(Debug, Clone, Copy)]
pub struct NetworkConfig {
	/// The product namespace: what `HostChainSet::network` reports and where
	/// the CLI stores this network's state.
	pub id: &'static str,
	/// The network's dotNS TLD without the dot, as its runtimes report it in
	/// `NetworkSuffix.NetworkSuffix`. Every reserved RFC-0022 identity the CLI
	/// derives ends in it (`uid.<suffix>`, `peopl.<suffix>`), so a person the
	/// CLI creates here is the same person a phone derives from that seed on
	/// this network. `live_people_chain::network_suffix_matches_the_preset`
	/// holds it against the chain.
	pub network_suffix: &'static str,
	/// Identity backend base URL including `/api/v1`. `None` on a network with
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
	/// network has none.
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

	/// The chain set this network serves, for `Features::supported_chains`.
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

	/// The network's own URL for a served role, independent of the genesis-keyed
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
		const SPEC: &str = include_str!("../SPEC.md");

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
	/// plaintext account store keep its identities must route only to test
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
				let ws =
					config.url_for_role(entry.identifier).expect("every served role has a URL");
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
		let Ok(Document::Envelope { payload, .. }) = Document::parse(PASEO_NEXT_V2_STACK.bundle)
		else {
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

	/// A loopback WebSocket node that answers every request with `genesis`.
	async fn serve_genesis(genesis: [u8; 32]) -> String {
		use futures_util::{SinkExt, StreamExt};
		use tokio_tungstenite::tungstenite::Message;

		let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
			.await
			.expect("loopback bind failed");
		let url = format!("ws://{}", listener.local_addr().expect("no listener address"));
		tokio::spawn(async move {
			loop {
				let (socket, _) = listener.accept().await.expect("accept failed");
				tokio::spawn(async move {
					let mut socket =
						tokio_tungstenite::accept_async(socket).await.expect("handshake failed");
					while let Some(Ok(Message::Text(request))) = socket.next().await {
						let request: serde_json::Value =
							serde_json::from_str(&request).expect("request is not JSON");
						let response = serde_json::json!({
							"jsonrpc": "2.0",
							"id": request["id"],
							"result": format!("0x{}", hex::encode(genesis)),
						});
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

	/// An endpoint that does not answer is not evidence of a wrong stack, so it
	/// is left to fail on first use.
	#[tokio::test]
	async fn test_stack_endpoint_unreachable() {
		let config = stack_network_config(&stack_on_closed_ports()).unwrap();
		let endpoints: Vec<_> = config.live_chain_endpoints.iter().collect();
		verify_endpoint_genesis(&config, &endpoints)
			.await
			.expect("unreachable endpoint rejected");
	}

	/// The case the endpoint check exists for: an endpoint that answers as a
	/// different chain from the genesis the descriptor declares.
	#[tokio::test]
	async fn test_stack_endpoint_genesis_mismatch() {
		let url = serve_genesis([0x5a; 32]).await;
		let mut descriptor = stack_on_closed_ports();
		descriptor
			.chains
			.iter_mut()
			.find(|chain| chain.role == "People")
			.expect("People missing")
			.rpc = vec![url.clone()];
		let config = stack_network_config(&descriptor).unwrap();
		let endpoints: Vec<_> = config.live_chain_endpoints.iter().collect();
		let error = verify_endpoint_genesis(&config, &endpoints)
			.await
			.expect_err("wrong genesis accepted");
		assert!(error.to_string().contains(&url));
		assert!(error.to_string().contains("reports genesis"));
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
		assert!(error.to_string().contains(IDENTITY_BACKEND_BASE_ENV), "{error}");

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

	/// The light client has chain specs only for the networks truapi-provider
	/// bundles, so a `--stack`, whatever its namespace, fails before the command
	/// starts rather than on every connect.
	#[test]
	fn test_light_client_with_stack_error() {
		let rpc = Network::PaseoNextV2.config();
		let light = NetworkConfig { chain_backend: ChainBackend::LightClient, ..rpc };
		let error =
			refuse_light_client(&light, Some("https://stack.example/s.json"), &[]).unwrap_err();
		assert!(error.to_string().contains("--stack"));
		assert!(refuse_light_client(&light, None, &[]).is_ok());
		assert!(refuse_light_client(&rpc, Some("https://stack.example/s.json"), &[]).is_ok());
	}

	/// The light client syncs the chains by genesis from its bundled specs, so
	/// it cannot follow a chain rerouted to another URL.
	#[test]
	fn test_light_client_with_overrides_error() {
		let rpc = Network::PaseoNextV2.config();
		let light = NetworkConfig { chain_backend: ChainBackend::LightClient, ..rpc };
		let error = refuse_light_client(&light, None, &[PEOPLE_WS_ENV]).unwrap_err();
		assert!(error.to_string().contains(PEOPLE_WS_ENV));
		assert!(refuse_light_client(&rpc, None, &[PEOPLE_WS_ENV]).is_ok());
	}

	#[test]
	fn test_chain_overrides_blank_unset() {
		let overrides =
			ChainOverrides::parse([None, Some(String::new()), Some("  ".to_string())]).unwrap();
		assert!(overrides.set_names().is_empty());
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
			assert_eq!(overrides.set_names(), [PEOPLE_WS_ENV]);
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
		let error = refuse_light_client(&restarted, None, &[]).unwrap_err();
		assert!(error.to_string().contains("--chain-backend rpc"));
	}

	#[test]
	fn test_light_client_unknown_network_error() {
		let polkadot = NetworkConfig {
			chain_backend: ChainBackend::LightClient,
			..Network::Polkadot.config()
		};
		let error = refuse_light_client(&polkadot, None, &[]).unwrap_err();
		assert!(error.to_string().contains("no chain specs for polkadot"), "{error}");
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
			refuse_light_client(&light, None, &[]).expect("light-client catalog differs");
		}
	}

	/// A newer published copy is what the network runs on, and the user learns
	/// that it is in use ahead of the release that ships it.
	#[test]
	fn test_refresh_newer_notice() {
		let published = sign_copy(11, "g1");
		let refresh = refresh(&sign_copy(10, "g1"), Fetched::Document(&published)).unwrap();
		assert_eq!(refresh.descriptor.sequence, 11);
		let notice = refresh.notice.expect("newer copy not reported");
		assert_eq!((notice.bundle_sequence, notice.remote_sequence), (10, 11));
		assert!(!notice.new_generation);
		let text = notice.to_string();
		assert!(text.contains("10") && text.contains("11") && text.contains("in use"));
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
	}

	/// A signed copy with a lower sequence is a replay of an older descriptor.
	#[test]
	fn test_refresh_rollback_bundle() {
		let older = sign_copy(9, "g0");
		let refresh = refresh(&sign_copy(10, "g1"), Fetched::Document(&older)).unwrap();
		assert_eq!(refresh.descriptor.sequence, 10);
		assert!(refresh.notice.is_none());
	}

	#[test]
	fn test_refresh_unreachable_bundle() {
		let refresh = refresh(&sign_copy(10, "g1"), Fetched::Unreachable).unwrap();
		assert_eq!(refresh.descriptor.sequence, 10);
		assert!(refresh.notice.is_none());
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
}
