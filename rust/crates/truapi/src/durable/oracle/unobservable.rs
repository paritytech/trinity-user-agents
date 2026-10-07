//! [`Unobservable`]: the oracle of a domain that cannot read its effects.

use subxt::utils::H256;

use super::{CompletionOracle, LedgerView, PassScope};
use crate::chain::Heads;
use crate::chain_runtime::RuntimeFailure;
use crate::durable::model::{DurableTxEntry, HeadKind};

/// [`CompletionOracle`] for a domain that cannot read its effects: its
/// transactions are decided by their recorded inclusion and the body search
/// alone, which is correct, only slower.
pub struct Unobservable(pub H256);

#[async_trait::async_trait]
impl CompletionOracle for Unobservable {
    fn chain(&self) -> H256 {
        self.0
    }

    async fn open_pass(
        &self,
        _transactions: &[DurableTxEntry],
        _ledger: &LedgerView,
        _heads: &Heads,
    ) -> Result<Box<dyn PassScope>, RuntimeFailure> {
        Ok(Box::new(ProvesNothing))
    }
}

struct ProvesNothing;

impl PassScope for ProvesNothing {
    fn proven_completed(&self, _tx: &DurableTxEntry, _head: HeadKind) -> bool {
        false
    }
}
