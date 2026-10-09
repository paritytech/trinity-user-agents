// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

//! Host-owned Coinage domain engine, licensed AGPL-3.0-only.
//!
//! The signing authority owns this crate and every memo it creates. Nothing
//! here grants product permission or exports a secret through a guest API.
//! The Host must authorize each outgoing operation before `confirm`, persist
//! the encrypted transport handoff, and reconcile ambiguous outcomes.
//!
//! Durable adapters implement repositories, monotonic indices, WAL and claim
//! plans. Chain adapters provide pinned storage/finality and exact transaction
//! submission; Bandersnatch primitives are supplied by the signing authority.
//! `tokio::sync` supplies runtime-independent channels/locks only. Background
//! execution uses the injected [`Spawner`]; timers support native and WASM.
//!
//! Source and modification notices: `NOTICE`; full license: `LICENSE`.

/// Allocator domain contracts and algorithms.
pub mod allocator;
/// Balance domain contracts and algorithms.
pub mod balance;
/// Claim domain contracts and algorithms.
pub mod claim;
/// Claim plan domain contracts and algorithms.
pub mod claim_plan;
/// Clock domain contracts and algorithms.
pub mod clock;
/// Constants domain contracts and algorithms.
pub mod constants;
/// Denomination domain contracts and algorithms.
pub mod denomination;
/// Index store domain contracts and algorithms.
pub mod index_store;
/// Keys domain contracts and algorithms.
pub mod keys;
/// Members domain contracts and algorithms.
pub mod members;
/// Memo domain contracts and algorithms.
pub mod memo;
/// Model domain contracts and algorithms.
pub mod model;
/// Outgoing transfer domain contracts and algorithms.
pub mod outgoing_transfer;
/// Pallet domain contracts and algorithms.
pub mod pallet;
/// Query domain contracts and algorithms.
pub mod query;
/// Recipient domain contracts and algorithms.
pub mod recipient;
/// Recovery domain contracts and algorithms.
pub mod recovery;
/// Repo domain contracts and algorithms.
pub mod repo;
/// Ring proof domain contracts and algorithms.
pub mod ring_proof;
/// Secret claim domain contracts and algorithms.
pub mod secret_claim;
/// Selection domain contracts and algorithms.
pub mod selection;
/// Sync domain contracts and algorithms.
pub mod sync;
/// Tasks domain contracts and algorithms.
pub mod tasks;
mod timer;
/// Transfer sender domain contracts and algorithms.
pub mod transfer_sender;
/// Tx extensions domain contracts and algorithms.
pub mod tx_extensions;
/// Voucher location domain contracts and algorithms.
pub mod voucher_location;
/// Crash-safe write-ahead journal contracts and encoding.
pub mod wal;

/// Host executor used for all long-lived Coinage work.
pub type Spawner = std::sync::Arc<dyn Fn(futures::future::BoxFuture<'static, ()>) + Send + Sync>;

pub use allocator::{
    CoinAllocator, FixedDelayProvider, SystemJitterDelayProvider, VoucherAllocator,
    VoucherDelayProvider,
};
pub use balance::{BalanceBuckets, compute_balance, next_unlock_at_ms};
pub use claim::{
    ClaimError, ClaimExecutor, ClaimOrchestrator, ClaimStatus, ClaimStatusStore, IncomingClaim,
    SendConfirmation, TransferSendVerifying, claimed_amount_from_plan, restore_persisted_statuses,
};
pub use claim_plan::{
    ClaimPlan, ClaimPlanStatus, ClaimPlanStore, CodableClaimPlanEntry, decode_claim_plan_entries,
    encode_claim_plan_entries,
};
pub use clock::{Clock, FixedClock, SystemClock};
pub use constants::{
    CASH_ASSET_PRECISION, CASH_PLANKS_PER_CENT, COIN_MAX_AGE, MAX_VOUCHER_WAIT_TIME,
    MINIMUM_RING_SIZE, RECYCLE_AT_AGE, SEND_VERIFY_BLOCK_TIMEOUT, WAL_MORTALITY_BLOCKS,
};
pub use denomination::{Denomination, DenominationBreakdown, DenominationBreakdownContext};
pub use index_store::{
    COIN_INDEX_KEY, CoinageIndexStore, InMemoryCoinageIndexStore, IndexKind, VOUCHER_INDEX_KEY,
    decode_index, encode_index,
};
pub use keys::VoucherCryptography;
pub use keys::{
    CoinDerivedWallet, CoinKeypairFactory, MAIN_PURSE, PAGE, VoucherKeypairFactory, VoucherSeed,
};
pub use memo::{MemoEntry, TransferMemo};
pub use model::{
    Coin, CoinState, EffectivePrivacy, Voucher, VoucherLocalState, VoucherPrivacyLevel,
    VoucherRemoteState, ring_readiness_upgraded,
};
pub use outgoing_transfer::{
    OutgoingCoinTransferParts, OutgoingCoinTransferService, OutgoingHandoffRejected,
    OutgoingTransferError, OutgoingTransferReconciliation,
};
pub use query::{
    AliasState, CoinOnChainQueryService, CoinageQueryError, CoinageStorageKey, CoinageStorageQuery,
    LockInfo, LockReason, QueryVoucherLocationSubscriber, RecyclerReadinessLoader,
    RecyclerRevisionSnapshot, VoucherOnChainInfo, VoucherOnChainQueryService,
};
pub use recipient::{CoinageSendMessage, TransferRecipientService};
pub use recovery::{RecoveryChainProbe, RecoveryReport, TransferRecoveryService};
pub use repo::{
    CoinRepository, InMemoryCoinRepository, InMemoryVoucherRepository, TransferContext,
    TransferStateCommitter, VoucherRepository,
};
pub use ring_proof::BandersnatchRingProofProvider;
pub use ring_proof::PersonRingProofSigner;
pub use ring_proof::{
    FREE_UNLOAD_TOKEN_CONTEXT_PREFIX, PersonOriginKind, RECYCLER_ALIAS_CONTEXT, RING_VRF_PROOF_LEN,
    ResolvedUnloadToken, RingProofError, RingProofParams, RingProofProvider, UnloadProofRequest,
    UnloadTokenProof,
};
pub use secret_claim::{
    ExternalCoinTransferBackend, ExternalCoinTransferRequest, ExternalSecretClaimService,
    SpentCoinTransferRecoveryReport, SpentCoinTransferRecoveryService, external_claim_message_id,
};
pub use secret_claim::{ExternalMemoClaiming, SpentCoinsRecovering};
pub use selection::{
    CoinSelectionError, CoinSelectionResult, CoinSelector, PrivacyLevel, RecyclerKey,
    TransferStrategy, VoucherGroup, find_exact_match,
};
pub use sync::{
    CoinStateSubscriber, CoinStateSyncService, CoinStateUpdate, CoinageDatabaseDependencyFactory,
    NotifyingCoinRepository, NotifyingVoucherRepository, OnChainCoin,
};
pub use tasks::ActiveTaskRegistry;
pub use transfer_sender::{
    OpaqueTransferPreview, PreparedUnloadGroup, RegularCoinTransferParts,
    RegularCoinTransferService, RegularTransferError, RegularTransferSubmitter,
    SplitTransferSubmission, TransferPreviewChoice, TransferPreviewStrategy, UnloadGroupDraft,
    UnloadOriginPreparation, VoucherSelectionDiagnostic, voucher_selection_diagnostics,
};
pub use voucher_location::{
    RingPosition, RingStatus, VoucherLocationService, VoucherLocationSubscriber,
    VoucherLocationUpdate,
};

#[cfg(test)]
fn test_spawner() -> Spawner {
    std::sync::Arc::new(|future| {
        tokio::spawn(future);
    })
}

pub use wal::{CheckpointBlock, TransferWalEntry, WalCoinRef, WalOperation, WalPayload, WalStore};
