//! The Trinity stack a host runs, read from a signed descriptor.
//!
//! The core takes all of its chain configuration from the stack.
//! [`StackRefresh::load`] checks the descriptor a host build bundles against
//! the key in the stack's link, starts from it or from a newer copy the core
//! stored, and fetches newer copies while the core runs. It resolves the
//! [`Stack`] the core runs on.

mod records;
mod refresh;
mod source;

use parity_scale_codec::{Decode, Encode, Error as CodecError, Input, Output};
use trinity_stack::{Chain, Descriptor, Pointer, open};
use truapi::latest::{
	ChainIdentifier, RemoteChainInfoError, RemoteChainInfoRequest, RemoteChainInfoResponse,
};

use crate::dotns_name::DOTNS_TLDS;

pub use records::{CoreStorageRecords, ReplaceCheck, StackRecords};
pub use refresh::{StackHost, StackRefresh};
pub use source::{StackFetchRequest, StackFetchResponse, StackRefreshOutcome, StackStatus};

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

/// The `contracts` name of the contract each seed registers its installations in.
const ACCOUNT_DATA_STORE_CONTRACT: &str = "accountDataStore";

/// Polkadot's dotNS TLD. The RFC-0022 identities derived under it are a
/// person's Polkadot identities.
const RESERVED_TLD: &str = "dot";

/// The stack a host runs: its [`StackIdentity`], the network `get_chain_info`
/// reports, the dotNS TLD, the genesis hash of each chain role the core uses,
/// and the Asset Hub contracts the core reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stack {
	identity: StackIdentity,
	network: String,
	tld: String,
	chains: StackChains,
	account_data_store: Option<[u8; 20]>,
}

/// A stack's id and the generation of its chains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackIdentity {
	/// An opaque, non-empty string that tells this stack apart from others,
	/// including a fork with its source's genesis hashes.
	///
	/// [`Stack::from_descriptor`], which [`Stack::bundled`] and
	/// [`StackRefresh::load`] build through, takes
	/// `trinity_stack::Pointer::stack_id` of the pointer, the key in hex. The CLI takes the same
	/// for a link or a URL, and `file:` and the canonical path for a local descriptor file. A
	/// test host passes any string.
	pub id: String,
	/// The descriptor's `generation`, which changes whenever the stack
	/// restarts its chains, as after a wipe. Must not be empty.
	pub generation: String,
}

/// One run of a stack's chains, from one wipe to the next: the descriptor's
/// `generation` and the genesis hashes of the chains the core reads.
///
/// A wipe changes both. The hashes are compared as well, so a wipe that
/// changes one is noticed even when the operator left `generation` as it was. What the core stored
/// about the chains in another run is stale. The relay chain is left out:
/// the core never reads it, so adding or removing it is not a wipe.
///
/// It encodes as a version byte, then its fields, independent of
/// [`StackChains`]. An encoding with another version decodes as an error,
/// which every reader takes for another run.
#[derive(Clone, PartialEq)]
pub struct StackRun {
	generation: String,
	asset_hub: [u8; 32],
	people: [u8; 32],
	bulletin: [u8; 32],
}

/// The version byte that leads an encoded [`StackRun`].
const STACK_RUN_V1: u8 = 1;

impl Encode for StackRun {
	fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
		STACK_RUN_V1.encode_to(dest);
		(&self.generation, &self.asset_hub, &self.people, &self.bulletin).encode_to(dest);
	}
}

impl Decode for StackRun {
	fn decode<I: Input>(input: &mut I) -> Result<Self, CodecError> {
		if u8::decode(input)? != STACK_RUN_V1 {
			return Err("unknown stack run version".into());
		}
		let (generation, asset_hub, people, bulletin) = Decode::decode(input)?;
		Ok(Self { generation, asset_hub, people, bulletin })
	}
}

impl StackRun {
	/// `value` led by this run, for a value read from the chains that is
	/// stored. [`Self::read_stamped`] gives it back only in the same run.
	pub fn stamp(&self, value: &[u8]) -> Vec<u8> {
		let mut stamped = self.encode();
		stamped.extend_from_slice(value);
		stamped
	}

	/// The value [`Self::stamp`] stored in `stamped`, or `None` when another
	/// run stamped it or nothing did: the chains it was read from are gone.
	pub fn read_stamped<'a>(&self, stamped: &'a [u8]) -> Option<&'a [u8]> {
		let mut value = stamped;
		(Self::decode(&mut value).ok()? == *self).then_some(value)
	}
}

/// The genesis hash of each chain role the host serves. Asset Hub, People and
/// Bulletin are required: the core reads dotNS and runs SSO and its own
/// preimage submission over them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackChains {
	/// The relay chain, which the core never reads itself. `None` for a host
	/// that serves none, such as the CLI.
	pub relay: Option<[u8; 32]>,
	/// Asset Hub, where the dotNS contracts are deployed. Product manifests and
	/// session usernames resolve from it.
	pub asset_hub: [u8; 32],
	/// The People chain.
	pub people: [u8; 32],
	/// The Bulletin chain.
	pub bulletin: [u8; 32],
}

impl StackChains {
	/// Whether one of these chains has `genesis_hash`.
	pub fn contains(&self, genesis_hash: [u8; 32]) -> bool {
		let Self { relay, asset_hub, people, bulletin } = self;
		*relay == Some(genesis_hash) || [asset_hub, people, bulletin].contains(&&genesis_hash)
	}
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
	/// The descriptor breaks a rule or does not verify under the pointer's
	/// key.
	#[display("stack descriptor refused ({}): {_0}", _0.code())]
	Descriptor(#[error(not(source))] trinity_stack::Error),
	/// The descriptor has no chain with this role.
	#[display("stack has no {} chain", <&str>::from(*chain))]
	MissingChain {
		/// The missing role.
		chain: ChainIdentifier,
	},
	/// The Relay or Asset Hub genesis hash is all zero, which names no chain.
	/// A stack without a Relay leaves the role out.
	#[display("stack {} chain has an all-zero genesis hash", <&str>::from(*chain))]
	ZeroGenesis {
		/// The role with the all-zero hash.
		chain: ChainIdentifier,
	},
	/// The stack's id or generation is empty.
	#[display("stack {field} is empty")]
	EmptyIdentity {
		/// `id` or `generation`.
		field: &'static str,
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
	/// The stack a host build bundles, read from the bundled copy alone:
	/// `pointer` is the stack's link (`polkadotapp://stack?key=0x…&url=…`) and
	/// `descriptor` the signed copy the build ships. A host starts through
	/// [`StackRefresh::load`] instead, which also reads the copy the core
	/// stored.
	///
	/// Fails like [`Self::from_descriptor`], and when the copy does not verify
	/// under the pointer's key.
	pub fn bundled(
		pointer: &str,
		descriptor: &[u8],
		allows_reserved_values: bool,
	) -> Result<Self, StackError> {
		let pointer = signed_pointer(pointer)?;
		let verified = open(descriptor, &pointer).map_err(StackError::Descriptor)?;
		Self::from_descriptor(&pointer, &verified.descriptor, allows_reserved_values)
	}

	/// The stack a verified descriptor read from `pointer` names, when the
	/// core can run it. Its id is the pointer's and its generation the
	/// descriptor's. `allows_reserved_values` is whether the build lets it use
	/// Polkadot's genesis hashes and the `dot` TLD.
	///
	/// Fails with a [`StackError`] naming the rule the descriptor breaks.
	/// `validUntil` and `expiresAt` are not checked.
	pub fn from_descriptor(
		pointer: &Pointer,
		descriptor: &Descriptor,
		allows_reserved_values: bool,
	) -> Result<Self, StackError> {
		if !allows_reserved_values {
			refuse_reserved_genesis(descriptor.chains.iter().map(Chain::genesis))?;
		}
		let genesis_of =
			|chain: ChainIdentifier| descriptor.chain(chain.into()).map(Chain::genesis);
		let required = |chain| genesis_of(chain).ok_or(StackError::MissingChain { chain });
		let chains = StackChains {
			relay: genesis_of(ChainIdentifier::Relay),
			asset_hub: required(ChainIdentifier::AssetHub)?,
			people: required(ChainIdentifier::People)?,
			bulletin: required(ChainIdentifier::Bulletin)?,
		};
		let identity =
			StackIdentity { id: pointer.stack_id(), generation: descriptor.generation.clone() };
		let stack = Self::new(
			identity,
			descriptor.network.clone(),
			descriptor.tld.clone(),
			chains,
			allows_reserved_values,
		)?;
		Ok(Self { account_data_store: account_data_store_in(descriptor), ..stack })
	}

	/// A stack from genesis hashes a host resolved itself rather than from a
	/// bundled descriptor, such as the CLI's. It names no contracts.
	///
	/// Applies the same TLD and reserved-value rules as
	/// [`Self::from_descriptor`], and refuses an empty id or generation and an
	/// all-zero Relay or Asset Hub genesis hash.
	pub fn new(
		identity: StackIdentity,
		network: String,
		tld: String,
		chains: StackChains,
		allows_reserved_values: bool,
	) -> Result<Self, StackError> {
		let fields = [("id", &identity.id), ("generation", &identity.generation)];
		if let Some((field, _)) = fields.into_iter().find(|(_, value)| value.is_empty()) {
			return Err(StackError::EmptyIdentity { field });
		}
		if !DOTNS_TLDS.contains(&tld.as_str()) {
			return Err(StackError::UnsupportedTld { tld });
		}
		let StackChains { relay, asset_hub, people, bulletin } = chains;
		if relay == Some([0; 32]) {
			return Err(StackError::ZeroGenesis { chain: ChainIdentifier::Relay });
		}
		if asset_hub == [0; 32] {
			return Err(StackError::ZeroGenesis { chain: ChainIdentifier::AssetHub });
		}
		if !allows_reserved_values {
			if tld == RESERVED_TLD {
				return Err(StackError::ReservedTld);
			}
			refuse_reserved_genesis(relay.into_iter().chain([asset_hub, people, bulletin]))?;
		}
		Ok(Self { identity, network, tld, chains, account_data_store: None })
	}

	/// The stack's identity. See [`StackIdentity::id`].
	pub fn id(&self) -> &str {
		&self.identity.id
	}

	/// The descriptor's `generation`. See [`StackIdentity::generation`].
	pub fn generation(&self) -> &str {
		&self.identity.generation
	}

	/// The run the stack's chains are in.
	pub fn run(&self) -> StackRun {
		let StackChains { asset_hub, people, bulletin, .. } = self.chains;
		StackRun { generation: self.identity.generation.clone(), asset_hub, people, bulletin }
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

	/// The `AccountDataStore` contract on Asset Hub, where each seed registers
	/// its installations. `None` when the descriptor names none, or names the
	/// all-zero address.
	pub fn account_data_store(&self) -> Option<[u8; 20]> {
		self.account_data_store
	}

	/// The genesis hash of the chain with this role, or `None` when the stack
	/// has no such chain.
	fn genesis_hash(&self, chain: ChainIdentifier) -> Option<[u8; 32]> {
		let StackChains { relay, asset_hub, people, bulletin } = self.chains;
		match chain {
			ChainIdentifier::Relay => relay,
			ChainIdentifier::AssetHub => Some(asset_hub),
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

/// The `AccountDataStore` address `descriptor` names, or `None` when it names
/// none or the all-zero address, which no contract has.
fn account_data_store_in(descriptor: &Descriptor) -> Option<[u8; 20]> {
	descriptor
		.contract_address(ACCOUNT_DATA_STORE_CONTRACT)
		.filter(|address| *address != [0; 20])
}

/// The pointer a stack link carries. A host runs only a signed stack, so a
/// link without a key is refused.
fn signed_pointer(link: &str) -> Result<Pointer, StackError> {
	let pointer = Pointer::from_link(link).map_err(StackError::Pointer)?;
	match pointer.key() {
		Some(_) => Ok(pointer),
		None => Err(StackError::Unsigned),
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
