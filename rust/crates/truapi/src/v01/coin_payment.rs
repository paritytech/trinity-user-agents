use alloc::{string::String, vec::Vec};
use parity_scale_codec::{Decode, Encode};

/// Well-known ordinary user-owned CoinPayment purse.
pub const MAIN_PURSE: u32 = u32::MAX;

/// Product-visible metadata and balance state for a CoinPayment purse.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CoinPaymentPurseInfo {
    /// Human-readable purse name supplied by the creating product.
    pub name: String,
    /// Creation timestamp.
    pub created: u64,
    /// Product that created the purse.
    pub creator: String,
    /// Current product-visible balance.
    pub balance: u32,
}

/// Standardized encrypted Coinage secret transmission payload.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CoinPaymentCheque {
    /// Receivable public key protecting the cheque contents.
    pub id: [u8; 32],
    /// Claimed payment amount.
    pub amount: u32,
    /// Concatenated coin secrets encrypted to the receivable.
    pub encrypted_secrets: Vec<u8>,
}

/// Errors returned by CoinPayment host operations.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CoinPaymentError {
    /// Source purse has too little balance.
    BalanceLow,
    /// User agent denied spend, transfer, or access.
    Denied,
    /// Coin secrets do not control valid coins.
    BadCoins,
    /// Coin secrets were claimed elsewhere.
    SnipedCoins,
    /// Purse does not exist or is not visible to the caller.
    PurseNotFound,
    /// Receivable does not exist or is not visible to the caller.
    ReceivableNotFound,
    /// Requested transmission channel is not supported.
    UnsupportedChannel,
    /// Required host/user-agent capability is unavailable.
    UserAgentCapabilityUnavailable,
    /// Unexpected runtime failure.
    Internal,
}

/// Product-visible clearing reference for reconciliation and receipts.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CoinPaymentClearingReference {
    /// Clearing Merkle root.
    pub root: [u8; 32],
    /// Product-visible coin key and transaction hash leaves.
    pub leaves: Vec<([u8; 32], [u8; 32])>,
}

/// Clearing status stream item.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CoinPaymentStatus {
    /// More coins have cleared.
    Clearing {
        /// Amount clearing in this update.
        clearing: u32,
        /// Cumulative cleared amount.
        cleared: u32,
    },
    /// Some or all coins failed to transfer.
    Failed {
        /// Failure reason.
        error: CoinPaymentError,
        /// Cumulative cleared amount.
        cleared: u32,
        /// Clearing reference for any cleared portion.
        reference: CoinPaymentClearingReference,
    },
    /// All coins cleared.
    Done {
        /// Cleared amount.
        cleared: u32,
        /// Clearing reference.
        reference: CoinPaymentClearingReference,
    },
}

/// Standardized cheque transmission channel.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CoinPaymentTransmissionChannel {
    /// Statement-store/HOP handoff identified by an SSS topic.
    Standard {
        /// Statement-store topic.
        sss_topic: [u8; 32],
    },
}

/// Request to create a new firewalled CoinPayment purse.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentCreatePurseRequest {
    /// Human-readable purse name.
    pub name: String,
}

/// Created purse identifier.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentCreatePurseResponse {
    /// Assigned purse identifier.
    pub purse: u32,
}

/// Request to query product-visible purse metadata.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentQueryPurseRequest {
    /// Purse to query.
    pub purse: u32,
}

/// Product-visible purse metadata response.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentQueryPurseResponse {
    /// Purse information.
    pub info: CoinPaymentPurseInfo,
}

/// Request to transfer balance between local purses.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentRebalancePurseRequest {
    /// Source purse.
    pub from: u32,
    /// Destination purse.
    pub to: u32,
    /// Amount to move.
    pub amount: u32,
}

/// Request to delete a purse after draining its balance.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentDeletePurseRequest {
    /// Purse to delete.
    pub target: u32,
    /// Purse that receives drained funds.
    pub drain_into: u32,
}

/// Request to create a fresh receivable for a purse.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentCreateReceivableRequest {
    /// Target purse for future deposits.
    pub into: u32,
}

/// Created receivable response.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentCreateReceivableResponse {
    /// Receivable public key.
    pub receivable: [u8; 32],
}

/// Request to create a cheque from a local purse to a receivable.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentCreateChequeRequest {
    /// Source purse.
    pub from: u32,
    /// Destination receivable.
    pub to: [u8; 32],
    /// Payment amount.
    pub amount: u32,
}

/// Created cheque response.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentCreateChequeResponse {
    /// Encrypted cheque.
    pub cheque: CoinPaymentCheque,
}

/// Request to deposit a cheque into the purse associated with its receivable.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentDepositRequest {
    /// Cheque to deposit.
    pub cheque: CoinPaymentCheque,
}

/// Request to refund coins associated with a receivable.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentRefundRequest {
    /// Receivable to refund.
    pub receivable: [u8; 32],
}

/// Request to listen for a cheque delivered to a receivable.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCoinPaymentListenForRequest {
    /// Receivable to listen for.
    pub receivable: [u8; 32],
}

/// Stream item for `host_coin_payment_listen_for`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostCoinPaymentListenForItem {
    /// Handoff channel suitable for inclusion in an invoice.
    Channel(CoinPaymentTransmissionChannel),
    /// Cheque received through the handoff channel.
    Cheque(CoinPaymentCheque),
}
