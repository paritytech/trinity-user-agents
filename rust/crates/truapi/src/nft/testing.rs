//! A ledger for tests, over a fixed installation and fixed entropy.

use std::sync::Arc;

use truapi::latest::GenericError;

use super::{
	keys::{ItemKey, ItemKeyDeriver, ItemSlot, item_keys_from_entropy},
	ledger::{NftLedger, NftLedgerDeps},
};
use crate::{
	installation::{InstallationId, InstallationIdProvider},
	platform::async_trait,
	runtime::AuthorityError,
	store::Db,
};

/// The installation every test ledger allocates under.
pub const TEST_INSTALLATION: InstallationId = InstallationId([0x1d; 32]);

/// The entropy every test ledger derives item keys from.
pub const TEST_ENTROPY: [u8; 16] = [0x42; 16];

/// A ledger over `db` that allocates under [`TEST_INSTALLATION`] and
/// derives from [`TEST_ENTROPY`].
pub fn test_ledger(db: Db) -> NftLedger {
	NftLedger::new(test_ledger_deps(db))
}

/// What [`test_ledger`] runs on, for tests that replace one part.
pub fn test_ledger_deps(db: Db) -> NftLedgerDeps {
	NftLedgerDeps {
		db,
		installation: Arc::new(FixedInstallation(TEST_INSTALLATION)),
		keys: Arc::new(FixedEntropy(TEST_ENTROPY.to_vec())),
	}
}

/// Always the same installation.
pub struct FixedInstallation(pub InstallationId);

#[async_trait]
impl InstallationIdProvider for FixedInstallation {
	async fn current_installation(&self) -> Result<InstallationId, GenericError> {
		Ok(self.0)
	}
}

/// Item keys derived from one entropy.
pub struct FixedEntropy(pub Vec<u8>);

impl ItemKeyDeriver for FixedEntropy {
	fn derive_item_keys(&self, slots: &[ItemSlot]) -> Result<Vec<ItemKey>, AuthorityError> {
		item_keys_from_entropy(&self.0, slots)
			.map_err(|error| AuthorityError::Unavailable { reason: error.to_string() })
	}
}
