//! dotNS gateway (Asset Hub) payload building and contract reads.
//!
//! Every host shares these helpers. They cover the candidate-signed
//! reservation message for `pallet_dotns_gateway::reserve_name`, the ring-VRF
//! proof message for `register_name`, the dotNS contract interfaces for
//! reading usernames back through `ReviveApi_call`, and the raw storage keys of
//! the gateway pallet.
//!
//! Username resolution ([`discover_pop_controller`], [`resolve_labels`]) is
//! written once against the [`DotnsTransport`] trait. The headless CLI's
//! plain-RPC reader and the in-core `chainHead_v1` lookup therefore walk the
//! same contract chain.
//!
//! Byte layouts mirror `pallets/dotns-gateway` in `paritytech/individuality`
//! and the dotNS contracts (`paritytech/dotns`).

use alloy_primitives::{Address, U256};
use parity_scale_codec::Encode;
use sp_crypto_hashing::{blake2_128, blake2_256, keccak_256, twox_128};
use subxt::{ext::scale_encode::EncodeAsType, utils::MultiSignature};
use thiserror::Error;
use tracing::warn;

use crate::{
	chain::{
		EvmContractError, ExtensionError, InheritedImplication, SubstrateSigner,
		TransactionExtension,
	},
	dotns_views::{network_tld, protocol_component, read_view, tld_node},
	platform::{MAX_DNS_LABEL_LEN, is_dns_label},
};

alloy_sol_types::sol!("src/host_logic/dotns_gateway/contracts.sol");

/// Ring-VRF context for `register_name` proofs.
///
/// The ASCII label, space-padded to 32 bytes. Matches `DOTNS_GATEWAY_CONTEXT`
/// in the gateway pallet.
pub const DOTNS_GATEWAY_CONTEXT: [u8; 32] = *b"pop:polkadot.network/dotns      ";

/// Reservation message prefix, matching `RESERVE_MSG_PREFIX` in the gateway
/// pallet.
pub const RESERVE_MSG_PREFIX: &[u8] = b"pop:dotns-gateway:reserve";

/// Builds the payload the candidate signs to authorize a lite-name
/// reservation.
///
/// Commits to the *base* of the lite label only. The digit suffix is excluded,
/// so the candidate can sign before the gateway assigns digits. Mirrors
/// `Pallet::construct_reservation_message`: a SCALE tuple of
/// `(prefix, candidate, attester, username_base, chat_key, reserved, signed_at)`.
pub fn build_reservation_message(
	candidate: &[u8; 32],
	attester: &[u8; 32],
	username_base: &[u8],
	chat_key: &[u8; 65],
	reserved_base_label: Option<&[u8]>,
	signed_at_secs: u64,
) -> Vec<u8> {
	(
		RESERVE_MSG_PREFIX,
		candidate,
		attester,
		username_base,
		chat_key.as_slice(),
		reserved_base_label,
		signed_at_secs,
	)
		.encode()
}

/// The `Link` argument of `register_name`: how the new full-person username
/// relates to the account's previous lite identity.
#[derive(Clone, PartialEq, Eq, Encode, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub enum Link {
	/// Links the full username to an existing lite username, in dotted form
	/// such as `alice.01`. The chat key is inherited from the lite entry.
	LiteUsername(Vec<u8>),
	/// Standalone registration carrying a fresh 65-byte uncompressed ECDH
	/// chat key.
	None([u8; 65]),
}

/// Hashes the registration intent into the 32-byte ring-VRF proof message.
/// Matches the pallet's `(who, label, link).using_encoded(blake2_256)`.
pub fn build_register_proof_message(who: &[u8; 32], label: &[u8], link: &Link) -> [u8; 32] {
	blake2_256(&(who, label, link).encode())
}

/// `DotnsGateway.register_name` arguments.
#[derive(Clone, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct RegisterNameCallArgs {
	/// Account the username is registered for.
	pub who: [u8; 32],
	/// Base label to register.
	pub label: Vec<u8>,
	/// How the new username relates to the account's lite identity.
	pub link: Link,
}

/// `AsDotnsGatewayInfo`: what an `AsDotnsGateway` value authorizes.
#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub enum AsDotnsGatewayInfo {
	/// A full-person username registration.
	RegisterFullName {
		/// Ring-VRF proof of full personhood over the registration intent.
		proof: Vec<u8>,
		/// The People ring the proof is built in.
		ring_index: u32,
		/// The People root revision the proof is built against, which Asset
		/// Hub's `members-subscriber` must hold.
		revision: u32,
		/// The account's signature over the extrinsic's implication.
		signature: MultiSignature,
	},
}

/// The Asset Hub `AsDotnsGateway` authorizer for a full-person username
/// registration. The ring proof covers only the registration intent, so it is
/// built beforehand; the account signs the extrinsic's implication here.
pub struct AsDotnsGatewayExtension<'a> {
	/// Ring-VRF proof over [`build_register_proof_message`].
	pub ring_proof: Vec<u8>,
	/// The People ring the proof is built in.
	pub ring_index: u32,
	/// The People root revision the proof is built against.
	pub revision: u32,
	/// The account registering the username.
	pub who: &'a SubstrateSigner,
}

impl TransactionExtension for AsDotnsGatewayExtension<'_> {
	type Implicit = ();
	type Explicit = Option<AsDotnsGatewayInfo>;

	fn name(&self) -> &str {
		"AsDotnsGateway"
	}

	fn implicit(&self) {}

	fn explicit(
		&self,
		implication: &InheritedImplication,
	) -> Result<Option<AsDotnsGatewayInfo>, ExtensionError> {
		Ok(Some(AsDotnsGatewayInfo::RegisterFullName {
			proof: self.ring_proof.clone(),
			ring_index: self.ring_index,
			revision: self.revision,
			signature: self.who.sign(&implication.hash()),
		}))
	}
}

/// Maps an AccountId32 to the H160 the contracts see.
///
/// Mirrors `pallet_revive::AccountId32Mapper::to_address`. Eth-derived
/// accounts — trailing 12 bytes all `0xEE` — truncate. Everything else hashes.
pub fn account_to_h160(public_key: &[u8; 32]) -> [u8; 20] {
	if public_key[20..].iter().all(|byte| *byte == 0xEE) {
		public_key[..20]
			.try_into()
			.expect("slice of first 20 bytes from a 32-byte array; qed")
	} else {
		keccak_256(public_key)[12..]
			.try_into()
			.expect("keccak_256 output is 32 bytes, tail slice is 20; qed")
	}
}

/// Usernames of one account as recorded by the dotNS contracts. Holds at least
/// one of the two; [`classify_labels`] is the only constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DotnsIdentity {
	lite_username: Option<String>,
	full_username: Option<String>,
}

impl DotnsIdentity {
	/// Dotted lite username (`alice.01`), if any.
	pub fn lite_username(&self) -> Option<&str> {
		self.lite_username.as_deref()
	}

	/// Full-person username, if any.
	pub fn full_username(&self) -> Option<&str> {
		self.full_username.as_deref()
	}

	/// Preferred display username: full over lite.
	pub fn primary_username(&self) -> &str {
		self.full_username
			.as_deref()
			.or(self.lite_username.as_deref())
			.expect("classify_labels only builds an identity with a username; qed")
	}

	/// The dotted lite username, if any.
	pub fn into_lite_username(self) -> Option<String> {
		self.lite_username
	}

	/// The `(lite, full)` usernames, at least one of them `Some`.
	pub fn into_usernames(self) -> (Option<String>, Option<String>) {
		(self.lite_username, self.full_username)
	}
}

/// Classifies bare contract labels into lite and full usernames.
///
/// Only labels the PoP controller actually issued become usernames:
/// `DotnsPopController.isPopIssued(label)` is the provenance authority, so a
/// public registration, an incoming transfer, or a subname under a digit-only
/// parent never fills a username slot no matter its shape. A label matching
/// [`is_dotted_lite_username`] (`alice.42`, stored dotted) is the lite
/// username verbatim; any other issued canonical DNS label is the full
/// username. Anything that is not one of those shapes — oversized, non-ASCII,
/// control characters, markup, other dotted strings — is skipped before the
/// provenance read, so no unscreened contract string is even asked about.
/// First hit per slot wins. Labels are expected bare: [`resolve_labels`]
/// strips the network TLD. Returns `None` when no label becomes a username.
pub async fn classify_labels<T, I>(
	transport: &mut T,
	controller: &[u8; 20],
	labels: I,
) -> Result<Option<DotnsIdentity>, DotnsError>
where
	T: DotnsTransport + ?Sized,
	I: IntoIterator,
	I::Item: AsRef<str>,
{
	let (mut lite_username, mut full_username) = (None, None);
	for label in labels {
		let label = label.as_ref();
		let shape_ok = is_dotted_lite_username(label) || is_dns_label(label);
		if !shape_ok {
			continue;
		}
		if !is_pop_issued(transport, controller, label).await? {
			continue;
		}
		if is_dotted_lite_username(label) {
			lite_username.get_or_insert_with(|| label.to_string());
		} else {
			full_username.get_or_insert_with(|| label.to_string());
		}
	}
	let has_username = lite_username.is_some() || full_username.is_some();
	Ok(has_username.then_some(DotnsIdentity { lite_username, full_username }))
}

/// Whether the PoP controller issued `label`, per
/// `DotnsPopController.isPopIssued(string)`.
pub async fn is_pop_issued<T: DotnsTransport + ?Sized>(
	transport: &mut T,
	controller: &[u8; 20],
	label: &str,
) -> Result<bool, DotnsError> {
	read_view(
		transport,
		controller,
		DotnsPopController::isPopIssuedCall { label: label.to_string() },
	)
	.await
	.map_err(|source| DotnsError::IsPopIssued { label: label.to_string(), source })
}

/// The dotted lite shape: a letters-only stem, one dot, exactly two digits
/// (`StringUtils.isLitePersonLabel`; the stem follows `_isPersonLabel`, so a
/// digit or hyphen in it is not a lite label). Shape only — see
/// [`is_dotted_lite_username`] for the gateway's byte bound on top.
pub fn is_lite_label(label: &str) -> bool {
	match label.rsplit_once('.') {
		Some((stem, digits)) => {
			!stem.is_empty() &&
				stem.len() <= MAX_DNS_LABEL_LEN &&
				stem.chars().all(|c| c.is_ascii_lowercase()) &&
				digits.len() == 2 &&
				digits.chars().all(|c| c.is_ascii_digit())
		},
		None => false,
	}
}

/// Longest label the gateway pallet accepts (`BaseLabel = BoundedVec<u8, 32>`).
pub const MAX_BASE_LABEL_LEN: usize = 32;

/// Whether `label` is a full-person base label as the gateway pallet accepts
/// it (`is_person_label`): non-empty, lowercase ASCII letters only, no digits
/// or hyphens, at most [`MAX_BASE_LABEL_LEN`] bytes.
pub fn is_full_person_label(label: &str) -> bool {
	!label.is_empty() &&
		label.len() <= MAX_BASE_LABEL_LEN &&
		label.chars().all(|c| c.is_ascii_lowercase())
}

/// Shortest base a person can register or reserve: `PopRules` classifies bases
/// of five characters or fewer as reserved for governance.
pub const MIN_PERSON_LABEL_LEN: usize = 6;

/// Whether `label` can be registered as a full-person username, or reserved
/// for a later claim through the gateway (`PopRules.reserveBaseNameForPop`):
/// the pallet's [`is_full_person_label`] plus the governance-reserved tier.
pub fn is_registrable_full_label(label: &str) -> bool {
	is_full_person_label(label) && label.len() >= MIN_PERSON_LABEL_LEN
}

/// Whether `value` is a dotted lite username the gateway can hold:
/// [`is_lite_label`] within [`MAX_BASE_LABEL_LEN`] bytes. The pallet keys
/// `LiteLabelOwner` by the dotted label itself (`BaseLabel` is a 32-byte
/// `BoundedVec`), so the bound applies to the dotted form, not a flattened
/// spelling.
pub fn is_dotted_lite_username(value: &str) -> bool {
	is_lite_label(value) && value.len() <= MAX_BASE_LABEL_LEN
}

/// Page size for `LabelStore.getLabels`.
const LABEL_PAGE_LIMIT: u64 = 16;
/// Upper bound on pages read from one `LabelStore`. The store is an append-only
/// ledger shared with public registrations and incoming transfers, so it can
/// grow well past the one lite and one full name a gateway user holds.
const LABEL_PAGE_MAX: u64 = 16;

/// Page size for `DotnsPopController.pendingClaims`. The contract clamps a page
/// to `DotnsConstants.MAX_PAGE_SIZE` (200), so this stays well under it.
const CLAIM_PAGE_LIMIT: u64 = 16;
/// Upper bound on pages read from one account's pending-claim queue. A gateway
/// user stages one lite and one full name, so one page is the normal case.
const CLAIM_PAGE_MAX: u64 = 16;

/// A failed dotNS lookup, retaining the operation and its underlying failure.
/// Use [`Self::reason`] for a complete diagnostic.
#[derive(Debug, Error, derive_more::From)]
pub enum DotnsError {
	/// The chain could not serve a storage read.
	#[error("{0}")]
	#[from(String)]
	Storage(String),
	/// The chain follow the lookup reads through gave no best block to pin.
	#[error("{0}")]
	Follow(String),
	/// The controller's protocol registry could not be read.
	#[error("DotnsPopController.protocolRegistry()")]
	ProtocolRegistry(#[source] DotnsCallError),
	/// The gateway dispatcher could not identify its controller.
	#[error("RootGatewayDispatcher.TARGET()")]
	DispatcherTarget(#[source] DotnsCallError),
	/// A protocol component address could not be read.
	#[error("ProtocolRegistry.get({name})")]
	ProtocolComponent {
		/// Registry component being resolved.
		name: String,
		/// Failure reading the component address.
		source: DotnsCallError,
	},
	/// A label's proof-of-personhood issuance could not be determined.
	#[error("DotnsPopController.isPopIssued({label})")]
	IsPopIssued {
		/// Label whose issuance is being checked.
		label: String,
		/// Failure reading the issuance status.
		source: DotnsCallError,
	},
	/// The user's label store could not be located.
	#[error("StoreFactory.getLabelStore")]
	LabelStore(#[source] DotnsCallError),
	/// A label store could not serve a page of labels.
	#[error("LabelStore.getLabels")]
	Labels(#[source] DotnsCallError),
	/// A registrar could not determine whether a label already exists.
	#[error("DotnsRegistrar.exists({label})")]
	RegistrarExists {
		/// Label whose availability is being checked.
		label: String,
		/// Failure reading the registration status.
		source: DotnsCallError,
	},
	/// A pending-claim page could not be read.
	#[error("DotnsPopController.pendingClaims")]
	PendingClaims(#[source] DotnsCallError),
	/// The network's top-level domain could not be read.
	#[error("ProtocolRegistry.tld()")]
	Tld(#[source] DotnsCallError),
	/// The fallback top-level domain could not be checked.
	#[error("DotnsRegistry.recordExists")]
	RecordExists(#[source] DotnsCallError),
	/// A name's content resolver could not be located.
	#[error("DotnsRegistry.resolver()")]
	Resolver(#[source] DotnsCallError),
	/// A text record could not be read.
	#[error("ContentResolver.text()")]
	TextRecord(#[source] DotnsCallError),
	/// The stored dispatcher address has the wrong width.
	#[error("DotnsGateway.DispatcherAddress is {actual} bytes")]
	DispatcherAddressLength {
		/// Length of the stored address.
		actual: usize,
	},
	/// The stored address implements neither supported controller interface.
	#[error(
		"DotnsGateway.DispatcherAddress {} has neither protocolRegistry() nor TARGET()",
		hex::encode(.address)
	)]
	UnknownDispatcher {
		/// Address that answered neither discovery call.
		address: [u8; 20],
	},
	/// Neither the TLD view nor the fallback registry record identifies the network.
	#[error(
		"ProtocolRegistry has no tld() view and the registry holds no record for {:?}; the network TLD cannot be determined",
		crate::dotns_views::TLD_WITHOUT_VIEW
	)]
	MissingTld,
}

impl DotnsError {
	/// Formats the operation and its cause chain for diagnostics.
	pub fn reason(&self) -> String {
		core::iter::successors(Some(self as &dyn core::error::Error), |error| error.source())
			.map(ToString::to_string)
			.collect::<Vec<_>>()
			.join(": ")
	}
}

/// A contract read failed during execution or decoding of its return data.
#[derive(Debug, Error, derive_more::From)]
pub enum DotnsCallError {
	/// The view failed before returning data to decode.
	#[error(transparent)]
	View(EvmContractError),
	/// The view returned data that did not match the expected ABI shape.
	#[error(transparent)]
	Decode(alloy_sol_types::Error),
}

/// How a caller reaches Asset Hub for dotNS reads.
///
/// The surface is storage reads and single or batched contract views. The headless CLI
/// implements it over plain RPC. The in-core runtime implements it over a
/// `chainHead_v1` follow. The resolution steps below are written once.
#[async_trait::async_trait]
pub trait DotnsTransport: Send {
	/// Reads one storage value. `None` when the entry is absent.
	async fn storage(&mut self, key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError>;

	/// Dry-runs a contract view against `dest` and returns its return data.
	///
	/// [`EvmContractError::Reverted`] is an answer from the contract; any other
	/// error leaves the view's result unknown.
	async fn view(&mut self, dest: &[u8; 20], input: Vec<u8>) -> Result<Vec<u8>, EvmContractError>;

	/// Reads independent contract views for the same destination.
	///
	/// Returns one result per input in input order, retaining individual failures.
	/// The transport controls concurrency and the block context.
	async fn views(
		&mut self,
		dest: &[u8; 20],
		inputs: Vec<Vec<u8>>,
	) -> Vec<Result<Vec<u8>, EvmContractError>>;
}

/// Resolves the `DotnsPopController` address from `DotnsGateway.DispatcherAddress`.
///
/// The stored address is either the controller itself or a `RootGatewayDispatcher`
/// whose `TARGET()` is the controller. Both are in service: a chain keeps its dispatcher
/// until the gateway pallet is repointed.
///
/// `protocolRegistry()` decides which. Only the controller answers it: the dispatcher has
/// no such function, and its fallback is Root-gated, so a dry-run view reverts there
/// instead of being forwarded. A revert therefore means the address is not the
/// controller, and `TARGET()` resolves it as a dispatcher. A contract answering neither
/// is reported by address rather than mistaken for either.
///
/// The order is deliberate: keying on a function the controller has, rather than one it
/// lacks, keeps discovery correct even if the controller later grows a `TARGET()`.
///
/// A transport failure is never read as an answer: it propagates, so an unreachable node
/// cannot masquerade as a repointed chain.
///
/// `None` when the gateway is not deployed on the chain at all.
pub async fn discover_pop_controller<T: DotnsTransport + ?Sized>(
	transport: &mut T,
) -> Result<Option<[u8; 20]>, DotnsError> {
	let Some(value) = transport.storage(dispatcher_address_key()).await? else {
		return Ok(None);
	};
	let stored: [u8; 20] = value
		.try_into()
		.map_err(|value: Vec<u8>| DotnsError::DispatcherAddressLength { actual: value.len() })?;
	match read_view(transport, &stored, DotnsPopController::protocolRegistryCall {}).await {
		Ok(_) => Ok(Some(stored)),
		// A chain still storing its dispatcher pays this second view; a repointed one
		// answers on the first. Both hops go once no chain stores a dispatcher.
		Err(DotnsCallError::View(EvmContractError::Reverted { .. })) => {
			match read_view(transport, &stored, RootGatewayDispatcher::TARGETCall {}).await {
				Ok(controller) => Ok(Some(controller.into_array())),
				Err(DotnsCallError::View(EvmContractError::Reverted { .. })) => {
					Err(DotnsError::UnknownDispatcher { address: stored })
				},
				Err(error) => Err(DotnsError::DispatcherTarget(error)),
			}
		},
		Err(error) => Err(DotnsError::ProtocolRegistry(error)),
	}
}

/// Resolves the bare contract labels `account` holds.
///
/// Two sources are merged. The controller's pending claims hold gateway-minted
/// names the user has not settled into a `LabelStore` yet (`claimLabelStore`);
/// those come first. A pending claim never lapses: the controller settles a
/// claim of any age, so its `mintedAt` is not consulted. The user's
/// `LabelStore`, when deployed, holds every name written for them: gateway names once settled, plus
/// public registrations and incoming transfers. Store labels carry the network TLD
/// (`alice01.paseo`), which is stripped here; subnames (`app.alice`) are dropped.
///
/// A store alone is not proof that pending claims are settled: a public
/// registration or an incoming transfer deploys the store while gateway names
/// stay pending, so both sources are always read. The store is append-only,
/// so a name transferred away is still listed; ownership is not re-checked
/// here.
pub async fn resolve_labels<T: DotnsTransport + ?Sized>(
	transport: &mut T,
	controller: &[u8; 20],
	account: &[u8; 32],
) -> Result<Vec<String>, DotnsError> {
	let user = account_to_h160(account);

	let mut labels = pending_claim_labels(transport, controller, &user).await?;

	let registry = read_view(transport, controller, DotnsPopController::protocolRegistryCall {})
		.await
		.map_err(DotnsError::ProtocolRegistry)?
		.into_array();

	let factory = protocol_component(transport, &registry, "storeFactory").await?;

	let store =
		read_view(transport, &factory, StoreFactory::getLabelStoreCall { user: user.into() })
			.await
			.map_err(DotnsError::LabelStore)?
			.into_array();
	if store == [0u8; 20] {
		return Ok(labels);
	}

	let tld = network_tld(transport, &registry).await?;

	for page in 0..LABEL_PAGE_MAX {
		let stored = read_view(
			transport,
			&store,
			LabelStore::getLabelsCall {
				offset: U256::from(page * LABEL_PAGE_LIMIT),
				limit: U256::from(LABEL_PAGE_LIMIT),
			},
		)
		.await
		.map_err(DotnsError::Labels)?;
		let short_page = (stored.len() as u64) < LABEL_PAGE_LIMIT;
		for label in stored {
			if let Some(bare) = bare_store_label(&label, &tld) &&
				!labels.iter().any(|known| known == bare)
			{
				labels.push(bare.to_string());
			}
		}
		if short_page {
			break;
		}
		if page + 1 == LABEL_PAGE_MAX {
			// A LabelStore address resolves back to the account that owns it,
			// so it is named as an account.
			warn!(
				store = %crate::logging::Account::new(&store[..]),
				read = LABEL_PAGE_MAX * LABEL_PAGE_LIMIT,
				"LabelStore holds more labels than the pages read; a name past this point is not resolved"
			);
		}
	}
	Ok(labels)
}

/// Strips the network TLD from a `LabelStore` label. `None` for labels that do
/// not carry it or that still hold a dot afterwards, unless that dot is a lite
/// label's separator (`alice.42`); other dotted remainders are subnames.
fn bare_store_label<'a>(label: &'a str, tld: &str) -> Option<&'a str> {
	let bare = if tld.is_empty() { label } else { label.strip_suffix(tld)? };
	(!bare.is_empty() && (!bare.contains('.') || is_dotted_lite_username(bare))).then_some(bare)
}

/// ENS-style node of `label` directly under `parent`:
/// `keccak256(parent ‖ keccak256(label))`.
pub fn namehash_under(parent: &[u8; 32], label: &str) -> [u8; 32] {
	keccak_256(&[parent.as_slice(), &keccak_256(label.as_bytes())].concat())
}

/// Whether the gateway could still mint `label` on this network: the name's
/// node under the network TLD (`namehash(label.tld)`, derived locally) does
/// not exist on the `DotnsRegistrar` (`exists(uint256)`, a total view, so a
/// revert is a broken deployment and an error, never "available"). A minted
/// name is taken whoever holds it, the name escrow included: the gateway's
/// registration path only mints fresh ids.
///
/// A lite-name reservation carrying a `reserved_base_label` that is already
/// registered can never be claimed, yet it holds the reservation queue for
/// that stem for the whole reservation window. The gateway pallet does not
/// check this, so callers ask before attesting or registering.
pub async fn label_available<T: DotnsTransport + ?Sized>(
	transport: &mut T,
	controller: &[u8; 20],
	label: &str,
) -> Result<bool, DotnsError> {
	let registry = read_view(transport, controller, DotnsPopController::protocolRegistryCall {})
		.await
		.map_err(DotnsError::ProtocolRegistry)?
		.into_array();

	let tld = network_tld(transport, &registry).await?;

	let registrar = protocol_component(transport, &registry, "registrar").await?;

	let node = namehash_under(&tld_node(&tld), label);
	read_view(
		transport,
		&registrar,
		DotnsRegistrar::existsCall { tokenId: U256::from_be_bytes(node) },
	)
	.await
	.map(|exists| !exists)
	.map_err(|source| DotnsError::RegistrarExists { label: label.to_string(), source })
}

/// Gateway-minted labels of `user` still waiting for `claimLabelStore`, paged
/// out of `DotnsPopController.pendingClaims(address,uint256,uint256)`. Every
/// entry counts, whatever its `mintedAt`: the controller no longer expires
/// pending claims and settles one of any age.
async fn pending_claim_labels<T: DotnsTransport + ?Sized>(
	transport: &mut T,
	controller: &[u8; 20],
	user: &[u8; 20],
) -> Result<Vec<String>, DotnsError> {
	let mut claims = Vec::new();
	for page in 0..CLAIM_PAGE_MAX {
		let page_claims = match read_view(
			transport,
			controller,
			DotnsPopController::pendingClaimsCall {
				user: Address::from(*user),
				offset: U256::from(page * CLAIM_PAGE_LIMIT),
				limit: U256::from(CLAIM_PAGE_LIMIT),
			},
		)
		.await
		{
			Ok(claims) => claims,
			Err(DotnsCallError::View(EvmContractError::Reverted { detail: reason })) => {
				warn!(
					%reason,
					retained = claims.len(),
					"DotnsPopController.pendingClaims page reverted; retaining earlier pages"
				);
				break;
			},
			Err(error) => return Err(DotnsError::PendingClaims(error)),
		};
		let short_page = (page_claims.len() as u64) < CLAIM_PAGE_LIMIT;
		claims.extend(page_claims);
		if short_page {
			break;
		}
		if page + 1 == CLAIM_PAGE_MAX {
			warn!(
				user = %crate::logging::Account::new(&user[..]),
				read = CLAIM_PAGE_MAX * CLAIM_PAGE_LIMIT,
				"pending-claim page cap reached; later entries, if any, are not resolved"
			);
		}
	}
	Ok(claims.into_iter().map(|claim| claim.label).collect())
}

/// `DotnsGateway.DispatcherAddress` storage key.
/// The entry holds either a `RootGatewayDispatcher`, whose `TARGET()` is the
/// `DotnsPopController`, or that controller directly. See
/// [`discover_pop_controller`], which tells them apart.
pub fn dispatcher_address_key() -> Vec<u8> {
	plain_key(b"DotnsGateway", b"DispatcherAddress")
}

/// `DotnsGateway.AccountAlias[account]` storage key.
/// The value is the 32-byte alias the account registered with.
pub fn account_alias_key(account: &[u8; 32]) -> Vec<u8> {
	let mut key = plain_key(b"DotnsGateway", b"AccountAlias");
	key.extend_from_slice(&blake2_128(account));
	key.extend_from_slice(account);
	key
}

/// `DotnsGateway.LiteLabelOwner[lite_label]` storage key, the dotted lite
/// username (`alice.01`) as a `BaseLabel`. The value is the owning account.
pub fn lite_label_owner_key(lite_label: &[u8]) -> Vec<u8> {
	let encoded = lite_label.encode();
	let mut key = plain_key(b"DotnsGateway", b"LiteLabelOwner");
	key.extend_from_slice(&blake2_128(&encoded));
	key.extend_from_slice(&encoded);
	key
}

/// `Timestamp.Now` storage key. The value is a u64 of Unix milliseconds.
pub fn timestamp_now_key() -> Vec<u8> {
	plain_key(b"Timestamp", b"Now")
}

/// `twox128(pallet) ‖ twox128(entry)`.
fn plain_key(pallet: &[u8], entry: &[u8]) -> Vec<u8> {
	[twox_128(pallet).as_slice(), twox_128(entry).as_slice()].concat()
}

#[cfg(test)]
mod tests;
