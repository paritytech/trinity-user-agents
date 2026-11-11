//! The Trinity stack a host runs, read from the signed descriptor its build
//! bundles.
//!
//! [`Stack::bundled`] checks the descriptor against the key in the stack's
//! link. The [`Stack`] it returns holds the network, the dotNS TLD and each
//! chain's genesis hash, and answers `get_chain_info`.

use trinity_stack::{Chain, Pointer, open};
use truapi::latest::{
	ChainIdentifier, RemoteChainInfoError, RemoteChainInfoRequest, RemoteChainInfoResponse,
};

use crate::dotns_name::DOTNS_TLDS;

const POLKADOT_RELAY_GENESIS: [u8; 32] = [
	0x91, 0xb1, 0x71, 0xbb, 0x15, 0x8e, 0x2d, 0x38, 0x48, 0xfa, 0x23, 0xa9, 0xf1, 0xc2, 0x51, 0x82,
	0xfb, 0x8e, 0x20, 0x31, 0x3b, 0x2c, 0x1e, 0xb4, 0x92, 0x19, 0xda, 0x7a, 0x70, 0xce, 0x90, 0xc3,
];

const POLKADOT_ASSET_HUB_GENESIS: [u8; 32] = [
	0x68, 0xd5, 0x6f, 0x15, 0xf8, 0x5d, 0x31, 0x36, 0x97, 0x0e, 0xc1, 0x69, 0x46, 0x04, 0x0b, 0xc1,
	0x75, 0x26, 0x54, 0xe9, 0x06, 0x14, 0x7f, 0x7e, 0x43, 0xe9, 0xd5, 0x39, 0xd7, 0xc3, 0xde, 0x2f,
];

const POLKADOT_PEOPLE_GENESIS: [u8; 32] = [
	0x67, 0xfa, 0x17, 0x7a, 0x09, 0x7b, 0xfa, 0x18, 0xf7, 0x7e, 0xa9, 0x5a, 0xb5, 0x6e, 0x9b, 0xcd,
	0xfe, 0xb0, 0xe5, 0xb8, 0xa4, 0x0e, 0x46, 0x29, 0x8b, 0xb9, 0x3e, 0x16, 0xb6, 0xfc, 0x50, 0x08,
];

const POLKADOT_BULLETIN_GENESIS: [u8; 32] = [
	0x27, 0x61, 0xc9, 0x52, 0x59, 0xd5, 0x9e, 0x55, 0xae, 0x3d, 0xaf, 0x75, 0x6c, 0x14, 0x13, 0xb4,
	0x6e, 0x45, 0xa5, 0xa2, 0x98, 0x72, 0x99, 0xf8, 0xef, 0x8e, 0x5d, 0x8e, 0x47, 0x76, 0xcb, 0xc4,
];

/// Polkadot's Relay, Asset Hub, People and Bulletin genesis hashes, from PCF's
/// signed Polkadot descriptor. A transaction signed for a chain with one of
/// them can also be valid on Polkadot itself.
const RESERVED_GENESIS_HASHES: [[u8; 32]; 4] = [
	POLKADOT_RELAY_GENESIS,
	POLKADOT_ASSET_HUB_GENESIS,
	POLKADOT_PEOPLE_GENESIS,
	POLKADOT_BULLETIN_GENESIS,
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
	/// manifests and session usernames do not resolve, and a `trustedProducts`
	/// grant the core has not cached is refused.
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
	#[display("stack has no {} chain", <&str>::from(*chain))]
	MissingChain {
		/// The missing role.
		chain: ChainIdentifier,
	},
	/// The Relay or Asset Hub genesis hash is all zero, which names no chain.
	/// A stack without one of them leaves the role out.
	#[display(
		"stack {} chain has an all-zero genesis hash; leave the role out instead",
		<&str>::from(*chain)
	)]
	ZeroGenesis {
		/// The role with the all-zero hash.
		chain: ChainIdentifier,
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
	/// The stack a host build bundles: `pointer` is the stack's link
	/// (`polkadotapp://stack?key=0x…&url=…`), `descriptor` the signed copy the
	/// build ships, and `allows_reserved_values` whether it may use Polkadot's
	/// genesis hashes and the `dot` TLD.
	///
	/// Fails with a [`StackError`] naming the rule the copy breaks. `validUntil`
	/// and `expiresAt` are not checked, because the bundled copy is the only one
	/// the host has.
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
		let genesis_of =
			|chain: ChainIdentifier| descriptor.chain(chain.into()).map(Chain::genesis);
		let required = |chain| genesis_of(chain).ok_or(StackError::MissingChain { chain });
		let chains = StackChains {
			relay: genesis_of(ChainIdentifier::Relay),
			asset_hub: genesis_of(ChainIdentifier::AssetHub),
			people: required(ChainIdentifier::People)?,
			bulletin: required(ChainIdentifier::Bulletin)?,
		};
		Self::new(descriptor.network, descriptor.tld, chains, allows_reserved_values)
	}

	/// A stack from genesis hashes a host resolved itself rather than from a
	/// bundled descriptor, such as the CLI's.
	///
	/// Applies the same TLD and reserved-value rules as [`Self::bundled`], and
	/// refuses an all-zero Relay or Asset Hub genesis hash.
	pub fn new(
		network: String,
		tld: String,
		chains: StackChains,
		allows_reserved_values: bool,
	) -> Result<Self, StackError> {
		if !DOTNS_TLDS.contains(&tld.as_str()) {
			return Err(StackError::UnsupportedTld { tld });
		}
		let StackChains { relay, asset_hub, people, bulletin } = chains;
		let optional_roles =
			[(ChainIdentifier::Relay, relay), (ChainIdentifier::AssetHub, asset_hub)];
		if let Some((chain, _)) = optional_roles
			.into_iter()
			.find(|(_, genesis_hash)| *genesis_hash == Some([0; 32]))
		{
			return Err(StackError::ZeroGenesis { chain });
		}
		if !allows_reserved_values {
			if tld == RESERVED_TLD {
				return Err(StackError::ReservedTld);
			}
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
	/// reserved RFC-0022 identity under it: `peopl.<tld>` for the identity
	/// account and for the person ring-VRF keys.
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
	match genesis_hashes.into_iter().find(|hash| RESERVED_GENESIS_HASHES.contains(hash)) {
		Some(genesis_hash) => Err(StackError::ReservedGenesis { genesis_hash }),
		None => Ok(()),
	}
}

#[cfg(test)]
mod tests;
