//! Turning a policy's answer into ledger state: an attempt started on a
//! waiting row, or the row failed.

use std::sync::Arc;

use subxt::utils::H256;
use tracing::warn;

use crate::{
	chain::MortalExtrinsic,
	durable::{
		dao,
		engine::{
			DurableTxEngine,
			tracker::{Attempt, spawn_watch},
		},
		model::{DurableTxId, PolicyParams},
	},
	store::DbError,
};

/// Gives waiting row `id` the attempt `extrinsic` makes, with `params` when
/// the policy replaced them, and watches it.
/// Ownership is taken before the row becomes `Pending`, so a recovery pass
/// never decides an attempt its watch is about to follow.
pub async fn start_built_attempt(
	engine: &Arc<DurableTxEngine>,
	id: DurableTxId,
	genesis: H256,
	extrinsic: MortalExtrinsic,
	params: Option<PolicyParams>,
) {
	engine.ownership.acquire(id);
	if !record_attempt(engine, id, &extrinsic, params).await {
		engine.ownership.release(id);
		return;
	}
	let attempt = Attempt::new(id, genesis, &extrinsic);
	spawn_watch(engine, attempt, extrinsic.extrinsic);
}

/// Fails waiting row `id`.
pub async fn give_up_submission(engine: &DurableTxEngine, id: DurableTxId) {
	let abandon = move |tx: &rusqlite::Transaction<'_>| dao::abandon_submission(tx, id);
	let failure = "durable submission could not be abandoned";
	write_or_warn(engine, id, failure, abandon).await;
}

/// Writes the attempt, and `params` when given, onto the row while it still
/// waits. Returns whether it wrote.
async fn record_attempt(
	engine: &DurableTxEngine,
	id: DurableTxId,
	extrinsic: &MortalExtrinsic,
	params: Option<PolicyParams>,
) -> bool {
	let extrinsic = extrinsic.clone();
	let start = move |tx: &rusqlite::Transaction<'_>| {
		dao::start_attempt(tx, id, &extrinsic, params.as_ref())
	};
	write_or_warn(engine, id, "durable attempt could not be recorded", start).await
}

/// Runs `write` on row `id` and returns whether it changed the row. A failed
/// write changes nothing and is logged as `failure`.
async fn write_or_warn(
	engine: &DurableTxEngine,
	id: DurableTxId,
	failure: &'static str,
	write: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<bool, DbError> + Send + 'static,
) -> bool {
	match engine.db.write(write).await {
		Ok(changed) => changed,
		Err(error) => {
			warn!(id = id.0, %error, "{failure}");
			false
		},
	}
}
