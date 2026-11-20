//! [`PgasClaimOracle`]: proves a claim landed when the pallet records the
//! alias its attempt claimed.

use std::{collections::HashSet, sync::Arc};

use subxt::utils::H256;

use super::params::{ClaimAttempt, ClaimParams};
use crate::{
	chain::{Heads, WatchedStorage},
	chain_runtime::RuntimeFailure,
	durable::{CompletionOracle, DurableTxEntry, DurableTxId, HeadKind, PassScope},
	runtime::statement_allowance::slot::claimed_gas_alias_key,
};

/// Decides PGAS claims from `Pgas.ClaimedGasAliases`, which records the
/// person's alias for a `(day, slot)` once a claim of that slot succeeds, and
/// keeps it until about 01:00 UTC the next day. A recorded alias proves the
/// attempt that claimed the slot landed. An absent one proves nothing: the
/// record may not exist yet or may be pruned, so the engine's block search
/// decides failures.
pub struct PgasClaimOracle {
	storage: Arc<dyn WatchedStorage>,
	genesis: H256,
}

impl PgasClaimOracle {
	/// An oracle reading the pallet on the chain with `genesis`.
	pub fn new(storage: Arc<dyn WatchedStorage>, genesis: H256) -> Self {
		Self { storage, genesis }
	}

	/// The claims of `attempted` whose alias is recorded at block `at`.
	async fn recorded_claims(
		&self,
		attempted: &[AttemptedClaim],
		at: H256,
	) -> Result<HashSet<DurableTxId>, RuntimeFailure> {
		let keys = attempted.iter().map(AttemptedClaim::alias_key).collect();
		let snapshot = self.storage.read_raw_keys_at(self.genesis, at, keys).await?;
		Ok(attempted
			.iter()
			.zip(snapshot.value)
			.filter(|(_, recorded)| recorded.is_some())
			.map(|(claim, _)| claim.id)
			.collect())
	}
}

#[async_trait::async_trait]
impl CompletionOracle for PgasClaimOracle {
	async fn open_pass(
		&self,
		transactions: &[DurableTxEntry],
		heads: &Heads,
	) -> Result<Box<dyn PassScope>, RuntimeFailure> {
		let attempted: Vec<_> = transactions.iter().filter_map(AttemptedClaim::of).collect();
		let (at_finalized, at_best) = futures::future::try_join(
			self.recorded_claims(&attempted, heads.finalized.hash),
			self.recorded_claims(&attempted, heads.best.hash),
		)
		.await?;
		Ok(Box::new(RecordedClaims { at_finalized, at_best }))
	}
}

/// A claim whose live attempt was built, with the slot and alias it claims.
struct AttemptedClaim {
	id: DurableTxId,
	attempt: ClaimAttempt,
}

impl AttemptedClaim {
	/// The claim `entry` makes, once an attempt of it was built.
	fn of(entry: &DurableTxEntry) -> Option<Self> {
		let plan = entry.plan.as_ref()?;
		let attempt = ClaimParams::try_from(&plan.params).ok()?.attempt?;
		Some(Self { id: entry.id, attempt })
	}

	/// The `Pgas.ClaimedGasAliases` key the attempt's success writes.
	fn alias_key(&self) -> Vec<u8> {
		claimed_gas_alias_key(self.attempt.slot.day, &self.attempt.alias)
	}
}

/// Which claims the pallet recorded at each head of one pass.
struct RecordedClaims {
	at_finalized: HashSet<DurableTxId>,
	at_best: HashSet<DurableTxId>,
}

impl PassScope for RecordedClaims {
	fn proven_completed(&self, tx: &DurableTxEntry, head: HeadKind) -> bool {
		let recorded = match head {
			HeadKind::Finalized => &self.at_finalized,
			HeadKind::Best => &self.at_best,
		};
		recorded.contains(&tx.id)
	}
}

#[cfg(test)]
mod tests;
