//! Ledger and driver for automatic statement-store allowance renewal.
//!
//! The ledger records which accounts this signing host promised to keep
//! allowed, as derivation recipes where possible so entries stay valid when
//! the host rotates to a new root entropy. The driver resolves them against
//! the active session and runs the chain-pure pass in
//! `statement_allowance::renewal`, either once (`renew_now`) or on a periodic
//! tick (`start_renewal_loop`).

#[cfg(not(target_arch = "wasm32"))]
mod driver;

#[cfg(not(target_arch = "wasm32"))]
pub use driver::{
	active_owner_key, list, renew_now, start_renewal_loop, untrack_account_for_signing_host,
};

use crate::{
	host_logic::product_account::derive_root_keypair_from_entropy,
	platform::{CoreStorage, CoreStorageKey, normalize_product_identifier},
};
use futures::lock::Mutex;
use parity_scale_codec::{Decode, Encode};
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
    /// These legacy entries cannot identify the artifact-scoped decision that
    /// authorized them. Never infer authorization from host-global storage.
    #[cfg(any(test, not(target_arch = "wasm32")))]
    fn is_product_grant(&self) -> bool {
        match self {
            Self::ProductStatementAllowance { .. } => true,
            Self::Account { label, .. } => label.starts_with("product-account:"),
            Self::WalletSso => false,
        }
    }
}

/// One persisted ledger entry, which is also what a host reads back.
///
/// A derivation recipe resolves under whatever root entropy is active, so it
/// carries no owner and keeps working across a rotation. A raw account id does
/// not re-derive, so it records the root public key that promised it and is
/// ignored under any other identity: without that, a later account would spend
/// its own slot-table capacity keeping a previous account's peer allowed.
///
/// Reading it back does not resolve it: resolution needs root entropy, and a
/// host inspecting its slots may hold none.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct TrackedStatementRenewalTarget {
    /// The account, or the recipe for one, that the host promised to renew.
    pub target: StatementRenewalTarget,
    /// Root public key that promised a raw account id. A recipe carries none
    /// and resolves under whichever identity is active.
    pub owner: Option<[u8; 32]>,
}

impl StatementRenewalTarget {
    /// This target with its product identifier in canonical form.
    ///
    /// The renewal account is derived from `product_id`, and a product
    /// connection derives its own from the normalized form, so an unnormalized
    /// id renews an account no product uses while the real one lapses.
    #[cfg(any(test, not(target_arch = "wasm32")))]
    fn normalized(self) -> Result<Self, String> {
        match self {
            Self::ProductStatementAllowance { product_id } => {
                let normalized = normalize_product_identifier(&product_id).map_err(|_| {
                    format!("product_id {product_id} is not a valid product identifier")
                })?;
                Ok(Self::ProductStatementAllowance {
                    product_id: normalized,
                })
            }
            target @ (Self::WalletSso | Self::Account { .. }) => Ok(target),
        }
    }
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

/// Read the renewal ledger; an absent or undecodable slot is an empty ledger.
///
/// A ledger this build cannot decode is treated as empty rather than failing
/// the pass: the entries are recipes and raw account ids that
/// [`track_targets`] rebuilds on the next allocation or pairing, so refusing to
/// renew anything is strictly worse than starting over.
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
async fn track_targets(
	storage: &(impl CoreStorage + ?Sized),
	ledger_lock: &Mutex<()>,
	owner: [u8; 32],
	new_targets: Vec<StatementRenewalTarget>,
) -> Result<(), String> {
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

/// Record `targets` in the ledger under the active identity.
#[cfg(not(target_arch = "wasm32"))]
pub async fn track(
	signing_host: &SigningHost,
	targets: Vec<StatementRenewalTarget>,
) -> Result<(), String> {
    let targets = targets
        .into_iter()
        .map(StatementRenewalTarget::normalized)
        .collect::<Result<Vec<_>, _>>()?;
    let entropy = signing_host.root_entropy().map_err(|err| err.to_string())?;
    track_targets(
        signing_host.platform.as_ref(),
        signing_host.renewal.ledger_lock(),
        owner_key(&entropy)?,
        targets,
    )
    .await
}
