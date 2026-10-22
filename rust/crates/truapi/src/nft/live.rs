//! NFT purses against a live Asset Hub. Ignored by default; run with
//! `cargo test -p truapi --lib nft::live -- --ignored --nocapture`.
//!
//! The test creates a collection, mints into keys the ledger allocated,
//! waits for the tracker to see them, rebalances them into another purse
//! through the durable engine, and burns everything and deletes the
//! collection whatever happened. The collection's owner is `//Alice` unless
//! `LIVE_NFT_OWNER_MNEMONIC` names another funded account, which then signs
//! with its root key. `LIVE_ASSET_HUB_WS` and `LIVE_ASSET_HUB_GENESIS` point
//! it at another Asset Hub.

use core::{num::NonZeroU32, time::Duration};
use std::{collections::BTreeSet, sync::Arc};

use futures::StreamExt;
use parity_scale_codec::{Compact, Encode};
use subxt::{
	dynamic::{At, Value, tx},
	utils::{AccountId32, H256},
};
use truapi::latest::TxPayloadExtension;

use super::{
	ItemKey, NftLedger, NftLedgerDeps, OperationKey, PageRequest, PurseId, PurseTag,
	ScarcityTransfersDeps, TransferDestinations, TransferRequest, TransferState,
	testing::{FixedEntropy, FixedInstallation, TEST_INSTALLATION},
	with_scarcity_transfers,
};
use crate::{
	chain::{
		BlockBackend, ChainHeads, DispatchOutcome, EncodedExtrinsic, HashAndNumber, TxSubmitter,
		WatchEvent, WatchedStorage, live_asset_hub, live_asset_hub_genesis, tokio_spawner,
	},
	chain_runtime::ChainRuntime,
	durable::{DurableDeps, DurableRegistry, DurableTxEngine},
	host_internal::extrinsic::{Sr25519Signer, build_signed_extrinsic_v5},
	host_logic::product_account::{derive_root_keypair_from_entropy, derive_sr25519_hard_path},
	nft::HoldingTracker,
	store::open_temporary_core_db,
	time::RealTimer,
};

const DEV_PHRASE: &str = "bottom drive obey lake curtain smoke basket hold race lonely fit walk";
const ALICE_HEX: &str = "d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d";
const ITEM_COUNT: u32 = 2;
const WAIT: Duration = Duration::from_secs(240);
const POLL: Duration = Duration::from_secs(3);

/// The account that owns the test collection and pays for its calls.
struct CollectionOwner {
	chain: ChainRuntime,
	genesis: H256,
	signer: Sr25519Signer,
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
		Self { chain, genesis, signer: Sr25519Signer::from_keypair(&keypair) }
	}

	/// Signs `call` of `Scarcity`, submits it, and waits until a best block
	/// includes it, failing the test unless it dispatched successfully.
	async fn dispatch(&self, call: &str, args: Vec<Value>) {
		let extrinsic = self.signed(call, args).await;
		let at = self.included_at(&extrinsic).await;
		let outcome = self.chain.dispatch_outcome(self.genesis, at, extrinsic.hash()).await;
		assert_eq!(outcome.expect("outcome readable"), Some(DispatchOutcome::Succeeded), "{call}");
	}

	async fn signed(&self, call: &str, args: Vec<Value>) -> EncodedExtrinsic {
		let client = self.chain.client_at_finalized_head(self.genesis).await.expect("client");
		let call_data = client.tx().call_data(&tx("Scarcity", call, args)).expect("call encodes");
		let nonce = self.next_nonce().await;
		let extensions = paying_extensions(
			self.genesis,
			client.spec_version(),
			client.transaction_version(),
			nonce,
		);
		let genesis = self.genesis.0;
		let bytes = build_signed_extrinsic_v5(
			&self.signer,
			genesis,
			&call_data,
			&extensions,
			1,
			client.metadata(),
		)
		.expect("extrinsic builds");
		EncodedExtrinsic::new(bytes)
	}

	async fn next_nonce(&self) -> u64 {
		let legacy = self.chain.legacy_connection(self.genesis.as_bytes()).await.expect("node");
		let account = AccountId32(self.signer_account());
		legacy.methods.system_account_next_index(&account).await.expect("nonce")
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

/// The extensions of an account-signed, fee-paying, immortal transaction.
fn paying_extensions(
	genesis: H256,
	spec: u32,
	tx_version: u32,
	nonce: u64,
) -> Vec<TxPayloadExtension> {
	let nonce = u32::try_from(nonce).expect("nonce fits u32");
	let extension = |id: &str, extra: Vec<u8>, implicit: Vec<u8>| TxPayloadExtension {
		id: id.to_owned(),
		extra,
		additional_signed: implicit,
	};
	vec![
		extension("UnitTransactionExtension", vec![], vec![]),
		extension("AsScarcity", vec![0], vec![]),
		extension("AuthorizeCall", vec![], vec![]),
		extension("AsPgas", vec![0], vec![]),
		extension("AsDotnsGateway", vec![0], vec![]),
		extension("RestrictOrigins", vec![0], vec![]),
		extension("CheckNonZeroSender", vec![], vec![]),
		extension("CheckSpecVersion", vec![], spec.encode()),
		extension("CheckTxVersion", vec![], tx_version.encode()),
		extension("CheckGenesis", vec![], genesis.0.to_vec()),
		extension("CheckMortality", vec![0], genesis.0.to_vec()),
		extension("CheckNonce", Compact(nonce).encode(), vec![]),
		extension("CheckWeight", vec![], vec![]),
		extension("ChargeAssetTxPayment", vec![0, 0], vec![]),
		extension("PrevalidateAttests", vec![], vec![]),
		extension("CheckMetadataHash", vec![0], vec![0]),
		extension("EthSetOrigin", vec![], vec![]),
		extension("StorageWeightReclaim", vec![], vec![]),
	]
}

/// A ledger with its durable engine and holding tracker, all over the live
/// chain, deriving item keys from entropy fresh for this run.
struct LivePurses {
	_dir: tempfile::TempDir,
	ledger: NftLedger,
	_tracker: HoldingTracker,
}

fn live_purses(chain: &ChainRuntime, genesis: H256) -> LivePurses {
	let (dir, db) = open_temporary_core_db();
	let keys = Arc::new(FixedEntropy(rand::random::<[u8; 16]>().to_vec()));
	let storage: Arc<dyn WatchedStorage> = Arc::new(chain.clone());
	let heads = Arc::new(chain.clone());
	let transfers = ScarcityTransfersDeps {
		db: db.clone(),
		storage: storage.clone(),
		heads,
		genesis,
		signer: keys.clone(),
	};
	let engine =
		live_engine(chain, db.clone(), with_scarcity_transfers(DurableRegistry::new(), transfers));
	let installation = Arc::new(FixedInstallation(TEST_INSTALLATION));
	let ledger = NftLedger::new(NftLedgerDeps { db, installation, keys, durable: engine });
	let tracking = super::HoldingTrackerDeps {
		ledger: ledger.clone(),
		storage,
		genesis,
		spawner: tokio_spawner(),
	};
	LivePurses { _dir: dir, ledger, _tracker: HoldingTracker::start(tracking) }
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
		self.dispatch("create_collection", vec![]).await;
		let transferable = Value::unnamed_variant("Transferable", []);
		let args = vec![Value::u128(collection.into()), transferable, Value::unnamed_composite([])];
		self.dispatch("define_item", args).await;
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
			self.dispatch("mint", args).await;
		}
	}

	/// Burns every NFT `keys` hold now, then deletes the item definition and
	/// the collection. Runs whatever the test did, so each step only logs.
	async fn clean_up(&self, collection: u32, keys: &[ItemKey]) {
		for instance in self.instances_held_by(keys).await {
			self.dispatch("force_burn", vec![Value::u128(instance.into())]).await;
		}
		self.dispatch("delete_item", vec![Value::u128(collection.into()), Value::u128(0)])
			.await;
		self.dispatch("delete_collection", vec![Value::u128(collection.into())]).await;
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

async fn available(ledger: &NftLedger, purse: PurseId) -> BTreeSet<ItemKey> {
	let page = PageRequest { index: 0, size: NonZeroU32::new(100).unwrap() };
	ledger
		.available_items(purse, page)
		.await
		.expect("page reads")
		.items
		.into_iter()
		.collect()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Asset Hub and a funded collection owner"]
async fn items_minted_into_host_keys_rebalance_into_another_purse() {
	let genesis = live_asset_hub_genesis();
	let chain = live_asset_hub(genesis);
	let owner = CollectionOwner::from_env(chain.clone(), genesis);
	if std::env::var("LIVE_NFT_OWNER_MNEMONIC").is_err() {
		assert_eq!(hex::encode(owner.signer_account()), ALICE_HEX);
	}
	let purses = live_purses(&chain, genesis);
	let from = purses.ledger.create_purse("from".into(), PurseTag::default()).await.unwrap();
	let to = purses.ledger.create_purse("to".into(), PurseTag::default()).await.unwrap();
	let count = NonZeroU32::new(ITEM_COUNT).unwrap();
	let items = purses.ledger.allocate_items(from, count).await.unwrap();
	let collection = owner.create_collection().await;

	let transferred = futures::FutureExt::catch_unwind(core::panic::AssertUnwindSafe(
		rebalance_round_trip(&owner, &purses.ledger, collection, (from, to), &items),
	))
	.await;

	let mut every_key = items.clone();
	every_key.extend(transferred.as_ref().map(|minted| minted.clone()).unwrap_or_default());
	owner.clean_up(collection, &every_key).await;
	if let Err(panic) = transferred {
		std::panic::resume_unwind(panic);
	}
}

/// Mints into `items`, waits for the tracker to list them, rebalances them
/// from the first purse into the second, waits for every transfer to finalize
/// and for the second purse to list the new keys. Returns those keys.
async fn rebalance_round_trip(
	owner: &CollectionOwner,
	ledger: &NftLedger,
	collection: u32,
	(from, to): (PurseId, PurseId),
	items: &[ItemKey],
) -> Vec<ItemKey> {
	owner.mint_into(collection, items).await;
	let expected: BTreeSet<_> = items.iter().copied().collect();
	wait_for("the minted items", || async {
		(available(ledger, from).await == expected).then_some(())
	})
	.await;

	let operation = OperationKey::new(b"live", rand::random());
	let request = TransferRequest {
		operation: operation.clone(),
		from,
		items: expected,
		destinations: TransferDestinations::AllocatedIn(to),
	};
	ledger.start_transfers(request).await.expect("transfers register");
	let minted = wait_for_finalized_transfers(ledger, &operation).await;

	let landed: BTreeSet<_> = minted.iter().copied().collect();
	wait_for("the transferred items", || async {
		(available(ledger, to).await == landed).then_some(())
	})
	.await;
	assert_eq!(available(ledger, from).await, BTreeSet::new());
	minted
}

/// The destination of every transfer of `operation`, once each finalized.
async fn wait_for_finalized_transfers(
	ledger: &NftLedger,
	operation: &OperationKey,
) -> Vec<ItemKey> {
	let mut transfers = ledger.observe_operation(operation);
	let finalized = async {
		while let Some(read) = transfers.next().await {
			let progress = read.expect("transfers read");
			assert!(
				progress.iter().all(|step| step.state != TransferState::Failed),
				"{progress:?}"
			);
			if progress
				.iter()
				.all(|step| step.state == TransferState::Landed { finalized: true })
			{
				return progress.into_iter().map(|step| step.minted).collect();
			}
		}
		panic!("the operation stopped being observed")
	};
	tokio::time::timeout(WAIT, finalized)
		.await
		.expect("every transfer finalizes in time")
}
