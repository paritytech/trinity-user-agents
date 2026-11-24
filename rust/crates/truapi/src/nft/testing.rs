//! A ledger for tests, over a fixed installation and fixed entropy.

use std::{collections::HashMap, num::NonZeroU32, sync::Arc};

use futures::executor::block_on;
use truapi::latest::GenericError;

use super::{
	holding::{HeldItem, TrackedHolding},
	imported_secrets::ImportedSecretStore,
	keys::{
		ImportedItemKey, ItemIndex, ItemKey, ItemKeyDeriver, ItemSecret, ItemSecrets, ItemSigner,
		ItemSlot,
	},
	ledger::{NftLedger, NftLedgerDeps},
	operation::nft_domain,
	purse::PurseId,
	recovery::NftRecovery,
	scarcity::{FixedMoveBudget, MoveBudget, TransferParams},
};
use crate::{
	durable::{
		DeferredDurableTx, DurableRegistry, DurableTxEngine, DurableTxEntry, DurableTxId,
		PolicyError, PolicyParams, Preparation, SubmissionPolicy, Unobservable, VerdictFailureKind,
		engine_on,
	},
	encrypted_storage::MemoryEncryptedStorage,
	host_internal::extrinsic::Sr25519Signer,
	host_logic::root_entropy::RootEntropy,
	installation::{InstallationId, InstallationIdProvider},
	pgas_claims::{
		BuiltClaim, ClaimBuildError, ClaimSlot, PgasClaimBuilder, PgasClaims, with_pgas_claims,
	},
	runtime::AuthorityError,
	store::Db,
};

/// The installation every test ledger allocates under.
pub const TEST_INSTALLATION: InstallationId = InstallationId([0x1d; 32]);

/// The entropy every test ledger derives item keys from.
pub const TEST_ENTROPY: [u8; 16] = [0x42; 16];

/// The fee-less move budget of every test ledger, as on Paseo Next.
pub const TEST_MAXIMUM_MOVES: u16 = 16;

/// A ledger over `db` that allocates under [`TEST_INSTALLATION`] and
/// derives from [`TEST_ENTROPY`].
pub fn test_ledger(db: Db) -> NftLedger {
	NftLedger::new(test_ledger_deps(db))
}

/// What [`test_ledger`] runs on, for tests that replace one part. Transfers are
/// registered with an engine that never builds them.
pub fn test_ledger_deps(db: Db) -> NftLedgerDeps {
	let entropy = Arc::new(FixedEntropy::new(TEST_ENTROPY));
	let durable = unbuilt_transfers_engine(db.clone());
	NftLedgerDeps {
		db,
		installation: Arc::new(FixedInstallation(TEST_INSTALLATION)),
		keys: entropy.clone(),
		durable: durable.clone(),
		secrets: entropy,
		imported: ImportedSecretStore::new(Arc::new(MemoryEncryptedStorage::default())),
		recovery: Arc::new(NftRecovery::off()),
		budget: Arc::new(FixedMoveBudget(MoveBudget::new(TEST_MAXIMUM_MOVES))),
		claims: PgasClaims::new(durable),
	}
}

/// An engine over `db` that serves the NFT domain and the PGAS claims its
/// paid moves wait for, whose transactions all stay waiting to be built.
pub fn unbuilt_transfers_engine(db: Db) -> Arc<DurableTxEngine> {
	let genesis = subxt::utils::H256::zero();
	let registry = DurableRegistry::new().with_domain(
		nft_domain(),
		genesis,
		Arc::new(Unobservable),
		Arc::new(NeverBuilds),
	);
	engine_on(db, with_pgas_claims(registry, genesis, Arc::new(NoSessionClaimBuilder)))
}

/// Builds no PGAS claim, since no session is ever signed in, so claims wait.
struct NoSessionClaimBuilder;

#[async_trait::async_trait]
impl PgasClaimBuilder for NoSessionClaimBuilder {
	async fn build_claim(
		&self,
		_: subxt::utils::AccountId32,
		_: &std::collections::BTreeSet<ClaimSlot>,
	) -> Result<BuiltClaim, ClaimBuildError> {
		Err(ClaimBuildError::NoSession)
	}
}

/// A transfer policy that leaves every transfer waiting.
struct NeverBuilds;

#[async_trait::async_trait]
impl SubmissionPolicy for NeverBuilds {
	fn can_retry(&self, _: &DurableTxEntry, _: &PolicyParams, _: VerdictFailureKind) -> bool {
		false
	}

	async fn prepare(
		&self,
		_: &[DeferredDurableTx],
	) -> Result<HashMap<DurableTxId, Preparation>, PolicyError> {
		Ok(HashMap::new())
	}
}

/// Always the same installation.
pub struct FixedInstallation(pub InstallationId);

#[async_trait::async_trait]
impl InstallationIdProvider for FixedInstallation {
	async fn current_installation(&self) -> Result<InstallationId, GenericError> {
		Ok(self.0)
	}
}

/// Item keys derived from one entropy.
pub struct FixedEntropy(RootEntropy);

impl FixedEntropy {
	/// Derives every key from `entropy`.
	pub fn new(entropy: impl Into<Vec<u8>>) -> Self {
		Self(RootEntropy::new(entropy))
	}
}

impl ItemKeyDeriver for FixedEntropy {
	fn derive_item_keys(&self, slots: &[ItemSlot]) -> Result<Vec<ItemKey>, AuthorityError> {
		self.0
			.item_keys(slots)
			.map_err(|error| AuthorityError::Unavailable { reason: error.to_string() })
	}
}

impl ItemSecrets for FixedEntropy {
	fn derive_item_secrets(&self, slots: &[ItemSlot]) -> Result<Vec<ItemSecret>, AuthorityError> {
		self.0
			.item_secrets(slots)
			.map_err(|error| AuthorityError::Unavailable { reason: error.to_string() })
	}
}

impl ItemSigner for FixedEntropy {
	fn item_signer(&self, slot: ItemSlot) -> Result<Sr25519Signer, AuthorityError> {
		self.0
			.item_signer(slot)
			.map_err(|error| AuthorityError::Unavailable { reason: error.to_string() })
	}
}

/// The entropy a cheque's sender derives the keys of [`imported_test_key`] from.
const SENDER_ENTROPY: [u8; 16] = [0x77; 16];

/// The key at `index` of the sender's purse 0, as a cheque hands it over.
pub fn imported_test_key(index: u32) -> ImportedItemKey {
	let slot =
		ItemSlot { purse: PurseId(0), installation: TEST_INSTALLATION, index: ItemIndex(index) };
	let sender = RootEntropy::new(SENDER_ENTROPY);
	let secret = sender.item_secrets(&[slot]).unwrap().remove(0);
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

/// Allocates `count` keys in `purse` and records each as holding instance
/// 100, 101, ..., so they are idle there.
pub fn allocate_held_items(ledger: &NftLedger, purse: PurseId, count: u32) -> Vec<ItemKey> {
	let count = NonZeroU32::new(count).expect("count is positive");
	let keys = block_on(ledger.allocate_items(purse, count)).expect("items are allocated");
	let holdings = keys
		.iter()
		.enumerate()
		.map(|(index, key)| (*key, Some(unmoved_holding(100 + index as u64))))
		.collect();
	block_on(ledger.record_holdings(holdings)).expect("holdings are recorded");
	keys
}

/// The params of every registered transfer, in registration order.
pub fn stored_transfer_params(db: &Db) -> Vec<TransferParams> {
	let stored = |conn: &rusqlite::Connection| {
		let mut statement =
			conn.prepare("SELECT policy_params FROM durable_tx WHERE domain = 'nft' ORDER BY id")?;
		let rows = statement.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
		Ok(rows.collect::<Result<Vec<_>, _>>()?)
	};
	let rows = block_on(db.read(stored)).expect("params are read");
	rows.into_iter()
		.map(|params| TransferParams::try_from(&PolicyParams(params)).expect("params decode"))
		.collect()
}

/// Marks the transfer `id` as finalized, as a landing at a finalized block
/// would.
pub fn finalize_transfer(db: &Db, id: DurableTxId) {
	update_transfer(db, id, "UPDATE durable_tx SET status = 'FINALIZED_SUCCESS' WHERE id = ?1");
}

/// Marks the transfer `id` as failed, as a rejection would.
pub fn fail_transfer(db: &Db, id: DurableTxId) {
	let statement =
		"UPDATE durable_tx SET status = 'FAILURE', failure_kind = 'REJECTED' WHERE id = ?1";
	update_transfer(db, id, statement);
}

/// Runs `statement` with the transfer `id` as its one parameter.
fn update_transfer(db: &Db, id: DurableTxId, statement: &'static str) {
	let written = db.write(move |tx| {
		tx.execute(statement, [id.0])?;
		Ok(())
	});
	block_on(written).expect("status is written");
}

/// A holding of `instance` in collection 1, item 1, with every fee-less
/// move left.
pub fn unmoved_holding(instance: u64) -> TrackedHolding {
	TrackedHolding { item: HeldItem { collection: 1, item: 1, instance }, moves: 0 }
}

/// [`unmoved_holding`] of an NFT that used `moves` fee-less moves.
pub fn holding_after_moves(instance: u64, moves: u16) -> TrackedHolding {
	TrackedHolding { moves, ..unmoved_holding(instance) }
}
