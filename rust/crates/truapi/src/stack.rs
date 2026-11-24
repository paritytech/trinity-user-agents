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

use trinity_stack::{Chain, Descriptor, Pointer, open};
use truapi::latest::{
	ChainIdentifier, RemoteChainInfoError, RemoteChainInfoRequest, RemoteChainInfoResponse,
};

use crate::{CallError, ProductId, dotns_name::is_dns_label};

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

/// A product under a TLD the stack does not serve.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
#[display("product {product_id} is not under a TLD the {stack_tld} stack serves")]
pub struct UnservedProduct {
	/// The product that was named.
	pub product_id: ProductId,
	/// The stack's own TLD.
	pub stack_tld: String,
}

/// A request naming such a product is malformed, the same as one whose id
/// does not parse.
impl<D> From<UnservedProduct> for CallError<D> {
	fn from(error: UnservedProduct) -> Self {
		CallError::MalformedFrame { reason: error.to_string() }
	}
}

/// The stack a host runs: its [`StackIdentity`], the network `get_chain_info`
/// reports, the dotNS TLD, the genesis hash of each chain role the core uses,
/// and the Asset Hub contracts the core reads.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Object))]
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
	/// The TLD is not a DNS label, so no dotNS name can end in it.
	#[display("stack tld {tld:?} is not a lowercase DNS label")]
	InvalidTld {
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
		if !is_dns_label(&tld) {
			return Err(StackError::InvalidTld { tld });
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

	/// The run of the stack's chains. See [`StackIdentity::generation`].
	pub fn generation(&self) -> &str {
		&self.identity.generation
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

	/// The dotNS TLDs this stack serves names under: Polkadot's reserved TLD,
	/// then its own when it is another one.
	///
	/// The core has no other list of TLDs. Product ids, dotNS links and share
	/// hosts are all judged against this one.
	pub fn served_tlds(&self) -> impl Iterator<Item = &str> {
		let own = (self.tld != RESERVED_TLD).then_some(self.tld.as_str());
		[RESERVED_TLD].into_iter().chain(own)
	}

	/// Whether `tld` is one of [`Self::served_tlds`].
	pub fn serves_tld(&self, tld: &str) -> bool {
		self.served_tlds().any(|served| served == tld)
	}

	/// Whether this stack serves `product_id`: a localhost development id, or a
	/// name under one of [`Self::served_tlds`].
	pub fn serves(&self, product_id: &ProductId) -> bool {
		product_id.network_suffix().is_none_or(|tld| self.serves_tld(tld))
	}

	/// [`Self::serves`] as a check: fails with the product and the stack's TLD.
	pub fn require_served(&self, product_id: &ProductId) -> Result<(), UnservedProduct> {
		if self.serves(product_id) {
			return Ok(());
		}
		Err(UnservedProduct { product_id: product_id.clone(), stack_tld: self.tld.clone() })
	}

	/// Whether this stack gives `product_id` every `RemotePermission` without a
	/// prompt: a trusted label under a TLD it serves.
	pub fn trusts(&self, product_id: &ProductId) -> bool {
		self.serves(product_id) && product_id.has_trusted_remote_permissions()
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
