//! The recovery loop: passes on every head and wake until nothing is live.

use core::ops::ControlFlow;
use std::{
	collections::{BTreeMap, BTreeSet},
	sync::Arc,
};

use futures::{
	FutureExt,
	channel::mpsc,
	future::ready,
	stream::{self, BoxStream, StreamExt},
};
use rusqlite::Connection;
use subxt::utils::H256;
use tracing::warn;

use super::DurableTxEngine;
use crate::{
	chain::HeadEvent,
	chain_runtime::RuntimeFailure,
	durable::{dao, model::DomainId},
	store::DbError,
};

/// What wakes the loop.
enum Trigger {
	/// Something may have changed: run another pass.
	Pass,
	/// The loop can no longer follow the new blocks of the chain with
	/// `genesis`.
	HeadsLost { genesis: H256, reason: String },
}

type Triggers = BoxStream<'static, Trigger>;

/// Chains whose new blocks the loop can no longer follow, with the reason.
#[derive(Default)]
struct LostChains(BTreeMap<H256, String>);

impl LostChains {
	fn contains(&self, genesis: H256) -> bool {
		self.0.contains_key(&genesis)
	}

	/// Whether recovery lost the heads of every chain in `chains`.
	fn contains_all(&self, chains: &BTreeSet<H256>) -> bool {
		chains.iter().all(|genesis| self.contains(*genesis))
	}

	/// The [`RecoveryError::HeadsLost`] of the first lost chain in
	/// `live_chains`, or `None` when no lost chain has live transactions.
	fn heads_lost_error(&self, live_chains: &BTreeSet<H256>) -> Option<RecoveryError> {
		let (genesis, reason) = self.0.iter().find(|(genesis, _)| live_chains.contains(genesis))?;
		Some(RecoveryError::HeadsLost { genesis: *genesis, reason: reason.clone() })
	}
}

/// Why recovery stopped while transactions were still live. The host retries.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum RecoveryError {
	/// The runtime has no core database, so there is nothing to recover.
	#[error("no core database configured")]
	NotConfigured,
	/// A chain's head subscription failed or ended while transactions on it
	/// were still pending, so nothing would drive their next pass.
	#[error("head events of chain {genesis:?} unavailable: {reason}")]
	HeadsLost {
		/// Genesis hash of the chain.
		genesis: H256,
		/// What happened.
		reason: String,
	},
}

impl DurableTxEngine {
	/// Runs recovery passes until no pending transaction is left whose outcome
	/// recovery can look up, and none waits to be built.
	///
	/// Recovery looks up a transaction's outcome on chain only when no sending
	/// task is following it and its domain has a registered oracle. It also
	/// starts building waiting transactions, since after a restart nothing
	/// else does.
	///
	/// The first pass runs as soon as this is called. After that, a pass runs on
	/// each new block of a registered chain, whenever a sending task stops
	/// following a transaction, and whenever the last transaction waiting
	/// to be built stops waiting. Anything that arrives while a pass runs is
	/// handled by the next pass.
	///
	/// If the ledger cannot be read, recovery keeps running. When it can no
	/// longer follow a chain's new blocks, it keeps serving the other chains,
	/// then fails with [`RecoveryError::HeadsLost`] if that chain still has
	/// pending transactions. Failing or being dropped leaves every pending
	/// transaction in the ledger for the next run.
	pub async fn run_until_settled(self: &Arc<Self>) -> Result<(), RecoveryError> {
		self.ensure_submissions_started();
		let wakes = self.recovery_wakes.subscribe();
		let built = self.built_triggers().await;
		let mut lost = LostChains::default();
		self.run_pass().await;
		if let ControlFlow::Break(ended) = self.after_pass(&lost).await {
			return ended;
		}
		let mut triggers = self.triggers(wakes, built).await;
		loop {
			next_trigger(&mut triggers, &mut lost).await;
			self.run_pass().await;
			if let ControlFlow::Break(ended) = self.after_pass(&lost).await {
				return ended;
			}
		}
	}

	/// Every head of every registered chain, every wake, and `built`.
	/// Heads are subscribed only after the first pass, so a ledger that is
	/// already settled needs no head at all.
	async fn triggers(&self, wakes: mpsc::Receiver<()>, built: Triggers) -> Triggers {
		let mut sources = vec![wakes.map(|()| Trigger::Pass).boxed(), built];
		for genesis in self.registry.chains() {
			sources.push(self.head_triggers(genesis).await);
		}
		stream::select_all(sources).boxed()
	}

	/// One trigger each time the last row waiting to be built stops
	/// waiting, from now on. Building follows no chain's heads, so a row
	/// given up after they were lost would otherwise end recovery's work
	/// with no pass left to notice.
	async fn built_triggers(&self) -> Triggers {
		let registry = self.registry.clone();
		let mut waiting = dao::observe_waiting_domains(&self.db)
			.map(move |domains| domains.map(|domains| registry.serves_any(&domains)));
		waiting.next().await;
		waiting.filter_map(|waiting| ready(as_built_trigger(waiting))).boxed()
	}

	/// One trigger per head of the chain with `genesis`, ending in
	/// [`Trigger::HeadsLost`] when the node's head subscription cannot open
	/// or ends.
	async fn head_triggers(&self, genesis: H256) -> Triggers {
		let heads = match self.heads.head_events(genesis).await {
			Ok(heads) => heads,
			Err(error) => return heads_lost(genesis, error.to_string()),
		};
		heads
			.filter_map(move |head| ready(as_trigger(genesis, head)))
			.chain(heads_lost(genesis, "head events ended".into()))
			.boxed()
	}

	/// Whether recovery goes on after a pass. It goes on while a transaction
	/// some registered domain can decide is live on a chain whose heads it
	/// still receives. Rows of an unregistered domain do not count: no
	/// pass could decide them. Once no such chain is left, it fails if a
	/// chain whose heads were lost still has live transactions, and
	/// otherwise goes on only while a transaction waits to be built.
	async fn after_pass(&self, lost: &LostChains) -> ControlFlow<Result<(), RecoveryError>> {
		let Some(live_chains) = self.readable_live_chains().await else {
			return ControlFlow::Continue(());
		};
		if !lost.contains_all(&live_chains) {
			return ControlFlow::Continue(());
		}
		if let Some(error) = lost.heads_lost_error(&live_chains) {
			return ControlFlow::Break(Err(error));
		}
		match self.waits_to_be_built().await {
			true => ControlFlow::Continue(()),
			false => ControlFlow::Break(Ok(())),
		}
	}

	/// The chains with live transactions, or `None` when the ledger cannot
	/// be read.
	async fn readable_live_chains(&self) -> Option<BTreeSet<H256>> {
		let domains = self.read_or_warn(dao::live_domains).await?;
		Some(self.chains_of(&domains))
	}

	/// Whether a transaction of a registered domain waits to be built. An unreadable ledger counts
	/// as one waiting.
	async fn waits_to_be_built(&self) -> bool {
		let waiting = self.read_or_warn(dao::waiting_domains).await;
		waiting.is_none_or(|domains| self.registry.serves_any(&domains))
	}

	/// The result of `query`, or `None` when the ledger cannot be read.
	/// An unreadable ledger keeps recovery running: abandoning transactions
	/// is far worse than one more pass.
	async fn read_or_warn<T: Send + 'static>(
		&self,
		query: fn(&Connection) -> Result<T, DbError>,
	) -> Option<T> {
		self.db
			.read(query)
			.await
			.inspect_err(|error| {
				warn!(%error, "durable recovery could not read the ledger");
			})
			.ok()
	}

	/// The chains `domains` live on, skipping unregistered domains.
	fn chains_of(&self, domains: &[DomainId]) -> BTreeSet<H256> {
		domains.iter().filter_map(|domain| self.registry.chain(domain)).collect()
	}
}

/// A head read is one trigger. A failed read is one missed head, not a
/// reason to stop: later heads still follow.
fn as_trigger(genesis: H256, head: Result<HeadEvent, RuntimeFailure>) -> Option<Trigger> {
	match head {
		Ok(_) => Some(Trigger::Pass),
		Err(error) => {
			warn!(?genesis, %error, "durable recovery missed a head");
			None
		},
	}
}

/// A pass once nothing waits to be built. A failed read is skipped: the
/// ledger is observed again on the next commit.
fn as_built_trigger(waiting: Result<bool, DbError>) -> Option<Trigger> {
	match waiting {
		Ok(false) => Some(Trigger::Pass),
		Ok(true) | Err(_) => None,
	}
}

fn heads_lost(genesis: H256, reason: String) -> Triggers {
	stream::once(ready(Trigger::HeadsLost { genesis, reason })).boxed()
}

/// Waits for the next trigger, then takes every one already queued behind
/// it, so heads that arrived during a pass cost one more pass, not one each.
async fn next_trigger(triggers: &mut Triggers, lost: &mut LostChains) {
	let first = triggers.next().await;
	first.into_iter().for_each(|trigger| record(trigger, lost));
	while let Some(Some(trigger)) = triggers.next().now_or_never() {
		record(trigger, lost);
	}
}

fn record(trigger: Trigger, lost: &mut LostChains) {
	if let Trigger::HeadsLost { genesis, reason } = trigger {
		warn!(?genesis, %reason, "durable recovery lost a chain's heads");
		lost.0.insert(genesis, reason);
	}
}

#[cfg(test)]
mod tests;
