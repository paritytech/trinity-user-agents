//! The recovery pass: one evaluation of every live transaction no submission
//! watch owns.

use std::{
	collections::{BTreeSet, HashMap},
	sync::Arc,
};

use itertools::Itertools;
use subxt::utils::H256;
use tracing::warn;

use super::{DurableTxEngine, verdict::write_verdict};
use crate::{
	chain_runtime::RuntimeFailure,
	durable::{
		dao,
		ladder::{PinnedView, RecordedCanonicity, RuleOutcome, evaluate_ladder},
		model::{DomainId, DurableTxEntry, DurableTxId, DurableTxStatus, Verdict},
		oracle::CompletionOracle,
		search::PinnedChain,
	},
	store::DbError,
};

/// Why a domain was left undecided for a pass.
#[derive(Debug, thiserror::Error)]
enum DecideError {
	#[error(transparent)]
	Ledger(#[from] DbError),
	#[error(transparent)]
	Oracle(#[from] RuntimeFailure),
}

/// A live domain and the oracle that decides it.
struct LiveDomain<'a> {
	domain: DomainId,
	oracle: &'a Arc<dyn CompletionOracle>,
}

impl DurableTxEngine {
	/// Decides what it can of every live transaction. At most one pass runs
	/// at a time; a call made while one runs returns at once. The ladder runs
	/// outside any write, and each verdict is a compare-and-set against the
	/// status it was derived from.
	pub async fn run_pass(&self) {
		let Some(_running) = self.pass_lock.try_lock() else {
			return;
		};
		let domains = match self.db.read(dao::live_domains).await {
			Ok(domains) => domains,
			Err(error) => return warn!(%error, "durable recovery pass could not read the ledger"),
		};
		let by_chain = self.group_domains_by_chain(domains);
		let chains = by_chain
			.into_iter()
			.map(|(genesis, domains)| self.decide_chain(genesis, domains));
		futures::future::join_all(chains).await;
	}

	/// Groups `domains` by the chain they live on, so each chain's heads
	/// are read once.
	fn group_domains_by_chain(&self, domains: Vec<DomainId>) -> HashMap<H256, Vec<LiveDomain<'_>>> {
		domains
			.into_iter()
			.filter_map(|domain| self.live_domain(domain))
			.into_group_map()
	}

	/// `domain` with its oracle, keyed by its chain. An unregistered domain
	/// is skipped.
	fn live_domain(&self, domain: DomainId) -> Option<(H256, LiveDomain<'_>)> {
		let (Some(chain), Some(oracle)) =
			(self.registry.chain(&domain), self.registry.oracle(&domain))
		else {
			warn!(domain = domain.as_str(), "durable recovery pass skips an unregistered domain");
			return None;
		};
		Some((chain, LiveDomain { domain, oracle }))
	}

	/// Pins the chain with `genesis` and decides each of its domains.
	async fn decide_chain(&self, genesis: H256, domains: Vec<LiveDomain<'_>>) {
		let view = match PinnedChain::pin(&*self.heads, &*self.blocks, genesis).await {
			Ok(view) => view,
			Err(error) => {
				return warn!(?genesis, %error, "durable recovery pass could not read heads");
			},
		};
		for LiveDomain { domain, oracle } in domains {
			self.decide_domain(&domain, oracle.as_ref(), genesis, &view).await;
		}
	}

	/// Decides `domain` once, logging why when it could not.
	async fn decide_domain(
		&self,
		domain: &DomainId,
		oracle: &dyn CompletionOracle,
		genesis: H256,
		view: &PinnedChain<'_>,
	) {
		if let Err(error) = self.try_decide_domain(domain, oracle, genesis, view).await {
			warn!(domain = domain.as_str(), %error, "durable recovery of a domain failed");
		}
	}

	/// Asks `oracle` once about every decidable transaction of `domain` and
	/// writes the verdicts the ladder reaches.
	async fn try_decide_domain(
		&self,
		domain: &DomainId,
		oracle: &dyn CompletionOracle,
		genesis: H256,
		view: &PinnedChain<'_>,
	) -> Result<(), DecideError> {
		let decidable = self.decidable(domain).await?;
		if decidable.is_empty() {
			return Ok(());
		}
		let (scope, canonical) = futures::future::join(
			oracle.open_pass(&decidable, &view.heads()),
			self.recorded_canonicity(genesis, &decidable),
		)
		.await;
		let scope = scope?;

		for tx in &decidable {
			let recorded = canonical.get(&tx.id).copied().unwrap_or(RecordedCanonicity::Unknown);
			if let RuleOutcome::Decided(verdict) =
				evaluate_ladder(tx, scope.as_ref(), view, recorded).await
			{
				self.write_if_changed(tx, verdict).await;
			}
		}
		Ok(())
	}

	/// The domain's transactions that await a verdict and no watch owns.
	async fn decidable(&self, domain: &DomainId) -> Result<Vec<DurableTxEntry>, DbError> {
		let domain = domain.clone();
		let entries = self.db.read(move |conn| dao::undecided_entries(conn, &domain)).await?;
		Ok(entries.into_iter().filter(|entry| !self.ownership.is_owned(entry.id)).collect())
	}

	/// Writes `verdict` unless it restates `tx`.
	async fn write_if_changed(&self, tx: &DurableTxEntry, verdict: Verdict) {
		let scan_moved = verdict.scanned_to.is_some_and(|scanned| Some(scanned) != tx.scanned_to);
		if DurableTxStatus::from(verdict.status) == tx.status && !scan_moved {
			return;
		}
		if let Err(error) = write_verdict(self, tx, verdict).await {
			warn!(id = tx.id.0, %error, "durable verdict write failed");
		}
	}

	/// Whether each recorded success block is still canonical. A failed read
	/// leaves its transactions out, so they count as
	/// [`RecordedCanonicity::Unknown`] and Rule 0 stays undecided instead of
	/// discarding a record on a transport error.
	async fn recorded_canonicity(
		&self,
		genesis: H256,
		transactions: &[DurableTxEntry],
	) -> HashMap<DurableTxId, RecordedCanonicity> {
		let heights = transactions
			.iter()
			.filter_map(|tx| tx.status.success_block().map(|block| block.number));
		let hashes = self.canonical_hashes(genesis, heights).await;
		transactions
			.iter()
			.filter_map(|tx| {
				let recorded = tx.status.success_block()?;
				// A chain shorter than the record no longer has its block.
				let hash = hashes.get(&recorded.number)?;
				let canonicity = if *hash == Some(recorded.hash) {
					RecordedCanonicity::Canonical
				} else {
					RecordedCanonicity::Reorged
				};
				Some((tx.id, canonicity))
			})
			.collect()
	}

	/// The canonical hash at each height, read once per distinct height.
	/// Heights whose read failed are missing.
	async fn canonical_hashes(
		&self,
		genesis: H256,
		heights: impl Iterator<Item = u64>,
	) -> HashMap<u64, Option<H256>> {
		let heights: BTreeSet<u64> = heights.collect();
		futures::future::join_all(
			heights.into_iter().map(|number| async move {
				(number, self.blocks.block_hash(genesis, number).await)
			}),
		)
		.await
		.into_iter()
		.filter_map(|(number, read)| read.ok().map(|hash| (number, hash)))
		.collect()
	}
}

#[cfg(test)]
mod tests;
