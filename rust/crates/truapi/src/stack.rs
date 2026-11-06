//! The Trinity stack a host runs, the core's only source of chain
//! configuration.
//!
//! A host build bundles a signed descriptor and hands it to
//! [`Stack::bundled`] with the stack's pointer. The resolved [`Stack`] answers
//! `get_chain_info`, names the People, Bulletin and Asset Hub chains the core
//! talks to, and carries the TLD the signing role derives identities under.

use trinity_stack::{Chain, Pointer, open};
use truapi::latest::{
	ChainIdentifier, RemoteChainInfoError, RemoteChainInfoRequest, RemoteChainInfoResponse,
};

use crate::dotns_name::DOTNS_TLDS;

/// Polkadot's Relay, Asset Hub, People and Bulletin genesis hashes, in
/// lowercase hex, from PCF's signed Polkadot descriptor. A transaction signed
/// for a chain with one of them can also be valid on Polkadot itself.
const RESERVED_GENESIS_HASHES: [&str; 4] = [
	"91b171bb158e2d3848fa23a9f1c25182fb8e20313b2c1eb49219da7a70ce90c3",
	"68d56f15f85d3136970ec16946040bc1752654e906147f7e43e9d539d7c3de2f",
	"67fa177a097bfa18f77ea95ab56e9bcdfeb0e5b8a40e46298bb93e16b6fc5008",
	"2761c95259d59e55ae3daf756c1413b46e45a5a2987299f8ef8e5d8e4776cbc4",
];

/// Polkadot's dotNS TLD. The RFC-0022 identities derived under it are a
/// person's Polkadot identities.
const RESERVED_TLD: &str = "dot";

/// The stack a host runs: the network `get_chain_info` reports, the dotNS TLD,
/// and the genesis hash of each chain role the core uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stack {
	network: String,
	tld: String,
	chains: StackChains,
}

/// The genesis hash of each chain role a stack names. People and Bulletin are
/// required: the core runs SSO and its own preimage submission over them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackChains {
	/// The relay chain.
	pub relay: Option<[u8; 32]>,
	/// Asset Hub, where the dotNS contracts are deployed. Without one, product
	/// manifests and session usernames do not resolve.
	pub asset_hub: Option<[u8; 32]>,
	/// The People chain.
	pub people: [u8; 32],
	/// The Bulletin chain.
	pub bulletin: [u8; 32],
}

/// Why a stack was refused.
#[derive(Debug, PartialEq, derive_more::Display, derive_more::Error)]
pub enum StackError {
	/// The pointer is not a stack link.
	#[display("stack pointer is not a stack link: {_0}")]
	Pointer(#[error(not(source))] trinity_stack::Error),
	/// The pointer carries no key. A host runs only a signed stack, so that a
	/// copy it fetches later verifies against the same key as the bundled one.
	#[display("stack pointer has no key; a host's stack must be signed")]
	Unsigned,
	/// The bundled descriptor breaks a rule or does not verify under the
	/// pointer's key.
	#[display("bundled stack descriptor refused ({}): {_0}", _0.code())]
	Descriptor(#[error(not(source))] trinity_stack::Error),
	/// The descriptor has no chain with this role.
	#[display("stack has no {role} chain")]
	MissingChain {
		/// The missing role, as descriptors name it.
		role: &'static str,
	},
	/// The TLD is not one the core serves dotNS names under.
	#[display("stack tld {tld:?} is not one of {}", DOTNS_TLDS.join(", "))]
	UnsupportedTld {
		/// The descriptor's TLD.
		tld: String,
	},
	/// The stack runs on Polkadot's TLD, which this build does not allow.
	#[display("stack uses Polkadot's reserved tld {RESERVED_TLD}, which this build does not allow")]
	ReservedTld,
	/// A chain, under any role, has one of Polkadot's genesis hashes, which
	/// this build does not allow.
	#[display(
		"stack uses Polkadot's reserved genesis hash 0x{}, which this build does not allow",
		hex::encode(genesis_hash)
	)]
	ReservedGenesis {
		/// The reserved genesis hash.
		genesis_hash: [u8; 32],
	},
}

impl Stack {
	/// The stack a host build bundles: `pointer` is the stack's link, such as
	/// `polkadotapp://stack?key=0x…&url=…`, and `descriptor` the signed
	/// envelope the build ships.
	///
	/// Fails when the pointer has no key, when the descriptor does not verify
	/// under it, when it has no People or Bulletin chain, when its TLD is not a
	/// dotNS TLD the core serves, or when it names a reserved value and
	/// `allows_reserved_values` is false. Neither `validUntil` nor `expiresAt`
	/// is checked: the bundled copy is the only one a host holds, so a stale or
	/// expired copy still starts.
	pub fn bundled(
		pointer: &str,
		descriptor: &[u8],
		allows_reserved_values: bool,
	) -> Result<Self, StackError> {
		let pointer = Pointer::from_link(pointer).map_err(StackError::Pointer)?;
		if pointer.key().is_none() {
			return Err(StackError::Unsigned);
		}
		let descriptor = open(descriptor, &pointer).map_err(StackError::Descriptor)?.descriptor;
		if !allows_reserved_values {
			refuse_reserved_genesis(descriptor.chains.iter().map(Chain::genesis))?;
		}
		let genesis_of = |role: &str| descriptor.chain(role).map(Chain::genesis);
		let required = |role| genesis_of(role).ok_or(StackError::MissingChain { role });
		let chains = StackChains {
			relay: genesis_of("Relay"),
			asset_hub: genesis_of("AssetHub"),
			people: required("People")?,
			bulletin: required("Bulletin")?,
		};
		Self::new(descriptor.network, descriptor.tld, chains, allows_reserved_values)
	}

	/// A stack from genesis hashes a host resolved itself rather than from a
	/// bundled descriptor, such as the CLI's.
	///
	/// Applies the same TLD and reserved-value rules as [`Self::bundled`].
	pub fn new(
		network: String,
		tld: String,
		chains: StackChains,
		allows_reserved_values: bool,
	) -> Result<Self, StackError> {
		if !DOTNS_TLDS.contains(&tld.as_str()) {
			return Err(StackError::UnsupportedTld { tld });
		}
		if !allows_reserved_values {
			if tld == RESERVED_TLD {
				return Err(StackError::ReservedTld);
			}
			let StackChains { relay, asset_hub, people, bulletin } = chains;
			refuse_reserved_genesis(
				[relay, asset_hub].into_iter().flatten().chain([people, bulletin]),
			)?;
		}
		Ok(Self { network, tld, chains })
	}

	/// The ecosystem `get_chain_info` reports, such as `polkadot` or `paseo`.
	pub fn network(&self) -> &str {
		&self.network
	}

	/// The dotNS TLD without the leading dot. The signing role derives every
	/// reserved RFC-0022 identity under it: `uid.<tld>` for the identity
	/// account, `peopl.<tld>` for the person ring-VRF keys.
	pub fn tld(&self) -> &str {
		&self.tld
	}

	/// The stack's chains.
	pub fn chains(&self) -> &StackChains {
		&self.chains
	}

	/// The genesis hash of the chain with this role, or `None` when the stack
	/// has no such chain.
	fn genesis_hash(&self, chain: ChainIdentifier) -> Option<[u8; 32]> {
		let StackChains { relay, asset_hub, people, bulletin } = self.chains;
		match chain {
			ChainIdentifier::Relay => relay,
			ChainIdentifier::AssetHub => asset_hub,
			ChainIdentifier::People => Some(people),
			ChainIdentifier::Bulletin => Some(bulletin),
		}
	}

	/// Answers `get_chain_info` (RFC 0026): the requested chain's genesis hash
	/// and the stack's network, or `NotSupported` when the stack has no such
	/// chain.
	pub fn chain_info(
		&self,
		request: &RemoteChainInfoRequest,
	) -> Result<RemoteChainInfoResponse, RemoteChainInfoError> {
		self.genesis_hash(request.chain)
			.map(|genesis_hash| RemoteChainInfoResponse {
				network: self.network.clone(),
				chain: request.chain,
				genesis_hash,
			})
			.ok_or(RemoteChainInfoError::NotSupported)
	}
}

fn refuse_reserved_genesis(
	genesis_hashes: impl IntoIterator<Item = [u8; 32]>,
) -> Result<(), StackError> {
	let is_reserved =
		|hash: &[u8; 32]| RESERVED_GENESIS_HASHES.contains(&hex::encode(hash).as_str());
	match genesis_hashes.into_iter().find(is_reserved) {
		Some(genesis_hash) => Err(StackError::ReservedGenesis { genesis_hash }),
		None => Ok(()),
	}
}

#[cfg(test)]
mod tests;
