//! Turning a policy's answer into ledger state: an attempt started on a
//! waiting row, or the row failed.

use std::sync::Arc;

use subxt::utils::H256;
use tracing::{debug, warn};

use crate::{
	chain::MortalExtrinsic,
	durable::{
		dao,
		engine::{
			DurableTxEngine,
			tracker::{Attempt, spawn_watch},
		},
		model::DurableTxId,
	},
	store::DbError,
};

/// Gives waiting row `id` the attempt `extrinsic` makes and watches it.
/// Ownership is taken before the row becomes `Pending`, so a recovery pass
/// never decides an attempt its watch is about to follow. Returns whether
/// the attempt started.
pub async fn start_built_attempt(
	engine: &Arc<DurableTxEngine>,
	id: DurableTxId,
	genesis: H256,
	extrinsic: MortalExtrinsic,
) -> bool {
	let tx_hash = extrinsic.extrinsic.hash();
	if !engine.ownership.acquire(id, tx_hash) {
		debug!(id = id.0, ?tx_hash, "durable rebuild repeats a released attempt");
		return false;
	}
	if !record_attempt(engine, id, &extrinsic).await {
		engine.ownership.abandon_all(&[(id, tx_hash)]);
		return false;
	}
	let attempt = Attempt::new(id, genesis, &extrinsic);
	spawn_watch(engine, attempt, extrinsic.extrinsic);
	true
}

/// Fails waiting row `id`. Returns whether it was still waiting.
pub async fn give_up_submission(engine: &DurableTxEngine, id: DurableTxId) -> bool {
	let abandon = move |tx: &rusqlite::Transaction<'_>| dao::abandon_submission(tx, id);
	let failure = "durable submission could not be abandoned";
	let abandoned = write_or_warn(engine, id, failure, abandon).await;
	if abandoned {
		engine.executor.forget_attempts(id);
	}
	abandoned
}

/// Writes the attempt onto the row while it still waits. Returns whether
/// it did.
async fn record_attempt(
	engine: &DurableTxEngine,
	id: DurableTxId,
	extrinsic: &MortalExtrinsic,
) -> bool {
	let extrinsic = extrinsic.clone();
	let start = move |tx: &rusqlite::Transaction<'_>| dao::start_attempt(tx, id, &extrinsic);
	write_or_warn(engine, id, "durable attempt could not be recorded", start).await
}

/// Runs `write` on row `id`. Returns what it reports, or false with a
/// warning when the write itself fails.
async fn write_or_warn(
	engine: &DurableTxEngine,
	id: DurableTxId,
	failure: &'static str,
	write: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<bool, DbError> + Send + 'static,
) -> bool {
	let written = engine.db.write(write).await;
	written.unwrap_or_else(|error| {
		warn!(id = id.0, %error, "{failure}");
		false
	})
}
