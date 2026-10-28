//! NFT purses against a live Asset Hub. Ignored by default; run with
//! `cargo test -p truapi --lib nft::live -- --ignored --nocapture`.
//!
//! One run goes through every purse call a product makes, as a sending and a
//! receiving product on one host: it creates a collection, mints into keys
//! the sender allocated, rebalances them into another purse, hands one off
//! in an `Ephemeral` and one in a `Static` cheque that the receiver deposits,
//! revokes an undelivered cheque, refunds a deposited one and revokes it
//! after, and transfers one out of the purses, following each operation
//! until it clears. It burns everything and deletes the collection whatever happened.
//! The collection's owner is `//Alice` unless `LIVE_NFT_OWNER_MNEMONIC` names
//! another funded account, which then signs with its root key.
//! `LIVE_ASSET_HUB_WS` and `LIVE_ASSET_HUB_GENESIS` point it at another
//! Asset Hub.

use core::time::Duration;
use std::{collections::HashSet, sync::Arc};

use futures::StreamExt;
use sp_crypto_hashing::blake2_256;
use subxt::{
	dynamic::{At, Value, tx},
	utils::{AccountId32, H256},
};
use truapi::latest::{
	NftPurseCheque, NftPurseChequeDestination, NftPurseChequeId, NftPurseClearedItem,
	NftPurseIdempotencyKey, NftPurseItemKey, NftPurseStatus,
};

use super::{
	ImportedSecretStore, ItemKey, NftLedger, NftLedgerDeps, PurseId, ScarcityTransfersDeps,
	scarcity::ASSET_HUB_PIPELINE,
	testing::{FixedEntropy, FixedInstallation, TEST_INSTALLATION},
	with_scarcity_transfers,
};
use crate::{
	chain::{
		BlockBackend, ChainHeads, DispatchOutcome, EncodedExtrinsic, ExtrinsicService,
		FixedExtensionVersions, HashAndNumber, TxSubmitter, WatchEvent, WatchedStorage,
		live_asset_hub, live_asset_hub_genesis, tokio_spawner,
	},
	chain_runtime::ChainRuntime,
	durable::{DurableDeps, DurableRegistry, DurableTxEngine},
	encrypted_storage::MemoryEncryptedStorage,
	host_internal::extrinsic::Sr25519Signer,
	host_logic::product_account::{derive_root_keypair_from_entropy, derive_sr25519_hard_path},
	nft::HoldingTracker,
	nft_purse::{FixedStaticSecret, ItemCount, NftPurses, NftPursesDeps, PageSize},
	platform::ProductId,
	store::{Db, open_temporary_core_db},
	time::RealTimer,
};

const DEV_PHRASE: &str = "bottom drive obey lake curtain smoke basket hold race lonely fit walk";
const ALICE_HEX: &str = "d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d";
const ITEM_COUNT: u32 = 4;
const WAIT: Duration = Duration::from_secs(240);
const POLL: Duration = Duration::from_secs(3);

/// The account that owns the test collection and pays for its calls.
struct CollectionOwner {
	chain: ChainRuntime,
	genesis: H256,
	signer: Sr25519Signer,
	/// Builds the owner's calls on pipeline 1, as the host builds Asset Hub.
	extrinsics: ExtrinsicService,
}

impl CollectionOwner {
	/// `//Alice`, or the root account of `LIVE_NFT_OWNER_MNEMONIC`.
	fn from_env(chain: ChainRuntime, genesis: H256) -> Self {
		let mnemonic = std::env::var("LIVE_NFT_OWNER_MNEMONIC").ok();
		let phrase = mnemonic.as_deref().unwrap_or(DEV_PHRASE);
		let entropy = bip39::Mnemonic::parse(phrase).expect("a BIP-39 phrase").to_entropy();
		let keypair = match mnemonic {
			Some(_) => derive_root_keypair_from_entropy(&entropy),
			None => derive_sr25519_hard_path(&entropy, &["Alice"]),
		}
		.expect("the owner key derives");
		let versions = Arc::new(FixedExtensionVersions::one_chain(genesis, ASSET_HUB_PIPELINE));
		let extrinsics = ExtrinsicService::over_chain(chain.clone(), versions);
		Self { chain, genesis, signer: Sr25519Signer::from_keypair(&keypair), extrinsics }
	}

	/// Signs `call` of `Scarcity`, submits it, and waits until a best block
	/// includes it, failing the test unless it dispatched successfully.
	async fn dispatch_successfully(&self, call: &str, args: Vec<Value>) {
		let extrinsic = self.signed_by_owner(call, args).await;
		let at = self.included_at(&extrinsic).await;
		let outcome = self.chain.dispatch_outcome(self.genesis, at, extrinsic.hash()).await;
		assert_eq!(outcome.expect("outcome readable"), Some(DispatchOutcome::Succeeded), "{call}");
	}

	async fn signed_by_owner(&self, call: &str, args: Vec<Value>) -> EncodedExtrinsic {
		let built = self
			.extrinsics
			.build_mortal_extrinsic(self.genesis)
			.with_call(tx("Scarcity", call, args))
			.with_signer(&self.signer)
			.build()
			.await;
		built.expect("extrinsic builds").extrinsic
	}

	fn signer_account(&self) -> [u8; 32] {
		use subxt::tx::Signer;
		self.signer.account_id().0
	}

	async fn included_at(&self, extrinsic: &EncodedExtrinsic) -> HashAndNumber {
		let mut events =
			self.chain.submit_and_watch(self.genesis, extrinsic).await.expect("submits");
		while let Some(event) = events.next().await {
			match event {
				WatchEvent::InBestBlock(hash) | WatchEvent::InFinalizedBlock(hash) => {
					let number = self.chain.block_number(self.genesis, hash).await.expect("number");
					return HashAndNumber { hash, number: number.expect("known block") };
				},
				WatchEvent::NoLongerInBestBlock => continue,
				terminal => panic!("extrinsic not included: {terminal:?}"),
			}
		}
		panic!("the watch ended before inclusion")
	}
}

/// A ledger with its durable engine and holding tracker, all over the live
/// chain, deriving item keys from entropy fresh for this run.
struct LivePurses {
	_dir: tempfile::TempDir,
	db: Db,
	ledger: NftLedger,
	_tracker: HoldingTracker,
}

fn live_purses(chain: &ChainRuntime, genesis: H256) -> LivePurses {
	let (dir, db) = open_temporary_core_db();
	let keys = Arc::new(FixedEntropy(rand::random::<[u8; 16]>().to_vec()));
	let imported = ImportedSecretStore::new(Arc::new(MemoryEncryptedStorage::default()));
	let storage: Arc<dyn WatchedStorage> = Arc::new(chain.clone());
	let heads = Arc::new(chain.clone());
	let versions = Arc::new(FixedExtensionVersions::one_chain(genesis, ASSET_HUB_PIPELINE));
	let transfers = ScarcityTransfersDeps {
		storage: storage.clone(),
		heads,
		genesis,
		signer: keys.clone(),
		imported: imported.clone(),
		extrinsics: Arc::new(ExtrinsicService::over_chain(chain.clone(), versions)),
	};
	let engine =
		live_engine(chain, db.clone(), with_scarcity_transfers(DurableRegistry::new(), transfers));
	let installation = Arc::new(FixedInstallation(TEST_INSTALLATION));
	let secrets = keys.clone();
	let deps =
		NftLedgerDeps { db: db.clone(), installation, keys, durable: engine, secrets, imported };
	let ledger = NftLedger::new(deps);
	let tracking = super::HoldingTrackerDeps {
		ledger: ledger.clone(),
		storage,
		genesis,
		spawner: tokio_spawner(),
	};
	LivePurses { _dir: dir, db, ledger, _tracker: HoldingTracker::start(tracking) }
}

/// An engine over the live chain that builds and settles transfers on its own.
fn live_engine(
	chain: &ChainRuntime,
	db: crate::store::Db,
	registry: DurableRegistry,
) -> Arc<DurableTxEngine> {
	let chain = Arc::new(chain.clone());
	let engine = DurableTxEngine::new(DurableDeps {
		db,
		registry,
		heads: chain.clone(),
		blocks: chain.clone(),
		validator: chain.clone(),
		submitter: chain,
		timer: Arc::new(RealTimer),
		spawner: tokio_spawner(),
	});
	engine.ensure_submissions_started();
	let settling = engine.clone();
	tokio::spawn(async move {
		while settling.run_until_settled().await.is_ok() {
			tokio::time::sleep(POLL).await
		}
	});
	engine
}

impl CollectionOwner {
	/// Creates a collection with one transferable item definition, and
	/// returns its id.
	async fn create_collection(&self) -> u32 {
		let collection = self.next_collection_id().await;
		self.dispatch_successfully("create_collection", vec![]).await;
		let transferable = Value::unnamed_variant("Transferable", []);
		let args = vec![Value::u128(collection.into()), transferable, Value::unnamed_composite([])];
		self.dispatch_successfully("define_item", args).await;
		collection
	}

	async fn mint_into(&self, collection: u32, keys: &[ItemKey]) {
		for key in keys {
			let to = Value::from_bytes(key.0);
			let args = vec![
				Value::u128(collection.into()),
				Value::u128(0),
				to,
				Value::unnamed_composite([]),
			];
			self.dispatch_successfully("mint", args).await;
		}
	}

	/// Burns every NFT `keys` hold now, then deletes the item definition and
	/// the collection. Runs whatever the test did, so each step only logs.
	async fn clean_up(&self, collection: u32, keys: &[ItemKey]) {
		for instance in self.instances_held_by(keys).await {
			self.dispatch_successfully("force_burn", vec![Value::u128(instance.into())])
				.await;
		}
		self.dispatch_successfully(
			"delete_item",
			vec![Value::u128(collection.into()), Value::u128(0)],
		)
		.await;
		self.dispatch_successfully("delete_collection", vec![Value::u128(collection.into())])
			.await;
	}

	async fn instances_held_by(&self, keys: &[ItemKey]) -> Vec<u64> {
		let owners: Vec<_> = keys.iter().map(|key| (AccountId32(key.0),)).collect();
		let address = subxt::dynamic::storage::<(AccountId32,), Value>("Scarcity", "NftsByOwner");
		let best = self.chain.heads(self.genesis).await.expect("heads").best.hash;
		let storage: &dyn WatchedStorage = &self.chain;
		let held = storage
			.read_entries_at(self.genesis, best, &address, &owners)
			.await
			.expect("holdings");
		held.into_iter().flatten().map(|nft| instance_of(&nft)).collect()
	}

	/// The id the next created collection gets. Racy with other owners; a
	/// collection that turns out not to be ours fails `define_item`, which
	/// only its owner may call.
	async fn next_collection_id(&self) -> u32 {
		let address = subxt::dynamic::storage::<(), u32>("Scarcity", "NextCollectionId");
		let best = self.chain.heads(self.genesis).await.expect("heads").best.hash;
		let storage: &dyn WatchedStorage = &self.chain;
		let values = storage
			.read_entries_at(self.genesis, best, &address, &[()])
			.await
			.expect("reads");
		values[0].unwrap_or_default()
	}
}

fn instance_of(nft: &Value) -> u64 {
	let instance = nft.at("instance").and_then(|value| value.as_u128()).expect("an instance field");
	u64::try_from(instance).expect("instance fits u64")
}

/// Polls `read` until it returns `Some`, failing the test after [`WAIT`].
async fn wait_for<T, F, Fut>(what: &str, mut read: F) -> T
where
	F: FnMut() -> Fut,
	Fut: core::future::Future<Output = Option<T>>,
{
	let waiting = async {
		loop {
			if let Some(value) = read().await {
				return value;
			}
			tokio::time::sleep(POLL).await;
		}
	};
	tokio::time::timeout(WAIT, waiting)
		.await
		.unwrap_or_else(|_| panic!("timed out waiting for {what}"))
}

/// The purse calls over the live ledger, with receive key secrets in memory
/// and a static receive secret fresh for this run.
fn live_product_purses(purses: &LivePurses) -> NftPurses {
	let ledger = purses.ledger.clone();
	let encrypted = Arc::new(MemoryEncryptedStorage::default());
	let static_secret = Arc::new(FixedStaticSecret(rand::random()));
	NftPurses::new(NftPursesDeps { ledger, db: purses.db.clone(), encrypted, static_secret })
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Asset Hub and a funded collection owner"]
async fn every_purse_call_moves_items_on_the_live_chain() {
	let genesis = live_asset_hub_genesis();
	let chain = live_asset_hub(genesis);
	let owner = CollectionOwner::from_env(chain.clone(), genesis);
	if std::env::var("LIVE_NFT_OWNER_MNEMONIC").is_err() {
		assert_eq!(hex::encode(owner.signer_account()), ALICE_HEX);
	}
	let live = live_purses(&chain, genesis);
	let purses = live_product_purses(&live);
	let collection = owner.create_collection().await;
	let foreign = ItemKey(rand::random());

	let run = LiveRun { owner: &owner, purses: &purses, collection };
	let ran =
		futures::FutureExt::catch_unwind(core::panic::AssertUnwindSafe(run.every_call(foreign)))
			.await;

	let mut every_key = every_item_key(&live.ledger).await;
	every_key.push(foreign);
	owner.clean_up(collection, &every_key).await;
	if let Err(panic) = ran {
		std::panic::resume_unwind(panic);
	}
}

/// Every key the ledger allocated, in this run's purses or as an import's
/// destination.
async fn every_item_key(ledger: &NftLedger) -> Vec<ItemKey> {
	let mut keys = ledger.observe_item_keys();
	keys.next().await.expect("item keys are read").expect("item keys read")
}

/// The idempotency key a revoke or refund of `id` is observed under, as a
/// product derives it from `domain` and the cheque id.
fn derived_key(domain: &[u8], id: &NftPurseChequeId) -> NftPurseIdempotencyKey {
	NftPurseIdempotencyKey(blake2_256(&[domain, &id.0].concat()))
}

/// The product that mints and sends.
fn sender() -> ProductId {
	ProductId::for_test("sender.dot")
}

/// The product that receives.
fn receiver() -> ProductId {
	ProductId::for_test("receiver.dot")
}

/// What the live run goes through its calls with.
struct LiveRun<'a> {
	owner: &'a CollectionOwner,
	purses: &'a NftPurses,
	collection: u32,
}

/// Items one purse lists.
struct ListedItems {
	purse: PurseId,
	keys: Vec<NftPurseItemKey>,
}

impl LiveRun<'_> {
	/// Every purse call a product makes, in the order a product would make them.
	/// The last one transfers an item to `foreign`, outside the purses.
	async fn every_call(&self, foreign: ItemKey) {
		let minted = self.mint_into_new_purse().await;
		let rebalanced = self.rebalance_into_spare_purse(minted).await;
		let into = self.purses.create_purse(&receiver(), "received".into()).await.unwrap();
		self.deposit_ephemeral_cheque(rebalanced.purse, rebalanced.keys[0], into).await;
		self.deposit_static_cheque(rebalanced.purse, rebalanced.keys[1], into).await;
		self.revoke_undelivered_cheque(rebalanced.purse, rebalanced.keys[2]).await;
		self.refund_then_revoke(rebalanced.purse, rebalanced.keys[3], into).await;
		self.transfer_out_of_the_purses(into, foreign).await;
	}

	/// The sender hands `item` of `from` off in a cheque nobody deposits,
	/// revokes it, and waits until `from` lists the reclaimed key.
	async fn revoke_undelivered_cheque(&self, from: PurseId, item: NftPurseItemKey) {
		let cheque = self.send_cheque(from, item, &self.static_destination()).await;
		self.revoke_until_listed(&cheque, from).await;
	}

	/// The receiver deposits a cheque of `item` into `into` and refunds it,
	/// then the sender revokes it and waits until `from` lists the reclaimed
	/// key.
	async fn refund_then_revoke(&self, from: PurseId, item: NftPurseItemKey, into: PurseId) {
		let cheque = self.send_cheque(from, item, &self.static_destination()).await;
		self.deposit_until_cleared(&cheque, into).await;
		let refunded =
			self.purses.refund(&receiver(), &cheque.id).await.expect("the refund starts");
		assert_eq!(refunded.unavailable, vec![]);
		let refund_key = derived_key(b"nft-purse/refund", &cheque.id);
		self.follow_until_cleared(&receiver(), refund_key).await;
		self.revoke_until_listed(&cheque, from).await;
	}

	/// Revokes `cheque` as the sender, follows it until it clears, and waits
	/// until `from` lists the reclaimed key too.
	async fn revoke_until_listed(&self, cheque: &NftPurseCheque, from: PurseId) {
		let mut expected = self.listed_items(from).await;
		self.purses
			.revoke_cheque(&sender(), &cheque.id)
			.await
			.expect("the revoke starts");
		let revoke_key = derived_key(b"nft-purse/revoke", &cheque.id);
		let cleared = self.follow_until_cleared(&sender(), revoke_key).await;
		expected.extend(cleared.iter().map(|item| item.minted));
		self.wait_until_listed(from, expected).await;
	}

	/// A `Static` destination under a fresh cheque id.
	fn static_destination(&self) -> NftPurseChequeDestination {
		let encryption_key = self.purses.static_receive_key().expect("a static receive key");
		let cheque_id = NftPurseChequeId(rand::random());
		NftPurseChequeDestination::Static { encryption_key, cheque_id }
	}

	/// Creates a sender purse with new keys, mints into them, and waits until
	/// the purse lists them.
	async fn mint_into_new_purse(&self) -> ListedItems {
		let purse = self.purses.create_purse(&sender(), "minted".into()).await.unwrap();
		let count = ItemCount::new(ITEM_COUNT).unwrap();
		let keys = self.purses.create_items(purse, count).await.unwrap();
		let item_keys: Vec<_> = keys.iter().map(|key| ItemKey(key.0)).collect();
		self.owner.mint_into(self.collection, &item_keys).await;
		self.wait_until_listed(purse, keys.iter().copied().collect()).await;
		ListedItems { purse, keys }
	}

	/// Rebalances `minted` into a second sender purse, follows it until it
	/// clears, and waits until the second purse lists the new keys.
	async fn rebalance_into_spare_purse(&self, minted: ListedItems) -> ListedItems {
		let purse = self.purses.create_purse(&sender(), "spare".into()).await.unwrap();
		let idempotency_key = NftPurseIdempotencyKey(rand::random());
		let items = minted.keys.iter().map(|key| ItemKey(key.0)).collect();
		let sender = sender();
		let rebalance = self.purses.rebalance(&sender, minted.purse, purse, items, idempotency_key);
		rebalance.await.expect("the rebalance registers");
		let cleared = self.follow_until_cleared(&sender, idempotency_key).await;
		let keys: Vec<_> = cleared.iter().map(|item| item.minted).collect();
		self.wait_until_listed(purse, keys.iter().copied().collect()).await;
		assert_eq!(self.listed_items(minted.purse).await, HashSet::new());
		ListedItems { purse, keys }
	}

	/// The sender hands `item` of `from` to a receive key the receiver made into
	/// `into`, and the receiver deposits the cheque.
	async fn deposit_ephemeral_cheque(&self, from: PurseId, item: NftPurseItemKey, into: PurseId) {
		let receive_key = self.purses.create_receive_key(&receiver(), into).await.unwrap();
		let to = NftPurseChequeDestination::Ephemeral(receive_key);
		let cheque = self.send_cheque(from, item, &to).await;
		self.deposit_until_cleared(&cheque, into).await;
	}

	/// The sender hands `item` of `from` to the host's static receive key, and
	/// the receiver deposits the cheque into `into`.
	async fn deposit_static_cheque(&self, from: PurseId, item: NftPurseItemKey, into: PurseId) {
		let cheque = self.send_cheque(from, item, &self.static_destination()).await;
		self.deposit_until_cleared(&cheque, into).await;
	}

	async fn send_cheque(
		&self,
		from: PurseId,
		item: NftPurseItemKey,
		to: &NftPurseChequeDestination,
	) -> NftPurseCheque {
		let items = [ItemKey(item.0)].into_iter().collect();
		self.purses
			.create_cheque(&sender(), from, to, items)
			.await
			.expect("the cheque is made")
	}

	/// Deposits `cheque` into `into` as the receiver, follows it by the cheque
	/// id until it clears, and waits until `into` lists the new keys too.
	async fn deposit_until_cleared(&self, cheque: &NftPurseCheque, into: PurseId) {
		let mut expected = self.listed_items(into).await;
		self.purses
			.deposit(&receiver(), cheque, into)
			.await
			.expect("the deposit registers");
		let idempotency_key = NftPurseIdempotencyKey(cheque.id.0);
		let cleared = self.follow_until_cleared(&receiver(), idempotency_key).await;
		expected.extend(cleared.iter().map(|item| item.minted));
		self.wait_until_listed(into, expected).await;
	}

	/// Transfers one item of `from` to `foreign` as the receiver, follows it
	/// until it clears, and checks that `foreign` holds an NFT.
	async fn transfer_out_of_the_purses(&self, from: PurseId, foreign: ItemKey) {
		let item = *self.listed_items(from).await.iter().next().expect("a received item");
		let idempotency_key = NftPurseIdempotencyKey(rand::random());
		let destination = NftPurseItemKey(foreign.0);
		let receiver = receiver();
		let transfer = self.purses.transfer(&receiver, from, item, destination, idempotency_key);
		transfer.await.expect("the transfer registers");
		self.follow_until_cleared(&receiver, idempotency_key).await;
		assert_eq!(self.owner.instances_held_by(&[foreign]).await.len(), 1);
	}

	/// The items `caller`'s operation `idempotency_key` cleared, once it is
	/// cleared in a finalized block. Fails the test on any other end.
	async fn follow_until_cleared(
		&self,
		caller: &ProductId,
		idempotency_key: NftPurseIdempotencyKey,
	) -> Vec<NftPurseClearedItem> {
		let mut statuses = self.purses.observe_status(caller, idempotency_key);
		let cleared = async {
			while let Some(read) = statuses.next().await {
				let status = read.expect("the status reads");
				if let Some(NftPurseStatus::Cleared { cleared, .. }) = status {
					return cleared;
				}
				assert!(!matches!(status, Some(NftPurseStatus::Incomplete { .. })), "{status:?}");
			}
			panic!("the operation ended without clearing")
		};
		tokio::time::timeout(WAIT, cleared).await.expect("the operation clears in time")
	}

	async fn listed_items(&self, purse: PurseId) -> HashSet<NftPurseItemKey> {
		let size = PageSize::new(100).unwrap();
		let page = self.purses.query_purse(purse, 0, size).await.expect("the purse reads");
		page.items.into_iter().collect()
	}

	async fn wait_until_listed(&self, purse: PurseId, expected: HashSet<NftPurseItemKey>) {
		wait_for("the purse to list its items", || async {
			(self.listed_items(purse).await == expected).then_some(())
		})
		.await;
	}
}
