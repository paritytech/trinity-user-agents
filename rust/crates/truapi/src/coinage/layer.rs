//! G1 · The primitive API, and G3 · Initialization: the running layer, and
//! the start sequence that constructs it.
//!
//! [`start`] returns a [`Layer`] only after the start sequence has run: the
//! runtime check, the store opened (minting the installation on a fresh
//! store), every uncommitted export released, and [`Running`] opened. Every
//! other primitive is a method on the `Layer`. A long-running primitive
//! checks its arguments, then records its operation and registers its
//! records in one store transaction, or joins the operation its key already
//! names (B4); it returns before any attempt is submitted (invariant 1).
//! The streams are [`Layer::status`], [`Layer::balance`],
//! [`Layer::export_status`] and [`Layer::events`].

mod streams;

use core::time::Duration;
use std::sync::Arc;

use parity_scale_codec::Encode;
use parking_lot::Mutex;
use subxt::Metadata;

pub use self::streams::{Event, ExportedCoinStatus};
use super::{
	Asset, ChainConfig, ChainParams, CoinSecret, Initiator, PrivacyPolicy, PurseId, Snapshot,
	assembly::Funding,
	balance::{Balance, balance},
	chain::CoinageChain,
	classification::{Class, CoinClass, EntryLocal, classify},
	ledger::{self, LedgerError, Mark, PurseState},
	maintenance::{self, Due},
	observation::{self, ObserverDeps},
	offload::{self, Offload},
	operation::{
		self, Begun, DeleteArguments, ImportArguments, OpKey, OpKind, OperationEnd, OperationError, OperationRecord,
		ReclaimArguments, Retry,
	},
	params::{InstanceRecord, validate},
	planning::{Destination, PlanInput, account_of},
	plugins::{PersonhoodSource, Running, RunningState, Seed},
	policy::{PolicyError, check},
	reconstruction::{self, InstallationRegistry, Purses, ReconstructionDeps, ScanConfig},
	registration::RegisterError,
	selection::{Forms, SelectRequest, Want, select},
	start::{
		Begin, ExportedCoin, Launched, Launcher, Request, StartError as LaunchError, begin_and_start, exported_coins,
		needs_confirmation,
	},
	status::{Completion, Status, completion, status},
};
use crate::{
	Spawner,
	chain::HashAndNumber,
	durable::DurableTxEngine,
	installation::InstallationId,
	store::{Db, DbError},
};

/// The layer's draws from §1's randomness.
pub trait Randomness: Send + Sync {
	/// A fresh installation identifier.
	fn installation(&self) -> InstallationId;
}

/// What A3's check reads from the chain.
pub struct RuntimeFacts {
	/// The runtime description.
	pub metadata: Arc<Metadata>,
	/// The configured instance's record; none when the chain has none.
	pub instance: Option<InstanceRecord>,
	/// The maximum coin age the runtime reports; none when it reports none.
	pub maximum_age: Option<u16>,
	/// The runtime's spec version.
	pub version: u32,
}

/// Reads [`RuntimeFacts`] from the People chain.
#[async_trait::async_trait]
pub trait RuntimeSource: Send + Sync {
	/// The facts at block `at`, or at the finalized head when none; none
	/// when they cannot be read.
	async fn runtime(&self, at: Option<HashAndNumber>) -> Option<RuntimeFacts>;
	/// The runtime's spec version at block `at`; none when it cannot be
	/// read.
	async fn version(&self, at: HashAndNumber) -> Option<u32>;
}

/// §1's interfaces, supplied by the host.
pub struct Interfaces {
	/// The shared store.
	pub store: Db,
	/// The durability layer, created with the plug-ins of [`with_coinage`](super::plugins::with_coinage).
	pub engine: Arc<DurableTxEngine>,
	/// State reads of the People chain.
	pub chain: Arc<dyn CoinageChain>,
	/// The runtime description and instance record.
	pub runtime: Arc<dyn RuntimeSource>,
	/// The recovery phrase's entropy.
	pub seed: Arc<dyn Seed>,
	/// The current time, in Unix seconds.
	pub clock: Arc<dyn Fn() -> u64 + Send + Sync>,
	/// The layer's random draws.
	pub randomness: Arc<dyn Randomness>,
	/// The person whose free tokens pay for unloads.
	pub personhood: Arc<dyn PersonhoodSource>,
	/// The installation registry.
	pub registry: Arc<dyn InstallationRegistry>,
	/// Runs a reconstruction in the background.
	pub spawner: Spawner,
	/// The initial policy, stored only on a fresh store.
	pub policy: PrivacyPolicy,
}

/// Why the layer did not start (G3 invariant 1).
#[derive(Debug, derive_more::Display, derive_more::Error, derive_more::From)]
pub enum StartError {
	/// A3's check failed for the current runtime.
	#[display("unsupported runtime: {check}")]
	UnsupportedRuntime {
		/// The check that failed.
		check: String,
	},
	/// The local store does not decode; the layer refuses it rather than
	/// discard what it holds.
	#[display("the local store does not decode")]
	UndecodableStore,
	/// The seed is unavailable.
	#[display("the seed is unavailable")]
	Unavailable,
	/// The store failed.
	Store(DbError),
}

/// A failure to start an operation, or a query that cannot be answered.
#[derive(Debug, derive_more::Display, derive_more::Error, derive_more::From)]
pub enum Error {
	/// The purse is unknown, closed, or, as a destination, being deleted.
	#[display("purse {} not found", _0.0)]
	PurseNotFound(#[error(not(source))] PurseId),
	/// The main purse cannot be deleted.
	#[display("the main purse cannot be deleted")]
	MainPurse,
	/// A3's check fails for the current runtime.
	#[display("unsupported runtime: {check}")]
	UnsupportedRuntime {
		/// The check that failed.
		check: String,
	},
	/// The spend needs held-back value the user has not confirmed.
	#[display("the spend needs the user's confirmation")]
	NeedsConfirmation,
	/// Another operation holds value of the purse.
	#[display("the purse holds value that cannot move")]
	PurseHoldsUnmovableValue,
	/// The purse already has a deletion open.
	#[display("purse {} already has a deletion open", _0.0)]
	DeletionOpen(#[error(not(source))] PurseId),
	/// Short even counting what is waiting.
	#[display("insufficient funds: {available} cents available")]
	InsufficientFunds {
		/// What the purse may take now or later.
		available: u64,
	},
	/// No unload token is free.
	#[display("no unload token is free")]
	NoUnloadToken,
	/// Another registration took an input the request named first.
	#[display("an input was taken by another operation")]
	InputTaken,
	/// The coin secret at this position is not a canonical key.
	#[display("the coin secret at position {_0} is not a canonical key")]
	InvalidSecret(#[error(not(source))] u32),
	/// No operation has this key.
	#[display("no operation has this key")]
	UnknownOperation(#[error(not(source))] OpKey),
	/// The key names an operation with different arguments.
	#[display("the key names an operation with different arguments")]
	KeyReused(#[error(not(source))] OpKey),
	/// The caller used the empty scope, which is the layer's own.
	#[display("the empty scope is the layer's own")]
	ReservedScope,
	/// An export, or one of the layer's own operations, cannot be cancelled.
	#[display("the operation cannot be cancelled")]
	NotCancellable,
	/// The operation is not an export.
	#[display("the operation is not an export")]
	NotAnExport(#[error(not(source))] OpKey),
	/// The export was released or has ended, or, for a release, is
	/// committed.
	#[display("the export is closed")]
	ExportClosed(#[error(not(source))] OpKey),
	/// The export is not committed: release it instead.
	#[display("the export is not committed")]
	ExportNotCommitted(#[error(not(source))] OpKey),
	/// The request moves no value.
	#[display("the request moves no value")]
	EmptyRequest,
	/// The seed is unavailable, or the durability layer does not serve the
	/// coinage domains.
	#[display("the seed or the durability layer is unavailable")]
	Unavailable,
	/// The store failed.
	#[from]
	Store(DbError),
}

/// The caller's identity for an operation, with its retry policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Op {
	/// The operation's key.
	pub key: OpKey,
	/// When a failed attempt is built again.
	pub retry: Retry,
}

/// What starting a long-running operation returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Started {
	/// The key was new.
	New,
	/// The key names a live operation with the same arguments.
	Joined,
	/// The key names one that has completed.
	Finished(Completion),
}

/// What [`Layer::export`] returned: the same coins however often it is
/// joined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
	/// Whether the export is new.
	pub started: Started,
	/// The coins handed to the payee.
	pub coins: Vec<ExportedCoin>,
}

/// A purse's metadata and balance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PurseInfo {
	/// Its identifier.
	pub id: PurseId,
	/// Its name.
	pub name: String,
	/// When it was created, in Unix seconds.
	pub created: u64,
	/// Opaque bytes its creator attached.
	pub tag: Vec<u8>,
	/// Its balance.
	pub balance: Balance,
}

/// Whether the current installation can be found from the seed alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recoverability {
	/// Its registration succeeded.
	Registered,
	/// Its registration has not succeeded; the wallet is usable, but value
	/// in this installation is found only through this store.
	Pending {
		/// Since when, in Unix seconds.
		since: u64,
	},
}

/// §1's randomness from the operating system: each installation identifier a
/// fresh 32-byte draw.
pub struct SystemRandomness;

impl Randomness for SystemRandomness {
	fn installation(&self) -> InstallationId {
		let mut id = [0u8; 32];
		getrandom::getrandom(&mut id).expect("the operating system provides randomness");
		InstallationId(id)
	}
}

/// The running layer.
#[derive(Clone)]
pub struct Layer(Arc<Inner>);

struct Inner {
	store: Db,
	engine: Arc<DurableTxEngine>,
	chain: Arc<dyn CoinageChain>,
	runtime: Arc<dyn RuntimeSource>,
	seed: Arc<dyn Seed>,
	clock: Arc<dyn Fn() -> u64 + Send + Sync>,
	personhood: Arc<dyn PersonhoodSource>,
	registry: Arc<dyn InstallationRegistry>,
	spawner: Spawner,
	config: ChainConfig,
	scan: ScanConfig,
	running: Running,
	failed_check: Mutex<Option<String>>,
	/// The runtime version of the latest check.
	checked_version: Mutex<u32>,
	streams: streams::Wakes,
}

/// Starts the layer (G3): validates the runtime, opens the store, releases
/// every uncommitted export, opens `running`, and starts the installation's
/// registration unless one succeeded or is open.
pub async fn start(
	interfaces: Interfaces,
	config: ChainConfig,
	scan: ScanConfig,
	purse_backup: Option<Vec<u8>>,
	running: Running,
) -> Result<Layer, StartError> {
	let facts = interfaces
		.runtime
		.runtime(None)
		.await
		.ok_or_else(|| StartError::UnsupportedRuntime { check: "the runtime could not be read".into() })?;
	let params = validate(&facts.metadata, facts.instance.as_ref(), facts.maximum_age, config.clone())
		.map_err(|unsupported| StartError::UnsupportedRuntime { check: unsupported.check })?;
	let now = (interfaces.clock)();

	let fresh = interfaces.randomness.installation();
	let policy = interfaces.policy.clone();
	interfaces
		.store
		.write_unless_refused(move |tx| {
			if ledger::current_installation(tx)?.is_some() {
				return Ok(Ok(()));
			}
			Ok(ledger::mint_installation(tx, fresh, &policy, purse_backup.as_deref(), now)?
				.map_err(|_| StartError::UndecodableStore))
		})
		.await??;
	interfaces.store.write(release_uncommitted).await.map_err(undecodable)?;

	let Interfaces { store, engine, chain, runtime, seed, clock, personhood, registry, spawner, .. } =
		interfaces;
	running.open(RunningState { params, check_failed: false, funding: Default::default() });
	let layer = Layer(Arc::new(Inner {
		store,
		engine,
		chain,
		runtime,
		seed,
		clock,
		personhood,
		registry,
		spawner,
		config,
		scan,
		running,
		failed_check: Mutex::new(None),
		checked_version: Mutex::new(facts.version),
		streams: streams::Wakes::default(),
	}));
	layer.register_installation().await?;
	Ok(layer)
}

/// Step 3 of the start sequence: deletes every uncommitted disclosure mark
/// and records the export it belonged to as released.
fn release_uncommitted(tx: &rusqlite::Transaction<'_>) -> Result<(), DbError> {
	let uncommitted = ledger::uncommitted(tx)?;
	if uncommitted.is_empty() {
		return Ok(());
	}
	for export in operation::all(tx)?.iter().filter(|operation| operation.kind == OpKind::Export) {
		if export.exported.iter().any(|(coin, _)| uncommitted.contains(coin)) {
			operation::record_release(tx, &export.key)?;
		}
	}
	ledger::release_disclosed(tx, &uncommitted)
}

/// A store that fails to decode is refused; any other failure is the
/// store's.
fn undecodable(error: DbError) -> StartError {
	match error {
		DbError::Sqlite(rusqlite::Error::FromSqlConversionFailure(..)) => StartError::UndecodableStore,
		other => StartError::Store(other),
	}
}

impl Layer {
	// ---- Purses ----

	/// Creates a product purse (B2).
	pub async fn create_purse(&self, name: String, tag: Vec<u8>) -> Result<PurseId, Error> {
		let now = self.now();
		let purse = self.0.store.write(move |tx| ledger::create_purse(tx, name, tag, now)).await?;
		self.changed();
		Ok(purse)
	}

	/// A purse that is not closed, with its balance.
	pub async fn query_purse(&self, purse: PurseId) -> Result<PurseInfo, Error> {
		let view = self.view().await?;
		let record = view.purse(purse).filter(|record| !matches!(record.state, PurseState::Closed { .. }));
		let record = record.ok_or(Error::PurseNotFound(purse))?;
		Ok(PurseInfo {
			id: record.id,
			name: record.name.clone(),
			created: record.created,
			tag: record.tag.clone(),
			balance: balance(&view, purse, self.now(), &self.params()),
		})
	}

	/// Every purse that is not closed.
	pub async fn list_purses(&self) -> Result<Vec<PurseInfo>, Error> {
		let view = self.view().await?;
		let (now, params) = (self.now(), self.params());
		Ok(view
			.purses()
			.filter(|record| !matches!(record.state, PurseState::Closed { .. }))
			.map(|record| PurseInfo {
				id: record.id,
				name: record.name.clone(),
				created: record.created,
				tag: record.tag.clone(),
				balance: balance(&view, record.id, now, &params),
			})
			.collect())
	}

	/// The open purse a purse's value goes to, following deletions (B2).
	pub async fn resolve_purse(&self, purse: PurseId) -> Result<PurseId, Error> {
		let view = self.view().await?;
		view.purse(purse).ok_or(Error::PurseNotFound(purse))?;
		Ok(view.resolve_purse(purse))
	}

	/// Moves `amount` from `from` to `to`.
	pub async fn rebalance(&self, op: Op, from: PurseId, to: PurseId, amount: u64) -> Result<Started, Error> {
		let view = self.view().await?;
		source(&view, from)?;
		destination(&view, to)?;
		if amount == 0 {
			return Err(Error::EmptyRequest);
		}
		let request = Request::Select { from, amount, to: Destination::Purse(to), initiator: None, confirmed: false };
		let arguments = (from, to, amount).encode();
		Ok(self.launch(&op, OpKind::Rebalance, arguments, request).await?.0)
	}

	/// Deletes `purse`, draining its value into `drain_into`; the purse
	/// closes once the chain confirms its value has left (invariant 4).
	pub async fn delete_purse(&self, op: Op, purse: PurseId, drain_into: PurseId) -> Result<Started, Error> {
		let state = self.accepting(&op.key)?;
		let view = self.view().await?;
		if purse == PurseId::MAIN {
			return Err(Error::MainPurse);
		}
		match view.purse(purse).map(|record| record.state) {
			Some(PurseState::Open) => {},
			Some(PurseState::Deleting { .. }) if view.operation(&op.key).is_some() => {},
			Some(PurseState::Deleting { .. }) => return Err(Error::DeletionOpen(purse)),
			_ => return Err(Error::PurseNotFound(purse)),
		}
		if drain_into == purse {
			return Err(Error::PurseNotFound(drain_into));
		}
		destination(&view, drain_into)?;
		if holds_unmovable_value(&view, purse, &op.key, self.now(), &state.params) {
			return Err(Error::PurseHoldsUnmovableValue);
		}
		let key = op.key.clone();
		let arguments = DeleteArguments { purse, drain_into }.encode();
		let begun = self
			.0
			.store
			.write_unless_refused(move |tx| {
				match operation::begin(tx, &key, OpKind::Delete, &arguments)? {
					Ok(Begun::New) => {},
					Ok(Begun::Existing) => return Ok(Ok(Begun::Existing)),
					Err(_) => return Ok(Err(Error::KeyReused(key))),
				}
				Ok(ledger::begin_delete(tx, purse, drain_into)?.map(|()| Begun::New).map_err(|error| match error {
					LedgerError::DeletionOpen => Error::DeletionOpen(purse),
					LedgerError::MainPurse => Error::MainPurse,
					_ => Error::PurseNotFound(drain_into),
				}))
			})
			.await??;
		if begun == Begun::Existing {
			return self.joined(&op.key).await;
		}
		self.drive_deletion(&op.key, purse, drain_into, op.retry).await?;
		self.changed();
		Ok(Started::New)
	}

	// ---- Value in and out of coinage ----

	/// Loads `amount` cents into entries of `into`, paid by `funding`
	/// (invariant 6).
	pub async fn top_up(&self, op: Op, into: PurseId, amount: u64, funding: Arc<dyn Funding>) -> Result<Started, Error> {
		let view = self.view().await?;
		destination(&view, into)?;
		if amount == 0 {
			return Err(Error::EmptyRequest);
		}
		let from = funding.account();
		let key = op.key.clone();
		self.0.running.update(|state| {
			state.funding.insert(key, funding);
		});
		let request = Request::Given(PlanInput::Load { from, amount, into });
		let started = self.launch(&op, OpKind::TopUp, (into, amount, from).encode(), request).await;
		if !matches!(started, Ok((Started::New | Started::Joined, _))) {
			let key = op.key.clone();
			self.0.running.update(|state| {
				state.funding.remove(&key);
			});
		}
		Ok(started?.0)
	}

	/// Moves `amount` out of `from` to the external account `to`, in F3's
	/// phases.
	pub async fn offload(
		&self,
		op: Op,
		from: PurseId,
		amount: u64,
		to: [u8; 32],
		initiator: Initiator,
		confirmed: bool,
	) -> Result<Started, Error> {
		let state = self.accepting(&op.key)?;
		let view = self.view().await?;
		source(&view, from)?;
		if amount == 0 {
			return Err(Error::EmptyRequest);
		}
		let spend = Request::Select { from, amount, to: Destination::External(to), initiator: Some(initiator), confirmed };
		if view.operation(&op.key).is_none() && needs_confirmation(&view, &spend, self.now(), &state.params) {
			return Err(Error::NeedsConfirmation);
		}
		let offload = Offload { from, amount, to, initiator, confirmed };
		if let Some(started) = self.begin_own_step(&op, OpKind::Offload, (offload, op.retry).encode()).await? {
			return Ok(started);
		}
		let entropy = self.entropy()?;
		let launcher = self.launcher(&state.params, &entropy);
		offload::advance(&launcher, &self.0.store, &op.key, &offload, op.retry, self.now()).await?;
		self.changed();
		Ok(Started::New)
	}

	// ---- Payments ----

	/// Hands coins worth `amount` from `from` to a payee: the coins and
	/// their secrets at once, with uncommitted disclosure marks (invariant
	/// 3).
	pub async fn export(
		&self,
		op: Op,
		from: PurseId,
		amount: u64,
		initiator: Initiator,
		confirmed: bool,
	) -> Result<Export, Error> {
		let state = self.accepting(&op.key)?;
		let view = self.view().await?;
		source(&view, from)?;
		if amount == 0 {
			return Err(Error::EmptyRequest);
		}
		let request = Request::Select { from, amount, to: Destination::Export(from), initiator: Some(initiator), confirmed };
		if view.operation(&op.key).is_none() && needs_confirmation(&view, &request, self.now(), &state.params) {
			return Err(Error::NeedsConfirmation);
		}
		let arguments = (from, amount, initiator, confirmed).encode();
		let (started, coins) = self.launch(&op, OpKind::Export, arguments, request).await?;
		if started == Started::New {
			return Ok(Export { started, coins });
		}
		let view = self.view().await?;
		let exported = view.operation(&op.key).map(|export| export.exported.clone()).unwrap_or_default();
		Ok(Export { started, coins: exported_coins(&self.entropy()?, &exported) })
	}

	/// Commits an export's disclosure marks in a store transaction of its
	/// own; [`commit_export_in`] commits them inside the caller's.
	pub async fn commit_export(&self, export: OpKey) -> Result<(), Error> {
		self.0.store.write_unless_refused(move |tx| commit_export_in(tx, &export)).await??;
		self.changed();
		Ok(())
	}

	/// Gives up an export that was not committed: its marks are deleted,
	/// its minting given up, and it ends `Failed(NotCommitted)`.
	pub async fn release_export(&self, export: OpKey) -> Result<(), Error> {
		self.0
			.store
			.write_unless_refused(move |tx| {
				let record = export_record(tx, &export)?;
				let Ok(record) = record else { return Ok(Err(record.unwrap_err())) };
				let coins: Vec<Asset> = record.exported.iter().map(|(coin, _)| *coin).collect();
				if coins.iter().any(|coin| matches!(ledger::mark(tx, coin), Ok(Some(Mark::Committed)))) {
					return Ok(Err(Error::ExportClosed(export)));
				}
				ledger::release_disclosed(tx, &coins)?;
				operation::record_release(tx, &export)?;
				Ok(Ok(()))
			})
			.await??;
		self.changed();
		Ok(())
	}

	/// Takes back the coins of a committed export into fresh accounts of
	/// `into`.
	pub async fn reclaim(&self, op: Op, export: OpKey, into: PurseId) -> Result<Started, Error> {
		let view = self.view().await?;
		destination(&view, into)?;
		let record = view.operation(&export).ok_or_else(|| Error::UnknownOperation(export.clone()))?;
		if record.kind != OpKind::Export {
			return Err(Error::NotAnExport(export));
		}
		let coins: Vec<Asset> = record.exported.iter().map(|(coin, _)| *coin).collect();
		if record.released || !coins.iter().all(|coin| view.mark(coin) == Some(Mark::Committed)) {
			return Err(Error::ExportNotCommitted(export));
		}
		let arguments = ReclaimArguments { export, into }.encode();
		let request = Request::Given(PlanInput::Reclaim { coins, into });
		Ok(self.launch(&op, OpKind::Reclaim, arguments, request).await?.0)
	}

	/// Claims each coin into a fresh account of `into` (invariant 5).
	pub async fn import(&self, op: Op, into: PurseId, coins: Vec<CoinSecret>) -> Result<Started, Error> {
		let view = self.view().await?;
		destination(&view, into)?;
		let mut accounts = Vec::with_capacity(coins.len());
		for (position, secret) in coins.iter().enumerate() {
			let position = u32::try_from(position).unwrap_or(u32::MAX);
			accounts.push(account_of(secret).ok_or(Error::InvalidSecret(position))?);
		}
		if accounts.is_empty() {
			return Err(Error::EmptyRequest);
		}
		observation::observe_peer_coins(&self.observer(), &accounts, self.now(), &self.params())
			.await
			.map_err(observe_error)?;
		let arguments = ImportArguments { into, accounts }.encode();
		let request = Request::Given(PlanInput::Import { coins, into });
		Ok(self.launch(&op, OpKind::Import, arguments, request).await?.0)
	}

	/// Whether a claim that has not reached a stable verdict holds one of
	/// `coins`, so that another import of it could not start.
	pub async fn importing(&self, coins: &[CoinSecret]) -> Result<bool, Error> {
		let view = self.view().await?;
		Ok(coins.iter().filter_map(account_of).any(|account| {
			view.registrations().consumers(&Asset::PeerCoin(account)).any(|claim| {
				!matches!(
					claim.state.status,
					crate::durable::DurableTxStatus::FinalizedSuccess { .. } | crate::durable::DurableTxStatus::Failure(_)
				)
			})
		}))
	}

	// ---- Maintenance and recovery ----

	/// Runs whatever maintenance is due at `now` (F2), advances every open
	/// deletion, and ends every cancelled offload that has nothing in flight.
	pub async fn run_due(&self, now: u64) -> Result<Due, Error> {
		let Some(state) = self.0.running.state().filter(|state| !state.check_failed) else {
			return Ok(Due { cannot_tell: Vec::new(), started: Vec::new(), call_again_within: Some(Duration::ZERO) });
		};
		let entropy = self.entropy()?;
		let launcher = self.launcher(&state.params, &entropy);
		let due = maintenance::run_due(&launcher, &self.0.store, now).await?;
		let view = self.view().await?;
		for deletion in view.operations().filter(|operation| operation.kind == OpKind::Delete) {
			let Some(DeleteArguments { purse, drain_into }) = deletion.arguments_as() else { continue };
			if matches!(view.purse(purse).map(|record| record.state), Some(PurseState::Deleting { .. })) {
				self.drive_deletion(&deletion.key, purse, drain_into, Retry::ExpiryOnly).await?;
			}
		}
		for cancelled in view.operations().filter(|operation| ending_cancelled_offload(&view, operation)) {
			let key = cancelled.key.clone();
			self.0.store.write(move |tx| operation::record_end(tx, &key, OperationEnd::Cancelled)).await?;
		}
		self.drop_ended_funding(&view);
		self.changed();
		Ok(due)
	}

	/// Drops the funding capability of every top-up that has ended, so that
	/// a key a product handed over is held no longer than its top-up needs
	/// it (invariant 6).
	fn drop_ended_funding(&self, view: &Snapshot) {
		let (now, params) = (self.now(), self.params());
		self.0.running.update(|state| {
			state.funding.retain(|key, _| !status(view, key, now, &params).is_some_and(|item| item.status.is_terminal()));
		});
	}

	/// Starts a reconstruction of `purses` (F4); its scan runs in the
	/// background, and the operation is `Done` once it ends.
	pub async fn reconstruct(&self, op: Op, purses: Purses, deep: bool) -> Result<Started, Error> {
		let state = self.accepting(&op.key)?;
		if let Some(started) = self.begin_own_step(&op, OpKind::Reconstruct, (purses.clone(), deep).encode()).await? {
			return Ok(started);
		}
		let deps = ReconstructionDeps {
			store: self.0.store.clone(),
			seed: self.0.seed.clone(),
			chain: self.0.chain.clone(),
			registry: self.0.registry.clone(),
		};
		let (scan, layer, key) = (self.0.scan, self.clone(), op.key);
		self.0.spawner.spawn(async move {
			if reconstruction::reconstruct(&deps, purses, deep, &scan, &state.params).await.is_ok() {
				let ended = layer.0.store.write(move |tx| operation::record_end(tx, &key, OperationEnd::Done)).await;
				if ended.is_ok() {
					layer.changed();
				}
			}
		});
		Ok(Started::New)
	}

	// ---- Policy, backup, parameters ----

	/// Stores `policy` exactly as given, once A4's check accepts it.
	pub async fn set_policy(&self, policy: PrivacyPolicy) -> Result<Result<(), PolicyError>, Error> {
		if let Err(refused) = check(&policy) {
			return Ok(Err(refused));
		}
		self.0.store.write(move |tx| ledger::set_policy(tx, &policy)).await?;
		self.changed();
		Ok(Ok(()))
	}

	/// B2's purse backup: `SCALE(Vec<PurseRecord>)` of every purse ever
	/// created.
	pub async fn purse_backup(&self) -> Result<Vec<u8>, Error> {
		Ok(self.0.store.read(ledger::purse_backup).await?)
	}

	/// The external asset's amount in one cent.
	pub fn asset_unit(&self) -> u128 {
		self.params().instance().asset_unit
	}

	/// Whether the current installation's registration has succeeded (G3
	/// invariant 4).
	pub async fn recoverable(&self) -> Result<Recoverability, Error> {
		Ok(recoverability(&self.view().await?, self.now(), &self.params()))
	}

	// ---- Following an operation ----

	/// The operation's completion once it has ended.
	pub async fn completion(&self, key: OpKey) -> Result<Option<Completion>, Error> {
		let view = self.view().await?;
		Ok(completion(&view, &key, self.now(), &self.params(), &self.entropy()?))
	}

	/// Requests the cancellation of an operation (B4 invariant 1).
	pub async fn cancel(&self, key: OpKey) -> Result<(), Error> {
		let requested = key.clone();
		self.0.store.write(move |tx| operation::request_cancel(tx, &requested)).await?.map_err(|error| match error {
			OperationError::NotCancellable => Error::NotCancellable,
			OperationError::UnknownOperation | OperationError::KeyReused => Error::UnknownOperation(key),
		})?;
		self.changed();
		Ok(())
	}

	// ---- Runtime and blocks ----

	/// Re-checks the runtime at `at`, after F1 observed a new runtime
	/// version there. While the check fails, no operation starts and
	/// nothing is built; the parameters of the last check that passed stay
	/// in use for reads (G3 invariant 7).
	pub async fn revalidate(&self, at: HashAndNumber) {
		let checked = match self.0.runtime.runtime(Some(at)).await {
			Some(facts) => {
				*self.0.checked_version.lock() = facts.version;
				validate(&facts.metadata, facts.instance.as_ref(), facts.maximum_age, self.0.config.clone())
					.map_err(|unsupported| unsupported.check)
			},
			None => Err("the runtime could not be read".to_owned()),
		};
		match checked {
			Ok(params) => {
				self.0.running.update(|state| {
					state.params = params;
					state.check_failed = false;
				});
				*self.0.failed_check.lock() = None;
			},
			Err(check) => {
				self.0.running.update(|state| state.check_failed = true);
				*self.0.failed_check.lock() = Some(check);
			},
		}
	}

	/// Observes `block` (F1), checks the runtime again when its version at
	/// `block` is not the version of the latest check (G3 invariant 1), and
	/// tells the streams; the host's entry point may run maintenance on it.
	pub async fn on_block(&self, block: HashAndNumber) -> Result<(), Error> {
		observation::on_block(&self.observer(), block, self.now(), &self.params()).await.map_err(observe_error)?;
		let version = self.0.runtime.version(block).await;
		if version.is_some_and(|version| version != *self.0.checked_version.lock()) {
			self.revalidate(block).await;
		}
		self.0.streams.new_block(block);
		self.changed();
		Ok(())
	}

	// ---- Internals ----

	fn observer(&self) -> ObserverDeps {
		ObserverDeps {
			store: self.0.store.clone(),
			seed: self.0.seed.clone(),
			chain: self.0.chain.clone(),
			personhood: self.0.personhood.clone(),
		}
	}

	fn now(&self) -> u64 {
		(self.0.clock)()
	}

	/// The parameters of the last runtime check that passed.
	fn params(&self) -> ChainParams {
		self.0.running.state().expect("start opens the running handle before returning the layer").params
	}

	fn entropy(&self) -> Result<Vec<u8>, Error> {
		self.0.seed.entropy().ok_or(Error::Unavailable)
	}

	fn launcher<'a>(&'a self, params: &'a ChainParams, entropy: &'a [u8]) -> Launcher<'a> {
		Launcher { engine: &self.0.engine, params, entropy }
	}

	async fn view(&self) -> Result<Snapshot, Error> {
		Ok(self.0.store.read(ledger::snapshot).await?)
	}

	/// The running state, when an operation under `key` may start: the
	/// caller's scope is not the layer's own, and the runtime check passes.
	fn accepting(&self, key: &OpKey) -> Result<RunningState, Error> {
		if key.is_own() {
			return Err(Error::ReservedScope);
		}
		let state = self.0.running.state().ok_or(Error::Unavailable)?;
		if state.check_failed {
			let check = self.0.failed_check.lock().clone().unwrap_or_default();
			return Err(Error::UnsupportedRuntime { check });
		}
		Ok(state)
	}

	/// Records `op` and registers its records in one store transaction, or
	/// joins the operation its key names.
	async fn launch(
		&self,
		op: &Op,
		kind: OpKind,
		arguments: Vec<u8>,
		request: Request,
	) -> Result<(Started, Vec<ExportedCoin>), Error> {
		let state = self.accepting(&op.key)?;
		let entropy = self.entropy()?;
		let launcher = self.launcher(&state.params, &entropy);
		let begin = Begin { kind, arguments };
		let launched = begin_and_start(&launcher, &op.key, begin, op.retry, request, self.now()).await;
		let started = match launched {
			Ok(Launched::New(registered)) => (Started::New, registered.exported),
			Ok(Launched::Existing) => (self.joined(&op.key).await?, Vec::new()),
			Err(error) => return Err(launch_error(error, &op.key)),
		};
		self.changed();
		Ok(started)
	}

	/// Records an operation whose steps run apart from its start, such as
	/// an offload or a reconstruction; none when the key was new.
	async fn begin_own_step(&self, op: &Op, kind: OpKind, arguments: Vec<u8>) -> Result<Option<Started>, Error> {
		let key = op.key.clone();
		let begun = self.0.store.write(move |tx| operation::begin(tx, &key, kind, &arguments)).await?;
		match begun {
			Ok(Begun::New) => Ok(None),
			Ok(Begun::Existing) => Ok(Some(self.joined(&op.key).await?)),
			Err(_) => Err(Error::KeyReused(op.key.clone())),
		}
	}

	/// `Joined` while the operation is open, `Finished` once it has ended.
	async fn joined(&self, key: &OpKey) -> Result<Started, Error> {
		let view = self.view().await?;
		Ok(match completion(&view, key, self.now(), &self.params(), &self.entropy()?) {
			Some(completion) => Started::Finished(completion),
			None => Started::Joined,
		})
	}

	/// One step of a deletion: closes the purse once nothing in it remains
	/// and none of its transactions is in flight, or drains what it holds
	/// once those transactions are stable.
	async fn drive_deletion(&self, key: &OpKey, purse: PurseId, drain_into: PurseId, retry: Retry) -> Result<(), Error> {
		let view = self.view().await?;
		let params = self.params();
		let now = self.now();
		let in_flight = status(&view, key, now, &params).is_some_and(|item| {
			matches!(item.status, Status::Preparing | Status::Submitted | Status::Settling) ||
				view.registrations().of_operation(key).any(|facts| facts.state.status.awaits_verdict())
		});
		if in_flight {
			return Ok(());
		}
		let held = balance(&view, purse, now, &params);
		if held.spendable + held.held_back + held.arriving == 0 {
			self.0.store.write(move |tx| ledger::close_purse(tx, purse)).await?;
			return Ok(());
		}
		let request = SelectRequest { purse, want: Want::Everything, forms: Forms::Any, entries_first: false, held_back: true };
		let Ok(selection) = select(&view, request, now, &params) else { return Ok(()) };
		let entropy = self.entropy()?;
		let launcher = self.launcher(&params, &entropy);
		let drain = Request::Given(PlanInput::Spend { selection, to: Destination::Purse(drain_into) });
		match super::start::start(&launcher, key, retry, drain, now).await {
			Ok(_) | Err(LaunchError::Nothing | LaunchError::Refused(_) | LaunchError::NoUnloadToken) => Ok(()),
			Err(error) => Err(launch_error(error, key)),
		}
	}

	/// Step 4 of the start sequence: starts the current installation's
	/// registration, under the layer's own scope, unless one succeeded or
	/// is open.
	async fn register_installation(&self) -> Result<(), StartError> {
		let view = self.0.store.read(ledger::snapshot).await.map_err(undecodable)?;
		let Some(installation) = view.current_installation() else { return Ok(()) };
		let (now, params) = (self.now(), self.params());
		let registered_or_open = view.operations().filter(|operation| operation.kind == OpKind::RegisterInstallation).any(
			|operation| {
				operation.arguments == installation.encode() &&
					status(&view, &operation.key, now, &params).is_some_and(|item| !matches!(item.status, Status::Failed(_)))
			},
		);
		if registered_or_open {
			return Ok(());
		}
		let entropy = self.0.seed.entropy().ok_or(StartError::Unavailable)?;
		let key = OpKey::own(self.0.store.write(ledger::next_operation_id).await?);
		let launcher = self.launcher(&params, &entropy);
		let begin = Begin { kind: OpKind::RegisterInstallation, arguments: installation.encode() };
		let request = Request::Given(PlanInput::RegisterInstallation(installation));
		match begin_and_start(&launcher, &key, begin, Retry::ExpiryOnly, request, now).await {
			Err(LaunchError::Db(error)) => Err(StartError::Store(error)),
			_ => Ok(()),
		}
	}

	/// Tells the streams that something they read may have changed.
	fn changed(&self) {
		self.0.streams.wake();
	}
}

/// Commits an export's disclosure marks inside the caller's store
/// transaction `tx`, so they become durable together with the caller's
/// record of the secrets (invariant 3). Committing again succeeds.
pub fn commit_export_in(tx: &rusqlite::Transaction<'_>, export: &OpKey) -> Result<Result<(), Error>, DbError> {
	let record = match export_record(tx, export)? {
		Ok(record) => record,
		Err(error) => return Ok(Err(error)),
	};
	let coins: Vec<Asset> = record.exported.iter().map(|(coin, _)| *coin).collect();
	let marks = coins.iter().map(|coin| ledger::mark(tx, coin)).collect::<Result<Vec<_>, _>>()?;
	if record.released || marks.iter().any(Option::is_none) {
		return Ok(Err(Error::ExportClosed(export.clone())));
	}
	ledger::commit_disclosed(tx, &coins)?;
	Ok(Ok(()))
}

/// The export `key` names, or why it is not one.
fn export_record(conn: &rusqlite::Connection, key: &OpKey) -> Result<Result<OperationRecord, Error>, DbError> {
	let record = operation::all(conn)?.into_iter().find(|operation| &operation.key == key);
	Ok(match record {
		None => Err(Error::UnknownOperation(key.clone())),
		Some(record) if record.kind != OpKind::Export => Err(Error::NotAnExport(key.clone())),
		Some(record) if record.released => Err(Error::ExportClosed(key.clone())),
		Some(record) => Ok(record),
	})
}

/// A purse value may leave: open, or being deleted.
fn source(view: &Snapshot, purse: PurseId) -> Result<(), Error> {
	match view.purse(purse).map(|record| record.state) {
		Some(PurseState::Open) => Ok(()),
		Some(PurseState::Deleting { .. }) => Err(Error::DeletionOpen(purse)),
		_ => Err(Error::PurseNotFound(purse)),
	}
}

/// A purse value may arrive in: open, with no deletion open.
fn destination(view: &Snapshot, purse: PurseId) -> Result<(), Error> {
	match view.purse(purse).map(|record| record.state) {
		Some(PurseState::Open) => Ok(()),
		_ => Err(Error::PurseNotFound(purse)),
	}
}

/// Whether another operation holds value of `purse`, which a drain could
/// then not take.
fn holds_unmovable_value(view: &Snapshot, purse: PurseId, deletion: &OpKey, now: u64, params: &ChainParams) -> bool {
	view.assets(purse).any(|record| {
		let holder = match classify(&record.asset, view, now, params).class {
			Class::Coin(CoinClass::InUse(holder)) | Class::Entry { local: EntryLocal::InUse(holder), .. } => holder,
			_ => return false,
		};
		&holder != deletion
	})
}

/// Whether `operation` is an offload whose cancellation was requested and
/// none of whose records is in flight.
fn ending_cancelled_offload(view: &Snapshot, operation: &OperationRecord) -> bool {
	operation.kind == OpKind::Offload &&
		operation.cancel_requested &&
		operation.end.is_none() &&
		!view.registrations().of_operation(&operation.key).any(|facts| {
			!matches!(
				facts.state.status,
				crate::durable::DurableTxStatus::FinalizedSuccess { .. } | crate::durable::DurableTxStatus::Failure(_)
			)
		})
}

/// Whether the current installation's registration has succeeded.
fn recoverability(view: &Snapshot, now: u64, params: &ChainParams) -> Recoverability {
	let Some(installation) = view.current_installation() else { return Recoverability::Pending { since: now } };
	let registered = view.operations().any(|operation| {
		operation.kind == OpKind::RegisterInstallation &&
			operation.arguments == installation.encode() &&
			status(view, &operation.key, now, params).is_some_and(|item| item.status == Status::Done)
	});
	if registered {
		return Recoverability::Registered;
	}
	let since = view.purse(PurseId::MAIN).map_or(now, |main| main.created);
	Recoverability::Pending { since }
}

fn observe_error(error: observation::ObserveError) -> Error {
	match error {
		observation::ObserveError::Store(error) => Error::Store(error),
		_ => Error::Unavailable,
	}
}

fn launch_error(error: LaunchError, key: &OpKey) -> Error {
	match error {
		LaunchError::InsufficientFunds { available } => Error::InsufficientFunds { available },
		LaunchError::NoUnloadToken => Error::NoUnloadToken,
		LaunchError::Refused(RegisterError::InputTaken(_)) | LaunchError::Refused(_) => Error::InputTaken,
		LaunchError::Nothing => Error::EmptyRequest,
		LaunchError::NotServed => Error::Unavailable,
		LaunchError::KeyReused => Error::KeyReused(key.clone()),
		LaunchError::Db(error) => Error::Store(error),
	}
}

#[cfg(test)]
mod tests;
