//! Backup recovery: after a reinstall, finds the NFTs the seed's purses held
//! under its earlier installations and records their keys, so the holding
//! tracker picks them up.
//!
//! [`NftRecovery::start`] runs one task for the life of the runtime. While a
//! session is active it reads the installations the seed registered and
//! scans every one it has not finished, purse by purse, at one finalized
//! block. A failed run is retried with a growing pause while the session
//! lasts. [`NftRecovery::wait_until_finished`] is what new purse ids wait on.

mod asset_hub;
mod recovered_purses;

use std::{collections::HashSet, ops::ControlFlow, sync::Arc, time::Duration};

use futures::StreamExt;
use subxt::utils::H256;
use truapi::latest::GenericError;

pub use self::{asset_hub::AssetHubRecoveryChain, recovered_purses::recovery_scan_config};
use self::recovered_purses::{NftScannedPurses, recovery_passes};
use super::{HeldItem, ItemKey, ItemKeyDeriver, dao};
use crate::{
	BackgroundTask, Spawner,
	chain_runtime::RuntimeFailure,
	gap_scan::{GapScanConfig, HoldingLookup, PurseScanError, PurseScanner},
	host_logic::session::SessionState,
	installation::{InstallationId, InstallationIdProvider},
	installation_registry::RegistryReadError,
	store::{Db, DbError},
	time::{Backoff, retry},
};

/// The pauses between failed runs while a session lasts.
const RETRY_PAUSES: Backoff = Backoff::new(Duration::from_secs(30), Duration::from_secs(30 * 60));

/// Why a backup recovery run stopped before it finished.
#[derive(Debug, thiserror::Error)]
pub enum NftRecoveryError {
	/// The finalized block or a contract read could not be reached.
	#[error(transparent)]
	Chain(#[from] RuntimeFailure),
	/// The installation registry could not be read.
	#[error(transparent)]
	Registry(#[from] RegistryReadError),
	/// The current installation is not known yet.
	#[error("the current installation is unknown: {0}")]
	Installation(GenericError),
	/// The purse scan failed.
	#[error(transparent)]
	Scan(#[from] PurseScanError),
	/// The core database failed.
	#[error(transparent)]
	Db(#[from] DbError),
}

/// The chain reads a recovery run makes, all at one finalized block.
#[async_trait::async_trait]
pub trait RecoveryChain: Send + Sync {
	/// The latest finalized block.
	async fn finalized_block(&self) -> Result<H256, NftRecoveryError>;

	/// Every installation the session's seed registered, as of `at`.
	async fn registered_installations(
		&self,
		at: H256,
	) -> Result<HashSet<InstallationId>, NftRecoveryError>;

	/// What each of `keys` holds at `at`, in the same order, with `None` for
	/// a key that holds nothing.
	async fn holdings_at(
		&self,
		at: H256,
		keys: &[ItemKey],
	) -> anyhow::Result<Vec<Option<HeldItem>>>;
}

/// What item keys hold at one block, read through a [`RecoveryChain`].
struct HoldingsAtBlock {
	chain: Arc<dyn RecoveryChain>,
	at: H256,
}

#[async_trait::async_trait]
impl HoldingLookup for HoldingsAtBlock {
	type Key = ItemKey;
	type Holding = HeldItem;

	async fn read_holdings(&self, keys: &[ItemKey]) -> anyhow::Result<Vec<Option<HeldItem>>> {
		self.chain.holdings_at(self.at, keys).await
	}
}

/// What backup recovery runs on.
pub struct NftRecoveryDeps {
	/// The core database the found keys go into.
	pub db: Db,
	/// The chain the installations and NFTs are read from.
	pub chain: Arc<dyn RecoveryChain>,
	/// Derives item keys from the session's seed.
	pub keys: Arc<dyn ItemKeyDeriver>,
	/// The installation this app runs as, which is never scanned.
	pub installation: Arc<dyn InstallationIdProvider>,
	/// The session the runs follow.
	pub session_state: Arc<SessionState>,
	/// Where the task runs.
	pub spawner: Spawner,
	/// How far each purse's keys are scanned, [`recovery_scan_config`] outside tests.
	pub scan_config: GapScanConfig,
}

/// Backup recovery, running for as long as this handle lives.
pub struct NftRecovery(RecoveryState);

/// Whether recovery runs on this host.
enum RecoveryState {
	/// The stack names no installation registry, so there is nothing to
	/// recover.
	Off,
	/// Recovery follows the sessions and records its progress in `db`.
	Running { db: Db, _following: BackgroundTask },
}

impl NftRecovery {
	/// Starts on the deps' spawner, recovering during each session.
	pub fn start(deps: NftRecoveryDeps) -> Self {
		let spawner = deps.spawner.clone();
		let db = deps.db.clone();
		let following = spawner.create_task(recover_during_sessions(Arc::new(deps)));
		Self(RecoveryState::Running { db, _following: following })
	}

	/// Recovery on a host with nothing to recover, which is always finished.
	pub fn off() -> Self {
		Self(RecoveryState::Off)
	}

	/// Completes once recovery has read the installation registry on this
	/// install and finished every installation it found there, at once when
	/// that is already so.
	///
	/// An installation registered after that is pending again until a later
	/// run scans it, and so is the wait. A purse created at an id an earlier
	/// purse is recreated at would take over that purse's NFTs, because an
	/// item's purse is also the purse its key is derived under.
	pub async fn wait_until_finished(&self) {
		let RecoveryState::Running { db, .. } = &self.0 else {
			return;
		};
		let readings = dao::observe_recovery_finished(db);
		readings.any(|reading| core::future::ready(reads_as_finished(reading))).await;
	}
}

/// Whether `reading` says recovery has finished. An unreadable state is
/// logged and is not finished.
fn reads_as_finished(reading: Result<bool, DbError>) -> bool {
	reading.unwrap_or_else(|error| {
		tracing::warn!(%error, "NFT backup recovery state unreadable");
		false
	})
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
	deps.spawner.create_task(recover_until_done(deps.clone()))
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

/// One run: reads the registered installations at the finalized block and
/// scans every installation not finished yet.
async fn run_recovery_once(deps: &NftRecoveryDeps) -> Result<(), NftRecoveryError> {
	let current = deps
		.installation
		.current_installation()
		.await
		.map_err(NftRecoveryError::Installation)?;
	let at = deps.chain.finalized_block().await?;
	let registered = deps.chain.registered_installations(at).await?;
	record_registered_installations(&deps.db, current, registered).await?;
	let pending = deps.db.read(dao::pending_recoveries).await?;
	recover_installations(deps, pending, at).await
}

/// Records `current` as finished, so it is never scanned, and each other
/// installation in `registered` as pending unless already recorded. After
/// the first read on this install, recovery is finished exactly when no
/// recorded installation is pending.
async fn record_registered_installations(
	db: &Db,
	current: InstallationId,
	registered: HashSet<InstallationId>,
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
) -> Result<(), NftRecoveryError> {
	if installations.is_empty() {
		return Ok(());
	}
	tracing::info!(installations = installations.len(), "NFT backup recovery scanning");
	let holdings = Arc::new(HoldingsAtBlock { chain: deps.chain.clone(), at });
	let store = NftScannedPurses::new(deps.db.clone());
	let scanner = PurseScanner::new(
		deps.keys.clone(),
		holdings,
		installations.clone(),
		store,
		deps.scan_config,
	);
	scanner.scan_purse_passes(&recovery_passes()).await?;
	mark_installations_done(&deps.db, installations).await?;
	tracing::info!("NFT backup recovery finished");
	Ok(())
}

/// Records each of `installations` as finished.
async fn mark_installations_done(
	db: &Db,
	installations: Vec<InstallationId>,
) -> Result<(), DbError> {
	db.write(move |tx| {
		installations
			.iter()
			.try_for_each(|installation| dao::mark_recovery_done(tx, *installation))
	})
	.await
}

#[cfg(test)]
mod tests;
