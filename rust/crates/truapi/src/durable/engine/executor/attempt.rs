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

/// Whether a write on a waiting row changed the ledger. It does not when
/// the row was no longer waiting, or when the write failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LedgerChange {
	/// The row changed.
	Changed,
	/// The ledger is as it was.
	Unchanged,
}

impl From<bool> for LedgerChange {
	/// [`Self::Changed`] when a write reports it `changed` a row.
	fn from(changed: bool) -> Self {
		if changed { Self::Changed } else { Self::Unchanged }
	}
}

/// Gives waiting row `id` the attempt `extrinsic` makes and watches it.
/// Ownership is taken before the row becomes `Pending`, so a recovery pass
/// never decides an attempt its watch is about to follow.
pub async fn start_built_attempt(
	engine: &Arc<DurableTxEngine>,
	id: DurableTxId,
	genesis: H256,
	extrinsic: MortalExtrinsic,
) -> LedgerChange {
	let tx_hash = extrinsic.extrinsic.hash();
	if !engine.ownership.acquire(id, tx_hash) {
		debug!(id = id.0, ?tx_hash, "durable rebuild repeats a released attempt");
		return LedgerChange::Unchanged;
	}
	let recorded = record_attempt(engine, id, &extrinsic).await;
	if recorded == LedgerChange::Unchanged {
		engine.ownership.abandon_all(&[(id, tx_hash)]);
		return recorded;
	}
	let attempt = Attempt::new(id, genesis, &extrinsic);
	spawn_watch(engine, attempt, extrinsic.extrinsic);
	recorded
}

/// Fails waiting row `id`.
pub async fn give_up_submission(engine: &DurableTxEngine, id: DurableTxId) -> LedgerChange {
	let abandon = move |tx: &rusqlite::Transaction<'_>| dao::abandon_submission(tx, id);
	let failure = "durable submission could not be abandoned";
	write_or_warn(engine, id, failure, abandon).await
}

/// Writes the attempt onto the row while it still waits.
async fn record_attempt(
	engine: &DurableTxEngine,
	id: DurableTxId,
	extrinsic: &MortalExtrinsic,
) -> LedgerChange {
	let extrinsic = extrinsic.clone();
	let start = move |tx: &rusqlite::Transaction<'_>| dao::start_attempt(tx, id, &extrinsic);
	write_or_warn(engine, id, "durable attempt could not be recorded", start).await
}

/// Runs `write` on row `id`, which reports whether it changed the row. A
/// failed write changes nothing and is logged as `failure`.
async fn write_or_warn(
	engine: &DurableTxEngine,
	id: DurableTxId,
	failure: &'static str,
	write: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<bool, DbError> + Send + 'static,
) -> LedgerChange {
	match engine.db.write(write).await {
		Ok(changed) => LedgerChange::from(changed),
		Err(error) => {
			warn!(id = id.0, %error, "{failure}");
			LedgerChange::Unchanged
		},
	}
}
