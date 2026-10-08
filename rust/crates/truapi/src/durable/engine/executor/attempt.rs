//! Turning a policy's answer into ledger state: an attempt started on a
//! waiting row, or the row failed.

use std::sync::Arc;

use subxt::utils::H256;
use tracing::{debug, warn};

use crate::chain::MortalExtrinsic;
use crate::durable::dao;
use crate::durable::engine::DurableTxEngine;
use crate::durable::engine::tracker::{Attempt, spawn_watch};
use crate::durable::model::DurableTxId;

impl DurableTxEngine {
    /// Gives waiting row `id` the attempt `extrinsic` makes and watches it.
    /// Ownership is taken before the row becomes `Pending`, so a recovery pass
    /// never decides an attempt its watch is about to follow. Returns whether
    /// the attempt started.
    pub async fn start_built_attempt(
        self: &Arc<Self>,
        id: DurableTxId,
        genesis: H256,
        extrinsic: MortalExtrinsic,
    ) -> bool {
        let tx_hash = extrinsic.extrinsic.hash();
        if !self.ownership.acquire(id, tx_hash) {
            debug!(
                id = id.0,
                ?tx_hash,
                "durable rebuild repeats a released attempt"
            );
            return false;
        }
        if !self.record_attempt(id, &extrinsic).await {
            self.ownership.abandon_all(&[(id, tx_hash)]);
            return false;
        }
        let attempt = Attempt::new(id, genesis, &extrinsic);
        spawn_watch(self, attempt, extrinsic.extrinsic);
        true
    }

    /// Fails waiting row `id`. Returns whether it was still waiting.
    pub async fn give_up_submission(&self, id: DurableTxId) -> bool {
        let abandoned = self
            .db
            .write(move |tx| dao::abandon_submission(tx, id))
            .await;
        abandoned.unwrap_or_else(|error| {
            warn!(id = id.0, %error, "durable submission could not be abandoned");
            false
        })
    }

    /// Writes the attempt onto the row while it still waits. Returns whether
    /// it did.
    async fn record_attempt(&self, id: DurableTxId, extrinsic: &MortalExtrinsic) -> bool {
        let extrinsic = extrinsic.clone();
        let recorded = self
            .db
            .write(move |tx| dao::start_attempt(tx, id, &extrinsic))
            .await;
        recorded.unwrap_or_else(|error| {
            warn!(id = id.0, %error, "durable attempt could not be recorded");
            false
        })
    }
}
