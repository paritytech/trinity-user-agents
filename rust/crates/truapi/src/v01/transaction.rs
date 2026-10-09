use parity_scale_codec::{Decode, Encode};

use crate::v01::contacts::ContactHandle;

use super::ProductAccountId;

/// A signed extension for a transaction payload.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct TxPayloadExtension {
    /// Extension name (e.g., `"CheckSpecVersion"`).
    pub id: String,
    /// SCALE-encoded extra data (in extrinsic body).
    pub extra: Vec<u8>,
    /// SCALE-encoded implicit data (signed, not in body).
    pub additional_signed: Vec<u8>,
}

/// Transaction payload for a product account.
///
/// Contains everything the host needs to construct a signed extrinsic.
/// The signer is a [`ProductAccountId`]; the host resolves the
/// corresponding key pair through its account management layer.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct ProductAccountTxPayload {
    /// Product account that will sign the transaction.
    pub signer: ProductAccountId,
    /// Chain where the transaction will execute.
    pub genesis_hash: [u8; 32],
    /// SCALE-encoded Call data.
    pub call_data: Vec<u8>,
    /// Transaction extensions supplied by the caller.
    pub extensions: Vec<TxPayloadExtension>,
    /// Version of the transaction extensions in `extensions`, as the runtime
    /// numbers them.
    pub tx_ext_version: u8,
    /// Contact handles `call_data` names, which the host replaces with the
    /// accounts they resolve to before anything is signed or shown.
    ///
    /// A product declares them rather than passing offsets: an offset is a
    /// number it computes about its own encoding and gets wrong silently,
    /// while a declared handle is either in the call or it is not, and a host
    /// that cannot find one refuses rather than signing a call that names
    /// somebody else. A call naming nobody leaves this empty.
    pub contacts: Vec<ContactHandle>,
}

/// Transaction payload for a legacy (non-product) account.
///
/// Identical to [`ProductAccountTxPayload`] except the signer is a raw
/// 32-byte account identifier.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct LegacyAccountTxPayload {
    /// Raw 32-byte public key of the legacy account.
    pub signer: [u8; 32],
    /// Chain where the transaction will execute.
    pub genesis_hash: [u8; 32],
    /// SCALE-encoded Call data.
    pub call_data: Vec<u8>,
    /// Transaction extensions supplied by the caller.
    pub extensions: Vec<TxPayloadExtension>,
    /// Version of the transaction extensions in `extensions`, as the runtime
    /// numbers them.
    pub tx_ext_version: u8,
}

/// Transaction creation error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostCreateTransactionError {
    /// Payload could not be deserialized.
    FailedToDecode,
    /// User rejected.
    Rejected,
    /// Unsupported payload version or extension.
    NotSupported {
        /// Unsupported payload or extension reason.
        reason: String,
    },
    /// Not authenticated.
    PermissionDenied,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
    /// A declared contact handle names nobody this host has a contact for, or
    /// does not appear in the call it was declared for, or a handle appears in
    /// the call without being declared. One refusal for all three, because
    /// telling them apart would say whether a handle is current.
    UnknownContact,
}
