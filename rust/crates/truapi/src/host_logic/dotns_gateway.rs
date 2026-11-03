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
use alloy_sol_types::{GenericContractError, SolInterface};
use parity_scale_codec::{Decode, Encode, Input};
use sp_crypto_hashing::{blake2_128, blake2_256, keccak_256, twox_128};
use subxt::{ext::scale_encode::EncodeAsType, utils::MultiSignature};
use thiserror::Error;
use tracing::warn;

alloy_sol_types::sol!("src/host_logic/dotns_gateway/contracts.sol");

use crate::{
	chain::{ExtensionError, InheritedImplication, SubstrateSigner, TransactionExtension},
	dotns_views::{network_tld, protocol_component, read_view, tld_node},
	platform::{MAX_DNS_LABEL_LEN, is_dns_label},
};

/// Ring-VRF context for `register_name` proofs.
///
/// The ASCII label, space-padded to 32 bytes. Matches `DOTNS_GATEWAY_CONTEXT`
/// in the gateway pallet.
pub const DOTNS_GATEWAY_CONTEXT: [u8; 32] = *b"pop:polkadot.network/dotns      ";

/// Reservation message prefix, matching `RESERVE_MSG_PREFIX` in the gateway
/// pallet.
pub const RESERVE_MSG_PREFIX: &[u8] = b"pop:dotns-gateway:reserve";

/// Origin for read-only `ReviveApi_call` dry-runs.
///
/// `pallet_revive` rejects unmapped signed origins with `AccountUnmapped`.
/// Eth-derived accounts — trailing 12 bytes `0xEE` — are implicitly mapped.
/// They need no on-chain state, so views work from this synthetic account on
/// any network. The address half is non-zero: a revert is read as an answer
/// here, and Solidity guards commonly reject `address(0)` outright.
pub const VIEW_CALL_ORIGIN: [u8; 32] = [0xEE; 32];

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
#[derive(Debug, Clone, PartialEq, Eq, Encode, EncodeAsType)]
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
#[derive(Debug, Clone, PartialEq, Eq, EncodeAsType)]
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

/// Error decoding a dotNS contract round-trip.
#[derive(Debug, Error, PartialEq)]
pub enum DotnsContractError {
	/// The `ReviveApi_call` output is not a `ContractResult`. The cause is
	/// kept out of the message, which SCALE renders across several lines.
	#[error("ReviveApi_call output is not a ContractResult")]
	Malformed(parity_scale_codec::Error),
	/// The runtime rejected the call before reaching the contract.
	#[error("ReviveApi_call dispatch failed: {detail}")]
	Dispatch {
		/// Raw hex of the `DispatchError` payload.
		detail: String,
	},
	/// The contract executed but reverted.
	#[error("contract reverted: {detail}")]
	Reverted {
		/// Rendered revert reason.
		detail: String,
	},
	/// ABI return data did not match the expected shape.
	#[error("ABI decode failed: {0}")]
	Abi(#[from] alloy_sol_types::Error),
}

/// `sp_weights::Weight`.
#[derive(Encode, Decode)]
struct Weight {
	#[codec(compact)]
	_ref_time: u64,
	#[codec(compact)]
	_proof_size: u64,
}

/// `pallet_revive::StorageDeposit` over a `u128` balance.
#[derive(Decode)]
#[cfg_attr(test, derive(Encode))]
enum StorageDeposit {
	Refund { _amount: u128 },
	Charge { _amount: u128 },
}

/// `pallet_revive::ExecReturnValue`.
#[derive(Decode)]
#[cfg_attr(test, derive(Encode))]
struct ExecReturnValue {
	flags: u32,
	data: Vec<u8>,
}

/// `ReturnFlags::REVERT`.
const REVERT_FLAG: u32 = 1;

/// A `DispatchError` kept as its raw bytes, since naming the error needs the
/// runtime's metadata. It is the last field of a `ContractResult`, so it takes
/// the rest of the input.
struct RawDispatchError(Vec<u8>);

impl Decode for RawDispatchError {
	fn decode<I: Input>(input: &mut I) -> Result<Self, parity_scale_codec::Error> {
		let len = input.remaining_len()?.ok_or("DispatchError of unknown length")?;
		let mut bytes = vec![0; len];
		input.read(&mut bytes)?;
		Ok(Self(bytes))
	}
}

/// `pallet_revive::ContractResult<ExecReturnValue, u128>`.
#[derive(Decode)]
struct ContractResult {
	_weight_consumed: Weight,
	_weight_required: Weight,
	_storage_deposit: StorageDeposit,
	_max_storage_deposit: StorageDeposit,
	_gas_consumed: u128,
	result: Result<ExecReturnValue, RawDispatchError>,
}

/// Arguments of the `ReviveApi_call` runtime API.
#[derive(Encode)]
struct ReviveCallArgs<'a> {
	origin: &'a [u8; 32],
	dest: &'a [u8; 20],
	value: u128,
	gas_limit: Option<Weight>,
	storage_deposit_limit: Option<u128>,
	input_data: &'a [u8],
}

/// SCALE arguments for the `ReviveApi_call` runtime API.
/// A zero-value dry-run of `input` against `dest` from `origin`, leaving the
/// gas and deposit limits for the node to estimate.
pub fn encode_revive_call(origin: &[u8; 32], dest: &[u8; 20], input: &[u8]) -> Vec<u8> {
	ReviveCallArgs {
		origin,
		dest,
		value: 0,
		gas_limit: None,
		storage_deposit_limit: None,
		input_data: input,
	}
	.encode()
}

/// A `ReviveApi_call` output whose contract returned or reverted with `data`,
/// for test transports.
#[cfg(test)]
pub fn encode_revive_call_output(reverted: bool, data: Vec<u8>) -> Vec<u8> {
	let flags = if reverted { REVERT_FLAG } else { 0 };
	let weight = || Weight { _ref_time: 7, _proof_size: 7 };
	let deposit = || StorageDeposit::Charge { _amount: 0 };
	let result: Result<_, ()> = Ok(ExecReturnValue { flags, data });
	(weight(), weight(), deposit(), deposit(), 0u128, result).encode()
}

/// Extracts the contract return data from a `ReviveApi_call` output.
/// Dispatch errors and reverts are rejected.
pub fn decode_revive_call_output(output: &[u8]) -> Result<Vec<u8>, DotnsContractError> {
	let contract_result =
		ContractResult::decode(&mut &output[..]).map_err(DotnsContractError::Malformed)?;
	match contract_result.result {
		Ok(ExecReturnValue { flags, data }) if flags & REVERT_FLAG != 0 => {
			Err(DotnsContractError::Reverted { detail: describe_revert(&data) })
		},
		Ok(ExecReturnValue { data, .. }) => Ok(data),
		Err(RawDispatchError(error)) => {
			Err(DotnsContractError::Dispatch { detail: format!("0x{}", hex::encode(error)) })
		},
	}
}

/// Renders a Solidity revert payload: `Error(string)` and `Panic(uint256)` by
/// their meaning, anything else as hex.
fn describe_revert(data: &[u8]) -> String {
	if data.is_empty() {
		return "(empty)".to_string();
	}
	GenericContractError::abi_decode(data)
		.map(|error| error.to_string())
		.unwrap_or_else(|_| format!("0x{}", hex::encode(data)))
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
#[derive(Debug, PartialEq, Error, derive_more::From)]
pub enum DotnsError {
	/// The chain could not serve a storage read.
	#[error("{0}")]
	#[from(String)]
	Storage(String),
	/// A contract view failed without an additional operation label.
	#[error(transparent)]
	#[from(DotnsViewError)]
	View(DotnsViewError),
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
	/// A product's manifest record could not be read.
	#[error("ContentResolver.text()")]
	ManifestText(#[source] DotnsCallError),
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
#[derive(Debug, PartialEq, Error, derive_more::From)]
pub enum DotnsCallError {
	/// The view failed before returning data to decode.
	#[error(transparent)]
	View(DotnsViewError),
	/// The view returned data that did not match the expected ABI shape.
	#[error(transparent)]
	Decode(DotnsContractError),
}

/// Why a contract view returned no data.
///
/// A revert is an answer from the contract (the view refused, or the address
/// has no such function); anything else means the answer is unknown.
#[derive(Debug, PartialEq, Error, derive_more::From)]
pub enum DotnsViewError {
	/// The contract executed and reverted.
	#[error(transparent)]
	Reverted(DotnsContractError),
	/// The runtime rejected the call or returned an invalid contract result.
	#[error(transparent)]
	Contract(DotnsContractError),
	/// The view could not be dry-run or its output could not be decoded.
	#[error("{0}")]
	#[from(String)]
	Failed(String),
}

/// Splits a `ReviveApi_call` output into return data, a revert, or a failure,
/// for [`DotnsTransport::view`] implementations.
pub fn view_output(output: &[u8]) -> Result<Vec<u8>, DotnsViewError> {
	decode_revive_call_output(output).map_err(|err| match err {
		DotnsContractError::Reverted { .. } => DotnsViewError::Reverted(err),
		other => DotnsViewError::Contract(other),
	})
}

/// How a caller reaches Asset Hub for dotNS reads.
///
/// The surface is one storage read plus one contract view. The headless CLI
/// implements it over plain RPC. The in-core runtime implements it over a
/// `chainHead_v1` follow. The resolution steps below are written once.
#[crate::platform::async_trait]
pub trait DotnsTransport {
	/// Reads one storage value. `None` when the entry is absent.
	async fn storage(&mut self, key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError>;

	/// Dry-runs a contract view against `dest` and returns its return data.
	async fn view(&mut self, dest: &[u8; 20], input: Vec<u8>) -> Result<Vec<u8>, DotnsViewError>;
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
		Err(DotnsCallError::View(DotnsViewError::Reverted(_))) => {
			match read_view(transport, &stored, RootGatewayDispatcher::TARGETCall {}).await {
				Ok(controller) => Ok(Some(controller.into_array())),
				Err(DotnsCallError::View(DotnsViewError::Reverted(_))) => {
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
			// A warning leaves the device, and a LabelStore address resolves
			// back to the account that owns it, so it is named by a handle.
			warn!(
				store = %crate::logging::account_handle(&store[..]),
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
			Err(DotnsCallError::View(DotnsViewError::Reverted(reason))) => {
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
			// A warning leaves the device, so the user is named by a handle
			// rather than in full.
			warn!(
				user = %crate::logging::account_handle(&user[..]),
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
mod tests {
	use super::*;
	use crate::dotns_views::{TLD_WITHOUT_VIEW, network_tld};
	use alloy_sol_types::SolCall;

	// Golden vectors from the validated reference implementation in
	// pop-dotns-testing: reservation.ts, proof.ts, verify.ts.
	const RESERVATION_WITH_RESERVED: &str = "64706f703a646f746e732d676174657761793a72657365727665111111111111111111111111111111111111111111111111111111111111111122222222222222222222222222222222222222222222222222222222222222221c616c696365626305013333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333013072657365727665646e616d65035e486800000000";
	const RESERVATION_NO_RESERVED: &str = "64706f703a646f746e732d676174657761793a72657365727665111111111111111111111111111111111111111111111111111111111111111122222222222222222222222222222222222222222222222222222222222222221c616c69636562630501333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333300035e486800000000";

	#[test]
	fn reservation_message_matches_reference_vectors() {
		let with_reserved = build_reservation_message(
			&[0x11; 32],
			&[0x22; 32],
			b"alicebc",
			&[0x33; 65],
			Some(b"reservedname"),
			1749573123,
		);
		assert_eq!(hex::encode(&with_reserved), RESERVATION_WITH_RESERVED);

		let without = build_reservation_message(
			&[0x11; 32],
			&[0x22; 32],
			b"alicebc",
			&[0x33; 65],
			None,
			1749573123,
		);
		assert_eq!(hex::encode(&without), RESERVATION_NO_RESERVED);
	}

	#[test]
	fn link_and_proof_message_match_reference_vectors() {
		let lite = Link::LiteUsername(b"alice.01".to_vec());
		assert_eq!(hex::encode(lite.encode()), "0020616c6963652e3031");
		assert_eq!(
			hex::encode(build_register_proof_message(&[0x44; 32], b"alicebc", &lite)),
			"506fe3afcb13b2e4fd182d49ff165bc9c834f9dd21d0d545eccb925993b68675"
		);

		let standalone = Link::None([0x55; 65]);
		assert_eq!(hex::encode(standalone.encode()), format!("01{}", hex::encode([0x55; 65])));
		assert_eq!(
			hex::encode(build_register_proof_message(&[0x44; 32], b"alicebc", &standalone)),
			"255da65ea6687123d5ac035a438a5c6fa18f7d04f23433611095a0fe58c479ab"
		);
	}

	/// The call arguments encode by name against Asset Hub's metadata into the
	/// gateway's positional layout: account, label, link.
	#[test]
	fn register_name_call_args_encode_to_the_reference_layout() {
		use subxt::{Metadata, ext::frame_decode::extrinsics::encode_call_data};

		let metadata = Metadata::decode_from(
			crate::runtime::statement_allowance::ASSET_HUB_TEST_METADATA_BYTES,
		)
		.unwrap();
		let link = Link::LiteUsername(b"alice.01".to_vec());
		let args = RegisterNameCallArgs {
			who: [0x44; 32],
			label: b"alicebc".to_vec(),
			link: link.clone(),
		};

		let call =
			encode_call_data("DotnsGateway", "register_name", &args, &metadata, metadata.types())
				.unwrap();

		let arguments = [vec![0x44; 32], b"alicebc".to_vec().encode(), link.encode()].concat();
		assert_eq!(&call[2..], &arguments[..]);
	}

	/// The registration value encodes by name into the gateway's positional
	/// layout: `Some ‖ RegisterFullName ‖ proof ‖ ring index ‖ revision ‖
	/// Sr25519 ‖ signature`.
	#[test]
	fn a_full_name_registration_encodes_as_the_gateway_expects() {
		use subxt::{Metadata, ext::frame_decode::extrinsics::ExtrinsicTypeInfo};

		let metadata = Metadata::decode_from(
			crate::runtime::statement_allowance::ASSET_HUB_TEST_METADATA_BYTES,
		)
		.unwrap();
		let pipeline = metadata.extrinsic_extension_info(Some(0)).unwrap();
		let slot = pipeline
			.extension_ids
			.iter()
			.find(|declared| declared.name == "AsDotnsGateway")
			.unwrap();
		let info = AsDotnsGatewayInfo::RegisterFullName {
			proof: vec![0xEE; 4],
			ring_index: 3,
			revision: 5,
			signature: MultiSignature::Sr25519([0xAB; 64]),
		};

		let encoded = Some(info).encode_as_type(slot.id, metadata.types()).unwrap();

		let expected = [
			vec![0x01, 0x00],
			vec![0xEEu8; 4].encode(),
			3u32.encode(),
			5u32.encode(),
			vec![0x01],
			vec![0xAB; 64],
		]
		.concat();
		assert_eq!(encoded, expected);
	}

	#[test]
	fn namehash_matches_the_reference_derivation() {
		// `cast namehash paseo` and `cast namehash alicebc.paseo`.
		let tld_node = namehash_under(&[0u8; 32], "paseo");
		assert_eq!(
			hex::encode(tld_node),
			"096b436ee9a398429fe33ad4b359bad4398dd74b412ec1dd043c93dfbf581874"
		);
		assert_eq!(
			hex::encode(namehash_under(&tld_node, "alicebc")),
			"d3b106171373cd9fea783acd85f69def1bb2085f81cf5f2a65088b0ce4edd82e"
		);
		assert_eq!(crate::dotns_views::tld_node(".paseo"), tld_node);
		assert_eq!(crate::dotns_views::tld_node("paseo"), tld_node);
	}

	#[test]
	fn account_to_h160_follows_the_revive_mapper() {
		let mut eth_derived = [0x0Fu8; 32];
		eth_derived[20..].fill(0xEE);
		assert_eq!(account_to_h160(&eth_derived), [0x0F; 20]);

		let hashed = account_to_h160(&[0x11; 32]);
		assert_eq!(hashed, keccak_256(&[0x11; 32])[12..]);
	}

	/// Answers a view of `Call` with `value`.
	fn returns<Call: SolCall>(value: Call::Return) -> Result<Vec<u8>, DotnsViewError> {
		Ok(Call::abi_encode_returns(&value))
	}

	/// The view of `Call` that `input` encodes, if it encodes one.
	fn decoded<Call: SolCall>(input: &[u8]) -> Option<Call> {
		Call::abi_decode(input).ok()
	}

	fn view_reverted() -> Result<Vec<u8>, DotnsViewError> {
		Err(DotnsViewError::Reverted(DotnsContractError::Reverted {
			detail: "(empty)".to_string(),
		}))
	}

	struct RevertingSecondClaimPage {
		first_page: Vec<DotnsPopController::PendingClaim>,
		pending_calls: usize,
	}

	#[crate::platform::async_trait]
	impl DotnsTransport for RevertingSecondClaimPage {
		async fn storage(&mut self, key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
			panic!("unscripted storage read {}", hex::encode(key));
		}

		async fn view(
			&mut self,
			_dest: &[u8; 20],
			input: Vec<u8>,
		) -> Result<Vec<u8>, DotnsViewError> {
			let page = DotnsPopController::pendingClaimsCall::abi_decode(&input)
				.expect("only pending claims are scripted");
			self.pending_calls += 1;
			assert_eq!(page.limit, U256::from(CLAIM_PAGE_LIMIT));
			if page.offset.is_zero() {
				return returns::<DotnsPopController::pendingClaimsCall>(self.first_page.clone());
			}
			assert_eq!(page.offset, U256::from(CLAIM_PAGE_LIMIT));
			Err(DotnsViewError::Reverted(DotnsContractError::Reverted {
				detail: "offset past end".to_string(),
			}))
		}
	}

	#[test]
	fn a_later_pending_claim_page_revert_keeps_complete_earlier_pages() {
		let expected = (0..CLAIM_PAGE_LIMIT)
			.map(|index| format!("claim{index:02}"))
			.collect::<Vec<_>>();
		let mut transport = RevertingSecondClaimPage {
			first_page: expected
				.iter()
				.cloned()
				.map(|label| DotnsPopController::PendingClaim { label, mintedAt: 50 })
				.collect(),
			pending_calls: 0,
		};

		let labels = futures::executor::block_on(pending_claim_labels(
			&mut transport,
			&[0xc0; 20],
			&[0xaa; 20],
		))
		.unwrap();

		assert_eq!(labels, expected);
		assert_eq!(transport.pending_calls, 2);
	}

	#[test]
	fn labels_classify_into_lite_and_full_usernames() {
		futures::executor::block_on(async {
			// Every label the transport is asked about counts as pop-issued
			// unless listed; assertions below separate shape from provenance.
			struct IssuedAll {
				denied: Vec<&'static str>,
			}
			#[crate::platform::async_trait]
			impl DotnsTransport for IssuedAll {
				async fn storage(&mut self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
					unreachable!("classify_labels reads no storage")
				}
				async fn view(
					&mut self,
					_dest: &[u8; 20],
					input: Vec<u8>,
				) -> Result<Vec<u8>, DotnsViewError> {
					let asked = DotnsPopController::isPopIssuedCall::abi_decode(&input)
						.expect("only isPopIssued is scripted");
					returns::<DotnsPopController::isPopIssuedCall>(
						!self.denied.contains(&asked.label.as_str()),
					)
				}
			}
			let identity = |lite: Option<&str>, full: Option<&str>| {
				Some(DotnsIdentity {
					lite_username: lite.map(str::to_owned),
					full_username: full.map(str::to_owned),
				})
			};
			let controller = [0xAA; 20];
			let mut all = IssuedAll { denied: vec![] };

			assert_eq!(
				classify_labels(&mut all, &controller, ["alice.01", "myproject"]).await.unwrap(),
				identity(Some("alice.01"), Some("myproject"))
			);

			// The lite stem is letters only (`isLitePersonLabel`): a digit or
			// hyphen in the stem is not a lite label. The flat spelling is an
			// ordinary label, and dotted non-lite labels are subnames.
			assert_eq!(
				classify_labels(
					&mut all,
					&controller,
					["app.web3app", "sub.alice.01", "aé.01", "a2b.34", "-x.01", "alice01"],
				)
				.await
				.unwrap(),
				identity(None, Some("alice01"))
			);

			assert_eq!(
				classify_labels(&mut all, &controller, Vec::<String>::new()).await.unwrap(),
				None
			);

			// Provenance gates every slot: a perfectly shaped label the
			// controller did not issue is skipped.
			let mut denying = IssuedAll { denied: vec!["app.42", "squatter"] };
			assert_eq!(
				classify_labels(
					&mut denying,
					&controller,
					["app.42", "squatter", "alice.01", "myproject"],
				)
				.await
				.unwrap(),
				identity(Some("alice.01"), Some("myproject"))
			);
			// An account whose labels are all unissued has no identity.
			assert_eq!(
				classify_labels(&mut denying, &controller, ["app.42", "squatter"])
					.await
					.unwrap(),
				None
			);

			// Hostile store data is dropped before the provenance read: the
			// transport panics on storage and asserts the selector, so reaching
			// it with garbage would fail loudly.
			assert_eq!(
				classify_labels(
					&mut all,
					&controller,
					[
						"a".repeat(5000),
						"admin\r\nx".to_string(),
						"a\0b".to_string(),
						"\x1b[31mred\x1b[0m".to_string(),
						"<img src=x onerror=alert(1)>".to_string(),
						"Upper".to_string(),
					],
				)
				.await
				.unwrap(),
				None
			);

			// The 63-octet DNS bound is the cut-off for full labels; the lite
			// dotted form is capped by the 32-byte gateway bound.
			let longest = "a".repeat(63);
			assert_eq!(
				classify_labels(&mut all, &controller, [longest.as_str()]).await.unwrap(),
				identity(None, Some(&longest))
			);
			assert_eq!(
				classify_labels(&mut all, &controller, ["a".repeat(64)]).await.unwrap(),
				None
			);
			let oversized = format!("{}.01", "a".repeat(30));
			assert_eq!(classify_labels(&mut all, &controller, [oversized]).await.unwrap(), None);

			// One digit after the dot is not lite format; three digits neither.
			for wrong in ["alice.1", "alice.012"] {
				assert_eq!(classify_labels(&mut all, &controller, [wrong]).await.unwrap(), None);
			}
		});
	}

	#[test]
	fn label_validators_follow_the_contract_rules() {
		assert!(is_full_person_label("alicebc"));
		assert!(is_full_person_label(&"a".repeat(32)));
		assert!(!is_full_person_label("web3-app"), "no digits or hyphens");
		assert!(!is_full_person_label("alice01"));
		assert!(!is_full_person_label("Alice"));
		assert!(!is_full_person_label(""));

		// PopRules: five letters or fewer are governance-reserved; longer
		// bases can be registered and reserved alike.
		assert!(is_registrable_full_label("george"));
		assert!(is_registrable_full_label("georgeabc"));
		assert!(is_registrable_full_label(&"a".repeat(32)));
		assert!(!is_registrable_full_label("alice"));
		assert!(!is_full_person_label("alice.bc"));
		assert!(!is_full_person_label("-alice"));
		assert!(!is_full_person_label(&"a".repeat(33)));

		assert!(is_dotted_lite_username("alice.01"));
		// The stem is letters only (`isLitePersonLabel`): digits and hyphens
		// in the stem are not lite labels.
		assert!(!is_dotted_lite_username("a2b.34"));
		assert!(!is_dotted_lite_username("a-b.34"));
		assert!(!is_dotted_lite_username("alice01"));
		assert!(!is_dotted_lite_username("alice.1"));
		assert!(!is_dotted_lite_username("alice.012"));
		assert!(!is_dotted_lite_username("a.lice.01"));
		assert!(!is_dotted_lite_username(".01"));
		// The 32-byte gateway bound applies to the dotted form: a 29-char stem
		// (32 bytes dotted) fits, a 30-char stem (33 bytes) does not.
		assert!(is_dotted_lite_username(&format!("{}.01", "a".repeat(29))));
		assert!(!is_dotted_lite_username(&format!("{}.01", "a".repeat(30))));
	}

	/// A transport for controller discovery: `DispatcherAddress` holds `stored`,
	/// and `TARGET()` / `protocolRegistry()` answer with scripted results.
	struct ScriptedDiscovery {
		stored: [u8; 20],
		target: fn() -> Result<Vec<u8>, DotnsViewError>,
		protocol_registry: fn() -> Result<Vec<u8>, DotnsViewError>,
	}

	#[crate::platform::async_trait]
	impl DotnsTransport for ScriptedDiscovery {
		async fn storage(&mut self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
			Ok(Some(self.stored.to_vec()))
		}

		async fn view(
			&mut self,
			_dest: &[u8; 20],
			input: Vec<u8>,
		) -> Result<Vec<u8>, DotnsViewError> {
			if decoded::<RootGatewayDispatcher::TARGETCall>(&input).is_some() {
				(self.target)()
			} else if decoded::<DotnsPopController::protocolRegistryCall>(&input).is_some() {
				(self.protocol_registry)()
			} else if decoded::<DotnsPopController::pendingClaimsCall>(&input).is_some() {
				returns::<DotnsPopController::pendingClaimsCall>(Vec::new())
			} else {
				panic!("unscripted view {}", hex::encode(input));
			}
		}
	}

	fn address_answer(byte: u8) -> Result<Vec<u8>, DotnsViewError> {
		returns::<RootGatewayDispatcher::TARGETCall>(Address::repeat_byte(byte))
	}

	#[test]
	fn discovery_follows_target_when_the_stored_address_is_a_dispatcher() {
		let mut transport = ScriptedDiscovery {
			stored: [0xdd; 20],
			target: || address_answer(0xcc),
			protocol_registry: view_reverted,
		};
		let found = futures::executor::block_on(discover_pop_controller(&mut transport))
			.expect("discovery");
		assert_eq!(found, Some([0xcc; 20]), "TARGET() names the controller");
	}

	#[test]
	fn discovery_uses_the_stored_address_once_the_pallet_points_at_the_controller() {
		let mut transport = ScriptedDiscovery {
			stored: [0xcc; 20],
			target: || panic!("must not probe the dispatcher view"),
			protocol_registry: || address_answer(0x9e),
		};
		let found = futures::executor::block_on(discover_pop_controller(&mut transport))
			.expect("discovery");
		assert_eq!(
			found,
			Some([0xcc; 20]),
			"protocolRegistry() answers, so the stored address is the controller"
		);
	}

	#[test]
	fn discovery_reports_an_address_that_is_neither() {
		let mut transport = ScriptedDiscovery {
			stored: [0xab; 20],
			target: view_reverted,
			protocol_registry: view_reverted,
		};
		let err = futures::executor::block_on(discover_pop_controller(&mut transport))
			.expect_err("neither");
		assert!(err.to_string().contains("neither protocolRegistry() nor"), "{err}");
	}

	#[test]
	fn discovery_separates_a_failed_confirmation_from_a_wrong_contract() {
		let mut transport = ScriptedDiscovery {
			stored: [0xdd; 20],
			target: || Err(DotnsViewError::Failed("node unreachable".into())),
			protocol_registry: view_reverted,
		};
		let err = futures::executor::block_on(discover_pop_controller(&mut transport))
			.expect_err("failure");
		assert_eq!(
			err,
			DotnsError::DispatcherTarget(DotnsCallError::View(DotnsViewError::Failed(
				"node unreachable".into()
			)))
		);
	}

	#[test]
	fn registry_reads_propagate_a_transport_failure_instead_of_guessing() {
		let mut transport = ScriptedDiscovery {
			stored: [0xcc; 20],
			target: || panic!("must not fall back on a transport failure"),
			protocol_registry: || Err(DotnsViewError::Failed("node unreachable".into())),
		};
		for error in [
			futures::executor::block_on(discover_pop_controller(&mut transport)).unwrap_err(),
			futures::executor::block_on(resolve_labels(&mut transport, &[0xcc; 20], &[0xaa; 32]))
				.unwrap_err(),
		] {
			assert_eq!(
				error,
				DotnsError::ProtocolRegistry(DotnsCallError::View(DotnsViewError::Failed(
					"node unreachable".into()
				)))
			);
		}
	}

	/// A transport answering views by selector: `tld()` with a scripted
	/// result, `get(bytes32)` with a registry address, `recordExists(bytes32)`
	/// with a scripted bool.
	struct ScriptedTld {
		tld: fn() -> Result<Vec<u8>, DotnsViewError>,
		dot_record_exists: bool,
	}

	#[crate::platform::async_trait]
	impl DotnsTransport for ScriptedTld {
		async fn storage(&mut self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
			Ok(None)
		}

		async fn view(
			&mut self,
			_dest: &[u8; 20],
			input: Vec<u8>,
		) -> Result<Vec<u8>, DotnsViewError> {
			if decoded::<ProtocolRegistry::tldCall>(&input).is_some() {
				(self.tld)()
			} else if decoded::<ProtocolRegistry::getCall>(&input).is_some() {
				returns::<ProtocolRegistry::getCall>(Address::repeat_byte(0xd0))
			} else if let Some(asked) = decoded::<DotnsRegistry::recordExistsCall>(&input) {
				assert_eq!(asked.node, tld_node(TLD_WITHOUT_VIEW));
				returns::<DotnsRegistry::recordExistsCall>(self.dot_record_exists)
			} else {
				panic!("unscripted view {}", hex::encode(input));
			}
		}
	}

	#[test]
	fn the_tld_falls_back_only_when_the_view_reverts_and_the_record_exists() {
		let registry = [0x9e; 20];
		let reverted = view_reverted;

		let served = futures::executor::block_on(network_tld(
			&mut ScriptedTld {
				tld: || returns::<ProtocolRegistry::tldCall>(".paseo".to_string()),
				dot_record_exists: false,
			},
			&registry,
		));
		assert_eq!(served.as_deref(), Ok(".paseo"));

		// The fallback is verified: the registry must hold the ".dot" record.
		let verified = futures::executor::block_on(network_tld(
			&mut ScriptedTld { tld: reverted, dot_record_exists: true },
			&registry,
		));
		assert_eq!(verified.as_deref(), Ok(TLD_WITHOUT_VIEW));

		let unverified = futures::executor::block_on(network_tld(
			&mut ScriptedTld { tld: reverted, dot_record_exists: false },
			&registry,
		));
		assert!(unverified.is_err(), "{unverified:?}");

		// A transport failure is not an answer: no guessing.
		let failed = futures::executor::block_on(network_tld(
			&mut ScriptedTld {
				tld: || Err(DotnsViewError::Failed("timed out".to_string())),
				dot_record_exists: true,
			},
			&registry,
		));
		assert!(failed.is_err(), "{failed:?}");
	}

	/// The full availability walk, scripted: tld → get(registrar) → exists.
	struct ScriptedAvailability {
		exists: fn() -> Result<Vec<u8>, DotnsViewError>,
	}

	#[crate::platform::async_trait]
	impl DotnsTransport for ScriptedAvailability {
		async fn storage(&mut self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
			Ok(None)
		}

		async fn view(
			&mut self,
			_dest: &[u8; 20],
			input: Vec<u8>,
		) -> Result<Vec<u8>, DotnsViewError> {
			if decoded::<DotnsPopController::protocolRegistryCall>(&input).is_some() {
				returns::<DotnsPopController::protocolRegistryCall>(Address::repeat_byte(0x9e))
			} else if decoded::<ProtocolRegistry::tldCall>(&input).is_some() {
				returns::<ProtocolRegistry::tldCall>(".paseo".to_string())
			} else if decoded::<ProtocolRegistry::getCall>(&input).is_some() {
				returns::<ProtocolRegistry::getCall>(Address::repeat_byte(0xd1))
			} else if let Some(asked) = decoded::<DotnsRegistrar::existsCall>(&input) {
				let node = namehash_under(&tld_node(".paseo"), "george");
				assert_eq!(asked.tokenId, U256::from_be_bytes(node));
				(self.exists)()
			} else {
				panic!("unscripted view {}", hex::encode(input));
			}
		}
	}

	/// `available` must fail closed: a registrar that reverts (a wrong or
	/// undeployed entry) is an error, never "available", or the already-minted
	/// guard would pass for every name.
	#[test]
	fn availability_fails_closed_when_the_registrar_does_not_answer() {
		let controller = [0xc0; 20];
		let run = |exists: fn() -> Result<Vec<u8>, DotnsViewError>| {
			futures::executor::block_on(label_available(
				&mut ScriptedAvailability { exists },
				&controller,
				"george",
			))
		};

		assert_eq!(run(|| returns::<DotnsRegistrar::existsCall>(false)), Ok(true));
		assert_eq!(run(|| returns::<DotnsRegistrar::existsCall>(true)), Ok(false));
		let reverted = run(view_reverted);
		assert!(reverted.is_err(), "{reverted:?}");
		let failed = run(|| Err(DotnsViewError::Failed("timed out".to_string())));
		assert!(failed.is_err(), "{failed:?}");
	}

	#[test]
	fn view_output_separates_reverts_from_failures() {
		let reverted = encode_revive_call_output(true, Vec::new());
		for (output, expected, message) in [
			(
				reverted,
				DotnsViewError::Reverted(DotnsContractError::Reverted {
					detail: "(empty)".to_string(),
				}),
				"contract reverted: (empty)",
			),
			(
				vec![0x01],
				DotnsViewError::Contract(DotnsContractError::Malformed(
					ContractResult::decode(&mut &[0x01][..]).err().expect("truncated"),
				)),
				"ReviveApi_call output is not a ContractResult",
			),
		] {
			let error = view_output(&output).unwrap_err();
			assert_eq!(error, expected);
			let error = DotnsError::ProtocolRegistry(DotnsCallError::View(error));
			let chain = core::iter::successors(Some(&error as &dyn core::error::Error), |error| {
				error.source()
			})
			.map(ToString::to_string)
			.collect::<Vec<_>>();
			assert_eq!(
				(chain, error.reason()),
				(
					vec!["DotnsPopController.protocolRegistry()".to_string(), message.to_string()],
					format!("DotnsPopController.protocolRegistry(): {message}")
				)
			);
		}
		let returned = encode_revive_call_output(false, vec![1, 2, 3, 4]);
		assert_eq!(view_output(&returned).unwrap(), vec![1, 2, 3, 4]);
	}

	#[test]
	fn store_labels_lose_the_network_tld_and_subnames_are_dropped() {
		assert_eq!(bare_store_label("alice01.paseo", ".paseo"), Some("alice01"));
		assert_eq!(bare_store_label("alice01.dot", ".dot"), Some("alice01"));
		// Wrong TLD, subname, or nothing left after the TLD.
		assert_eq!(bare_store_label("alice01.dot", ".paseo"), None);
		assert_eq!(bare_store_label("app.alice.paseo", ".paseo"), None);
		assert_eq!(bare_store_label(".paseo", ".paseo"), None);
		// A dotted lite label survives the strip; other dotted remainders
		// stay subnames.
		assert_eq!(bare_store_label("alice.01.paseo", ".paseo"), Some("alice.01"));
		assert_eq!(bare_store_label("sub.alice.01.paseo", ".paseo"), None);
		// An empty TLD leaves bare labels as they are.
		assert_eq!(bare_store_label("alice01", ""), Some("alice01"));
	}

	#[test]
	fn storage_keys_have_the_expected_layout() {
		// Blake2_128Concat over the SCALE-encoded BaseLabel (compact length ‖ bytes).
		let owner_key = lite_label_owner_key(b"alice.01");
		let encoded = [&[0x20u8][..], b"alice.01"].concat();
		assert_eq!(
			&owner_key[..32],
			[twox_128(b"DotnsGateway").as_slice(), twox_128(b"LiteLabelOwner").as_slice(),]
				.concat()
		);
		assert_eq!(&owner_key[32..48], &blake2_128(&encoded));
		assert_eq!(&owner_key[48..], &encoded);

		let alias_key = account_alias_key(&[0x11; 32]);
		let prefix =
			[twox_128(b"DotnsGateway").as_slice(), twox_128(b"AccountAlias").as_slice()].concat();
		assert_eq!(&alias_key[..32], prefix.as_slice());
		// Blake2_128Concat over the raw account bytes.
		assert_eq!(&alias_key[48..], &[0x11; 32]);

		assert_eq!(dispatcher_address_key().len(), 32);
		assert_eq!(timestamp_now_key().len(), 32);
	}
}
