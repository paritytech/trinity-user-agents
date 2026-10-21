//! Durable transactions: a ledger in the core database that follows signed
//! extrinsics from registration to a verdict, across crashes and restarts.
//!
//! A domain registers presigned [`MortalExtrinsic`](crate::chain::MortalExtrinsic)s
//! in its own write and observes their status; the engine broadcasts them,
//! watches them, and decides every one it no longer watches from the chain.
//! Signed bytes are never persisted, so a transaction left live by a previous
//! process is decided by recovery rather than resent. A domain may instead
//! schedule a transaction for a [`SubmissionPolicy`] to build, and a policy
//! builds again any failure it accepts to retry.

mod dao;
mod engine;
mod ladder;
mod model;
mod oracle;
mod ownership;
mod policy;
mod search;
mod time;

#[cfg(test)]
mod testing;

pub use engine::{
	DurableDeps, DurableRequest, DurableSubmission, DurableTxEngine, DurableWorkObserver,
	EmptyRequest, RecoveryError, RegistrationError,
};
pub use model::{
	DomainId, DurableTxEntry, DurableTxFailureKind, DurableTxId, DurableTxState, DurableTxStatus,
	GroupId, HeadKind, PolicyId, PolicyParams, ScheduledDurableTx, SubmissionPlan, Verdict,
	VerdictFailureKind, VerdictStatus,
};
pub use oracle::{
	CompletionOracle, DurableRegistry, LedgerView, Monotone, MonotoneEffect, PassScope,
	Unobservable,
};
pub use policy::{PolicyError, Preparation, SubmissionPolicy};
pub use time::{RealTimer, Timer};
