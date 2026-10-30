use derive_more::Display;
use parity_scale_codec::{Decode, Encode};

use super::{GenericError, StatementTopic, X25519PublicKey};

/// Longest purse name `create_purse` accepts, in bytes of UTF-8.
pub const NFT_PURSE_MAX_NAME_BYTES: usize = 64;

/// Id of an NFT purse. Ids are allocated in order from 0. The main purse,
/// which every user has and Root owns, is `u32::MAX`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct NftPurseId(pub u32);

/// Public key of one item in a purse. The host holds its secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct NftPurseItemKey(pub [u8; 32]);

/// Id of a cheque, and the idempotency key of its deposit and refund.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct NftPurseChequeId(pub [u8; 32]);

/// Key a product chooses for one chain operation. It is scoped to that
/// product, which follows the operation with `observe_operation_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct NftPurseIdempotencyKey(pub [u8; 32]);

/// One-time key a receiver hands to the sender to start a payment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct NftPurseReceiveKey(pub [u8; 32]);

/// Who owns an NFT purse.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Display)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum NftPurseOwner {
	/// The product with this id created the purse.
	#[display("{_0}")]
	Product(String),
	/// No product owns the purse: the main purse, and purses recovered
	/// without metadata.
	#[display("your account")]
	Root,
}

/// A purse as `list_purses` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct NftPursePurse {
	/// Purse id.
	pub id: NftPurseId,
	/// Name given at creation, or chosen by the host after a recovery that
	/// lost it.
	pub name: String,
}

/// One page of the spendable items of a purse.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct NftPurseInfoPage {
	/// Whether a later page holds more items.
	pub has_next: bool,
	/// Keys of items present on chain and not in use.
	pub items: Vec<NftPurseItemKey>,
}

/// Progress of an operation that moves items.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum NftPurseStatus {
	/// Neither an optimistic success nor a terminal failure is observed yet.
	InProgress,
	/// At least one item is not yet cleared in a finalized block.
	Clearing {
		/// Items cleared so far.
		cleared: Vec<NftPurseClearedItem>,
	},
	/// Terminal: every item is cleared in a finalized block.
	Cleared {
		/// Every item, each finalized.
		cleared: Vec<NftPurseClearedItem>,
		/// Reference for reconciliation and receipts.
		reference: NftPurseClearingReference,
	},
	/// Terminal: every transaction settled and some of them failed.
	Incomplete {
		/// Items that cleared.
		cleared: Vec<NftPurseClearedItem>,
		/// Source keys of the items that failed.
		failed: Vec<NftPurseItemKey>,
		/// Reference for the cleared part.
		reference: NftPurseClearingReference,
	},
}

/// One item an operation moved.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct NftPurseClearedItem {
	/// Source key, from the origin purse or from the cheque.
	pub consumed: NftPurseItemKey,
	/// Destination key, in the destination purse.
	pub minted: NftPurseItemKey,
	/// Whether the moving transaction is finalized.
	pub finalized: bool,
}

/// Product-visible clearing reference.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct NftPurseClearingReference {
	/// Binary Merkle root of the SCALE-encoded `leaves`, as
	/// `binary_merkle_tree::merkle_root` computes it with Blake2-256. A
	/// verifier must check the leaf count.
	pub root: [u8; 32],
	/// Source item key and the hash of the transaction that moved it,
	/// sorted by item key.
	pub leaves: Vec<(NftPurseItemKey, [u8; 32])>,
}

/// Where a cheque's items go.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum NftPurseChequeDestination {
	/// A one-time receive key from `create_receive_key`. It identifies the
	/// transfer, and the host derives the cheque id from it.
	Ephemeral(NftPurseReceiveKey),
	/// A long-lived receiver that expects cheques encrypted to `encryption_key`.
	Static {
		/// X25519 public key of the recipient.
		encryption_key: X25519PublicKey,
		/// Identity and idempotency key of the cheque.
		cheque_id: NftPurseChequeId,
	},
}

/// The part of a destination the recipient needs to find its decryption key.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum NftPurseChequeRecipient {
	/// The receive key the cheque was made for.
	Ephemeral(NftPurseReceiveKey),
	/// The recipient's long-lived key.
	Static,
}

/// Items handed off to a recipient.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct NftPurseCheque {
	/// Cheque format version, `0`.
	pub version: u8,
	/// Cheque id.
	pub id: NftPurseChequeId,
	/// Who can decrypt it.
	pub recipient: NftPurseChequeRecipient,
	/// Keys of the handed-off items.
	pub items: Vec<NftPurseItemKey>,
	/// Item secrets, encrypted to the recipient.
	pub encrypted_secrets: Vec<u8>,
}

/// Channel a receiver listens on for a cheque.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum NftPursePaymentTransmissionChannel {
	/// The statement-store topic the cheque is published under.
	Standard {
		/// Topic to watch.
		sss_topic: StatementTopic,
	},
}

/// Request to create a purse owned by the caller.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseCreatePurseRequest {
	/// Purse name, at most [`NFT_PURSE_MAX_NAME_BYTES`] bytes.
	pub name: String,
}

/// Created purse.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseCreatePurseResponse {
	/// Id of the new purse.
	pub purse: NftPurseId,
}

/// Request to create new item keys in a purse.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseCreateItemsRequest {
	/// Purse that receives the keys.
	pub purse: NftPurseId,
	/// Number of keys, in `1..=1000`.
	pub count: u32,
}

/// Created item keys.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseCreateItemsResponse {
	/// Exactly `count` new keys.
	pub items: Vec<NftPurseItemKey>,
}

/// Why item keys were not created.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum NftPurseCreateItemsError {
	/// The user did not grant access to the purse's owner.
	Denied,
	/// `count` is outside `1..=1000`.
	CountOutOfRange,
	/// Any other failure.
	Other(GenericError),
}

/// Request for one page of a purse's spendable items.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseQueryPurseRequest {
	/// Purse to read.
	pub purse: NftPurseId,
	/// Zero-based page index.
	pub page: u32,
	/// Items per page, in `1..=100`.
	pub page_size: u32,
}

/// One page of a purse.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseQueryPurseResponse {
	/// The page.
	pub page: NftPurseInfoPage,
}

/// Why a purse could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum NftPurseQueryPurseError {
	/// The user did not grant access to the purse's owner.
	Denied,
	/// `page_size` is outside `1..=100`.
	PageSizeOutOfBounds,
	/// Any other failure.
	Other(GenericError),
}

/// Request to list an owner's purses.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseListPursesRequest {
	/// Whose purses.
	pub owner: NftPurseOwner,
}

/// An owner's purses, by id.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseListPursesResponse {
	/// The purses, empty when the owner has none.
	pub purses: Vec<NftPursePurse>,
}

/// Why purses could not be listed.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum NftPurseListPursesError {
	/// The user did not grant access to the owner's purses.
	Denied,
	/// Any other failure.
	Other(GenericError),
}

/// Request to move items between purses.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseRebalancePurseRequest {
	/// Source purse.
	pub from: NftPurseId,
	/// Destination purse.
	pub to: NftPurseId,
	/// Items to move.
	pub keys: Vec<NftPurseItemKey>,
	/// Names the operation for `observe_operation_status`.
	pub idempotency_key: NftPurseIdempotencyKey,
}

/// Why items could not be moved between purses.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum NftPurseRebalancePurseError {
	/// The user did not confirm draining the source purse.
	Denied,
	/// `from` or `to` does not exist.
	PurseNotFound,
	/// `keys` is empty.
	NoItems,
	/// Some key is not in `from`.
	ItemNotFound,
	/// Some key is in use.
	ItemUnavailable,
	/// An operation is already registered under the idempotency key.
	OperationExists,
	/// Any other failure.
	Other(GenericError),
}

/// Request for a one-time receive key.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseCreateReceiveKeyRequest {
	/// Purse the received items go into.
	pub into: NftPurseId,
}

/// Created receive key.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseCreateReceiveKeyResponse {
	/// The receive key.
	pub key: NftPurseReceiveKey,
}

/// Why a receive key was not created.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum NftPurseCreateReceiveKeyError {
	/// `into` does not exist.
	PurseNotFound,
	/// Any other failure.
	Other(GenericError),
}

/// Request to wait for a cheque sent to a receive key.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseListenForPaymentRequest {
	/// Receive key from `create_receive_key`.
	pub key: NftPurseReceiveKey,
}

/// Stream item of `listen_for_payment`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostNftPurseListenForPaymentItem {
	/// The channel the host listens on, emitted first.
	Channel(NftPursePaymentTransmissionChannel),
	/// The cheque, once it arrives.
	Cheque(NftPurseCheque),
}

/// Why the host could not listen for a payment.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum NftPurseListenForPaymentError {
	/// The receive key is unknown.
	ReceiveKeyNotFound,
	/// Any other failure.
	Other(GenericError),
}

/// Request to hand items off in a cheque.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseCreateChequeRequest {
	/// Source purse.
	pub from: NftPurseId,
	/// Recipient.
	pub to: NftPurseChequeDestination,
	/// Items to hand off.
	pub items: Vec<NftPurseItemKey>,
}

/// Created cheque.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseCreateChequeResponse {
	/// The cheque, for the product to deliver.
	pub cheque: NftPurseCheque,
}

/// Why a cheque was not created.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum NftPurseCreateChequeError {
	/// The user did not confirm draining the source purse.
	Denied,
	/// `from` does not exist.
	PurseNotFound,
	/// Some item is not in `from`.
	ItemNotFound,
	/// Some item is in use.
	ItemUnavailable,
	/// `items` is empty.
	NoItems,
	/// This product already created a cheque with the same id.
	OperationExists,
	/// Any other failure.
	Other(GenericError),
}

/// Request to release the items of an undelivered cheque.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseRevokeChequeRequest {
	/// Cheque to revoke.
	pub cheque_id: NftPurseChequeId,
}

/// Why a cheque was not revoked.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum NftPurseRevokeChequeError {
	/// Another product created the cheque.
	Denied,
	/// The cheque is unknown.
	ChequeNotFound,
	/// Any other failure.
	Other(GenericError),
}

/// Request to deposit a received cheque.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseDepositRequest {
	/// The received cheque. Its id is the idempotency key.
	pub cheque: NftPurseCheque,
	/// Purse the items go into.
	pub to: NftPurseId,
}

/// Why a cheque could not be deposited.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum NftPurseDepositError {
	/// `to` does not exist.
	PurseNotFound,
	/// The cheque is already deposited.
	OperationExists,
	/// The cheque holds no items.
	EmptyCheque,
	/// The secrets do not decode, or do not match the items.
	CorruptedCheque,
	/// Any other failure.
	Other(GenericError),
}

/// Request to return a deposited cheque's items to their original keys.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseRefundRequest {
	/// The deposited cheque. It is the idempotency key.
	pub cheque_id: NftPurseChequeId,
}

/// Why a cheque could not be refunded.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum NftPurseRefundError {
	/// Another product deposited the cheque, or the user did not confirm.
	Denied,
	/// The cheque is unknown.
	ChequeNotFound,
	/// The cheque is already refunded.
	OperationExists,
	/// Any other failure.
	Other(GenericError),
}

/// Request to move one item to a key outside the purses.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseTransferItemRequest {
	/// Item to move.
	pub item: NftPurseItemKey,
	/// Key that receives it.
	pub destination: NftPurseItemKey,
	/// Names the operation for `observe_operation_status`.
	pub idempotency_key: NftPurseIdempotencyKey,
}

/// Why an item could not be transferred.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum NftPurseTransferItemError {
	/// The user did not confirm draining the item's purse.
	Denied,
	/// The item is in no known purse.
	ItemNotFound,
	/// The item is in use.
	ItemUnavailable,
	/// An operation is already registered under the idempotency key.
	OperationExists,
	/// Any other failure.
	Other(GenericError),
}

/// Request for the status of an operation the calling product registered.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNftPurseObserveOperationStatusRequest {
	/// The key the operation was registered under, or the cheque id of a
	/// deposit or refund.
	pub idempotency_key: NftPurseIdempotencyKey,
}

/// Why an operation's status cannot be observed.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum NftPurseObserveOperationStatusError {
	/// The calling product registered no operation under the key.
	OperationNotFound,
	/// Any other failure.
	Other(GenericError),
}
