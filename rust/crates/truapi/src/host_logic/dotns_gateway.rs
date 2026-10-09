//! dotNS gateway (Asset Hub) payload building and contract reads.
//!
//! Every host shares these helpers. They cover the candidate-signed
//! reservation message for `pallet_dotns_gateway::reserve_name`, the ring-VRF
//! proof message for `register_name`, the minimal Solidity ABI surface for
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

use parity_scale_codec::{Compact, Decode, Encode};
use sp_crypto_hashing::{blake2_128, blake2_256, keccak_256, twox_128};
use thiserror::Error;
use tracing::warn;

use crate::dotns_views::{call_string, network_tld, tld_node};

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
#[derive(Debug, Clone, PartialEq, Eq, Encode)]
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

/// Encodes the `DotnsGateway.register_name { who, label, link }` call.
/// Layout: dispatch indices, the raw account, the compact-prefixed label, then
/// the link.
pub fn encode_register_name_call(
    call_indices: [u8; 2],
    who: &[u8; 32],
    label: &[u8],
    link: &Link,
) -> Vec<u8> {
    let mut call = call_indices.to_vec();
    (who, label, link).encode_to(&mut call);
    call
}

/// Encodes the `AsDotnsGateway` extension extra.
///
/// The value is `Some(RegisterFullName { proof, ring_index, revision,
/// signature })`; `revision` is the People-collection root revision the proof
/// was built against, which Asset Hub's `members-subscriber` must hold. The
/// signature is wrapped as `MultiSignature::Sr25519`.
pub fn encode_register_full_name_extra(
    info_variant: u8,
    proof: &[u8],
    ring_index: u32,
    revision: u32,
    signature: &[u8; 64],
) -> Vec<u8> {
    // Option::Some ‖ info variant ‖ Vec(proof) ‖ ring_index ‖ revision ‖
    // Sr25519 ‖ sig.
    let mut extra = vec![0x01, info_variant];
    proof.encode_to(&mut extra);
    ring_index.encode_to(&mut extra);
    revision.encode_to(&mut extra);
    extra.push(0x01);
    extra.extend_from_slice(signature);
    extra
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

/// Solidity `bytes32("name")` literal: ASCII left-aligned in a 32-byte word.
/// The dotNS protocol registry keys (`storeFactory`, `registrar`, ...) use this
/// form. They are not hashes.
pub fn registry_key(name: &str) -> [u8; 32] {
    let bytes = name.as_bytes();
    assert!(bytes.len() <= 32, "registry key exceeds 32 bytes");
    let mut out = [0u8; 32];
    out[..bytes.len()].copy_from_slice(bytes);
    out
}

/// First 4 bytes of `keccak256(signature)` — the Solidity function selector.
pub fn selector(signature: &str) -> [u8; 4] {
    keccak_256(signature.as_bytes())[..4]
        .try_into()
        .expect("keccak_256 output is 32 bytes, head slice is 4; qed")
}

/// Calldata for a view function without arguments.
pub fn call_no_args(signature: &str) -> Vec<u8> {
    selector(signature).to_vec()
}

/// Calldata for a view function taking one `address` argument.
pub fn call_address(signature: &str, address: &[u8; 20]) -> Vec<u8> {
    let mut data = call_no_args(signature);
    data.extend_from_slice(&[0u8; 12]);
    data.extend_from_slice(address);
    data
}

/// Calldata for a view function taking one `bytes32` argument.
pub fn call_bytes32(signature: &str, word: &[u8; 32]) -> Vec<u8> {
    let mut data = call_no_args(signature);
    data.extend_from_slice(word);
    data
}

/// Calldata for a view function taking two `uint256` arguments.
pub fn call_u256_pair(signature: &str, first: u64, second: u64) -> Vec<u8> {
    let mut data = call_no_args(signature);
    push_u256_pair(&mut data, first, second);
    data
}

/// Calldata for a view function taking an `address` followed by two `uint256`
/// arguments (a paginated per-account view).
pub fn call_address_u256_pair(
    signature: &str,
    address: &[u8; 20],
    first: u64,
    second: u64,
) -> Vec<u8> {
    let mut data = call_address(signature, address);
    push_u256_pair(&mut data, first, second);
    data
}

fn push_u256_pair(data: &mut Vec<u8>, first: u64, second: u64) {
    for value in [first, second] {
        let mut word = [0u8; 32];
        word[24..].copy_from_slice(&value.to_be_bytes());
        data.extend_from_slice(&word);
    }
}

/// Error decoding a dotNS contract round-trip.
#[derive(Debug, Error)]
pub enum DotnsContractError {
    /// `ContractResult` bytes ended before the expected field.
    #[error("ContractResult truncated at {context}")]
    Truncated {
        /// Field being decoded when the input ended.
        context: &'static str,
    },
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
    #[error("ABI decode failed at {context}")]
    Abi {
        /// Value being decoded when the shape mismatched.
        context: &'static str,
    },
    /// ABI return data was not valid UTF-8 where a string was expected.
    #[error("ABI string is not UTF-8")]
    AbiUtf8,
}

/// SCALE arguments for the `ReviveApi_call` runtime API.
/// A zero-value, unlimited-gas dry-run of `input` against `dest` from `origin`.
pub fn encode_revive_call(origin: &[u8; 32], dest: &[u8; 20], input: &[u8]) -> Vec<u8> {
    let mut args = Vec::with_capacity(32 + 20 + 16 + 2 + 5 + input.len());
    args.extend_from_slice(origin);
    args.extend_from_slice(dest);
    args.extend_from_slice(&0u128.to_le_bytes());
    // gas_limit: Option<Weight> and storage_deposit_limit: Option<u128>.
    // Both absent, so the node estimates.
    args.push(0x00);
    args.push(0x00);
    input.encode_to(&mut args);
    args
}

/// Extracts the contract return data from a `ReviveApi_call` output.
/// The output is a `pallet_revive::ContractResult`. Dispatch errors and reverts
/// are rejected.
pub fn decode_revive_call_output(output: &[u8]) -> Result<Vec<u8>, DotnsContractError> {
    let truncated = |context| DotnsContractError::Truncated { context };
    let input = &mut &output[..];
    // weight_consumed + weight_required. Weight is a compact ref_time plus a
    // compact proof_size.
    for _ in 0..4 {
        Compact::<u64>::decode(input).map_err(|_| truncated("weight"))?;
    }
    // storage_deposit + max_storage_deposit. StorageDeposit is variant ‖ u128.
    for _ in 0..2 {
        <(u8, u128)>::decode(input).map_err(|_| truncated("storage_deposit"))?;
    }
    u128::decode(input).map_err(|_| truncated("gas_consumed"))?;
    match u8::decode(input).map_err(|_| truncated("result"))? {
        0 => {
            let flags = u32::decode(input).map_err(|_| truncated("flags"))?;
            let data = Vec::<u8>::decode(input).map_err(|_| truncated("data"))?;
            // ReturnFlags::REVERT.
            if flags & 1 == 1 {
                return Err(DotnsContractError::Reverted {
                    detail: describe_revert(&data),
                });
            }
            Ok(data)
        }
        _ => {
            Err(DotnsContractError::Dispatch {
                detail: format!("0x{}", hex::encode(*input)),
            })
        }
    }
}

/// Renders a Solidity revert payload. Recognizes `Error(string)` and
/// `Panic(uint256)`.
fn describe_revert(data: &[u8]) -> String {
    if data.is_empty() {
        return "(empty)".to_string();
    }
    if data.starts_with(&[0x08, 0xc3, 0x79, 0xa0]) && data.len() >= 68 {
        let len = u64::from_be_bytes(
            data[60..68]
                .try_into()
                .expect("8-byte slice from a length-checked buffer; qed"),
        ) as usize;
        if let Some(message) = 68usize
            .checked_add(len)
            .and_then(|end| data.get(68..end))
            .and_then(|bytes| core::str::from_utf8(bytes).ok())
        {
            return format!("Error({message:?})");
        }
    }
    if data.starts_with(&[0x4e, 0x48, 0x7b, 0x71]) && data.len() >= 36 {
        return format!("Panic(0x{})", hex::encode(&data[32..36]));
    }
    format!("0x{}", hex::encode(data))
}

/// Reads the 32-byte ABI word at index `index`.
fn word(data: &[u8], index: usize) -> Result<&[u8], DotnsContractError> {
    data.get(index * 32..(index + 1) * 32)
        .ok_or(DotnsContractError::Abi { context: "word" })
}

/// Interprets an ABI word as a right-aligned usize offset or length.
fn word_usize(data: &[u8], index: usize) -> Result<usize, DotnsContractError> {
    usize::try_from(word_u64(data, index)?).map_err(|_| {
        DotnsContractError::Abi {
            context: "oversized word",
        }
    })
}

/// Decodes an ABI `string` at byte offset `at`.
/// The layout is a length word followed by padded UTF-8 bytes.
fn decode_string_at(data: &[u8], at: usize) -> Result<String, DotnsContractError> {
    let region = data.get(at..).ok_or(DotnsContractError::Abi {
        context: "string offset",
    })?;
    let len = word_usize(region, 0)?;
    let bytes = 32usize
        .checked_add(len)
        .and_then(|end| region.get(32..end))
        .ok_or(DotnsContractError::Abi {
            context: "string bytes",
        })?;
    String::from_utf8(bytes.to_vec()).map_err(|_| DotnsContractError::AbiUtf8)
}

/// Decodes a single `string` return value (`ProtocolRegistry.tld()`).
pub fn decode_string(data: &[u8]) -> Result<String, DotnsContractError> {
    decode_string_at(data, word_usize(data, 0)?)
}

/// Decodes a single `bool` return value.
pub fn decode_bool(data: &[u8]) -> Result<bool, DotnsContractError> {
    let bytes = word(data, 0)?;
    if bytes[..31].iter().any(|b| *b != 0) {
        return Err(DotnsContractError::Abi { context: "bool" });
    }
    match bytes[31] {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(DotnsContractError::Abi { context: "bool" }),
    }
}

/// Decodes a single `address` return value.
pub fn decode_address(data: &[u8]) -> Result<[u8; 20], DotnsContractError> {
    let bytes = word(data, 0)?;
    if bytes[..12].iter().any(|b| *b != 0) {
        return Err(DotnsContractError::Abi { context: "address" });
    }
    Ok(bytes[12..]
        .try_into()
        .expect("20-byte tail of a 32-byte word; qed"))
}

/// Decodes a `string[]` return value (`LabelStore.getLabels`).
pub fn decode_string_array(data: &[u8]) -> Result<Vec<String>, DotnsContractError> {
    let array_at = word_usize(data, 0)?;
    let array = data.get(array_at..).ok_or(DotnsContractError::Abi {
        context: "array offset",
    })?;
    let len = word_usize(array, 0)?;
    let elements = array.get(32..).ok_or(DotnsContractError::Abi {
        context: "array elements",
    })?;
    (0..len)
        .map(|i| decode_string_at(elements, word_usize(elements, i)?))
        .collect()
}

/// Decodes one `DotnsPopController.pendingClaims(address,uint256,uint256)`
/// page, an ABI `(string label, uint64 mintedAt)[]`, as `(label, minted_at)`
/// pairs.
pub fn decode_pending_claims_array(data: &[u8]) -> Result<Vec<(String, u64)>, DotnsContractError> {
    let array_at = word_usize(data, 0)?;
    let array = data.get(array_at..).ok_or(DotnsContractError::Abi {
        context: "claims offset",
    })?;
    let len = word_usize(array, 0)?;
    let elements = array.get(32..).ok_or(DotnsContractError::Abi {
        context: "claims elements",
    })?;
    (0..len)
        .map(|i| {
            let claim =
                elements
                    .get(word_usize(elements, i)?..)
                    .ok_or(DotnsContractError::Abi {
                        context: "claim offset",
                    })?;
            let label = decode_string_at(claim, word_usize(claim, 0)?)?;
            let minted_at = word_u64(claim, 1)?;
            Ok((label, minted_at))
        })
        .collect()
}

/// Decodes a single `uint64` (or narrower) return value.
pub fn decode_u64(data: &[u8]) -> Result<u64, DotnsContractError> {
    word_u64(data, 0)
}

/// Reads the 32-byte ABI word at `index` as a `u64`; wider values are an error.
fn word_u64(data: &[u8], index: usize) -> Result<u64, DotnsContractError> {
    let bytes = word(data, index)?;
    if bytes[..24].iter().any(|b| *b != 0) {
        return Err(DotnsContractError::Abi {
            context: "oversized word",
        });
    }
    Ok(u64::from_be_bytes(
        bytes[24..]
            .try_into()
            .expect("8-byte tail of a 32-byte word; qed"),
    ))
}

/// Usernames of one account as recorded by the dotNS contracts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DotnsIdentity {
    /// Dotted lite username (`alice.01`), if any.
    pub lite_username: Option<String>,
    /// Full-person username, if any.
    pub full_username: Option<String>,
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
/// strips the network TLD.
pub async fn classify_labels<T, I>(
    transport: &mut T,
    controller: &[u8; 20],
    labels: I,
) -> Result<DotnsIdentity, String>
where
    T: DotnsTransport + ?Sized,
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    let mut identity = DotnsIdentity::default();
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
            identity
                .lite_username
                .get_or_insert_with(|| label.to_string());
        } else {
            identity
                .full_username
                .get_or_insert_with(|| label.to_string());
        }
    }
    Ok(identity)
}

/// Whether the PoP controller issued `label`, per
/// `DotnsPopController.isPopIssued(string)`.
pub async fn is_pop_issued<T: DotnsTransport + ?Sized>(
    transport: &mut T,
    controller: &[u8; 20],
    label: &str,
) -> Result<bool, String> {
    let output = transport
        .view(controller, call_string("isPopIssued(string)", label))
        .await
        .map_err(|err| format!("DotnsPopController.isPopIssued({label}): {err}"))?;
    decode_bool(&output).map_err(|err| format!("DotnsPopController.isPopIssued({label}): {err}"))
}

/// RFC 1035 label bound the contracts enforce (`StringUtils.MAX_DNS_LABEL_OCTETS`).
const MAX_DNS_LABEL_LEN: usize = 63;

/// A canonical DNS label: lowercase ASCII letters, digits and hyphens, neither
/// starting nor ending with a hyphen, at most 63 octets
/// (`StringUtils._isDnsLabel`).
pub fn is_dns_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_DNS_LABEL_LEN
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The dotted lite shape: a letters-only stem, one dot, exactly two digits
/// (`StringUtils.isLitePersonLabel`; the stem follows `_isPersonLabel`, so a
/// digit or hyphen in it is not a lite label). Shape only — see
/// [`is_dotted_lite_username`] for the gateway's byte bound on top.
pub fn is_lite_label(label: &str) -> bool {
    match label.rsplit_once('.') {
        Some((stem, digits)) => {
            !stem.is_empty()
                && stem.len() <= MAX_DNS_LABEL_LEN
                && stem.chars().all(|c| c.is_ascii_lowercase())
                && digits.len() == 2
                && digits.chars().all(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

/// Longest label the gateway pallet accepts (`BaseLabel = BoundedVec<u8, 32>`).
pub const MAX_BASE_LABEL_LEN: usize = 32;

/// Whether `label` is a full-person base label as the gateway pallet accepts
/// it (`is_person_label`): non-empty, lowercase ASCII letters only, no digits
/// or hyphens, at most [`MAX_BASE_LABEL_LEN`] bytes.
pub fn is_full_person_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= MAX_BASE_LABEL_LEN
        && label.chars().all(|c| c.is_ascii_lowercase())
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

/// Why a contract view returned no data.
///
/// A revert is an answer from the contract (the view refused, or the address
/// has no such function); anything else means the answer is unknown.
#[derive(Debug, Error)]
pub enum DotnsViewError {
    /// The contract executed and reverted.
    #[error("{0}")]
    Reverted(DotnsContractError),
    /// The view could not be dry-run or its output could not be decoded.
    #[error("{0}")]
    Failed(String),
}

impl From<DotnsViewError> for String {
    fn from(error: DotnsViewError) -> Self {
        error.to_string()
    }
}

/// Splits a `ReviveApi_call` output into return data, a revert, or a failure,
/// for [`DotnsTransport::view`] implementations.
pub fn view_output(output: &[u8]) -> Result<Vec<u8>, DotnsViewError> {
    decode_revive_call_output(output).map_err(|err| {
        match err {
            DotnsContractError::Reverted { .. } => DotnsViewError::Reverted(err),
            other => DotnsViewError::Failed(other.to_string()),
        }
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
    async fn storage(&mut self, key: Vec<u8>) -> Result<Option<Vec<u8>>, String>;

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
) -> Result<Option<[u8; 20]>, String> {
    let Some(value) = transport.storage(dispatcher_address_key()).await? else {
        return Ok(None);
    };
    let stored: [u8; 20] = value.try_into().map_err(|value: Vec<u8>| {
        format!("DotnsGateway.DispatcherAddress is {} bytes", value.len())
    })?;
    match transport
        .view(&stored, call_no_args("protocolRegistry()"))
        .await
    {
        Ok(output) => {
            decode_address(&output)
                .map_err(|err| format!("DotnsPopController.protocolRegistry(): {err}"))?;
            Ok(Some(stored))
        }
        // A chain still storing its dispatcher pays this second view; a repointed one
        // answers on the first. Both hops go once no chain stores a dispatcher.
        Err(DotnsViewError::Reverted(_)) => {
            match transport.view(&stored, call_no_args("TARGET()")).await {
                Ok(output) => {
                    decode_address(&output)
                        .map(Some)
                        .map_err(|err| format!("RootGatewayDispatcher.TARGET(): {err}"))
                }
                Err(DotnsViewError::Reverted(_)) => {
                    Err(format!(
                        "DotnsGateway.DispatcherAddress {} has neither protocolRegistry() nor \
                     TARGET()",
                        hex::encode(stored)
                    ))
                }
                Err(DotnsViewError::Failed(reason)) => {
                    Err(format!("RootGatewayDispatcher.TARGET(): {reason}"))
                }
            }
        }
        Err(err @ DotnsViewError::Failed(_)) => {
            Err(format!("DotnsPopController.protocolRegistry(): {err}"))
        }
    }
}

/// Resolves the bare contract labels `account` holds.
///
/// Two sources are merged. The controller's pending claims hold gateway-minted
/// names the user has not settled into a `LabelStore` yet (`claimLabelStore`);
/// those come first. A claim older than the controller's `reservationDuration`
/// is lapsed — `claimLabelStore` skips it and `expirePendingClaim` will sweep
/// it — so it is not a username here either, whether or not it has been swept. The user's `LabelStore`, when deployed, holds every name
/// written for them: gateway names once settled, plus public registrations and
/// incoming transfers. Store labels carry the network TLD (`alice01.paseo`),
/// which is stripped here; subnames (`app.alice`) are dropped.
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
) -> Result<Vec<String>, String> {
    let user = account_to_h160(account);

    let mut labels = pending_claim_labels(transport, controller, &user).await?;

    let registry_output = transport
        .view(controller, call_no_args("protocolRegistry()"))
        .await
        .map_err(|err| format!("DotnsPopController.protocolRegistry: {err}"))?;
    let registry = decode_address(&registry_output)
        .map_err(|err| format!("DotnsPopController.protocolRegistry(): {err}"))?;

    let factory_output = transport
        .view(
            &registry,
            call_bytes32("get(bytes32)", &registry_key("storeFactory")),
        )
        .await
        .map_err(|err| format!("ProtocolRegistry.get(storeFactory): {err}"))?;
    let factory = decode_address(&factory_output)
        .map_err(|err| format!("ProtocolRegistry.get(storeFactory): {err}"))?;

    let store_output = transport
        .view(&factory, call_address("getLabelStore(address)", &user))
        .await
        .map_err(|err| format!("StoreFactory.getLabelStore: {err}"))?;
    let store = decode_address(&store_output)
        .map_err(|err| format!("StoreFactory.getLabelStore: {err}"))?;
    if store == [0u8; 20] {
        return Ok(labels);
    }

    let tld = network_tld(transport, &registry).await?;

    for page in 0..LABEL_PAGE_MAX {
        let labels_output = transport
            .view(
                &store,
                call_u256_pair(
                    "getLabels(uint256,uint256)",
                    page * LABEL_PAGE_LIMIT,
                    LABEL_PAGE_LIMIT,
                ),
            )
            .await
            .map_err(|err| format!("LabelStore.getLabels: {err}"))?;
        let stored = decode_string_array(&labels_output)
            .map_err(|err| format!("LabelStore.getLabels: {err}"))?;
        let short_page = (stored.len() as u64) < LABEL_PAGE_LIMIT;
        for label in stored {
            if let Some(bare) = bare_store_label(&label, &tld)
                && !labels.iter().any(|known| known == bare)
            {
                labels.push(bare.to_string());
            }
        }
        if short_page {
            break;
        }
        if page + 1 == LABEL_PAGE_MAX {
            warn!(
                store = %hex::encode(store),
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
    let bare = if tld.is_empty() {
        label
    } else {
        label.strip_suffix(tld)?
    };
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
) -> Result<bool, String> {
    let registry_output = transport
        .view(controller, call_no_args("protocolRegistry()"))
        .await?;
    let registry = decode_address(&registry_output)
        .map_err(|err| format!("DotnsPopController.protocolRegistry(): {err}"))?;

    let tld = network_tld(transport, &registry).await?;

    let registrar_output = transport
        .view(
            &registry,
            call_bytes32("get(bytes32)", &registry_key("registrar")),
        )
        .await?;
    let registrar = decode_address(&registrar_output)
        .map_err(|err| format!("ProtocolRegistry.get(registrar): {err}"))?;

    let node = namehash_under(&tld_node(&tld), label);
    let output = transport
        .view(&registrar, call_bytes32("exists(uint256)", &node))
        .await
        .map_err(|err| format!("DotnsRegistrar.exists({label}): {err}"))?;
    decode_bool(&output)
        .map(|exists| !exists)
        .map_err(|err| format!("DotnsRegistrar.exists({label}): {err}"))
}

/// Gateway-minted labels of `user` still waiting for `claimLabelStore`, paged
/// out of `DotnsPopController.pendingClaims(address,uint256,uint256)`, without
/// the entries that have lapsed (`mintedAt + reservationDuration < now`, the
/// controller's own `_isExpired`; `now` is `Timestamp.Now` at the pinned block).
async fn pending_claim_labels<T: DotnsTransport + ?Sized>(
    transport: &mut T,
    controller: &[u8; 20],
    user: &[u8; 20],
) -> Result<Vec<String>, String> {
    let mut claims = Vec::new();
    for page in 0..CLAIM_PAGE_MAX {
        let output = match transport
            .view(
                controller,
                call_address_u256_pair(
                    "pendingClaims(address,uint256,uint256)",
                    user,
                    page * CLAIM_PAGE_LIMIT,
                    CLAIM_PAGE_LIMIT,
                ),
            )
            .await
        {
            Ok(output) => output,
            Err(DotnsViewError::Reverted(reason)) => {
                warn!(
                    %reason,
                    retained = claims.len(),
                    "DotnsPopController.pendingClaims page reverted; retaining earlier pages"
                );
                break;
            }
            Err(DotnsViewError::Failed(reason)) => {
                return Err(format!("DotnsPopController.pendingClaims: {reason}"));
            }
        };
        let page_claims = decode_pending_claims_array(&output)
            .map_err(|err| format!("DotnsPopController.pendingClaims: {err}"))?;
        let short_page = (page_claims.len() as u64) < CLAIM_PAGE_LIMIT;
        claims.extend(page_claims);
        if short_page {
            break;
        }
        if page + 1 == CLAIM_PAGE_MAX {
            warn!(
                user = %hex::encode(user),
                read = CLAIM_PAGE_MAX * CLAIM_PAGE_LIMIT,
                "pending-claim page cap reached; later entries, if any, are not resolved"
            );
        }
    }
    if claims.is_empty() {
        return Ok(Vec::new());
    }
    let duration_output = transport
        .view(controller, call_no_args("reservationDuration()"))
        .await
        .map_err(|err| format!("DotnsPopController.reservationDuration: {err}"))?;
    let duration = decode_u64(&duration_output)
        .map_err(|err| format!("DotnsPopController.reservationDuration: {err}"))?;
    let now = chain_time_secs(transport).await?;
    Ok(claims
        .into_iter()
        .filter(|(_, minted_at)| !claim_lapsed(*minted_at, duration, now))
        .map(|(label, _)| label)
        .collect())
}

/// Whether a pending claim minted at `minted_at` has lapsed at chain time
/// `now`, mirroring `DotnsPopController._isExpired`.
fn claim_lapsed(minted_at: u64, duration: u64, now: u64) -> bool {
    minted_at.saturating_add(duration) < now
}

/// Asset Hub chain time in Unix seconds, from `Timestamp.Now` (milliseconds).
async fn chain_time_secs<T: DotnsTransport + ?Sized>(transport: &mut T) -> Result<u64, String> {
    let value = transport
        .storage(timestamp_now_key())
        .await?
        .ok_or("Timestamp.Now is unset")?;
    let millis: [u8; 8] = value
        .as_slice()
        .try_into()
        .map_err(|_| "Timestamp.Now is not a u64".to_string())?;
    Ok(u64::from_le_bytes(millis) / 1000)
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

    // Golden vectors from the validated reference implementation in
    // pop-dotns-testing: reservation.ts, proof.ts, verify.ts, abi.ts.
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
        assert_eq!(
            hex::encode(standalone.encode()),
            format!("01{}", hex::encode([0x55; 65]))
        );
        assert_eq!(
            hex::encode(build_register_proof_message(
                &[0x44; 32],
                b"alicebc",
                &standalone
            )),
            "255da65ea6687123d5ac035a438a5c6fa18f7d04f23433611095a0fe58c479ab"
        );
    }

    #[test]
    fn register_name_call_and_extension_extra_have_the_reference_layout() {
        let link = Link::LiteUsername(b"alice.01".to_vec());
        let call = encode_register_name_call([0x6a, 0x01], &[0x44; 32], b"alicebc", &link);
        assert_eq!(&call[..2], &[0x6a, 0x01]);
        assert_eq!(&call[2..34], &[0x44; 32]);
        assert_eq!(
            &call[34..],
            &[b"alicebc".to_vec().encode(), link.encode()].concat()[..]
        );

        // Layout: 0x01 (Some) ‖ variant ‖ compact-len proof ‖ ring_index LE ‖
        // revision LE ‖ 0x01 (Sr25519) ‖ signature, the AsDotnsGatewayInfo::
        // RegisterFullName field order.
        let extra = encode_register_full_name_extra(0, &[0xEE; 785], 3, 5, &[0xAB; 64]);
        assert_eq!(
            extra,
            [
                vec![0x01, 0x00],
                vec![0xEEu8; 785].encode(),
                3u32.to_le_bytes().to_vec(),
                5u32.to_le_bytes().to_vec(),
                vec![0x01],
                vec![0xAB; 64],
            ]
            .concat()
        );
    }

    #[test]
    fn selectors_match_reference_vectors() {
        assert_eq!(hex::encode(selector("getLabelStore(address)")), "4fc5dce9");
        assert_eq!(
            hex::encode(selector("getLabels(uint256,uint256)")),
            "2d7b5794"
        );
        assert_eq!(hex::encode(selector("protocolRegistry()")), "7656419f");
        assert_eq!(hex::encode(selector("get(bytes32)")), "8eaa6ac0");
        assert_eq!(hex::encode(selector("TARGET()")), "cc1f2afa");
        assert_eq!(
            hex::encode(selector("pendingClaims(address,uint256,uint256)")),
            "76025b85"
        );
        assert_eq!(hex::encode(selector("tld()")), "2d551432");
        assert_eq!(hex::encode(selector("recordExists(bytes32)")), "f79fe538");
        assert_eq!(hex::encode(selector("ownerOf(uint256)")), "6352211e");
        assert_eq!(hex::encode(selector("exists(uint256)")), "4f558e79");
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
    fn calldata_encoders_place_arguments_in_padded_words() {
        let address_call = call_address("getLabelStore(address)", &[0xAA; 20]);
        assert_eq!(address_call.len(), 4 + 32);
        assert_eq!(&address_call[..4], &selector("getLabelStore(address)"));
        assert!(address_call[4..16].iter().all(|b| *b == 0));
        assert_eq!(&address_call[16..], &[0xAA; 20]);

        let pair_call = call_u256_pair("getLabels(uint256,uint256)", 0, 16);
        assert_eq!(pair_call.len(), 4 + 64);
        assert_eq!(pair_call[35], 0);
        assert_eq!(pair_call[67], 16);

        let paged_call = call_address_u256_pair(
            "pendingClaims(address,uint256,uint256)",
            &[0xBB; 20],
            32,
            16,
        );
        assert_eq!(paged_call.len(), 4 + 96);
        assert_eq!(
            &paged_call[..4],
            &selector("pendingClaims(address,uint256,uint256)")
        );
        let mut expected_args = vec![0u8; 96];
        expected_args[12..32].fill(0xBB);
        expected_args[63] = 32;
        expected_args[95] = 16;
        assert_eq!(&paged_call[4..], expected_args.as_slice());

        let key_call = call_bytes32("get(bytes32)", &registry_key("storeFactory"));
        assert_eq!(&key_call[4..16], b"storeFactory");
        assert!(key_call[16..36].iter().all(|b| *b == 0));
    }

    #[test]
    fn account_to_h160_follows_the_revive_mapper() {
        let mut eth_derived = [0x0Fu8; 32];
        eth_derived[20..].fill(0xEE);
        assert_eq!(account_to_h160(&eth_derived), [0x0F; 20]);

        let hashed = account_to_h160(&[0x11; 32]);
        assert_eq!(hashed, keccak_256(&[0x11; 32])[12..]);
    }

    /// Encodes a `ContractResult` prefix followed by `result`.
    fn contract_result(result: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for _ in 0..4 {
            Compact(7u64).encode_to(&mut out);
        }
        for _ in 0..2 {
            (1u8, 0u128).encode_to(&mut out);
        }
        0u128.encode_to(&mut out);
        out.extend_from_slice(result);
        out
    }

    #[test]
    fn revive_call_output_surfaces_data_reverts_and_dispatch_errors() {
        let mut ok = vec![0u8];
        0u32.encode_to(&mut ok);
        vec![0xABu8, 0xCD].encode_to(&mut ok);
        assert_eq!(
            decode_revive_call_output(&contract_result(&ok)).unwrap(),
            vec![0xAB, 0xCD]
        );

        // Error("nope") revert payload under ReturnFlags::REVERT.
        let mut revert_data = vec![0x08, 0xc3, 0x79, 0xa0];
        revert_data.extend_from_slice(&{
            let mut word = [0u8; 32];
            word[31] = 0x20;
            word
        });
        revert_data.extend_from_slice(&{
            let mut word = [0u8; 32];
            word[31] = 4;
            word
        });
        revert_data.extend_from_slice(b"nope");
        revert_data.extend_from_slice(&[0u8; 28]);
        let mut reverted = vec![0u8];
        1u32.encode_to(&mut reverted);
        revert_data.encode_to(&mut reverted);
        let err = decode_revive_call_output(&contract_result(&reverted)).unwrap_err();
        assert!(
            matches!(&err, DotnsContractError::Reverted { detail } if detail == "Error(\"nope\")"),
            "unexpected error: {err}"
        );

        let dispatch = contract_result(&[0x01, 0x02, 0x03]);
        assert!(matches!(
            decode_revive_call_output(&dispatch).unwrap_err(),
            DotnsContractError::Dispatch { .. }
        ));
    }

    /// ABI word with `value` right-aligned.
    fn abi_word(value: u64) -> [u8; 32] {
        let mut word = [0u8; 32];
        word[24..].copy_from_slice(&value.to_be_bytes());
        word
    }

    /// ABI string tail: a length word plus padded bytes.
    fn abi_string(value: &str) -> Vec<u8> {
        let mut out = abi_word(value.len() as u64).to_vec();
        out.extend_from_slice(value.as_bytes());
        out.resize(out.len().div_ceil(32) * 32, 0);
        out
    }

    fn abi_pending_claims(values: &[(String, u64)]) -> Vec<u8> {
        let encoded = values
            .iter()
            .map(|(label, minted_at)| {
                [
                    abi_word(0x40).to_vec(),
                    abi_word(*minted_at).to_vec(),
                    abi_string(label),
                ]
                .concat()
            })
            .collect::<Vec<_>>();
        let mut output = abi_word(0x20).to_vec();
        output.extend_from_slice(&abi_word(encoded.len() as u64));
        let mut offset = (encoded.len() * 32) as u64;
        for value in &encoded {
            output.extend_from_slice(&abi_word(offset));
            offset += value.len() as u64;
        }
        for value in encoded {
            output.extend_from_slice(&value);
        }
        output
    }

    #[test]
    fn abi_decoders_handle_addresses_string_arrays_and_pending_claims() {
        let mut address = [0u8; 32];
        address[12..].fill(0xBC);
        assert_eq!(decode_address(&address).unwrap(), [0xBC; 20]);
        assert!(decode_address(&[0xFF; 32]).is_err());

        // string[] = ["alice01", "bob"].
        let mut array = abi_word(0x20).to_vec();
        array.extend_from_slice(&abi_word(2));
        array.extend_from_slice(&abi_word(0x40));
        array.extend_from_slice(&abi_word(0x40 + 0x40));
        array.extend_from_slice(&abi_string("alice01"));
        array.extend_from_slice(&abi_string("bob"));
        assert_eq!(
            decode_string_array(&array).unwrap(),
            vec!["alice01".to_string(), "bob".to_string()]
        );
        assert_eq!(
            decode_string_array(&[abi_word(0x20), abi_word(0)].concat()).unwrap(),
            Vec::<String>::new()
        );

        // pendingClaims = [("alice01", 42), ("bob", 7)].
        let struct_a = [
            abi_word(0x40).to_vec(),
            abi_word(42).to_vec(),
            abi_string("alice01"),
        ]
        .concat();
        let struct_b = [
            abi_word(0x40).to_vec(),
            abi_word(7).to_vec(),
            abi_string("bob"),
        ]
        .concat();
        let mut claims = abi_word(0x20).to_vec();
        claims.extend_from_slice(&abi_word(2));
        claims.extend_from_slice(&abi_word(0x40));
        claims.extend_from_slice(&abi_word(0x40 + struct_a.len() as u64));
        claims.extend_from_slice(&struct_a);
        claims.extend_from_slice(&struct_b);
        assert_eq!(
            decode_pending_claims_array(&claims).unwrap(),
            vec![("alice01".to_string(), 42), ("bob".to_string(), 7)]
        );
        assert_eq!(
            decode_pending_claims_array(&[abi_word(0x20), abi_word(0)].concat()).unwrap(),
            Vec::<(String, u64)>::new()
        );
        assert_eq!(decode_u64(&abi_word(604_800)).unwrap(), 604_800);
        assert!(decode_u64(&[0xff; 32]).is_err());
    }

    #[test]
    fn bool_words_decode_strictly() {
        assert!(decode_bool(&abi_word(1)).unwrap());
        assert!(!decode_bool(&abi_word(0)).unwrap());
        assert!(decode_bool(&abi_word(2)).is_err());
        let mut high = abi_word(1);
        high[5] = 1;
        assert!(decode_bool(&high).is_err());
    }

    #[test]
    fn a_pending_claim_lapses_after_the_reservation_duration() {
        // DotnsPopController._isExpired: mintedAt + reservationDuration < now.
        assert!(!claim_lapsed(1_000, 100, 1_100));
        assert!(claim_lapsed(1_000, 100, 1_101));
        assert!(!claim_lapsed(u64::MAX, 100, u64::MAX));
    }

    struct RevertingSecondClaimPage {
        first_page: Vec<(String, u64)>,
        pending_calls: usize,
    }

    #[crate::platform::async_trait]
    impl DotnsTransport for RevertingSecondClaimPage {
        async fn storage(&mut self, key: Vec<u8>) -> Result<Option<Vec<u8>>, String> {
            assert_eq!(key, timestamp_now_key());
            Ok(Some(100_000u64.to_le_bytes().to_vec()))
        }

        async fn view(
            &mut self,
            _dest: &[u8; 20],
            input: Vec<u8>,
        ) -> Result<Vec<u8>, DotnsViewError> {
            let function: [u8; 4] = input[..4].try_into().expect("selector prefix");
            if function == selector("pendingClaims(address,uint256,uint256)") {
                self.pending_calls += 1;
                let offset = decode_u64(&input[36..68]).expect("offset word");
                let limit = decode_u64(&input[68..100]).expect("limit word");
                assert_eq!(limit, CLAIM_PAGE_LIMIT);
                if offset == 0 {
                    return Ok(abi_pending_claims(&self.first_page));
                }
                assert_eq!(offset, CLAIM_PAGE_LIMIT);
                return Err(DotnsViewError::Reverted(DotnsContractError::Reverted {
                    detail: "offset past end".to_string(),
                }));
            }
            if function == selector("reservationDuration()") {
                return Ok(abi_word(100).to_vec());
            }
            panic!("unscripted view {}", hex::encode(function));
        }
    }

    #[test]
    fn a_later_pending_claim_page_revert_keeps_complete_earlier_pages() {
        let expected = (0..CLAIM_PAGE_LIMIT)
            .map(|index| format!("claim{index:02}"))
            .collect::<Vec<_>>();
        let mut transport = RevertingSecondClaimPage {
            first_page: expected.iter().cloned().map(|label| (label, 50)).collect(),
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
                async fn storage(&mut self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, String> {
                    unreachable!("classify_labels reads no storage")
                }
                async fn view(
                    &mut self,
                    _dest: &[u8; 20],
                    input: Vec<u8>,
                ) -> Result<Vec<u8>, DotnsViewError> {
                    assert_eq!(&input[..4], &selector("isPopIssued(string)"));
                    let issued = !self
                        .denied
                        .iter()
                        .any(|denied| input.windows(denied.len()).any(|w| w == denied.as_bytes()));
                    let mut word = [0u8; 32];
                    word[31] = u8::from(issued);
                    Ok(word.to_vec())
                }
            }
            let controller = [0xAA; 20];
            let mut all = IssuedAll { denied: vec![] };

            let identity = classify_labels(&mut all, &controller, ["alice.01", "myproject"])
                .await
                .unwrap();
            assert_eq!(identity.lite_username.as_deref(), Some("alice.01"));
            assert_eq!(identity.full_username.as_deref(), Some("myproject"));

            // The lite stem is letters only (`isLitePersonLabel`): a digit or
            // hyphen in the stem is not a lite label. The flat spelling is an
            // ordinary label, and dotted non-lite labels are subnames.
            let identity = classify_labels(
                &mut all,
                &controller,
                [
                    "app.web3app",
                    "sub.alice.01",
                    "aé.01",
                    "a2b.34",
                    "-x.01",
                    "alice01",
                ],
            )
            .await
            .unwrap();
            assert_eq!(identity.lite_username, None);
            assert_eq!(identity.full_username.as_deref(), Some("alice01"));

            assert_eq!(
                classify_labels(&mut all, &controller, Vec::<String>::new())
                    .await
                    .unwrap(),
                DotnsIdentity::default()
            );

            // Provenance gates every slot: a perfectly shaped label the
            // controller did not issue is skipped.
            let mut denying = IssuedAll {
                denied: vec!["app.42", "squatter"],
            };
            let identity = classify_labels(
                &mut denying,
                &controller,
                ["app.42", "squatter", "alice.01", "myproject"],
            )
            .await
            .unwrap();
            assert_eq!(identity.lite_username.as_deref(), Some("alice.01"));
            assert_eq!(identity.full_username.as_deref(), Some("myproject"));

            // Hostile store data is dropped before the provenance read: the
            // transport panics on storage and asserts the selector, so reaching
            // it with garbage would fail loudly.
            let identity = classify_labels(
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
            .unwrap();
            assert_eq!(identity, DotnsIdentity::default());

            // The 63-octet DNS bound is the cut-off for full labels; the lite
            // dotted form is capped by the 32-byte gateway bound.
            assert!(
                classify_labels(&mut all, &controller, ["a".repeat(63)])
                    .await
                    .unwrap()
                    .full_username
                    .is_some()
            );
            assert!(
                classify_labels(&mut all, &controller, ["a".repeat(64)])
                    .await
                    .unwrap()
                    .full_username
                    .is_none()
            );
            let oversized = format!("{}.01", "a".repeat(30));
            assert_eq!(
                classify_labels(&mut all, &controller, [oversized])
                    .await
                    .unwrap(),
                DotnsIdentity::default()
            );

            // One digit after the dot is not lite format; three digits neither.
            for wrong in ["alice.1", "alice.012"] {
                assert_eq!(
                    classify_labels(&mut all, &controller, [wrong])
                        .await
                        .unwrap(),
                    DotnsIdentity::default()
                );
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
        async fn storage(&mut self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, String> {
            Ok(Some(self.stored.to_vec()))
        }

        async fn view(
            &mut self,
            _dest: &[u8; 20],
            input: Vec<u8>,
        ) -> Result<Vec<u8>, DotnsViewError> {
            let sel: [u8; 4] = input[..4].try_into().expect("selector prefix; qed");
            if sel == selector("TARGET()") {
                (self.target)()
            } else if sel == selector("protocolRegistry()") {
                (self.protocol_registry)()
            } else {
                panic!("unscripted view {}", hex::encode(sel));
            }
        }
    }

    fn address_word(byte: u8) -> Vec<u8> {
        let mut word = [0u8; 32];
        word[12..].copy_from_slice(&[byte; 20]);
        word.to_vec()
    }

    fn view_reverted() -> Result<Vec<u8>, DotnsViewError> {
        Err(DotnsViewError::Reverted(DotnsContractError::Reverted {
            detail: "(empty)".to_string(),
        }))
    }

    #[test]
    fn discovery_follows_target_when_the_stored_address_is_a_dispatcher() {
        let mut transport = ScriptedDiscovery {
            stored: [0xdd; 20],
            target: || Ok(address_word(0xcc)),
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
            protocol_registry: || Ok(address_word(0x9e)),
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
        assert!(err.contains("neither protocolRegistry() nor"), "{err}");
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
        assert!(err.contains("TARGET(): node unreachable"), "{err}");
        assert!(
            !err.contains("neither"),
            "a node failure is not a wrong contract: {err}"
        );
    }

    #[test]
    fn discovery_propagates_a_transport_failure_instead_of_guessing() {
        let mut transport = ScriptedDiscovery {
            stored: [0xcc; 20],
            target: || panic!("must not fall back on a transport failure"),
            protocol_registry: || Err(DotnsViewError::Failed("node unreachable".into())),
        };
        let err = futures::executor::block_on(discover_pop_controller(&mut transport))
            .expect_err("failure");
        assert!(err.contains("node unreachable"), "{err}");
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
        async fn storage(&mut self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, String> {
            Ok(None)
        }

        async fn view(
            &mut self,
            _dest: &[u8; 20],
            input: Vec<u8>,
        ) -> Result<Vec<u8>, DotnsViewError> {
            let sel: [u8; 4] = input[..4].try_into().expect("selector prefix; qed");
            if sel == selector("tld()") {
                (self.tld)()
            } else if sel == selector("get(bytes32)") {
                let mut word = [0u8; 32];
                word[12..].copy_from_slice(&[0xd0; 20]);
                Ok(word.to_vec())
            } else if sel == selector("recordExists(bytes32)") {
                assert_eq!(&input[4..36], &tld_node(TLD_WITHOUT_VIEW));
                Ok(abi_word(self.dot_record_exists as u64).to_vec())
            } else {
                panic!("unscripted view {}", hex::encode(sel));
            }
        }
    }

    #[test]
    fn the_tld_falls_back_only_when_the_view_reverts_and_the_record_exists() {
        let registry = [0x9e; 20];
        fn reverted() -> Result<Vec<u8>, DotnsViewError> {
            Err(DotnsViewError::Reverted(DotnsContractError::Reverted {
                detail: "(empty)".to_string(),
            }))
        }

        let served = futures::executor::block_on(network_tld(
            &mut ScriptedTld {
                tld: || Ok([abi_word(32).to_vec(), abi_string(".paseo")].concat()),
                dot_record_exists: false,
            },
            &registry,
        ));
        assert_eq!(served.as_deref(), Ok(".paseo"));

        // The fallback is verified: the registry must hold the ".dot" record.
        let verified = futures::executor::block_on(network_tld(
            &mut ScriptedTld {
                tld: reverted,
                dot_record_exists: true,
            },
            &registry,
        ));
        assert_eq!(verified.as_deref(), Ok(TLD_WITHOUT_VIEW));

        let unverified = futures::executor::block_on(network_tld(
            &mut ScriptedTld {
                tld: reverted,
                dot_record_exists: false,
            },
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
        async fn storage(&mut self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, String> {
            Ok(None)
        }

        async fn view(
            &mut self,
            _dest: &[u8; 20],
            input: Vec<u8>,
        ) -> Result<Vec<u8>, DotnsViewError> {
            let sel: [u8; 4] = input[..4].try_into().expect("selector prefix; qed");
            if sel == selector("protocolRegistry()") {
                let mut word = [0u8; 32];
                word[12..].copy_from_slice(&[0x9e; 20]);
                Ok(word.to_vec())
            } else if sel == selector("tld()") {
                Ok([abi_word(32).to_vec(), abi_string(".paseo")].concat())
            } else if sel == selector("get(bytes32)") {
                let mut word = [0u8; 32];
                word[12..].copy_from_slice(&[0xd1; 20]);
                Ok(word.to_vec())
            } else if sel == selector("exists(uint256)") {
                assert_eq!(
                    &input[4..36],
                    &namehash_under(&tld_node(".paseo"), "george")
                );
                (self.exists)()
            } else {
                panic!("unscripted view {}", hex::encode(sel));
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

        assert_eq!(run(|| Ok(abi_word(0).to_vec())), Ok(true));
        assert_eq!(run(|| Ok(abi_word(1).to_vec())), Ok(false));
        let reverted = run(|| {
            Err(DotnsViewError::Reverted(DotnsContractError::Reverted {
                detail: "(empty)".to_string(),
            }))
        });
        assert!(reverted.is_err(), "{reverted:?}");
        let failed = run(|| Err(DotnsViewError::Failed("timed out".to_string())));
        assert!(failed.is_err(), "{failed:?}");
    }

    #[test]
    fn view_output_separates_reverts_from_failures() {
        let reverted = contract_result(&{
            let mut out = vec![0x00];
            1u32.encode_to(&mut out); // ReturnFlags::REVERT
            Vec::<u8>::new().encode_to(&mut out);
            out
        });
        assert!(matches!(
            view_output(&reverted),
            Err(DotnsViewError::Reverted(
                DotnsContractError::Reverted { .. }
            ))
        ));
        assert!(matches!(
            view_output(&[0x01]),
            Err(DotnsViewError::Failed(_))
        ));
        let returned = contract_result(&{
            let mut out = vec![0x00];
            0u32.encode_to(&mut out);
            vec![1u8, 2, 3, 4].encode_to(&mut out);
            out
        });
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
        assert_eq!(
            bare_store_label("alice.01.paseo", ".paseo"),
            Some("alice.01")
        );
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
            [
                twox_128(b"DotnsGateway").as_slice(),
                twox_128(b"LiteLabelOwner").as_slice(),
            ]
            .concat()
        );
        assert_eq!(&owner_key[32..48], &blake2_128(&encoded));
        assert_eq!(&owner_key[48..], &encoded);

        let alias_key = account_alias_key(&[0x11; 32]);
        let prefix = [
            twox_128(b"DotnsGateway").as_slice(),
            twox_128(b"AccountAlias").as_slice(),
        ]
        .concat();
        assert_eq!(&alias_key[..32], prefix.as_slice());
        // Blake2_128Concat over the raw account bytes.
        assert_eq!(&alias_key[48..], &[0x11; 32]);

        assert_eq!(dispatcher_address_key().len(), 32);
        assert_eq!(timestamp_now_key().len(), 32);
    }

    #[test]
    fn revive_call_args_encode_origin_dest_and_input() {
        let args = encode_revive_call(&[0x11; 32], &[0x22; 20], &[0xAB, 0xCD]);
        assert_eq!(&args[..32], &[0x11; 32]);
        assert_eq!(&args[32..52], &[0x22; 20]);
        assert_eq!(&args[52..68], &[0u8; 16], "value is zero");
        assert_eq!(&args[68..70], &[0x00, 0x00], "no gas or deposit limit");
        assert_eq!(&args[70..], vec![0xABu8, 0xCD].encode().as_slice());
    }
}
