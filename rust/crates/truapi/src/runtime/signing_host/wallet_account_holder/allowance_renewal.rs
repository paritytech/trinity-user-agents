//! Ledger and driver for automatic statement-store allowance renewal.
//!
//! The ledger records which accounts this signing host promised to keep
//! allowed, as derivation recipes where possible so entries stay valid when
//! the host rotates to a new root entropy. The driver resolves them against
//! the active session and runs the chain-pure pass in
//! `statement_allowance::renewal`, either once (`renew_now`) or on a periodic
//! tick (`start_renewal_loop`).

#[cfg(not(target_arch = "wasm32"))]
use std::{
	sync::{
		Arc,
		atomic::{AtomicBool, Ordering},
	},
	time::Duration,
};

use crate::{
	host_logic::product_account::{ProductAccountError, derive_root_keypair_from_entropy},
	platform::{CoreStorage, CoreStorageKey, ProductId},
	runtime::{
		authority::AuthorityError, statement_allowance::StatementAllowanceError,
		statement_store_rpc::StatementStoreRpcClientError,
	},
};
use futures::lock::Mutex;
use parity_scale_codec::{Decode, Encode};
use tracing::warn;
#[cfg(not(target_arch = "wasm32"))]
use tracing::{debug, info};

use super::{SigningHost, allowance::AllowanceAllocationError};
#[cfg(not(target_arch = "wasm32"))]
use crate::{
	host_logic::product_account::{derive_identity_keypair, derive_sr25519_hard_path},
	runtime::{
		RuntimeServices,
		authority::ProductAuthority,
		signing_host::allowance::current_unix_secs,
		statement_allowance::{
			self, fetch_metadata, find_including_rings,
			renewal::{
				RenewalChainContext, ResolvedRenewalTarget, StatementRenewalReport,
				next_tick_delay, renew_targets,
			},
		},
	},
};
use truapi::latest::GenericError;

/// Failure reading renewal targets or preparing a statement-store renewal pass.
#[derive(Debug, thiserror::Error)]
pub enum StatementRenewalError {
	/// The active authority could not supply the renewal identity.
	#[error(transparent)]
	Authority(#[from] AuthorityError),
	/// A tracked account could not be derived.
	#[error(transparent)]
	ProductAccount(#[from] ProductAccountError),
	/// The clock could not supply the current allowance period.
	#[error(transparent)]
	Clock(#[from] AllowanceAllocationError),
	/// The chain could not be opened for a renewal pass.
	#[error(transparent)]
	RpcClient(#[from] StatementStoreRpcClientError),
	/// Chain state or allowance preparation failed.
	#[error(transparent)]
	Allowance(#[from] StatementAllowanceError),
	/// The persisted targets could not be read.
	#[error("renewal ledger read failed: {0}")]
	ReadLedger(#[source] GenericError),
	/// The persisted targets could not be written.
	#[error("renewal ledger write failed: {0}")]
	WriteLedger(#[source] GenericError),
	/// Persisted targets are not valid SCALE values.
	#[error("invalid persisted renewal targets: {0}")]
	DecodeLedger(#[from] parity_scale_codec::Error),
	/// Bytes after a complete target list make the ledger invalid.
	#[error("invalid persisted renewal targets: trailing bytes")]
	TrailingLedgerBytes,
	/// The session ended before ring membership could be resolved.
	#[error("no active session for statement-store renewal")]
	NoActiveSession,
	/// Renewal requires a personhood membership for its allowance proof.
	#[error(
		"signing account is not a member of any personhood ring; cannot renew statement-store allowances"
	)]
	MissingPersonhoodMembership,
}

impl From<StatementRenewalError> for GenericError {
	fn from(error: StatementRenewalError) -> Self {
		Self { reason: error.to_string() }
	}
}

/// A statement account or derivation recipe the wallet keeps renewed.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum StatementRenewalTarget {
	/// `//allowance//statement-store//{product_id}` from the active root entropy.
	ProductStatementAllowance {
		/// Product the allowance account belongs to.
		product_id: ProductId,
	},
	/// `//wallet//sso` from the active root entropy.
	WalletSso,
	/// A fixed account, e.g. a pairing peer's device statement key.
	Account {
		/// Account to keep allowed.
		account_id: [u8; 32],
		/// Human-readable name used in logs and reports.
		label: String,
	},
}

/// Renewal recipes follow the active wallet; fixed accounts belong to their recorded owner.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct TrackedStatementRenewalTarget {
	/// The account, or the recipe for one, that the host promised to renew.
	pub target: StatementRenewalTarget,
	/// Root public key that promised a raw account id. A recipe carries none
	/// and resolves under whichever identity is active.
	pub owner: Option<[u8; 32]>,
}

impl TrackedStatementRenewalTarget {
	/// Record `target` under `owner`, which only raw account ids retain.
	fn new(target: StatementRenewalTarget, owner: [u8; 32]) -> Self {
		let owner = match &target {
			StatementRenewalTarget::Account { .. } => Some(owner),
			StatementRenewalTarget::ProductStatementAllowance { .. } |
			StatementRenewalTarget::WalletSso => None,
		};
		Self { target, owner }
	}
}

/// Root public key of the identity rooted at `entropy`, used to own raw ledger
/// entries.
fn owner_key(entropy: &[u8]) -> Result<[u8; 32], StatementRenewalError> {
	Ok(derive_root_keypair_from_entropy(entropy)?.public.to_bytes())
}

/// Renewal coordination state owned by [`SigningHost`].
#[derive(Default)]
pub struct RenewalState {
	/// Serializes slot registrations between the renewal pass and on-demand
	/// allocation so both cannot race for the same free slot.
	registration_lock: Mutex<()>,
	/// Serializes read-modify-write cycles on the ledger so a concurrent
	/// allocation cannot drop another's entry.
	ledger_lock: Mutex<()>,
	#[cfg(not(target_arch = "wasm32"))]
	driver: DriverState,
}

impl RenewalState {
	/// Serialize registration scans with competing issuers.
	pub fn registration_lock(&self) -> &Mutex<()> {
		&self.registration_lock
	}

	fn ledger_lock(&self) -> &Mutex<()> {
		&self.ledger_lock
	}
}

/// Discard unreadable entries; later allocations rebuild the ledger.
async fn read_entries(
	storage: &(impl CoreStorage + ?Sized),
) -> Result<Vec<TrackedStatementRenewalTarget>, StatementRenewalError> {
	let Some(blob) = storage
		.read_core_storage(CoreStorageKey::StatementRenewalTargets)
		.await
		.map_err(StatementRenewalError::ReadLedger)?
	else {
		return Ok(Vec::new());
	};
	match decode_entries(&blob) {
		Ok(entries) => Ok(entries),
		Err(reason) => {
			warn!(%reason, "discarding an undecodable renewal ledger");
			Ok(Vec::new())
		},
	}
}

/// Append `new_targets` to the ledger, preserving order and skipping entries
/// already present.
async fn track_targets(
	storage: &(impl CoreStorage + ?Sized),
	ledger_lock: &Mutex<()>,
	owner: [u8; 32],
	new_targets: Vec<StatementRenewalTarget>,
) -> Result<(), StatementRenewalError> {
	let _guard = ledger_lock.lock().await;
	let mut entries = read_entries(storage).await?;
	let mut changed = false;
	for target in new_targets {
		let entry = TrackedStatementRenewalTarget::new(target, owner);
		if !entries.contains(&entry) {
			entries.push(entry);
			changed = true;
		}
	}
	if !changed {
		return Ok(());
	}
	write_entries(storage, &entries).await
}

async fn write_entries(
	storage: &(impl CoreStorage + ?Sized),
	entries: &[TrackedStatementRenewalTarget],
) -> Result<(), StatementRenewalError> {
	storage
		.write_core_storage(CoreStorageKey::StatementRenewalTargets, entries.encode())
		.await
		.map_err(StatementRenewalError::WriteLedger)
}

fn decode_entries(
	blob: &[u8],
) -> Result<Vec<TrackedStatementRenewalTarget>, StatementRenewalError> {
	let mut input = blob;
	let entries = Vec::<TrackedStatementRenewalTarget>::decode(&mut input)?;
	if !input.is_empty() {
		return Err(StatementRenewalError::TrailingLedgerBytes);
	}
	Ok(entries)
}

/// Record `targets` in the ledger under the active identity.
pub async fn track(
	signing_host: &SigningHost,
	targets: Vec<StatementRenewalTarget>,
) -> Result<(), StatementRenewalError> {
	let entropy = signing_host.root_entropy()?;
	track_targets(
		signing_host.platform.as_ref(),
		signing_host.renewal.ledger_lock(),
		owner_key(&entropy)?,
		targets,
	)
	.await
}

/// Fallback tick delay when the system clock is unusable.
#[cfg(not(target_arch = "wasm32"))]
const CLOCK_FAILURE_TICK_DELAY: Duration = Duration::from_secs(3_600);

/// Native renewal loop coordination and its latest completed report.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
pub struct DriverState {
	loop_started: AtomicBool,
	/// The most recent pass, so a host that drives the in-process loop can read
	/// what it achieved. The loop computes a report on every tick and has no
	/// caller to hand it to, and exhaustion is the outcome a host most needs to
	/// act on. A blocking lock rather than an async one: every use is a clone or
	/// a store with no await in between.
	last_report: std::sync::Mutex<Option<StatementRenewalReport>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl TrackedStatementRenewalTarget {
	/// Whether this entry belongs to the identity rooted at `owner`.
	fn is_owned_by(&self, owner: [u8; 32]) -> bool {
		self.owner.is_none_or(|recorded| recorded == owner)
	}
}

#[cfg(not(target_arch = "wasm32"))]
impl RenewalState {
	/// Record the pass a host has not seen the return value of.
	fn record_report(&self, report: &StatementRenewalReport) {
		if let Ok(mut last) = self.driver.last_report.lock() {
			*last = Some(report.clone());
		}
	}

	/// The most recent pass the loop ran. `None` until one has run, which a host
	/// should read as "not yet" rather than as healthy.
	pub fn last_report(&self) -> Option<StatementRenewalReport> {
		self.driver.last_report.lock().ok().and_then(|last| last.clone())
	}
}

/// Read a settled ledger while competing writes hold the same lock.
#[cfg(not(target_arch = "wasm32"))]
async fn list_entries(
	storage: &(impl CoreStorage + ?Sized),
	ledger_lock: &Mutex<()>,
) -> Result<Vec<TrackedStatementRenewalTarget>, StatementRenewalError> {
	let _guard = ledger_lock.lock().await;
	read_entries(storage).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn untrack_account(
	storage: &(impl CoreStorage + ?Sized),
	ledger_lock: &Mutex<()>,
	owner: [u8; 32],
	account_id: &[u8; 32],
) -> Result<bool, StatementRenewalError> {
	let _guard = ledger_lock.lock().await;
	let mut entries = read_entries(storage).await?;
	let original_len = entries.len();
	entries.retain(|entry| {
		entry.owner != Some(owner) ||
			!matches!(
				&entry.target,
				StatementRenewalTarget::Account {
					account_id: existing,
					..
				} if existing == account_id
			)
	});
	if entries.len() == original_len {
		return Ok(false);
	}
	write_entries(storage, &entries).await?;
	Ok(true)
}

/// Stable labels identify pruned entries even without an active wallet.
#[cfg(not(target_arch = "wasm32"))]
fn target_label(target: &StatementRenewalTarget) -> String {
	match target {
		StatementRenewalTarget::ProductStatementAllowance { product_id } => {
			format!("product:{product_id}")
		},
		StatementRenewalTarget::WalletSso => "wallet-sso".to_string(),
		StatementRenewalTarget::Account { label, .. } => label.clone(),
	}
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_target(
	entropy: &[u8],
	network_suffix: &str,
	target: &StatementRenewalTarget,
) -> Result<ResolvedRenewalTarget, StatementRenewalError> {
	let label = target_label(target);
	match target {
		StatementRenewalTarget::ProductStatementAllowance { product_id } => {
			let pair = derive_sr25519_hard_path(
				entropy,
				&["allowance", "statement-store", product_id.as_str()],
			)?;
			Ok(ResolvedRenewalTarget { label, account_id: pair.public.to_bytes() })
		},
		StatementRenewalTarget::WalletSso => {
			let pair = derive_identity_keypair(entropy, network_suffix)?;
			Ok(ResolvedRenewalTarget { label, account_id: pair.public.to_bytes() })
		},
		StatementRenewalTarget::Account { account_id, .. } => {
			Ok(ResolvedRenewalTarget { label, account_id: *account_id })
		},
	}
}

/// List tracked entries without requiring wallet unlock.
#[cfg(not(target_arch = "wasm32"))]
pub async fn list(
	signing_host: &SigningHost,
) -> Result<Vec<TrackedStatementRenewalTarget>, StatementRenewalError> {
	list_entries(signing_host.platform.as_ref(), signing_host.renewal.ledger_lock()).await
}

/// Identify which fixed ledger entries belong to the active wallet.
#[cfg(not(target_arch = "wasm32"))]
pub fn active_owner_key(signing_host: &SigningHost) -> Result<[u8; 32], StatementRenewalError> {
	let entropy = signing_host.root_entropy()?;
	owner_key(&entropy)
}

/// Stop renewing one fixed statement account for the active identity.
#[cfg(not(target_arch = "wasm32"))]
pub async fn untrack_account_for_signing_host(
	signing_host: &SigningHost,
	account_id: &[u8; 32],
) -> Result<bool, StatementRenewalError> {
	let entropy = signing_host.root_entropy()?;
	untrack_account(
		signing_host.platform.as_ref(),
		signing_host.renewal.ledger_lock(),
		owner_key(&entropy)?,
		account_id,
	)
	.await
}

/// Skip unusable entries so they cannot prevent renewal of other targets.
#[cfg(not(target_arch = "wasm32"))]
fn resolve_targets(
	entropy: &[u8],
	network_suffix: &str,
	targets: &[StatementRenewalTarget],
) -> Vec<ResolvedRenewalTarget> {
	targets
		.iter()
		.filter_map(|target| match resolve_target(entropy, network_suffix, target) {
			Ok(resolved) => Some(resolved),
			Err(reason) => {
				warn!(?target, %reason, "skipping an unresolvable renewal target");
				None
			},
		})
		.collect()
}

/// The ledger targets `owner` promised, dropping any it did not.
///
/// An entry promised by a different identity would consume this one's slots for
/// an account it never promised, so it is removed rather than skipped — the cost
/// is paid once per identity change instead of on every tick. The ledger is only
/// rewritten when something was actually dropped.
///
/// The lock covers the read as well as the write, because this is a
/// read-modify-write of the whole ledger: reading outside it lets a
/// [`track_targets`] call land in the gap and be overwritten by a view that
/// predates it, leaving that account tracked nowhere and never renewed again.
///
/// The labels are logged here rather than only returned. Every step between this
/// and the report can fail, and `run_tick` does not read the report at all, so
/// the log is the one place a prune is recorded unconditionally.
#[cfg(not(target_arch = "wasm32"))]
async fn owned_targets(
	storage: &(impl CoreStorage + ?Sized),
	ledger_lock: &Mutex<()>,
	owner: [u8; 32],
) -> Result<(Vec<StatementRenewalTarget>, Vec<String>), StatementRenewalError> {
	let _guard = ledger_lock.lock().await;
	let (owned, foreign): (Vec<_>, Vec<_>) = read_entries(storage)
		.await?
		.into_iter()
		.partition(|entry| entry.is_owned_by(owner));
	let pruned: Vec<String> = foreign.iter().map(|entry| target_label(&entry.target)).collect();
	if !foreign.is_empty() {
		warn!(
			dropped = ?pruned,
			"pruning renewal targets promised by a previous identity"
		);
		write_entries(storage, &owned).await?;
	}
	Ok((owned.into_iter().map(|entry| entry.target).collect(), pruned))
}

/// One renewal pass: resolve the ledger against the active session and renew
/// every target for the current period.
#[cfg(not(target_arch = "wasm32"))]
pub async fn renew_now(
	services: &Arc<RuntimeServices>,
	signing_host: &SigningHost,
) -> Result<StatementRenewalReport, StatementRenewalError> {
	let entropy = signing_host.root_entropy()?;
	let period = statement_allowance::slot::current_period(current_unix_secs()?);
	let (targets, pruned) = owned_targets(
		signing_host.platform.as_ref(),
		signing_host.renewal.ledger_lock(),
		owner_key(&entropy)?,
	)
	.await?;
	let resolved = resolve_targets(&entropy, signing_host.network_suffix(), &targets);
	if resolved.is_empty() {
		return Ok(StatementRenewalReport {
			period,
			outcomes: Vec::new(),
			pruned,
			slots_exhausted: false,
		});
	}

	// The same accessor on-demand allocation uses, so a change to the reserved
	// key reaches renewal too — and it revalidates the session first.
	let session = signing_host.current_session().ok_or(StatementRenewalError::NoActiveSession)?;
	let candidates = signing_host.reserved_person_collection_candidates(&session)?;
	// The pooled client, so the extrinsic's birth block and its submission
	// reach the same node.
	let client = services.statement_store.chain_client("statement-allowance renewal").await?;
	let rpc = client.rpc();
	let metadata = fetch_metadata(rpc).await?;
	let network_suffix = statement_allowance::slot::read_network_suffix(rpc).await?;
	// Every ring back to index 0, because a membership that stopped being
	// re-included still proves against the ring that holds it.
	let signer = statement_allowance::FixedPersonhoodSigner::new(&candidates).await?;
	let collections: Vec<_> = candidates.iter().map(|candidate| candidate.collection).collect();
	let memberships = find_including_rings(rpc, &metadata, &signer, &collections, u32::MAX).await?;
	if memberships.is_empty() {
		return Err(StatementRenewalError::MissingPersonhoodMembership);
	}
	let context = RenewalChainContext {
		rpc,
		metadata: &metadata,
		target_chain: client.extrinsic_target(&services.extrinsics),
		network_suffix: &network_suffix,
		signer: &signer,
		collections: &collections,
		memberships: &memberships,
	};
	let mut report =
		renew_targets(&context, period, &resolved, signing_host.renewal.registration_lock()).await;
	report.pruned = pruned;
	Ok(report)
}

/// Spawn the periodic renewal loop; repeated calls are no-ops. The loop holds
/// only weak references, so it exits when the owning runtime is dropped.
#[cfg(not(target_arch = "wasm32"))]
pub fn start_renewal_loop(services: &Arc<RuntimeServices>, signing_host: &Arc<SigningHost>) {
	if signing_host.renewal.driver.loop_started.swap(true, Ordering::SeqCst) {
		return;
	}
	let weak_services = Arc::downgrade(services);
	let weak_host = Arc::downgrade(signing_host);
	let spawner = services.spawner.clone();
	spawner(Box::pin(async move {
		loop {
			{
				let (Some(services), Some(signing_host)) =
					(weak_services.upgrade(), weak_host.upgrade())
				else {
					return;
				};
				run_tick(&services, &signing_host).await;
			}
			let delay = match current_unix_secs() {
				Ok(now) => next_tick_delay(now),
				Err(_) => CLOCK_FAILURE_TICK_DELAY,
			};
			futures_timer::Delay::new(delay).await;
		}
	}));
}

#[cfg(not(target_arch = "wasm32"))]
async fn run_tick(services: &Arc<RuntimeServices>, signing_host: &SigningHost) {
	if signing_host.root_entropy().is_err() {
		debug!("skipping statement-store renewal tick; no active session");
		return;
	}
	absorb_tick(&signing_host.renewal, renew_now(services, signing_host).await);
}

/// Preserve the last completed pass when a later tick fails.
#[cfg(not(target_arch = "wasm32"))]
fn absorb_tick(
	state: &RenewalState,
	result: Result<StatementRenewalReport, StatementRenewalError>,
) {
	match result {
		Ok(report) => {
			if report.slots_exhausted {
				warn!(period = report.period, "statement-store renewal hit slot exhaustion");
			} else {
				info!(
					period = report.period,
					targets = report.outcomes.len(),
					"statement-store renewal pass complete"
				);
			}
			state.record_report(&report);
		},
		// A tick that could not run leaves the previous pass readable rather than
		// replacing it with nothing: "the last thing we know" beats "no idea".
		Err(reason) => warn!(%reason, "statement-store renewal tick failed"),
	}
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
