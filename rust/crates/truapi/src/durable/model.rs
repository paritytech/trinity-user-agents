//! What the ledger records about a durable transaction.

use std::borrow::Cow;

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

/// A submission policy's name, as the ledger stores it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PolicyId(Cow<'static, str>);

impl PolicyId {
    /// A policy named by a compile-time constant.
    pub const fn from_static(name: &'static str) -> Self {
        Self(Cow::Borrowed(name))
    }

    /// A policy named at run time, such as one read back from the ledger.
    pub fn new(name: impl Into<String>) -> Self {
        Self(Cow::Owned(name.into()))
    }

    /// The name stored in the ledger.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What a policy stores with each transaction it builds. Opaque to the
/// engine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PolicyParams(pub Vec<u8>);

/// How a transaction gets built: the policy that builds it, and that
/// policy's params for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmissionPlan {
    /// The registered policy that builds it.
    pub policy: PolicyId,
    /// Handed back to the policy on every call.
    pub params: PolicyParams,
}

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

/// Where a transaction stands, with the evidence for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurableTxStatus {
    /// Waiting for its policy to build an attempt. Live, but nothing on chain
    /// can decide it.
    PendingSubmission,
    /// Submitted or about to be, with no evidence either way.
    Pending,
    /// Took effect at a block that is not finalized yet.
    PendingSuccess {
        /// The block it was seen at.
        at: HashAndNumber,
    },
    /// Took effect at a finalized block. Terminal.
    FinalizedSuccess {
        /// The block it was seen at, when the proof named one.
        at: Option<HashAndNumber>,
    },
    /// Can no longer take effect. Terminal.
    Failure(FailureKind),
}

impl DurableTxStatus {
    /// Whether a recovery pass or a submission watch may still decide it.
    pub fn awaits_verdict(self) -> bool {
        matches!(self, Self::Pending | Self::PendingSuccess { .. })
    }

    /// The block its success was seen at, if it has one.
    pub fn success_block(self) -> Option<HashAndNumber> {
        match self {
            Self::PendingSuccess { at } => Some(at),
            Self::FinalizedSuccess { at } => at,
            Self::PendingSubmission | Self::Pending | Self::Failure(_) => None,
        }
    }

    /// Whether nothing will ever change it again.
    pub fn is_final(self) -> bool {
        matches!(self, Self::FinalizedSuccess { .. } | Self::Failure(_))
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
    /// It waited to be built and its policy will never build it.
    Abandoned,
}

/// Which head a question is asked at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadKind {
    /// The finalized head.
    Finalized,
    /// The best head.
    Best,
}

/// One ledger row with a submitted attempt.
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
    /// Every block from birth through this one was searched and does not
    /// include the attempt. Only finalized blocks are searched, so no reorg
    /// can undo it.
    pub scanned_to: Option<u64>,
}

/// A transaction waiting for its policy to build an attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledDurableTx {
    /// Ledger id.
    pub id: DurableTxId,
    /// The domain that registered it.
    pub domain: DomainId,
    /// The operation it belongs to, if any.
    pub group: Option<GroupId>,
    /// How it gets built.
    pub plan: SubmissionPlan,
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
    /// How far the search has now read without finding the attempt. `None`
    /// keeps the recorded [`DurableTxEntry::scanned_to`].
    pub scanned_to: Option<u64>,
}

impl From<DurableTxStatus> for Verdict {
    /// A verdict that leaves the recorded search cursor as it is.
    fn from(status: DurableTxStatus) -> Self {
        Self {
            status,
            scanned_to: None,
        }
    }
}
