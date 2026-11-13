use futures::executor::block_on;
use subxt::{ext::frame_decode::extrinsics::decode_extrinsic, utils::H256};
use truapi::latest::GenericError;

use super::*;
use crate::{
	chain::{
		DEFAULT_MORTALITY_PERIOD, HashAndNumber, Mortality, StorageWatchError,
		extrinsic_service_serving,
	},
	durable::{DomainId, SubmissionPlan},
	encrypted_storage::{EncryptedStorage, MemoryEncryptedStorage},
	nft::{
		PurseId, item_keys_from_entropy,
		keys::ItemIndex,
		scarcity::{
			chain::TransferContext,
			testing::{ASSET_HUB_METADATA_BYTES, ASSET_HUB_PIPELINE, asset_hub_metadata},
		},
		testing::{FixedEntropy, TEST_ENTROPY, TEST_INSTALLATION, imported_test_key},
	},
	runtime::AuthorityError,
	unix_time::current_unix_secs,
};

const FINALIZED: HashAndNumber = HashAndNumber { hash: H256([0x0f; 32]), number: 2_138_796 };
/// The best head, ahead of [`FINALIZED`]: an NFT minted there is held only here.
const BEST: HashAndNumber = HashAndNumber { hash: H256([0x0b; 32]), number: 2_138_800 };
const DESTINATION: ItemKey = ItemKey([0x08; 32]);
const INSTANCE: u64 = 330;
const NOW: u64 = 1_700_000_000;
/// The genesis the scripted node's chain is configured with.
const GENESIS: H256 = H256([0xab; 32]);

/// Builds transfers over a scripted node serving the Asset Hub fixture.
fn transfer_extrinsics() -> Arc<ExtrinsicService> {
	Arc::new(extrinsic_service_serving(ASSET_HUB_METADATA_BYTES, ASSET_HUB_PIPELINE))
}

/// Scarcity storage a test writes by hand, held at [`BEST`] only.
#[derive(Default)]
struct ScriptedScarcity {
	held: HashMap<ItemKey, ScarcityNft>,
	locked_until: HashMap<ItemKey, u64>,
}

#[async_trait::async_trait]
impl ScarcityChain for ScriptedScarcity {
	async fn nfts_at(
		&self,
		at: H256,
		keys: &[ItemKey],
	) -> Result<Vec<Option<ScarcityNft>>, StorageWatchError> {
		let held = |key: &ItemKey| self.held.get(key).copied().filter(|_| at == BEST.hash);
		Ok(keys.iter().map(held).collect())
	}

	async fn locks_at(&self, at: H256, keys: &[ItemKey]) -> Result<KeyLocks, StorageWatchError> {
		assert_eq!(at, BEST.hash);
		let until = keys.iter().map(|key| self.locked_until.get(key).copied()).collect();
		Ok(KeyLocks { now: NOW, until })
	}

	async fn transfer_context(&self) -> Result<TransferContext, StorageWatchError> {
		Ok(TransferContext { birth: FINALIZED, best: BEST })
	}
}

/// What the source key holds when the transfer is still possible.
fn held_nft() -> ScarcityNft {
	ScarcityNft {
		instance: INSTANCE,
		collection: 27,
		item: 13,
		minted_at: 0,
		last_moved: 0,
		state_nonce: 3,
		moves: 0,
	}
}

/// A scheduled transfer of the first key of purse 0 to [`DESTINATION`], as
/// the ledger registers it.
struct RegisteredTransfer {
	source: ItemKey,
	scheduled: ScheduledDurableTx,
}

fn registered_transfer(attempts: u32) -> RegisteredTransfer {
	registered_transfer_until(attempts, current_unix_secs() + TRANSFER_WAIT_LIMIT_SECS)
}

/// [`registered_transfer`] that stops waiting at unix time `give_up_after`.
fn registered_transfer_until(attempts: u32, give_up_after: u64) -> RegisteredTransfer {
	let slot = ItemSlot { purse: PurseId(0), installation: TEST_INSTALLATION, index: ItemIndex(0) };
	let source = item_keys_from_entropy(&TEST_ENTROPY, &[slot]).unwrap()[0];
	let params = TransferParams {
		source,
		signer: TransferSigner::Derived(slot),
		destination: DESTINATION,
		submitted_attempt: None,
		give_up_after,
	};
	scheduled_transfer(params, attempts)
}

/// [`registered_transfer`] after one attempt that moved [`INSTANCE`].
fn retried_transfer() -> RegisteredTransfer {
	let mut registered = registered_transfer(1);
	let mut params = TransferParams::try_from(&registered.scheduled.plan.params).unwrap();
	let state_nonce = held_nft().state_nonce;
	params.submitted_attempt = Some(TransferAttempt { instance: INSTANCE, state_nonce });
	registered.scheduled.plan.params = params.into();
	registered
}

/// A scheduled import of [`imported_test_key`] 0 to [`DESTINATION`], as the
/// ledger registers it, that stops waiting at unix time `give_up_after`.
fn registered_import_until(give_up_after: u64) -> RegisteredTransfer {
	let params = TransferParams {
		source: imported_test_key(0).key(),
		signer: TransferSigner::Imported,
		destination: DESTINATION,
		submitted_attempt: None,
		give_up_after,
	};
	scheduled_transfer(params, 0)
}

fn scheduled_transfer(params: TransferParams, attempts: u32) -> RegisteredTransfer {
	let source = params.source;
	let plan = SubmissionPlan { params: params.into() };
	let id = DurableTxId(1);
	let scheduled =
		ScheduledDurableTx { id, domain: DomainId::new("nft"), group: None, plan, attempts };
	RegisteredTransfer { source, scheduled }
}

fn policy(chain: ScriptedScarcity) -> ScarcityTransferPolicy {
	policy_importing_from(chain, Arc::new(MemoryEncryptedStorage::default()))
}

/// A policy that reads imported secrets from `encrypted`.
fn policy_importing_from(
	chain: ScriptedScarcity,
	encrypted: Arc<dyn EncryptedStorage>,
) -> ScarcityTransferPolicy {
	ScarcityTransferPolicy::new(ScarcityTransferPolicyDeps {
		chain: Arc::new(chain),
		signer: Arc::new(FixedEntropy(TEST_ENTROPY.to_vec())),
		imported: ImportedSecretStore::new(encrypted),
		extrinsics: transfer_extrinsics(),
		genesis: GENESIS,
	})
}

/// Encrypted storage holding the secret of [`imported_test_key`] 0.
fn storage_with_imported_secret() -> Arc<dyn EncryptedStorage> {
	let encrypted = Arc::new(MemoryEncryptedStorage::default());
	let store = ImportedSecretStore::new(encrypted.clone());
	block_on(store.store_secrets(&[imported_test_key(0)])).unwrap();
	encrypted
}

fn prepare_import(
	registered: &RegisteredTransfer,
	chain: ScriptedScarcity,
	encrypted: Arc<dyn EncryptedStorage>,
) -> Option<Preparation> {
	let batch = [registered.scheduled.clone()];
	let policy = policy_importing_from(chain, encrypted);
	block_on(policy.prepare(&batch)).unwrap().remove(&registered.scheduled.id)
}

fn holding(registered: &RegisteredTransfer, nft: ScarcityNft) -> ScriptedScarcity {
	let held = HashMap::from([(registered.source, nft)]);
	ScriptedScarcity { held, ..ScriptedScarcity::default() }
}

fn prepare_one(registered: &RegisteredTransfer, chain: ScriptedScarcity) -> Option<Preparation> {
	let batch = [registered.scheduled.clone()];
	let mut prepared = block_on(policy(chain).prepare(&batch)).unwrap();
	prepared.remove(&registered.scheduled.id)
}

/// An item is available once the best block holds it, so a transfer is judged
/// there: an NFT minted at the best head and not finalized yet is built, its
/// era born at the finalized head.
#[test]
fn an_nft_held_at_the_best_head_is_built_born_at_the_finalized_head() {
	let registered = registered_transfer(0);

	let prepared = prepare_one(&registered, holding(&registered, held_nft()));

	let Some(Preparation::Ready { extrinsic: signed, .. }) = prepared else {
		panic!("built: {prepared:?}")
	};
	assert_eq!(signed.mortality, Mortality::new(FINALIZED, DEFAULT_MORTALITY_PERIOD).unwrap());
}

/// The chain only accepts a transfer signed by the key that holds the NFT,
/// so the build signs with the key the params name, derived again at the
/// slot registration recorded.
#[test]
fn a_transfer_is_signed_by_the_key_it_spends() {
	let registered = registered_transfer(0);
	let metadata = asset_hub_metadata();

	let prepared = prepare_one(&registered, holding(&registered, held_nft()));

	let Some(Preparation::Ready { extrinsic: signed, .. }) = prepared else {
		panic!("built: {prepared:?}")
	};
	let bytes = signed.extrinsic.bytes();
	let decoded = decode_extrinsic(&mut &bytes[..], &*metadata, metadata.types()).unwrap();
	let extensions = decoded.transaction_extension_payload().unwrap();
	let verify = extensions
		.iter()
		.find(|extension| extension.name() == "VerifyMultiSignature")
		.unwrap();
	assert!(bytes[verify.range()].ends_with(&registered.source.0));
}

/// The oracle proves a landing by the revision the live attempt signed,
/// so every build records it in the transfer's params.
#[test]
fn a_built_transfer_records_the_revision_it_signed() {
	let registered = registered_transfer(0);

	let prepared = prepare_one(&registered, holding(&registered, held_nft()));

	let Some(Preparation::Ready { params: Some(params), .. }) = prepared else {
		panic!("built without params: {prepared:?}")
	};
	let recorded = TransferParams::try_from(&params).unwrap().submitted_attempt;
	let state_nonce = held_nft().state_nonce;
	assert_eq!(recorded, Some(TransferAttempt { instance: INSTANCE, state_nonce }));
}

/// Products name items by key, so a first attempt moves whatever the key
/// holds, even when our records last saw another instance there.
#[test]
fn a_first_attempt_moves_whatever_instance_its_key_holds() {
	let registered = registered_transfer(0);
	let other = ScarcityNft { instance: INSTANCE + 1, ..held_nft() };

	let prepared = prepare_one(&registered, holding(&registered, other));

	let Some(Preparation::Ready { params: Some(params), .. }) = prepared else {
		panic!("built without params: {prepared:?}")
	};
	let recorded = TransferParams::try_from(&params).unwrap().submitted_attempt;
	let state_nonce = other.state_nonce;
	assert_eq!(recorded, Some(TransferAttempt { instance: INSTANCE + 1, state_nonce }));
}

/// A reorg can briefly show the source key without its NFT, and the oracle
/// proves a landing by the attempted instance, so a retry waits for that
/// instance to come back rather than moving another or failing.
#[test]
fn a_retry_waits_while_its_key_does_not_hold_the_attempted_instance() {
	let registered = retried_transfer();
	let moved_on = ScarcityNft { instance: INSTANCE + 1, ..held_nft() };

	let gone = prepare_one(&registered, ScriptedScarcity::default());
	let replaced = prepare_one(&registered, holding(&registered, moved_on));

	assert_eq!(gone, None);
	assert_eq!(replaced, None);
}

/// Waiting is bounded, so an NFT that is gone for good does not keep its
/// item in use forever.
#[test]
fn a_transfer_still_waiting_past_its_limit_is_given_up() {
	let expired = registered_transfer_until(0, current_unix_secs() - 1);
	let mut locked = holding(&expired, held_nft());
	locked.locked_until.insert(expired.source, NOW + 1);

	let without_nft = prepare_one(&expired, ScriptedScarcity::default());
	let while_locked = prepare_one(&expired, locked);

	assert_eq!(without_nft, Some(Preparation::GiveUp));
	assert_eq!(while_locked, Some(Preparation::GiveUp));
}

/// The limit only bounds waiting: a transfer the chain allows is still built.
#[test]
fn a_transfer_past_its_limit_is_still_built_when_nothing_holds_it_back() {
	let expired = registered_transfer_until(0, current_unix_secs() - 1);

	let prepared = prepare_one(&expired, holding(&expired, held_nft()));

	assert!(matches!(prepared, Some(Preparation::Ready { .. })));
}

#[test]
fn an_occupied_destination_is_given_up() {
	let registered = registered_transfer(0);
	let mut chain = holding(&registered, held_nft());
	chain.held.insert(DESTINATION, held_nft());

	let prepared = prepare_one(&registered, chain);

	assert_eq!(prepared, Some(Preparation::GiveUp));
}

#[test]
fn a_locked_key_waits_until_its_lock_ends() {
	let registered = registered_transfer(0);
	let locked = |until| {
		let mut chain = holding(&registered, held_nft());
		chain.locked_until.insert(registered.source, until);
		chain
	};

	let while_locked = prepare_one(&registered, locked(NOW + 1));
	let after_lock = prepare_one(&registered, locked(NOW));

	assert_eq!(while_locked, None);
	assert!(matches!(after_lock, Some(Preparation::Ready { .. })));
}

#[test]
fn a_transfer_is_given_up_after_its_last_attempt() {
	let registered = registered_transfer(MAX_ATTEMPTS);

	let prepared = prepare_one(&registered, holding(&registered, held_nft()));

	assert_eq!(prepared, Some(Preparation::GiveUp));
}

/// Without a session nothing can be signed, so the transfer waits like any
/// other, and its wait limit still frees the item if the user never returns.
#[test]
fn without_a_session_a_transfer_waits_until_its_limit() {
	let waiting = registered_transfer(0);
	let expired = registered_transfer_until(0, current_unix_secs() - 1);
	let signed_out = |registered: &RegisteredTransfer| {
		ScarcityTransferPolicy::new(ScarcityTransferPolicyDeps {
			chain: Arc::new(holding(registered, held_nft())),
			signer: Arc::new(SignedOut),
			imported: ImportedSecretStore::new(Arc::new(MemoryEncryptedStorage::default())),
			extrinsics: transfer_extrinsics(),
			genesis: GENESIS,
		})
	};
	let prepare = |registered: &RegisteredTransfer| {
		let batch = core::slice::from_ref(&registered.scheduled);
		block_on(signed_out(registered).prepare(batch))
			.unwrap()
			.remove(&registered.scheduled.id)
	};

	assert_eq!(prepare(&waiting), None);
	assert_eq!(prepare(&expired), Some(Preparation::GiveUp));
}

/// A cheque's sender may still be minting or moving the NFT, so an import
/// waits for its key to hold one, takes whichever instance it finds, and
/// records it so the oracle can prove the landing.
#[test]
fn an_import_waits_for_its_key_to_fill_and_records_the_instance_it_signed() {
	let import = registered_import_until(current_unix_secs() + 60);

	let empty =
		prepare_import(&import, ScriptedScarcity::default(), storage_with_imported_secret());
	let filled =
		prepare_import(&import, holding(&import, held_nft()), storage_with_imported_secret());

	assert_eq!(empty, None);
	let Some(Preparation::Ready { params: Some(params), .. }) = filled else {
		panic!("built without params: {filled:?}")
	};
	let registered = TransferParams::try_from(&import.scheduled.plan.params).unwrap();
	let state_nonce = held_nft().state_nonce;
	let submitted_attempt = Some(TransferAttempt { instance: INSTANCE, state_nonce });
	let expected = TransferParams { submitted_attempt, ..registered };
	assert_eq!(TransferParams::try_from(&params).unwrap(), expected);
}

/// The chain only accepts a transfer signed by the key that holds the NFT,
/// so an import signs with the stored secret of its source key.
#[test]
fn an_import_is_signed_with_the_stored_secret_of_its_key() {
	let import = registered_import_until(current_unix_secs() + 60);
	let metadata = asset_hub_metadata();

	let prepared =
		prepare_import(&import, holding(&import, held_nft()), storage_with_imported_secret());

	let Some(Preparation::Ready { extrinsic: signed, .. }) = prepared else {
		panic!("built: {prepared:?}")
	};
	let bytes = signed.extrinsic.bytes();
	let decoded = decode_extrinsic(&mut &bytes[..], &*metadata, metadata.types()).unwrap();
	let extensions = decoded.transaction_extension_payload().unwrap();
	let verify = extensions
		.iter()
		.find(|extension| extension.name() == "VerifyMultiSignature")
		.unwrap();
	assert!(bytes[verify.range()].ends_with(&import.source.0));
}

/// A secret that is not stored never will be, so waiting for it would only
/// hold the operation open for an hour.
#[test]
fn an_import_without_a_stored_secret_is_given_up() {
	let import = registered_import_until(current_unix_secs() + 60);
	let nothing_stored = Arc::new(MemoryEncryptedStorage::default());

	let prepared = prepare_import(&import, holding(&import, held_nft()), nothing_stored);

	assert_eq!(prepared, Some(Preparation::GiveUp));
}

/// A store that failed to answer may answer next time, so the import
/// waits, and its wait limit still ends it.
#[test]
fn an_import_whose_secret_cannot_be_read_waits_until_its_limit() {
	let waiting = registered_import_until(current_unix_secs() + 60);
	let expired = registered_import_until(current_unix_secs() - 1);

	let before_limit =
		prepare_import(&waiting, holding(&waiting, held_nft()), Arc::new(UnreadableStorage));
	let after_limit =
		prepare_import(&expired, holding(&expired, held_nft()), Arc::new(UnreadableStorage));

	assert_eq!(before_limit, None);
	assert_eq!(after_limit, Some(Preparation::GiveUp));
}

/// Encrypted storage that fails every read.
struct UnreadableStorage;

#[async_trait::async_trait]
impl EncryptedStorage for UnreadableStorage {
	async fn read_encrypted(
		&self,
		_: &[u8],
	) -> Result<Option<zeroize::Zeroizing<Vec<u8>>>, GenericError> {
		Err(GenericError { reason: "the store is locked".into() })
	}

	async fn write_encrypted(&self, _: &[u8], _: &[u8]) -> Result<(), GenericError> {
		Err(GenericError { reason: "the store is locked".into() })
	}

	async fn delete_encrypted(&self, _: &[u8]) -> Result<(), GenericError> {
		Err(GenericError { reason: "the store is locked".into() })
	}
}

/// A signer with no session.
struct SignedOut;

impl ItemSigner for SignedOut {
	fn item_signer(
		&self,
		_: ItemSlot,
	) -> Result<crate::host_internal::extrinsic::Sr25519Signer, AuthorityError> {
		Err(AuthorityError::Disconnected)
	}
}
