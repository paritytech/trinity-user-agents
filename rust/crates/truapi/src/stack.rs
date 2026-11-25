//! The Trinity stack a host runs, read from a signed descriptor.
//!
//! The core takes all of its chain configuration from the stack.
//! [`StackRefresh::load`] checks the descriptor a host build bundles against
//! the key in the stack's link and against the build's
//! [`ReservedValuesPolicy`], starts from it or from a newer copy the core
//! stored, and fetches newer copies while the core runs. Every copy is held to
//! the same policy, and a copy on another TLD than the bundled one is refused.
//! It resolves the [`Stack`] the core runs on. A runtime built on that
//! [`Stack`] also checks the genesis hash of each chain it connects to, and
//! the refresh reports the ones that differ.

mod held;
mod pinned;
mod records;
mod refresh;
mod source;

use std::sync::Arc;

use trinity_stack::{Chain, Descriptor, Pointer, open};
use truapi::latest::{
	ChainIdentifier, RemoteChainInfoError, RemoteChainInfoRequest, RemoteChainInfoResponse,
};

use crate::{
	CallError, ProductId, chain_runtime::GenesisCheck, dotns_name::is_dns_label,
	product_id::PRODUCT_SHARE_SUFFIX,
};

pub use records::{CoreStorageRecords, ReplaceCheck, StackRecords};
use refresh::StackChecks;
pub use refresh::{StackHost, StackRefresh};
pub use source::{
	StackFetchRequest, StackFetchResponse, StackMismatch, StackRefreshOutcome, StackStatus,
};

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

/// The TLD RFC 6761 reserves for the local machine: every name under it
/// resolves there.
const LOOPBACK_TLD: &str = "localhost";

/// A product the stack does not serve.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
#[display("the {stack_tld} stack does not serve product {product_id}")]
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

/// A host build's policy on reserved values for its stack.
///
/// The core reserves Polkadot's genesis hashes and the `dot` TLD. A build can
/// reserve more here and cannot reserve fewer. A stack that names a reserved
/// value, the core's or the build's, under any chain role starts only when
/// `allowed` is set.
///
/// The core refuses a policy that would reserve less than it says: `allowed`
/// together with extra values, which the allowed stack may name anyway, and
/// an extra TLD that no stack can have: one that is not a DNS label, or a
/// TLD whose names already mean something else. It refuses the policy before
/// it reads the stack.
#[derive(Clone, Default)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ReservedValuesPolicy {
	/// Whether the stack may name reserved values. Only a Polkadot or
	/// Polkadot-fork build sets it, and then reserves no extra values.
	pub allowed: bool,
	/// Genesis hashes reserved on top of the core's.
	#[cfg_attr(not(target_arch = "wasm32"), uniffi(default))]
	pub extra_genesis_hashes: Vec<[u8; 32]>,
	/// dotNS TLDs reserved on top of the core's, each written as a stack's TLD
	/// is, without the leading dot.
	#[cfg_attr(not(target_arch = "wasm32"), uniffi(default))]
	pub extra_tlds: Vec<String>,
}

impl ReservedValuesPolicy {
	/// Allows the stack every reserved value and reserves no extra ones.
	pub const ALLOWED: Self =
		Self { allowed: true, extra_genesis_hashes: Vec::new(), extra_tlds: Vec::new() };

	/// Refuses extra values that would reserve nothing: any at all when the
	/// stack is allowed reserved values, and a TLD no stack can have.
	fn refuse_extras_that_reserve_nothing(&self) -> Result<(), StackError> {
		if self.allowed && !(self.extra_genesis_hashes.is_empty() && self.extra_tlds.is_empty()) {
			return Err(StackError::AllowedWithExtras);
		}
		let no_stack_can_have =
			|tld: &&String| !is_dns_label(tld) || taken_tld_reason(tld).is_some();
		match self.extra_tlds.iter().find(no_stack_can_have) {
			Some(tld) => Err(StackError::UnsupportedReservedTld { tld: tld.clone() }),
			None => Ok(()),
		}
	}

	fn refuse_tld(&self, tld: &str) -> Result<(), StackError> {
		if self.allowed {
			return Ok(());
		}
		if tld == RESERVED_TLD || self.extra_tlds.iter().any(|extra| extra == tld) {
			return Err(StackError::ReservedTld { tld: tld.to_owned() });
		}
		Ok(())
	}

	fn refuse_genesis(
		&self,
		genesis_hashes: impl IntoIterator<Item = [u8; 32]>,
	) -> Result<(), StackError> {
		if self.allowed {
			return Ok(());
		}
		let is_reserved = |genesis_hash: &[u8; 32]| {
			RESERVED_GENESIS_HASHES.contains(genesis_hash) ||
				self.extra_genesis_hashes.contains(genesis_hash)
		};
		match genesis_hashes.into_iter().find(is_reserved) {
			Some(genesis_hash) => Err(StackError::ReservedGenesis { genesis_hash }),
			None => Ok(()),
		}
	}
}

/// The stack a host runs: its [`StackIdentity`], the network `get_chain_info`
/// reports, the dotNS TLD, the genesis hash of each chain role the core uses,
/// and the Asset Hub contracts the core reads.
///
/// Two stacks are equal when all of these are. The stack a [`StackRefresh`]
/// hands out also carries the checks of its chains, which equality ignores.
#[derive(derive_more::Debug, Clone, derive_more::PartialEq, derive_more::Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Object))]
pub struct Stack {
	identity: StackIdentity,
	network: String,
	tld: String,
	chains: StackChains,
	account_data_store: Option<[u8; 20]>,
	#[debug(skip)]
	#[partial_eq(skip)]
	checks: Option<StackChecks>,
}

/// A stack's id and the generation of its chains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackIdentity {
	/// An opaque, non-empty string that tells this stack apart from others,
	/// including a fork with its source's genesis hashes.
	///
	/// [`Stack::bundled`] and [`StackRefresh::load`] take
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
	/// The TLD is a DNS label whose names already mean something else: share
	/// hosts end in `.li`, and every name under `localhost` resolves to the
	/// local machine (RFC 6761).
	#[display("stack tld {tld:?} is reserved: {reason}")]
	TakenTld {
		/// The descriptor's TLD.
		tld: String,
		/// What names under it already are.
		reason: &'static str,
	},
	/// The build allows its stack reserved values and also reserves extra
	/// ones, which that stack may then name.
	#[display(
		"reserved values that allow the stack every reserved value cannot reserve extra ones"
	)]
	AllowedWithExtras,
	/// The build reserves a TLD that no stack can have, so it reserves nothing.
	#[display("reserved tld {tld:?} is not a TLD a stack can have")]
	UnsupportedReservedTld {
		/// The entry as the build wrote it.
		tld: String,
	},
	/// The TLD is reserved, and the build does not allow it.
	#[display("stack uses the reserved tld {tld}, which this build does not allow")]
	ReservedTld {
		/// The reserved TLD.
		tld: String,
	},
	/// A chain, under any role, has a reserved genesis hash, and the build
	/// does not allow it.
	#[display(
		"stack uses the reserved genesis hash 0x{}, which this build does not allow",
		hex::encode(genesis_hash)
	)]
	ReservedGenesis {
		/// The reserved genesis hash.
		genesis_hash: [u8; 32],
	},
	/// A stored or fetched copy changes a field the bundled copy pins.
	#[display("stack descriptor changes {field}, which the bundled copy pins")]
	PinnedFieldChanged {
		/// The field, as the descriptor spells it.
		field: &'static str,
	},
}

impl Stack {
	/// The stack a host build bundles, read from the bundled copy alone:
	/// `pointer` is the stack's link (`polkadotapp://stack?key=0x…&url=…`),
	/// `descriptor` the signed copy the build ships, and `reserved_values` the
	/// build's policy on reserved values. A host starts through
	/// [`StackRefresh::load`] instead, which also reads the copy the core
	/// stored.
	///
	/// Fails with a [`StackError`] naming the rule the policy or the copy
	/// breaks, the policy's first. `validUntil` and `expiresAt` are not
	/// checked.
	pub fn bundled(
		pointer: &str,
		descriptor: &[u8],
		reserved_values: &ReservedValuesPolicy,
	) -> Result<Self, StackError> {
		reserved_values.refuse_extras_that_reserve_nothing()?;
		let pointer = signed_pointer(pointer)?;
		let verified = open(descriptor, &pointer).map_err(StackError::Descriptor)?;
		Self::from_descriptor(&pointer, &verified.descriptor, reserved_values)
	}

	/// The stack a verified descriptor read from `pointer` names, when the
	/// core can run it under `reserved_values`, a policy already checked. Its
	/// id is the pointer's and its generation the descriptor's. A reserved
	/// genesis hash is refused under any role, including one the core does
	/// not read.
	/// `validUntil` and `expiresAt` are not checked.
	fn from_descriptor(
		pointer: &Pointer,
		descriptor: &Descriptor,
		reserved_values: &ReservedValuesPolicy,
	) -> Result<Self, StackError> {
		reserved_values.refuse_genesis(descriptor.chains.iter().map(Chain::genesis))?;
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
		let stack = Self::from_chains(
			identity,
			descriptor.network.clone(),
			descriptor.tld.clone(),
			chains,
			reserved_values,
		)?;
		Ok(Self { account_data_store: account_data_store_in(descriptor), ..stack })
	}

	/// A stack from genesis hashes a host resolved itself rather than from a
	/// bundled descriptor, such as the CLI's. It names no contracts.
	///
	/// Applies the same policy, TLD and reserved-value rules as
	/// [`Self::bundled`], and refuses an empty id or generation and an
	/// all-zero Relay or Asset Hub genesis hash.
	pub fn new(
		identity: StackIdentity,
		network: String,
		tld: String,
		chains: StackChains,
		reserved_values: &ReservedValuesPolicy,
	) -> Result<Self, StackError> {
		reserved_values.refuse_extras_that_reserve_nothing()?;
		Self::from_chains(identity, network, tld, chains, reserved_values)
	}

	/// Checks a stack's identity, TLD and chains under a policy already
	/// checked.
	fn from_chains(
		identity: StackIdentity,
		network: String,
		tld: String,
		chains: StackChains,
		reserved_values: &ReservedValuesPolicy,
	) -> Result<Self, StackError> {
		let fields = [("id", &identity.id), ("generation", &identity.generation)];
		if let Some((field, _)) = fields.into_iter().find(|(_, value)| value.is_empty()) {
			return Err(StackError::EmptyIdentity { field });
		}
		if !is_dns_label(&tld) {
			return Err(StackError::InvalidTld { tld });
		}
		if let Some(reason) = taken_tld_reason(&tld) {
			return Err(StackError::TakenTld { tld, reason });
		}
		let StackChains { relay, asset_hub, people, bulletin } = chains;
		if relay == Some([0; 32]) {
			return Err(StackError::ZeroGenesis { chain: ChainIdentifier::Relay });
		}
		if asset_hub == [0; 32] {
			return Err(StackError::ZeroGenesis { chain: ChainIdentifier::AssetHub });
		}
		reserved_values.refuse_tld(&tld)?;
		reserved_values.refuse_genesis(relay.into_iter().chain([asset_hub, people, bulletin]))?;
		Ok(Self { identity, network, tld, chains, account_data_store: None, checks: None })
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

	/// The dotNS TLDs a product name can be written under on this stack:
	/// Polkadot's reserved TLD, then its own when it is another one.
	///
	/// Navigation and share hosts recognise a name under either, and
	/// [`Self::served_id`] gives the id the stack serves it under. The native
	/// helpers that take a `network_suffix`, such as `parse_product_link` and
	/// `product_id_matches_network`, judge against the suffix the host passes
	/// instead.
	pub fn accepted_tlds(&self) -> impl Iterator<Item = &str> {
		let own = (self.tld != RESERVED_TLD).then_some(self.tld.as_str());
		[RESERVED_TLD].into_iter().chain(own)
	}

	/// Whether `tld` is one of [`Self::accepted_tlds`].
	pub fn accepts_tld(&self, tld: &str) -> bool {
		self.accepted_tlds().any(|accepted| accepted == tld)
	}

	/// The id this stack serves the product `product_id` names under.
	///
	/// A localhost development id and a name under the stack's own TLD are
	/// served as they are. On a stack with another TLD, a name under `dot` is
	/// served as the same name under the stack's TLD, because the stack's own
	/// dotNS registry resolves it: `game.dot` on a `paseo` stack is
	/// `game.paseo`. Keys, entropy, storage and permissions are keyed by the
	/// id returned here, so what a stack's registry publishes never gets the
	/// keys of a Polkadot product.
	///
	/// Fails for a name under any other TLD, and for a `dot` name too long to
	/// be an id under the stack's TLD.
	pub fn served_id(&self, product_id: &ProductId) -> Result<ProductId, UnservedProduct> {
		let unserved =
			|| UnservedProduct { product_id: product_id.clone(), stack_tld: self.tld.clone() };
		match product_id.network_suffix() {
			None => Ok(product_id.clone()),
			Some(tld) if self.accepts_tld(tld) => {
				product_id.under_tld(&self.tld).map_err(|_| unserved())
			},
			Some(_) => Err(unserved()),
		}
	}

	/// Whether `product_id` is an id this stack serves a product under: a
	/// localhost development id, or a name under its own TLD.
	///
	/// A `dot` name on a stack with another TLD is not one. It names a
	/// product the stack serves under another id, which [`Self::served_id`]
	/// gives.
	pub fn serves(&self, product_id: &ProductId) -> bool {
		product_id.network_suffix().is_none_or(|tld| tld == self.tld)
	}

	/// [`Self::serves`] as a check: fails with the product and the stack's TLD.
	pub fn require_served(&self, product_id: &ProductId) -> Result<(), UnservedProduct> {
		if self.serves(product_id) {
			return Ok(());
		}
		Err(UnservedProduct { product_id: product_id.clone(), stack_tld: self.tld.clone() })
	}

	/// Whether this stack gives the product `product_id` names every
	/// `RemotePermission` without a prompt: a trusted label under one of
	/// [`Self::accepted_tlds`].
	pub fn trusts(&self, product_id: &ProductId) -> bool {
		product_id.has_trusted_remote_permissions() && self.served_id(product_id).is_ok()
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

	/// The check a chain runtime reports each connection's genesis hash to.
	///
	/// `None` unless a [`StackRefresh`] handed the stack out: the core does
	/// not check the chains of a stack built any other way.
	pub fn genesis_check(&self) -> Option<Arc<dyn GenesisCheck>> {
		self.checks.as_ref().map(StackChecks::genesis_check)
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

/// What names under `tld` already are, when that rules it out as a stack's
/// TLD.
fn taken_tld_reason(tld: &str) -> Option<&'static str> {
	if PRODUCT_SHARE_SUFFIX.strip_prefix('.') == Some(tld) {
		return Some("share hosts end in .li");
	}
	(tld == LOOPBACK_TLD).then_some("every name under localhost resolves to the local machine")
}

#[cfg(test)]
mod tests;
