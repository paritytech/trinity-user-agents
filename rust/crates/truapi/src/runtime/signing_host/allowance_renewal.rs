//! Ledger and driver for automatic statement-store allowance renewal.
//!
//! The ledger records which accounts this signing host promised to keep
//! allowed, as derivation recipes where possible so entries stay valid when
//! the host rotates to a new root entropy. The driver resolves them against
//! the active session and runs the chain-pure pass in
//! `statement_allowance::renewal`, either once (`renew_now`) or on a periodic
//! tick (`start_renewal_loop`).
//!
//! Only host-owned wallet/paired-device targets are renewed in the background.
//! Product allowance decisions belong to product connection storage, which may
//! be artifact-scoped and is unavailable here. They are ensured on demand by an
//! authorized product request; old unscoped product ledger entries are pruned.

#[cfg(not(target_arch = "wasm32"))]
mod driver;

#[cfg(not(target_arch = "wasm32"))]
pub use driver::{
	active_owner_key, list, renew_now, start_renewal_loop, untrack_account_for_signing_host,
};

#[cfg(any(test, not(target_arch = "wasm32")))]
use crate::host_logic::product_account::derive_root_keypair_from_entropy;
#[cfg(any(test, not(target_arch = "wasm32")))]
use crate::platform::CoreStorage;
use crate::{
	host_logic::product_account::{derive_identity_keypair, derive_sr25519_hard_path},
	platform::{CoreStorageKey, ProductId},
	runtime::statement_allowance::renewal::ResolvedRenewalTarget,
};
use futures::lock::Mutex;
use parity_scale_codec::{Decode, Encode};
#[cfg(any(test, not(target_arch = "wasm32")))]
use tracing::warn;

use super::SigningHost;

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

impl StatementRenewalTarget {
	/// Legacy product entries cannot identify the artifact-scoped grant that authorized them.
	#[cfg(any(test, not(target_arch = "wasm32")))]
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
	#[cfg(any(test, not(target_arch = "wasm32")))]
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
#[cfg(any(test, not(target_arch = "wasm32")))]
fn owner_key(entropy: &[u8]) -> Result<[u8; 32], String> {
	derive_root_keypair_from_entropy(entropy)
		.map(|pair| pair.public.to_bytes())
		.map_err(|err| err.to_string())
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
	driver: driver::DriverState,
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
#[cfg(any(test, not(target_arch = "wasm32")))]
async fn read_entries(
	storage: &(impl CoreStorage + ?Sized),
) -> Result<Vec<TrackedStatementRenewalTarget>, String> {
	let Some(blob) = storage
		.read_core_storage(CoreStorageKey::StatementRenewalTargets)
		.await
		.map_err(|err| format!("renewal ledger read failed: {}", err.reason))?
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
#[cfg(any(test, not(target_arch = "wasm32")))]
async fn track_targets(
	storage: &(impl CoreStorage + ?Sized),
	ledger_lock: &Mutex<()>,
	owner: [u8; 32],
	new_targets: Vec<StatementRenewalTarget>,
) -> Result<(), String> {
	if new_targets.iter().any(StatementRenewalTarget::is_product_grant) {
		return Err("Product statement allowances require an authorized product request; background renewal cannot access artifact-scoped permissions".to_string());
	}
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

#[cfg(any(test, not(target_arch = "wasm32")))]
async fn write_entries(
	storage: &(impl CoreStorage + ?Sized),
	entries: &[TrackedStatementRenewalTarget],
) -> Result<(), String> {
	storage
		.write_core_storage(CoreStorageKey::StatementRenewalTargets, entries.encode())
		.await
		.map_err(|err| format!("renewal ledger write failed: {}", err.reason))
}

fn decode_entries(blob: &[u8]) -> Result<Vec<TrackedStatementRenewalTarget>, String> {
	let mut input = blob;
	let entries = Vec::<TrackedStatementRenewalTarget>::decode(&mut input)
		.map_err(|err| format!("invalid persisted renewal targets: {err}"))?;
	if !input.is_empty() {
		return Err("invalid persisted renewal targets: trailing bytes".to_string());
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
	entropy: &[u8],
	network_suffix: &str,
	target: &StatementRenewalTarget,
) -> Result<ResolvedRenewalTarget, String> {
	let label = target_label(target);
	match target {
		StatementRenewalTarget::ProductStatementAllowance { product_id } => {
			let pair = derive_sr25519_hard_path(
				entropy,
				&["allowance", "statement-store", product_id.as_str()],
			)
			.map_err(|err| err.to_string())?;
			Ok(ResolvedRenewalTarget { label, account_id: pair.public.to_bytes() })
		},
		StatementRenewalTarget::WalletSso => {
			let pair =
				derive_identity_keypair(entropy, network_suffix).map_err(|err| err.to_string())?;
			Ok(ResolvedRenewalTarget { label, account_id: pair.public.to_bytes() })
		},
		StatementRenewalTarget::Account { account_id, .. } => {
			Ok(ResolvedRenewalTarget { label, account_id: *account_id })
		},
	}
}

/// Resolve attribution without refreshing, pruning, or rewriting the ledger.
pub async fn inspection_labels(
	signing_host: &SigningHost,
	entropy: &[u8],
	owner: [u8; 32],
) -> Result<crate::runtime::statement_allowance::inspection::AccountLabels, String> {
	let _guard = signing_host.renewal.ledger_lock().lock().await;
	let bytes = signing_host
		.platform
		.read_core_storage(CoreStorageKey::StatementRenewalTargets)
		.await
		.map_err(|err| format!("renewal ledger read failed: {}", err.reason))?;
	let entries = bytes.as_deref().map(decode_entries).transpose()?.unwrap_or_default();
	let mut labels = crate::runtime::statement_allowance::inspection::AccountLabels::new();
	for entry in entries {
		if entry.owner.is_some_and(|recorded| recorded != owner) {
			continue;
		}
		let resolved = resolve_target(entropy, &signing_host.network_suffix, &entry.target)?;
		let product = match entry.target {
			StatementRenewalTarget::ProductStatementAllowance { product_id } => {
				Some(product_id.to_string())
			},
			_ => None,
		};
		labels.insert(resolved.account_id, (product, resolved.label));
	}
	Ok(labels)
}

/// Record `targets` in the ledger under the active identity.
#[cfg(not(target_arch = "wasm32"))]
pub async fn track(
	signing_host: &SigningHost,
	targets: Vec<StatementRenewalTarget>,
) -> Result<(), String> {
	let entropy = signing_host.root_entropy().map_err(|err| err.to_string())?;
	track_targets(
		signing_host.platform.as_ref(),
		signing_host.renewal.ledger_lock(),
		owner_key(&entropy)?,
		targets,
	)
	.await
}
