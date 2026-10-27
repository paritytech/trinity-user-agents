//! A ledger for tests, over a fixed installation and fixed entropy.

use std::{collections::HashMap, sync::Arc};

use futures::executor::block_on;
use truapi::latest::GenericError;

use super::{
	imported_secrets::ImportedSecretStore,
	keys::{
		ImportedItemKey, ItemIndex, ItemKey, ItemKeyDeriver, ItemSecret, ItemSecrets, ItemSigner,
		ItemSlot, item_keys_from_entropy, item_secrets_from_entropy, item_signer_from_entropy,
	},
	ledger::{NftLedger, NftLedgerDeps},
	operation::nft_domain,
	purse::PurseId,
	scarcity::TRANSFER_POLICY,
};
use crate::{
	durable::{
		DurableRegistry, DurableTxEngine, DurableTxEntry, DurableTxId, PolicyError, PolicyId,
		PolicyParams, Preparation, ScheduledDurableTx, SubmissionPolicy, Unobservable,
		VerdictFailureKind, engine_on,
	},
	encrypted_storage::MemoryEncryptedStorage,
	host_internal::extrinsic::Sr25519Signer,
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

/// What [`test_ledger`] runs on, for tests that replace one part. Transfers are
/// registered with an engine that never builds them.
pub fn test_ledger_deps(db: Db) -> NftLedgerDeps {
	NftLedgerDeps {
		db: db.clone(),
		installation: Arc::new(FixedInstallation(TEST_INSTALLATION)),
		keys: Arc::new(FixedEntropy(TEST_ENTROPY.to_vec())),
		durable: unbuilt_transfers_engine(db),
		secrets: Arc::new(FixedEntropy(TEST_ENTROPY.to_vec())),
		imported: ImportedSecretStore::new(Arc::new(MemoryEncryptedStorage::default())),
	}
}

/// An engine over `db` that serves the NFT domain, whose transfers stay waiting
/// to be built.
pub fn unbuilt_transfers_engine(db: Db) -> Arc<DurableTxEngine> {
	let oracle = Arc::new(Unobservable(subxt::utils::H256::zero()));
	let registry = DurableRegistry::new()
		.with_domain(nft_domain(), oracle)
		.with_policy(Arc::new(NeverBuilds));
	engine_on(db, registry)
}

/// A transfer policy that leaves every transfer waiting.
struct NeverBuilds;

#[async_trait]
impl SubmissionPolicy for NeverBuilds {
	fn id(&self) -> PolicyId {
		PolicyId::new(TRANSFER_POLICY)
	}

	fn can_retry(&self, _: &DurableTxEntry, _: &PolicyParams, _: VerdictFailureKind) -> bool {
		false
	}

	async fn prepare(
		&self,
		_: &[ScheduledDurableTx],
	) -> Result<HashMap<DurableTxId, Preparation>, PolicyError> {
		Ok(HashMap::new())
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

impl ItemSecrets for FixedEntropy {
	fn derive_item_secrets(&self, slots: &[ItemSlot]) -> Result<Vec<ItemSecret>, AuthorityError> {
		item_secrets_from_entropy(&self.0, slots)
			.map_err(|error| AuthorityError::Unavailable { reason: error.to_string() })
	}
}

impl ItemSigner for FixedEntropy {
	fn item_signer(&self, slot: ItemSlot) -> Result<Sr25519Signer, AuthorityError> {
		item_signer_from_entropy(&self.0, slot)
			.map_err(|error| AuthorityError::Unavailable { reason: error.to_string() })
	}
}

/// The entropy a cheque's sender derives the keys of [`imported_test_key`] from.
pub const SENDER_ENTROPY: [u8; 16] = [0x77; 16];

/// The key at `index` of the sender's purse 0, as a cheque hands it over.
pub fn imported_test_key(index: u32) -> ImportedItemKey {
	let slot =
		ItemSlot { purse: PurseId(0), installation: TEST_INSTALLATION, index: ItemIndex(index) };
	let secret = item_secrets_from_entropy(&SENDER_ENTROPY, &[slot]).unwrap().remove(0);
	ImportedItemKey::from_secret(secret).expect("a derived secret is valid")
}

/// Records `key` at `index` of purse 0 under [`TEST_INSTALLATION`], as an
/// allocation would, without deriving it.
pub fn insert_item_row(db: &Db, key: ItemKey, index: u32) {
	let insert = move |tx: &rusqlite::Transaction<'_>| {
		tx.execute(
			"INSERT INTO nft_item (key, purse, installation, idx) VALUES (?1, 0, ?2, ?3)",
			rusqlite::params![key.0, TEST_INSTALLATION.0, index],
		)?;
		Ok(())
	};
	block_on(db.write(insert)).expect("item row is written");
}
