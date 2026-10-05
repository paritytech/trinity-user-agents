//! Durable transactions: a ledger in the core database that follows signed
//! extrinsics from registration to a verdict, across crashes and restarts.
//!
//! A domain registers presigned [`MortalExtrinsic`](crate::chain::MortalExtrinsic)s
//! in its own write and observes their status; the engine broadcasts them,
//! watches them, and decides every one it no longer watches from the chain.
//! Signed bytes are never persisted, so a transaction left live by a previous
//! process is decided by recovery rather than resent.

mod dao;
mod engine;
mod ladder;
mod model;
mod oracle;
mod ownership;
mod search;
mod time;

#[cfg(test)]
mod testing;

pub use engine::{
    DurableDeps, DurableRequest, DurableTxEngine, DurableWorkObserver, EmptyRequest, RecoveryError,
    RegistrationError,
};
pub use model::{
    DomainId, DurableTxEntry, DurableTxId, DurableTxState, DurableTxStatus, FailureKind, GroupId,
    HeadKind, Verdict,
};
pub use oracle::{
    CompletionOracle, DurableRegistry, LedgerView, Monotone, MonotoneEffect, PassScope,
    Unobservable,
};
pub use time::{RealTimer, Timer};
