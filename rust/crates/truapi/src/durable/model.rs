//! What the ledger records about a durable transaction.

use subxt::utils::H256;

use crate::chain::{HashAndNumber, Mortality};

/// The domain a transaction belongs to; selects the oracle that decides it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DomainId(String);

impl DomainId {
    /// A domain with this name.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The name stored in the ledger.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A transaction's ledger id, in registration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DurableTxId(pub i64);

/// A caller-chosen operation that several transactions belong to.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GroupId(String);

impl GroupId {
    /// A group with this name.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The name stored in the ledger.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Where a transaction stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurableTxStatus {
    /// Submitted or about to be, with no evidence either way.
    Pending,
    /// Took effect at a block that is not finalized yet.
    PendingSuccess,
    /// Took effect at a finalized block. Terminal.
    FinalizedSuccess,
    /// Can no longer take effect. Terminal.
    Failure,
}

impl DurableTxStatus {
    /// Whether a recovery pass or a submission watch may still decide it.
    pub fn awaits_verdict(self) -> bool {
        matches!(self, Self::Pending | Self::PendingSuccess)
    }
}

/// Why a transaction failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// Its era ended without it being included.
    Expired,
    /// It was included and its dispatch failed.
    DispatchFailed,
    /// The node refused it before it reached the pool.
    Rejected,
}

/// Which head a question is asked at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadKind {
    /// The finalized head.
    Finalized,
    /// The best head.
    Best,
}

/// One ledger row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurableTxEntry {
    /// Ledger id.
    pub id: DurableTxId,
    /// The domain that registered it.
    pub domain: DomainId,
    /// The operation it belongs to, if any.
    pub group: Option<GroupId>,
    /// Hash of the extrinsic of the current attempt.
    pub tx_hash: H256,
    /// The era it was signed with.
    pub mortality: Mortality,
    /// Where it stands.
    pub status: DurableTxStatus,
    /// The block its success was seen at, while that is still the evidence.
    pub success_detected_at: Option<HashAndNumber>,
    /// Every block from birth through this one was searched and does not
    /// include the attempt. Only finalized blocks are searched, so no reorg
    /// can undo it.
    pub scanned_to: Option<u64>,
}

/// A transaction's id and status, as a group lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DurableTxState {
    /// Ledger id.
    pub id: DurableTxId,
    /// Where it stands.
    pub status: DurableTxStatus,
}

/// A decision about one transaction, written only against the status and
/// attempt it was derived from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verdict {
    /// The status to write.
    pub status: DurableTxStatus,
    /// The block the success was seen at, if any.
    pub success_detected_at: Option<HashAndNumber>,
    /// Why it failed, for a [`DurableTxStatus::Failure`].
    pub failure: Option<FailureKind>,
    /// How far the search has now read without finding the attempt. `None`
    /// keeps the recorded [`DurableTxEntry::scanned_to`].
    pub scanned_to: Option<u64>,
}
