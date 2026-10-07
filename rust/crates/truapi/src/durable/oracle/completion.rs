//! [`CompletionOracle`]: what a domain tells a recovery pass about its
//! transactions.

use subxt::utils::H256;

use crate::chain::Heads;
use crate::chain_runtime::RuntimeFailure;
use crate::durable::model::{DurableTxEntry, DurableTxId, DurableTxStatus, HeadKind};

/// A domain's answer to the two questions the completion ladder asks of it.
///
/// Neither answer ever becomes a verdict by being absent: a transaction
/// neither proven completed nor proven not completed falls through to the
/// block-body search.
#[async_trait::async_trait]
pub trait CompletionOracle: Send + Sync {
    /// Genesis hash of the chain this domain's transactions live on.
    fn chain(&self) -> H256;

    /// Every chain read the pass needs, for every transaction in it, at
    /// `heads`. The scope does not read the chain, so the reads are batched.
    /// An error leaves the domain undecided until the next pass.
    async fn open_pass(
        &self,
        transactions: &[DurableTxEntry],
        ledger: &LedgerView,
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

/// Every transaction of one domain as read at the start of a pass, so an
/// oracle can infer one transaction's completion from another's status.
pub struct LedgerView {
    entries: Vec<DurableTxEntry>,
}

impl LedgerView {
    /// A view of these entries.
    pub fn new(entries: Vec<DurableTxEntry>) -> Self {
        Self { entries }
    }

    /// Every entry of the domain, in registration order.
    pub fn entries(&self) -> &[DurableTxEntry] {
        &self.entries
    }

    /// The status of `id`, when it belongs to the domain.
    pub fn status_of(&self, id: DurableTxId) -> Option<DurableTxStatus> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.status)
    }
}
