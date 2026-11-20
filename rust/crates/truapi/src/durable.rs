//! Durable transactions: a ledger in the core database that follows signed
//! extrinsics from registration to a verdict, across crashes and restarts.
//!
//! A domain registers presigned [`MortalExtrinsic`](crate::chain::MortalExtrinsic)s
//! in its own write and observes their status; the engine broadcasts them,
//! watches them, and decides every one it no longer watches from the chain.
//! Signed bytes are never persisted, so a transaction left live by a previous
//! process is decided by recovery rather than resent. A domain may instead
//! defer a transaction for a [`SubmissionPolicy`] to build, and a policy
//! builds again any failure it accepts to retry.

#[macro_use]
mod dao;
mod engine;
mod ladder;
mod model;
mod oracle;
mod ownership;
mod policy;
mod search;

#[cfg(test)]
mod testing;
#[cfg(test)]
pub use testing::{
	FakeChain, GENESIS, block, block_hash, engine_on, engine_with, extrinsic, live_engine,
};

pub use dao::{group, group_exists};
pub use engine::{
	DeferredSubmission, DurableDeps, DurableRequest, DurableSubmission, DurableTxEngine,
	DurableWorkObserver, EmptyRequest, RecoveryError, RegistrationError,
};
pub use model::{
	DeferredDurableTx, DependencyStatus, DependencyStatusRecord, DomainId, DurableTxEntry,
	DurableTxFailureKind, DurableTxId, DurableTxState, DurableTxStatus, GroupId, HeadKind,
	OnDependencyFailure, PolicyParams, SubmissionPlan, TransactionDependency, Verdict,
	VerdictFailureKind, VerdictStatus, Window,
};
pub use oracle::{CompletionOracle, DurableRegistry, PassScope, Unobservable};
pub use policy::{PolicyError, Preparation, SubmissionPolicy};
