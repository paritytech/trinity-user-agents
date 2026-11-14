//! [`CompletionOracle`]: what a domain tells a recovery pass about its
//! transactions.

use crate::{
	chain::Heads,
	chain_runtime::RuntimeFailure,
	durable::model::{DurableTxEntry, HeadKind},
};

/// A domain's answer to the two questions the completion ladder asks of it.
///
/// Neither answer ever becomes a verdict by being absent: a transaction
/// neither proven completed nor proven not completed falls through to the
/// block-body search.
#[async_trait::async_trait]
pub trait CompletionOracle: Send + Sync {
	/// Every chain read the pass needs, for every transaction in it, at
	/// `heads`. The scope does not read the chain, so the reads are batched.
	/// An error leaves the domain undecided until the next pass.
	///
	/// A pass calls this once per domain with the domain's undecided
	/// transactions that no watch follows. An oracle whose answers depend on
	/// other transactions of its domain resolves those dependencies among
	/// `transactions` within this call; a dependency proven only later in the
	/// pass, by the block search, or followed by a watch, counts on a later
	/// pass.
	async fn open_pass(
		&self,
		transactions: &[DurableTxEntry],
		heads: &Heads,
	) -> Result<Box<dyn PassScope>, RuntimeFailure>;
}

/// What a domain proved about its transactions for one pass.
pub trait PassScope: Send + Sync {
	/// Positive proof `tx` took effect at `head`. Safe to claim at the best
	/// head: a reorg demotes it again.
	fn proven_completed(&self, tx: &DurableTxEntry, head: HeadKind) -> bool;

	/// Positive proof `tx` has not taken effect at `head`, and cannot have
	/// taken effect and been undone since. Past the era, at the finalized
	/// head, this fails the transaction for good, so claim it only for an
	/// effect that is monotone and written by this transaction alone.
	fn proven_not_completed(&self, _tx: &DurableTxEntry, _head: HeadKind) -> bool {
		false
	}
}
