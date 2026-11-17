//! Backup recovery: after a reinstall, finds the NFTs the seed's purses held
//! under its earlier installations and records their keys, so the holding
//! tracker picks them up.
//!
//! [`NftRecovery::start`] runs one task for the life of the runtime. While a
//! session is active it reads the installations the seed registered and
//! scans every one it has not finished, purse by purse, at one finalized
//! block. A failed run is retried with a growing pause while the session
//! lasts. Once a run has finished every registered installation it opens
//! the [`PurseIdGate`] new purses wait on.

mod asset_hub;
mod purse_scanner;

use std::{
	ops::ControlFlow,
	sync::{Arc, Weak},
	time::Duration,
};

use futures::StreamExt;
use subxt::utils::H256;
use truapi::latest::GenericError;

pub use self::asset_hub::AssetHubRecoveryChain;
use self::purse_scanner::{NftPurseScanner, recovery_passes};
use super::{HeldItem, ItemKey, dao, purse_id_gate::PurseIdGate};
use crate::{
	Spawner,
	background_task::BackgroundTask,
	chain::StorageWatchError,
	chain_runtime::RuntimeFailure,
	gap_scan::{HardKeyDerivation, HoldingLookup, scan_purse_passes},
	host_logic::session::SessionState,
	installation::{InstallationId, InstallationIdProvider},
	installation_registry::RegistryReadError,
	store::{Db, DbError},
	time::{Backoff, retry},
};

/// The pauses between failed runs while a session lasts.
const RETRY_PAUSES: Backoff = Backoff::new(Duration::from_secs(30), Duration::from_secs(30 * 60));

/// What item keys hold, as [`RecoveryChain::holdings_at`] reads them.
pub type ItemHoldings = dyn HoldingLookup<ItemKey, Holding = HeldItem, Error = StorageWatchError>;

/// The chain reads a recovery run makes, all at one finalized block.
#[async_trait::async_trait]
pub trait RecoveryChain: Send + Sync {
	/// The latest finalized block.
	async fn finalized_block(&self) -> Result<H256, RecoveryError>;

	/// Every installation the session's seed registered, as of `at`.
	async fn registered_installations(
		&self,
		at: H256,
	) -> Result<Vec<InstallationId>, RecoveryError>;

	/// Reads of what item keys hold at `at`.
	fn holdings_at(&self, at: H256) -> Arc<ItemHoldings>;
}

/// Why a recovery run stopped before it finished.
#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
	/// The finalized block or a contract read could not be reached.
	#[error(transparent)]
	Chain(#[from] RuntimeFailure),
	/// The installation registry could not be read.
	#[error(transparent)]
	Registry(#[from] RegistryReadError),
	/// The current installation is not known yet.
	#[error("the current installation is unknown: {0}")]
	Installation(GenericError),
	/// A purse could not be scanned.
	#[error("scanning purse {purse} failed: {reason}")]
	Scan {
		/// The purse's id.
		purse: u32,
		/// Why its scan failed.
		reason: String,
	},
	/// The core database failed.
	#[error(transparent)]
	Db(#[from] DbError),
}

/// What backup recovery runs on.
pub struct NftRecoveryDeps {
	/// The core database the found keys go into.
	pub db: Db,
	/// The chain the installations and NFTs are read from.
	pub chain: Arc<dyn RecoveryChain>,
	/// Derives item keys from the session's seed.
	pub keys: Weak<dyn HardKeyDerivation>,
	/// The installation this app runs as, which is never scanned.
	pub installation: Arc<dyn InstallationIdProvider>,
	/// The session the runs follow.
	pub session_state: Arc<SessionState>,
	/// The gate opened once every registered installation is finished.
	pub purse_ids: PurseIdGate,
	/// Where the task runs.
	pub spawner: Spawner,
}

/// Backup recovery, running for as long as this handle lives.
pub struct NftRecovery {
	_following: BackgroundTask,
}

impl NftRecovery {
	/// Starts on the deps' spawner: opens the gate at once when an earlier run
	/// on this install finished every installation it found, then recovers
	/// during each session.
	pub fn start(deps: NftRecoveryDeps) -> Self {
		let spawner = deps.spawner.clone();
		let deps = Arc::new(deps);
		let following = async move {
			open_gate_if_finished(&deps).await;
			recover_during_sessions(deps).await;
		};
		Self { _following: BackgroundTask::spawn(&spawner, following) }
	}
}

/// Opens the gate when an earlier run on this install read the registry and
/// finished every installation it found there.
async fn open_gate_if_finished(deps: &NftRecoveryDeps) {
	let finished = deps.db.read(dao::recovery_finished).await;
	match finished {
		Ok(true) => deps.purse_ids.open(),
		Ok(false) => {},
		Err(error) => tracing::warn!(%error, "NFT backup recovery state unreadable"),
	}
}

/// Starts a fresh run on every session change while a session is active,
/// and drops the run when the session ends. A run derives keys from the
/// seed loaded when it reads them, so a run never outlives its session.
async fn recover_during_sessions(deps: Arc<NftRecoveryDeps>) {
	let mut changes = deps.session_state.subscribe();
	let mut run = None;
	while changes.next().await.is_some() {
		drop(run.take());
		run = deps.session_state.current().map(|_| spawn_recovery_run(&deps));
	}
}

/// A run that retries until it succeeds.
fn spawn_recovery_run(deps: &Arc<NftRecoveryDeps>) -> BackgroundTask {
	BackgroundTask::spawn(&deps.spawner, recover_until_done(deps.clone()))
}

/// Runs recovery until a run succeeds, pausing longer after each failure.
async fn recover_until_done(deps: Arc<NftRecoveryDeps>) {
	let mut pauses = RETRY_PAUSES;
	retry(
		|| {
			let deps = deps.clone();
			async move {
				let outcome = run_recovery_once(&deps).await;
				outcome.map_or_else(ControlFlow::Continue, ControlFlow::Break)
			}
		},
		|error| {
			tracing::warn!(%error, "NFT backup recovery failed, retrying");
			pauses.next_pause()
		},
	)
	.await;
}

/// One run: reads the registered installations at the finalized block,
/// scans every installation not finished yet, and opens the gate.
async fn run_recovery_once(deps: &NftRecoveryDeps) -> Result<(), RecoveryError> {
	let current = deps
		.installation
		.current_installation()
		.await
		.map_err(RecoveryError::Installation)?;
	let at = deps.chain.finalized_block().await?;
	let registered = deps.chain.registered_installations(at).await?;
	record_registered_installations(&deps.db, current, registered).await?;
	let pending = deps.db.read(dao::pending_recoveries).await?;
	recover_installations(deps, pending, at).await?;
	deps.purse_ids.open();
	Ok(())
}

/// Records `current` as finished, so it is never scanned, and each other
/// installation in `registered` as pending unless already recorded. After
/// the first read on this install, recovery is finished exactly when no
/// recorded installation is pending.
async fn record_registered_installations(
	db: &Db,
	current: InstallationId,
	registered: Vec<InstallationId>,
) -> Result<(), DbError> {
	db.write(move |tx| {
		dao::mark_recovery_done(tx, current)?;
		registered
			.iter()
			.try_for_each(|installation| dao::insert_pending_recovery(tx, *installation))
	})
	.await
}

/// Walks the purse ids once across all of `installations` at `at`, then
/// records them all as finished. Every installation shares one sequence of
/// purse ids, so a purse counts as empty only when it is empty under each.
async fn recover_installations(
	deps: &NftRecoveryDeps,
	installations: Vec<InstallationId>,
	at: H256,
) -> Result<(), RecoveryError> {
	if installations.is_empty() {
		return Ok(());
	}
	tracing::info!(installations = installations.len(), "NFT backup recovery scanning");
	let holdings = deps.chain.holdings_at(at);
	let scanner =
		NftPurseScanner::new(deps.db.clone(), deps.keys.clone(), holdings, installations.clone());
	scan_purse_passes(&recovery_passes(), &scanner).await?;
	deps.db
		.write(move |tx| {
			installations
				.iter()
				.try_for_each(|installation| dao::mark_recovery_done(tx, *installation))
		})
		.await?;
	tracing::info!("NFT backup recovery finished");
	Ok(())
}

#[cfg(test)]
mod tests;
