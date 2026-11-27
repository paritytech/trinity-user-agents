//! An engine over a live chain, for ignored live tests.

use std::sync::Arc;

use crate::{
	chain::tokio_spawner,
	chain_runtime::ChainRuntime,
	durable::{
		engine::{DurableDeps, DurableTxEngine},
		oracle::DurableRegistry,
	},
	store::Db,
	time::RealTimer,
};

/// An engine over the live `chain` serving the domains of `registry`, which
/// builds and settles their transactions on its own: a recovery pass runs
/// every three seconds on the test's tokio runtime.
pub fn live_engine(
	chain: &ChainRuntime,
	db: Db,
	registry: DurableRegistry,
) -> Arc<DurableTxEngine> {
	let chain = Arc::new(chain.clone());
	let engine = DurableTxEngine::new(DurableDeps {
		db,
		registry,
		heads: chain.clone(),
		blocks: chain.clone(),
		validator: chain.clone(),
		submitter: chain,
		timer: Arc::new(RealTimer),
		spawner: tokio_spawner(),
	});
	let settling = engine.clone();
	tokio::spawn(async move {
		while settling.run_until_settled().await.is_ok() {
			tokio::time::sleep(core::time::Duration::from_secs(3)).await
		}
	});
	engine
}
