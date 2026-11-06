use clap::ValueEnum;
use trinity_stack::Descriptor;
use truapi::{
	latest::ChainIdentifier,
	platform::{ChainExtensionVersion, ExtensionVersionMap, HostChainEntry, HostChainSet},
};

use crate::{
	chain::ChainBackend,
	stack_descriptor::{ROLE_NAMES, Roles},
};

mod built_in;
mod checks;
mod polkadot_fork;
mod source;
mod stack;
#[cfg(test)]
mod testing;

/// Networks the headless hosts know by name, each a test network this build
/// ships a signed stack copy of. The CLI account store saves BIP-39 mnemonics
/// in plaintext (`accounts.rs`), which is only acceptable because every
/// network it serves has disposable test identities.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum Network {
	/// Read from the descriptor its operator publishes.
	#[value(name = "paseo-next-v2")]
	#[default]
	PaseoNextV2,
	/// Read from the descriptor its operator publishes. Its id is the id the
	/// playground and dotli already use for this network
	/// (`VITE_NETWORKS=paseo-next-v2,previewnet`), so `HostChainSet::network`
	/// reads the same string a product sees everywhere else.
	#[value(name = "previewnet")]
	Previewnet,
	/// PCF's devnet on Paseo, on the `dot` TLD, read from the descriptor PCF
	/// publishes.
	#[value(name = "pcf-devnet")]
	PcfDevnet,
}

/// Env var supplying `--stack` when the flag is absent; a blank value is unset.
pub const STACK_ENV: &str = "HOST_CLI_STACK";

/// Where a command's network comes from: `--chain-backend`, `--network` and
/// `--stack`.
#[derive(Clone)]
pub struct NetworkSource {
	/// How the command connects to the chains.
	pub chain_backend: ChainBackend,
	/// paseo-next-v2 when neither this nor `stack` is set.
	pub network: Option<Network>,
	/// The stack to run against instead of a built-in network.
	pub stack: Option<StackArg>,
}

/// A `--stack` value and where it came from, for error messages.
#[derive(Clone)]
pub struct StackArg {
	/// A descriptor URL, a stack link or a local descriptor file.
	pub value: String,
	/// `--stack` or [`STACK_ENV`].
	pub source: &'static str,
}

/// The stack a command runs against, connected to over `--chain-backend`.
/// Built only by [`Stack::new`], which checks every field the accessors read,
/// so none of them fails.
#[derive(Debug, Clone)]
pub struct Stack {
	descriptor: Descriptor,
	chain_backend: ChainBackend,
}

/// A chain's genesis hash and the WebSocket RPC URL to connect to it at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainEndpoint {
	pub genesis: [u8; 32],
	pub ws: String,
}

impl Stack {
	/// The product namespace: what `HostChainSet::network` reports and where
	/// the CLI stores this network's state. The descriptor's `id` without a
	/// trailing `-fork`, so a fork shares its source's state, except that a
	/// `polkadot-fork` keeps its full `id`.
	pub fn namespace(&self) -> &str {
		stack::namespace_of(&self.descriptor.id)
	}

	/// The network's dotNS TLD without the dot, as its runtimes report it in
	/// `NetworkSuffix.NetworkSuffix`. Every reserved RFC-0022 identity the CLI
	/// derives ends in it (`uid.<tld>`, `peopl.<tld>`), so a person the CLI
	/// creates here is the same person a phone derives from that seed on this
	/// network.
	pub fn tld(&self) -> &str {
		&self.descriptor.tld
	}

	/// Each role's genesis hash: both the role's identity in
	/// [`Stack::host_chain_set`] and its routing key. Asset Hub's is also the
	/// genesis the CLI's Asset Hub extrinsics sign in `CheckGenesis`.
	pub fn genesis(&self) -> Roles<[u8; 32]> {
		ROLE_NAMES.map(|role| self.chain(role).genesis())
	}

	/// Each role's endpoint: its genesis hash and first RPC URL.
	pub fn chains(&self) -> Roles<ChainEndpoint> {
		ROLE_NAMES.map(|role| {
			let chain = self.chain(role);
			ChainEndpoint { genesis: chain.genesis(), ws: chain.rpc[0].clone() }
		})
	}

	/// Identity backend base URL including `/api/v1`, from the stack's `dub`
	/// service.
	pub fn identity_backend(&self) -> String {
		let dub = self.descriptor.service("dub").expect("stack without a dub service");
		format!("{}/api/v1", dub.urls[0].trim_end_matches('/'))
	}

	/// The `DotnsPopController` address, the stack's `dotns.popController`.
	pub fn pop_controller(&self) -> [u8; 20] {
		self.descriptor
			.dotns_address("popController")
			.expect("stack without a dotns.popController")
	}

	/// How this run reaches the chains, from `--chain-backend`.
	pub fn chain_backend(&self) -> ChainBackend {
		self.chain_backend
	}

	/// The chain set this network serves, for `Features::supported_chains`.
	pub fn host_chain_set(&self) -> HostChainSet {
		let genesis = self.genesis();
		HostChainSet {
			network: self.namespace().to_owned(),
			chains: vec![
				HostChainEntry {
					identifier: ChainIdentifier::People,
					genesis_hash: genesis.people,
				},
				HostChainEntry {
					identifier: ChainIdentifier::Bulletin,
					genesis_hash: genesis.bulletin,
				},
				HostChainEntry {
					identifier: ChainIdentifier::AssetHub,
					genesis_hash: genesis.asset_hub,
				},
			],
		}
	}

	/// The chains this network builds as V5 general transactions: People and
	/// Asset Hub, both on pipeline 1, the one that declares their authorizers.
	/// The mobile apps read the same from remote config.
	pub fn extension_versions(&self) -> ExtensionVersionMap {
		let genesis = self.genesis();
		ExtensionVersionMap {
			chains: vec![
				ChainExtensionVersion { genesis_hash: genesis.people, extension_version: 1 },
				ChainExtensionVersion { genesis_hash: genesis.asset_hub, extension_version: 1 },
			],
		}
	}

	/// Whether `other` has the same value in every field the CLI reads: the
	/// TLD, `generation`, the `dub` service, `dotns.popController`, and each
	/// chain's role, genesis hash and RPC list, which the Polkadot fork check
	/// tries in order.
	pub fn reads_same(&self, other: &Self) -> bool {
		fn chain_fields(stack: &Descriptor) -> impl Iterator<Item = (&str, &str, &[String])> {
			stack.chains.iter().map(|chain| {
				(chain.role.as_str(), chain.genesis_hash.as_str(), chain.rpc.as_slice())
			})
		}
		let (this, other) = (&self.descriptor, &other.descriptor);
		this.tld == other.tld &&
			this.generation == other.generation &&
			this.service("dub") == other.service("dub") &&
			this.dotns_address("popController") == other.dotns_address("popController") &&
			chain_fields(this).eq(chain_fields(other))
	}

	/// The chain with `role`, which [`Stack::new`] checked is present.
	fn chain(&self, role: &str) -> &trinity_stack::Chain {
		self.descriptor.chain(role).expect("stack without a role chain")
	}

	/// The endpoint of a served role; `None` for the relay chain, which no
	/// network routes.
	#[cfg(test)]
	pub fn endpoint_for_role(&self, role: ChainIdentifier) -> Option<ChainEndpoint> {
		let chains = self.chains();
		match role {
			ChainIdentifier::People => Some(chains.people),
			ChainIdentifier::Bulletin => Some(chains.bulletin),
			ChainIdentifier::AssetHub => Some(chains.asset_hub),
			ChainIdentifier::Relay => None,
		}
	}

	/// The descriptor this stack reads.
	#[cfg(test)]
	pub fn descriptor(&self) -> &Descriptor {
		&self.descriptor
	}

	/// This stack with its descriptor edited as JSON.
	#[cfg(test)]
	pub fn edited(&self, edit: impl FnOnce(&mut serde_json::Value)) -> Self {
		let mut document = serde_json::to_value(&self.descriptor).expect("descriptor not JSON");
		edit(&mut document);
		let descriptor = Descriptor::from_json(document).expect("invalid edited descriptor");
		Self::new(descriptor, self.chain_backend).expect("edited stack refused")
	}

	/// This stack with its `dub` service at `url`, so its identity backend is
	/// `url` with `/api/v1` appended.
	#[cfg(test)]
	pub fn with_dub(&self, url: &str) -> Self {
		self.edited(|descriptor| {
			let services = descriptor["services"].as_array_mut().expect("services not an array");
			for service in services.iter_mut().filter(|service| service["kind"] == "dub") {
				service["urls"] = serde_json::json!([url]);
			}
		})
	}

	/// This stack with every chain's RPC list set to `url` alone.
	#[cfg(test)]
	pub fn with_rpc(&self, url: &str) -> Self {
		self.edited(|descriptor| {
			for chain in descriptor["chains"].as_array_mut().expect("chains not an array") {
				chain["rpc"] = serde_json::json!([url]);
			}
		})
	}
}

#[cfg(test)]
mod tests;
