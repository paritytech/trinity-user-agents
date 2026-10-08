//! The recovery loop: passes on every head and wake until nothing is live.

use std::sync::Arc;

use futures::FutureExt;
use futures::channel::mpsc;
use futures::future::ready;
use futures::stream::{self, BoxStream, StreamExt};
use subxt::utils::H256;
use tracing::warn;

use super::DurableTxEngine;
use crate::chain::HeadEvent;
use crate::chain_runtime::RuntimeFailure;
use crate::durable::dao;
use crate::durable::model::DomainId;
use crate::store::DbError;

/// What wakes the loop for another pass. An `Err` stops it.
type Triggers = BoxStream<'static, Result<(), RecoveryError>>;

/// Why recovery stopped while transactions were still live. The host retries.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecoveryError {
    /// The runtime has no core database, so there is nothing to recover.
    #[error("no core database configured")]
    NotConfigured,
    /// A chain's head subscription failed or ended, so nothing would drive
    /// the next pass.
    #[error("head events of chain {genesis:?} unavailable: {reason}")]
    HeadsLost {
        /// Genesis hash of the chain.
        genesis: H256,
        /// What happened.
        reason: String,
    },
}

impl DurableTxEngine {
    /// Runs recovery passes until no transaction a registered domain can
    /// decide is live and none waits to be built: one at once, then one per
    /// new head of any registered chain and per wake, collapsing whatever
    /// arrives while a pass runs. Starts building waiting transactions too,
    /// since after a restart nothing else does. An unreadable ledger counts
    /// as live. Fails when a head subscription does; dropping the future
    /// stops it.
    pub async fn run_until_settled(self: &Arc<Self>) -> Result<(), RecoveryError> {
        self.ensure_submissions_started();
        let wakes = self.recovery_wakes.subscribe();
        self.run_pass().await;
        if !self.has_live().await {
            return Ok(());
        }
        let mut triggers = self.triggers(wakes).await?;
        loop {
            next_trigger(&mut triggers).await?;
            self.run_pass().await;
            if !self.has_live().await {
                return Ok(());
            }
        }
    }

    /// Every head of every registered chain, and every wake. Subscribed only
    /// after the first pass, so a ledger that is already settled needs no
    /// head at all.
    async fn triggers(&self, wakes: mpsc::Receiver<()>) -> Result<Triggers, RecoveryError> {
        let mut sources = vec![wakes.map(Ok).boxed()];
        for genesis in self.registry.chains() {
            sources.push(self.head_triggers(genesis).await?);
        }
        Ok(stream::select_all(sources).boxed())
    }

    /// One trigger per head of the chain with `genesis`, ending in an error
    /// when the node's head subscription ends.
    async fn head_triggers(&self, genesis: H256) -> Result<Triggers, RecoveryError> {
        let heads = self
            .heads
            .head_events(genesis)
            .await
            .map_err(|error| heads_lost(genesis, error.to_string()))?;
        let ended =
            stream::once(async move { Err(heads_lost(genesis, "head events ended".into())) });
        Ok(heads
            .filter_map(move |head| ready(as_trigger(genesis, head)))
            .chain(ended)
            .boxed())
    }

    /// Whether a transaction some registered domain can decide is live, or
    /// one waits to be built. Rows of a domain without an oracle stay as they
    /// are: no pass could decide them. An unreadable ledger counts as live:
    /// abandoning transactions is far worse than one more pass.
    async fn has_live(&self) -> bool {
        let live = self.read_live().await;
        live.unwrap_or_else(|error| {
            warn!(%error, "durable recovery could not read the ledger");
            true
        })
    }

    /// [`Self::has_live`], failing when the ledger cannot be read.
    async fn read_live(&self) -> Result<bool, DbError> {
        if self.db.read(dao::has_pending_submissions).await? {
            return Ok(true);
        }
        let domains = self.db.read(dao::live_domains).await?;
        Ok(self.any_decidable(&domains))
    }

    /// Whether any of `domains` has an oracle to decide it.
    fn any_decidable(&self, domains: &[DomainId]) -> bool {
        domains
            .iter()
            .any(|domain| self.registry.oracle(domain).is_some())
    }
}

/// A head read is one trigger. A failed read is one missed head, not a
/// reason to stop: later heads still follow.
fn as_trigger(
    genesis: H256,
    head: Result<HeadEvent, RuntimeFailure>,
) -> Option<Result<(), RecoveryError>> {
    match head {
        Ok(_) => Some(Ok(())),
        Err(error) => {
            warn!(?genesis, %error, "durable recovery missed a head");
            None
        }
    }
}

fn heads_lost(genesis: H256, reason: String) -> RecoveryError {
    RecoveryError::HeadsLost { genesis, reason }
}

/// Waits for the next trigger, then takes every one already queued behind
/// it, so heads that arrived during a pass cost one more pass, not one each.
async fn next_trigger(triggers: &mut Triggers) -> Result<(), RecoveryError> {
    triggers.next().await.unwrap_or(Ok(()))?;
    while let Some(Some(trigger)) = triggers.next().now_or_never() {
        trigger?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
