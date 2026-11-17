//! [`Monotone`]: the ladder for a domain whose effects, once visible, stay
//! visible.

use std::collections::HashMap;

use subxt::utils::H256;

use super::{CompletionOracle, LedgerView, PassScope};
use crate::{
	chain::{HashAndNumber, Heads},
	chain_runtime::RuntimeFailure,
	durable::model::{DurableTxEntry, DurableTxId, HeadKind},
};

/// A domain whose transactions each produce one observation that, once true,
/// stays true and that nothing else can make true, such as a value appended
/// to a set only this app writes. Wrapped in [`Monotone`] it gets the whole
/// ladder from one batched read per head.
#[async_trait::async_trait]
pub trait MonotoneEffect: Send + Sync {
	/// Genesis hash of the chain the effects live on.
	fn chain(&self) -> H256;

	/// Whether each transaction's effect is visible at `at`. A transaction
	/// missing from the result is a read that did not answer and decides
	/// nothing; `false` claims the effect is absent.
	async fn effects_at(
		&self,
		transactions: &[DurableTxEntry],
		at: HashAndNumber,
	) -> Result<HashMap<DurableTxId, bool>, RuntimeFailure>;
}

/// [`CompletionOracle`] over a [`MonotoneEffect`].
pub struct Monotone<E>(pub E);

#[async_trait::async_trait]
impl<E: MonotoneEffect> CompletionOracle for Monotone<E> {
	fn chain(&self) -> H256 {
		self.0.chain()
	}

	async fn open_pass(
		&self,
		transactions: &[DurableTxEntry],
		_ledger: &LedgerView,
		heads: &Heads,
	) -> Result<Box<dyn PassScope>, RuntimeFailure> {
		let (at_finalized, at_best) = futures::future::try_join(
			self.0.effects_at(transactions, heads.finalized),
			self.0.effects_at(transactions, heads.best),
		)
		.await?;
		Ok(Box::new(MonotoneScope { at_finalized, at_best }))
	}
}

struct MonotoneScope {
	at_finalized: HashMap<DurableTxId, bool>,
	at_best: HashMap<DurableTxId, bool>,
}

impl MonotoneScope {
	fn effect(&self, tx: &DurableTxEntry, head: HeadKind) -> Option<bool> {
		match head {
			HeadKind::Finalized => self.at_finalized.get(&tx.id).copied(),
			HeadKind::Best => self.at_best.get(&tx.id).copied(),
		}
	}
}

impl PassScope for MonotoneScope {
	fn proven_completed(&self, tx: &DurableTxEntry, head: HeadKind) -> bool {
		self.effect(tx, head) == Some(true)
	}

	fn proven_not_completed(&self, tx: &DurableTxEntry, head: HeadKind) -> bool {
		self.effect(tx, head) == Some(false)
	}
}

#[cfg(test)]
mod tests;
