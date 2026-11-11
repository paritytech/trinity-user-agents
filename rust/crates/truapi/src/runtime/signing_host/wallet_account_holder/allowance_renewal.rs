//! Wallet-owned allowance renewal ledger and chain operations.
//!
//! The ledger records which accounts this signing host promised to keep
//! allowed, as derivation recipes where possible so entries stay valid when
//! the host rotates to a new root entropy. A pass resolves them against the
//! active wallet and runs the chain-pure pass in `statement_allowance::renewal`.
//!
//! Background renewal has no verified artifact-scoped product authority.
//! Product targets remain decodable for inspection and migration, but must be
//! authorized on demand and are never retained as background grants.

#[cfg(not(target_arch = "wasm32"))]
use crate::platform::CoreStorage;
use crate::{
	host_logic::product_account::ProductAccountError,
	platform::{CoreStorageKey, ProductId},
	runtime::{
		authority::{AccountHolder, AuthorityError, AuthoritySession},
		statement_allowance::StatementAllowanceError,
		statement_store_rpc::StatementStoreRpcClientError,
	},
};
use futures::lock::Mutex;
use parity_scale_codec::{Decode, Encode};
#[cfg(not(target_arch = "wasm32"))]
use tracing::{debug, info, warn};

#[cfg(not(target_arch = "wasm32"))]
use super::PersonhoodCollection;
use super::{WalletAccountHolder, WalletKeys};
use crate::runtime::statement_allowance::renewal::ResolvedRenewalTarget;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::{
	allowances::current_unix_secs,
	statement_allowance::{
		self, fetch_metadata, find_including_rings,
		renewal::{RenewalChainContext, StatementRenewalReport, renew_targets},
	},
};
use truapi::latest::GenericError;

/// Failure reading renewal targets or preparing a statement-store renewal pass.
#[derive(Debug, thiserror::Error)]
pub enum StatementRenewalError {
	/// The wallet that began the operation is no longer the active one.
	#[error(transparent)]
	Authority(#[from] AuthorityError),
	/// A tracked account could not be derived.
	#[error(transparent)]
	ProductAccount(#[from] ProductAccountError),
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
	/// Renewal requires a personhood membership for its allowance proof.
	#[error(
		"signing account is not a member of any personhood ring; cannot renew statement-store allowances"
	)]
	MissingPersonhoodMembership,
	/// Background work cannot recover the artifact-scoped product authorization.
	#[error("product statement allowances require an authorized product request")]
	ProductAuthorizationRequired,
}

impl From<StatementRenewalError> for GenericError {
	fn from(error: StatementRenewalError) -> Self {
		Self { reason: error.to_string() }
	}
}

/// A statement account or derivation recipe the wallet keeps renewed.
#[derive(Clone, derive_more::Debug, PartialEq, Eq, Encode, Decode)]
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
		#[debug("{}", crate::logging::Account::new(account_id))]
		account_id: [u8; 32],
		/// Human-readable name used in logs and reports.
		label: String,
	},
}

impl StatementRenewalTarget {
	#[cfg(not(target_arch = "wasm32"))]
	fn is_product_grant(&self) -> bool {
		match self {
			Self::ProductStatementAllowance { .. } => true,
			Self::Account { label, .. } => label.starts_with("product-account:"),
			Self::WalletSso => false,
		}
	}
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
	#[cfg(not(target_arch = "wasm32"))]
	fn new(target: StatementRenewalTarget, owner: [u8; 32]) -> Self {
		let owner = match &target {
			StatementRenewalTarget::Account { .. } => Some(owner),
			StatementRenewalTarget::ProductStatementAllowance { .. } |
			StatementRenewalTarget::WalletSso => None,
		};
		Self { target, owner }
	}

	/// Whether this entry belongs to the identity rooted at `owner`.
	#[cfg(not(target_arch = "wasm32"))]
	fn is_owned_by(&self, owner: [u8; 32]) -> bool {
		self.owner.is_none_or(|recorded| recorded == owner)
	}
}

/// Shared wallet coordination for on-demand allowance issuance and renewal.
#[derive(Default)]
pub struct RenewalState {
	/// Serializes slot registrations between the renewal pass and on-demand
	/// allocation so both cannot race for the same free slot.
	registration_lock: Mutex<()>,
	/// Serializes read-modify-write cycles on the ledger so a concurrent
	/// allocation cannot drop another's entry.
	ledger_lock: Mutex<()>,
	/// The most recent pass, so a host that drives the in-process loop can read
	/// what it achieved. The loop computes a report on every tick and has no
	/// caller to hand it to, and exhaustion is the outcome a host most needs to
	/// act on. A blocking lock rather than an async one: every use is a clone or
	/// a store with no await in between.
	#[cfg(not(target_arch = "wasm32"))]
	last_report: std::sync::Mutex<Option<StatementRenewalReport>>,
}

impl RenewalState {
	/// Serialize registration scans with competing issuers.
	pub fn registration_lock(&self) -> &Mutex<()> {
		&self.registration_lock
	}

	fn ledger_lock(&self) -> &Mutex<()> {
		&self.ledger_lock
	}

	/// Record the pass a host has not seen the return value of.
	#[cfg(not(target_arch = "wasm32"))]
	fn record_report(&self, report: &StatementRenewalReport) {
		if let Ok(mut last) = self.last_report.lock() {
			*last = Some(report.clone());
		}
	}

	/// The most recent pass the loop ran. `None` until one has run, which a host
	/// should read as "not yet" rather than as healthy.
	#[cfg(not(target_arch = "wasm32"))]
	fn last_report(&self) -> Option<StatementRenewalReport> {
		self.last_report.lock().ok().and_then(|last| last.clone())
	}
}

/// Discard unreadable entries; later allocations rebuild the ledger.
#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

/// Stable labels identify pruned entries even without an active wallet.
fn target_label(target: &StatementRenewalTarget) -> String {
	match target {
		StatementRenewalTarget::ProductStatementAllowance { product_id } => {
			format!("product:{product_id}")
		},
		StatementRenewalTarget::WalletSso => "wallet-sso".to_string(),
		StatementRenewalTarget::Account { label, .. } => label.clone(),
	}
}

fn resolve_target(
	keys: &WalletKeys,
	target: &StatementRenewalTarget,
) -> Result<ResolvedRenewalTarget, StatementRenewalError> {
	let label = target_label(target);
	match target {
		StatementRenewalTarget::ProductStatementAllowance { product_id } => {
			let pair = keys.statement_allowance_key(product_id)?;
			Ok(ResolvedRenewalTarget { label, account_id: pair.public.to_bytes() })
		},
		StatementRenewalTarget::WalletSso => {
			let pair = keys.identity_keypair()?;
			Ok(ResolvedRenewalTarget { label, account_id: pair.public.to_bytes() })
		},
		StatementRenewalTarget::Account { account_id, .. } => {
			Ok(ResolvedRenewalTarget { label, account_id: *account_id })
		},
	}
}

impl WalletAccountHolder {
	/// Resolve attribution without refreshing, pruning, or rewriting the ledger.
	pub(super) async fn inspection_labels(
		&self,
		session: &AuthoritySession,
	) -> Result<crate::runtime::statement_allowance::inspection::AccountLabels, String> {
		let _guard = self.renewal.ledger_lock().lock().await;
		self.require_current_session(session).map_err(|error| error.to_string())?;
		let bytes = self
			.services
			.platform
			.read_core_storage(CoreStorageKey::StatementRenewalTargets)
			.await
			.map_err(|error| format!("renewal ledger read failed: {}", error.reason))?;
		let entries = bytes
			.as_deref()
			.map(decode_entries)
			.transpose()
			.map_err(|error| error.to_string())?
			.unwrap_or_default();
		self.with_keys::<_, StatementRenewalError>(session, |keys| {
			let owner = keys.root_public_key()?;
			let mut labels = crate::runtime::statement_allowance::inspection::AccountLabels::new();
			for entry in entries {
				if entry.owner.is_some_and(|recorded| recorded != owner) {
					continue;
				}
				let resolved = resolve_target(keys, &entry.target)?;
				let product = match entry.target {
					StatementRenewalTarget::ProductStatementAllowance { product_id } => {
						Some(product_id.to_string())
					},
					_ => None,
				};
				labels.insert(resolved.account_id, (product, resolved.label));
			}
			Ok(labels)
		})
		.map_err(|error| error.to_string())
	}

	/// Keep an allocation's renewal record bound to its original wallet.
	#[cfg(not(target_arch = "wasm32"))]
	pub async fn track_statement_renewal_targets_for(
		&self,
		session: &AuthoritySession,
		targets: Vec<StatementRenewalTarget>,
	) -> Result<(), StatementRenewalError> {
		if targets.iter().any(StatementRenewalTarget::is_product_grant) {
			return Err(StatementRenewalError::ProductAuthorizationRequired);
		}
		let _guard = self.renewal.ledger_lock().lock().await;
		self.require_current_session(session)?;
		let mut entries = read_entries(self.services.platform.as_ref()).await?;
		self.require_current_session(session)?;
		let mut changed = false;
		for target in targets {
			let entry = TrackedStatementRenewalTarget::new(target, session.public_key);
			if !entries.contains(&entry) {
				entries.push(entry);
				changed = true;
			}
		}
		if changed {
			write_entries(self.services.platform.as_ref(), &entries).await?;
			self.require_current_session(session)?;
		}
		Ok(())
	}
}

#[cfg(not(target_arch = "wasm32"))]
impl WalletAccountHolder {
	/// Record `targets` in the ledger under the active identity.
	pub async fn track_statement_renewal_targets(
		&self,
		targets: Vec<StatementRenewalTarget>,
	) -> Result<(), StatementRenewalError> {
		let session = self.current_session().ok_or(AuthorityError::Disconnected)?;
		self.track_statement_renewal_targets_for(&session, targets).await
	}

	/// List tracked entries without requiring wallet unlock.
	pub async fn statement_renewal_targets(
		&self,
	) -> Result<Vec<TrackedStatementRenewalTarget>, StatementRenewalError> {
		let _guard = self.renewal.ledger_lock().lock().await;
		read_entries(self.services.platform.as_ref()).await
	}

	/// Identify which fixed ledger entries belong to the active wallet.
	pub fn statement_renewal_owner_key(&self) -> Result<[u8; 32], StatementRenewalError> {
		Ok(self.current_session().ok_or(AuthorityError::Disconnected)?.public_key)
	}

	/// Stop renewing one fixed statement account for the active identity.
	pub async fn untrack_statement_renewal_account(
		&self,
		account_id: &[u8; 32],
	) -> Result<bool, StatementRenewalError> {
		let session = self.current_session().ok_or(AuthorityError::Disconnected)?;
		let _guard = self.renewal.ledger_lock().lock().await;
		self.require_current_session(&session)?;
		let mut entries = read_entries(self.services.platform.as_ref()).await?;
		self.require_current_session(&session)?;
		let original_len = entries.len();
		entries.retain(|entry| {
			entry.owner != Some(session.public_key) ||
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
		write_entries(self.services.platform.as_ref(), &entries).await?;
		self.require_current_session(&session)?;
		Ok(true)
	}

	/// One renewal pass: resolve the ledger against the active session and renew
	/// every target for the current period.
	pub async fn renew_statement_allowances(
		&self,
	) -> Result<StatementRenewalReport, StatementRenewalError> {
		let session = self.current_session().ok_or(AuthorityError::Disconnected)?;
		let period = statement_allowance::slot::current_period(current_unix_secs()?);
		let (targets, pruned) = self.owned_targets(&session).await?;
		let resolved = self
			.with_keys::<_, AuthorityError>(&session, |keys| Ok(resolve_targets(keys, &targets)))?;
		if resolved.is_empty() {
			return Ok(StatementRenewalReport {
				period,
				outcomes: Vec::new(),
				pruned,
				slots_exhausted: false,
			});
		}

		let signer = self.personhood_signer(&session).await?;
		// The pooled client, so the extrinsic's birth block and its submission
		// reach the same node.
		let client = self
			.services
			.statement_store
			.chain_client("statement-allowance renewal")
			.await?;
		let rpc = client.rpc();
		let metadata = fetch_metadata(rpc).await?;
		let network_suffix = statement_allowance::slot::read_network_suffix(rpc).await?;
		// Every ring back to index 0, because a membership that stopped being
		// re-included still proves against the ring that holds it.
		let memberships =
			find_including_rings(rpc, &metadata, &signer, &PersonhoodCollection::ALL, u32::MAX)
				.await?;
		if memberships.is_empty() {
			return Err(StatementRenewalError::MissingPersonhoodMembership);
		}
		let context = RenewalChainContext {
			rpc,
			metadata: &metadata,
			target_chain: client.extrinsic_target(&self.services.extrinsics),
			network_suffix: &network_suffix,
			signer: &signer,
			collections: &PersonhoodCollection::ALL,
			memberships: &memberships,
		};
		self.require_current_session(&session)?;
		let mut report =
			renew_targets(&context, period, &resolved, self.renewal.registration_lock()).await;
		report.pruned = pruned;
		Ok(report)
	}

	/// Renew the active wallet and record the periodic pass result.
	pub async fn renewal_tick(&self) {
		if self.current_session().is_none() {
			debug!("skipping statement-store renewal tick; no active session");
			return;
		}
		absorb_tick(&self.renewal, self.renew_statement_allowances().await);
	}

	/// Most recent result from the host's periodic renewal loop.
	pub fn last_statement_renewal_report(&self) -> Option<StatementRenewalReport> {
		self.renewal.last_report()
	}

	/// The ledger targets `session`'s wallet promised, dropping any it did not.
	///
	/// An entry promised by a different identity would consume this one's slots for
	/// an account it never promised, so it is removed rather than skipped — the cost
	/// is paid once per identity change instead of on every tick. The ledger is only
	/// rewritten when something was actually dropped, and never once `session` is
	/// no longer the active wallet, so a stale pass cannot prune entries a newer
	/// wallet tracked.
	///
	/// The lock covers the read as well as the write, because this is a
	/// read-modify-write of the whole ledger: reading outside it lets a tracking
	/// call land in the gap and be overwritten by a view that predates it, leaving
	/// that account tracked nowhere and never renewed again.
	///
	/// The labels are logged here rather than only returned. Every step between this
	/// and the report can fail, and `renewal_tick` does not read the report at all,
	/// so the log is the one place a prune is recorded unconditionally.
	async fn owned_targets(
		&self,
		session: &AuthoritySession,
	) -> Result<(Vec<StatementRenewalTarget>, Vec<String>), StatementRenewalError> {
		let _guard = self.renewal.ledger_lock().lock().await;
		self.require_current_session(session)?;
		let entries = read_entries(self.services.platform.as_ref()).await?;
		self.require_current_session(session)?;
		let (owned, foreign): (Vec<_>, Vec<_>) = entries.into_iter().partition(|entry| {
			entry.is_owned_by(session.public_key) && !entry.target.is_product_grant()
		});
		let pruned: Vec<String> = foreign.iter().map(|entry| target_label(&entry.target)).collect();
		if !foreign.is_empty() {
			warn!(dropped = ?pruned, "pruning foreign or unscoped product renewal targets");
			write_entries(self.services.platform.as_ref(), &owned).await?;
		}
		Ok((owned.into_iter().map(|entry| entry.target).collect(), pruned))
	}
}

/// Skip unusable entries so they cannot prevent renewal of other targets.
#[cfg(not(target_arch = "wasm32"))]
fn resolve_targets(
	keys: &WalletKeys,
	targets: &[StatementRenewalTarget],
) -> Vec<ResolvedRenewalTarget> {
	targets
		.iter()
		.filter_map(|target| match resolve_target(keys, target) {
			Ok(resolved) => Some(resolved),
			Err(reason) => {
				warn!(?target, %reason, "skipping an unresolvable renewal target");
				None
			},
		})
		.collect()
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
