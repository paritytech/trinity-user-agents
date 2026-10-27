//! [`ScarcityTransferPolicy`]: builds each NFT transfer when the engine submits it,
//! from chain state read fresh at the best head.

use std::{collections::HashMap, sync::Arc};

use parity_scale_codec::{Decode, Encode};
use subxt::utils::H256;

use super::{
	chain::{KeyLocks, ScarcityChain, ScarcityNft, TransferContext},
	transfer::NftTransfer,
};
use crate::{
	chain::{ExtrinsicService, MortalExtrinsic},
	durable::{
		DurableTxEntry, DurableTxId, PolicyError, PolicyId, PolicyParams, Preparation,
		ScheduledDurableTx, SubmissionPolicy, VerdictFailureKind,
	},
	host_internal::extrinsic::Sr25519Signer,
	nft::{ImportedSecretStore, ItemKey, ItemSigner, ItemSlot},
	unix_time::current_unix_secs,
};

/// The name rows store for this policy.
pub const TRANSFER_POLICY: &str = "nft.scarcity.transfer";

/// Attempts after which a transfer is given up, so a rejection that repeats
/// cannot loop forever.
const MAX_ATTEMPTS: u32 = 5;

/// How long a registered transfer may keep waiting for its NFT or for a lock to
/// end before it is given up, so its item never stays in use for good.
pub const TRANSFER_WAIT_LIMIT_SECS: u64 = 30 * 60;

/// Who signs a transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub enum TransferSigner {
	/// A key the host derives, at this slot.
	Derived(ItemSlot),
	/// A key whose secret is in the imported secret store.
	Imported,
}

/// What the policy stores with each transfer: the key it spends and where
/// that key sits, the instance it transfers, the key it goes to, and when it
/// stops waiting. Everything a build needs, so it reads no table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub struct TransferParams {
	/// The key the NFT leaves, which signs the transfer.
	pub source: ItemKey,
	/// How the secret of `source` is found.
	pub signer: TransferSigner,
	/// The instance the transfer moves. `None` for an import until its source
	/// is first seen holding one; set when an attempt is built.
	pub instance: Option<u64>,
	/// The key it goes to.
	pub destination: ItemKey,
	/// The `state_nonce` the live attempt signed over, which the oracle
	/// proves its landing by. `None` until an attempt is built.
	pub signed_state_nonce: Option<u64>,
	/// Unix time, in seconds of the host's clock, after which a transfer that
	/// still waits is given up. Fixed at registration, so a restart never
	/// extends it.
	pub give_up_after: u64,
}

impl From<TransferParams> for PolicyParams {
	fn from(params: TransferParams) -> Self {
		PolicyParams(params.encode())
	}
}

impl TryFrom<&PolicyParams> for TransferParams {
	type Error = parity_scale_codec::Error;

	fn try_from(params: &PolicyParams) -> Result<Self, Self::Error> {
		Self::decode(&mut &params.0[..])
	}
}

/// What the policy runs on.
pub struct ScarcityTransferPolicyDeps {
	/// Scarcity storage, read fresh for every batch.
	pub chain: Arc<dyn ScarcityChain>,
	/// Signs with the derived keys transfers spend.
	pub signer: Arc<dyn ItemSigner>,
	/// Holds the secrets of the imported keys transfers spend.
	pub imported: ImportedSecretStore,
	/// Builds each transfer.
	pub extrinsics: Arc<ExtrinsicService>,
	/// Genesis of the chain that runs the pallet.
	pub genesis: H256,
}

/// Builds NFT transfers: one fee-less `Scarcity::transfer` per spent key.
pub struct ScarcityTransferPolicy {
	deps: ScarcityTransferPolicyDeps,
}

impl ScarcityTransferPolicy {
	/// A policy over `deps`.
	pub fn new(deps: ScarcityTransferPolicyDeps) -> Self {
		Self { deps }
	}
}

/// One scheduled transfer with everything needed to judge and build it.
struct PlannedTransfer {
	id: DurableTxId,
	attempts: u32,
	params: TransferParams,
}

/// What the chain says about one planned transfer at the best head.
#[derive(Clone, Copy)]
struct TransferView {
	/// What the source key holds.
	source: Option<ScarcityNft>,
	/// What the destination key holds.
	destination: Option<ScarcityNft>,
	/// Whether a failed transfer locked the source key.
	locked: bool,
}

/// What every transfer of one batch is built with.
struct BatchBuild {
	/// Builds every transfer of the batch born at the same finalized head.
	extrinsics: ExtrinsicService,
	/// Unix time of the host's clock, which wait limits are measured in.
	now: u64,
}

impl BatchBuild {
	/// The build of transfers born at the finalized head of `context`, through
	/// `extrinsics` with the runtime there cached.
	async fn born_at(
		extrinsics: &ExtrinsicService,
		genesis: H256,
		context: TransferContext,
	) -> Result<Self, PolicyError> {
		let extrinsics = extrinsics
			.cache_runtime_at(genesis, context.birth)
			.await
			.map_err(policy_error)?;
		Ok(Self { extrinsics, now: current_unix_secs() })
	}
}

/// What the chain says to do with one transfer this time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransferDecision {
	/// It can never land.
	GiveUp,
	/// Its NFT is not at the source, or the source is locked. It waits and
	/// is asked about again.
	Wait,
	/// Build this transfer of the instance the source holds, at its revision now.
	Build(NftTransfer),
}

/// What signing a planned transfer came to.
enum SignedTransfer {
	/// Ready to submit.
	Ready(Preparation),
	/// The signer is unavailable for now, as when signed out or when a store
	/// read failed. The transfer waits.
	Unavailable,
	/// The transfer can never be signed, because no imported secret is stored
	/// for its source. It is given up.
	Unsignable,
}

#[async_trait::async_trait]
impl SubmissionPolicy for ScarcityTransferPolicy {
	fn id(&self) -> PolicyId {
		PolicyId::new(TRANSFER_POLICY)
	}

	/// Every proven failure is built again: the next build reads the chain
	/// afresh and gives up once the transfer can never land.
	fn can_retry(&self, _: &DurableTxEntry, _: &PolicyParams, _: VerdictFailureKind) -> bool {
		true
	}

	async fn prepare(
		&self,
		batch: &[ScheduledDurableTx],
	) -> Result<HashMap<DurableTxId, Preparation>, PolicyError> {
		let planned = planned_transfers(batch);
		let context = self.deps.chain.transfer_context().await.map_err(policy_error)?;
		let views = self.read_transfer_views(context.best.hash, &planned).await?;
		let build = BatchBuild::born_at(&self.deps.extrinsics, self.deps.genesis, context).await?;
		let mut prepared = HashMap::new();
		for (transfer, view) in planned.iter().zip(views) {
			if let Some(answer) = self.preparation_of(transfer, view, &build).await {
				prepared.insert(transfer.id, answer);
			}
		}
		Ok(prepared)
	}
}

impl ScarcityTransferPolicy {
	/// What the chain holds at `at` for each of `planned`, in order.
	async fn read_transfer_views(
		&self,
		at: H256,
		planned: &[PlannedTransfer],
	) -> Result<Vec<TransferView>, PolicyError> {
		let sources: Vec<ItemKey> = planned.iter().map(|planned| planned.params.source).collect();
		let destinations: Vec<ItemKey> =
			planned.iter().map(|planned| planned.params.destination).collect();
		let chain = &self.deps.chain;
		let (sources_held, destinations_held, locks) = futures::future::try_join3(
			chain.nfts_at(at, &sources),
			chain.nfts_at(at, &destinations),
			chain.locks_at(at, &sources),
		)
		.await
		.map_err(policy_error)?;
		Ok(zip_views(sources_held, destinations_held, locks))
	}

	/// The answer for `transfer`, or `None` while it waits. A transfer that
	/// cannot be signed yet, as without a session, waits like any other, so
	/// its wait limit still applies.
	async fn preparation_of(
		&self,
		transfer: &PlannedTransfer,
		view: TransferView,
		build: &BatchBuild,
	) -> Option<Preparation> {
		let nft_transfer = match decide_from_chain(transfer, view) {
			TransferDecision::GiveUp => return Some(Preparation::GiveUp),
			TransferDecision::Wait => return give_up_after_limit(transfer, build.now),
			TransferDecision::Build(nft_transfer) => nft_transfer,
		};
		match self.ready_to_submit(transfer, &nft_transfer, build).await {
			SignedTransfer::Ready(ready) => Some(ready),
			SignedTransfer::Unsignable => Some(Preparation::GiveUp),
			SignedTransfer::Unavailable => give_up_after_limit(transfer, build.now),
		}
	}

	/// What signing `nft_transfer` for `transfer` came to: ready, with the
	/// instance and revision it signed recorded in its params, or why not.
	async fn ready_to_submit(
		&self,
		transfer: &PlannedTransfer,
		nft_transfer: &NftTransfer,
		build: &BatchBuild,
	) -> SignedTransfer {
		let signer = match self.signer_for(&transfer.params).await {
			Ok(Some(signer)) => signer,
			Ok(None) => return SignedTransfer::Unsignable,
			Err(error) => return transfer_not_signed(error),
		};
		build
			.extrinsics
			.sign_nft_transfer(self.deps.genesis, &signer, nft_transfer)
			.await
			.map(|extrinsic| ready_preparation(transfer, nft_transfer, extrinsic))
			.map_or_else(transfer_not_signed, SignedTransfer::Ready)
	}

	/// The signer of the source key of `params`, or `None` when it is an
	/// imported key whose secret is not stored. Fails without a session or
	/// when the store cannot be read.
	async fn signer_for(
		&self,
		params: &TransferParams,
	) -> Result<Option<Sr25519Signer>, PolicyError> {
		let TransferSigner::Derived(slot) = params.signer else {
			return self.imported_signer(&params.source).await;
		};
		self.deps.signer.item_signer(slot).map(Some).map_err(policy_error)
	}

	/// The signer of the imported key `source`, or `None` when its secret is
	/// not stored.
	async fn imported_signer(
		&self,
		source: &ItemKey,
	) -> Result<Option<Sr25519Signer>, PolicyError> {
		let secret = self.deps.imported.secret_of(source).await.map_err(policy_error)?;
		secret
			.map(|secret| Sr25519Signer::from_secret_bytes(secret.as_bytes()))
			.transpose()
			.map_err(policy_error)
	}
}

/// `transfer` ready to submit as `extrinsic`, with the instance and revision
/// `nft_transfer` signed recorded in its params.
fn ready_preparation(
	transfer: &PlannedTransfer,
	nft_transfer: &NftTransfer,
	extrinsic: MortalExtrinsic,
) -> Preparation {
	let params = TransferParams {
		instance: Some(nft_transfer.instance),
		signed_state_nonce: Some(nft_transfer.state_nonce),
		..transfer.params
	};
	Preparation::Ready { extrinsic, params: Some(params.into()) }
}

/// [`SignedTransfer::Unavailable`], after logging the `error` that kept the
/// transfer from being signed.
fn transfer_not_signed(error: impl core::fmt::Display) -> SignedTransfer {
	tracing::warn!(%error, "NFT transfer not signed");
	SignedTransfer::Unavailable
}

/// The transfers of `batch` whose params decode. Only registration writes
/// these params, so a row that does not decode is left out rather than
/// answered.
fn planned_transfers(batch: &[ScheduledDurableTx]) -> Vec<PlannedTransfer> {
	batch.iter().filter_map(planned_transfer).collect()
}

/// `scheduled` as a transfer, or `None` when its params do not decode.
fn planned_transfer(scheduled: &ScheduledDurableTx) -> Option<PlannedTransfer> {
	TransferParams::try_from(&scheduled.plan.params)
		.ok()
		.map(|params| PlannedTransfer { id: scheduled.id, attempts: scheduled.attempts, params })
}

/// One view per transfer from the three reads, which answer in the same order.
fn zip_views(
	sources: Vec<Option<ScarcityNft>>,
	destinations: Vec<Option<ScarcityNft>>,
	locks: KeyLocks,
) -> Vec<TransferView> {
	let KeyLocks { now, until } = locks;
	let locked = until.into_iter().map(move |until| until.is_some_and(|until| until > now));
	sources
		.into_iter()
		.zip(destinations)
		.zip(locked)
		.map(|((source, destination), locked)| TransferView { source, destination, locked })
		.collect()
}

/// [`Preparation::GiveUp`] once `transfer` waited past its limit, at unix
/// time `now`.
fn give_up_after_limit(transfer: &PlannedTransfer, now: u64) -> Option<Preparation> {
	(now >= transfer.params.give_up_after).then_some(Preparation::GiveUp)
}

/// What the chain says about `transfer`. A source that does not hold the
/// instance waits, since a reorg can put it back, and a transfer that names no
/// instance yet waits for its source to hold any. An occupied destination
/// fails at once.
fn decide_from_chain(transfer: &PlannedTransfer, view: TransferView) -> TransferDecision {
	if transfer.attempts >= MAX_ATTEMPTS || view.destination.is_some() {
		return TransferDecision::GiveUp;
	}
	let expected = transfer.params.instance;
	let is_expected =
		|held: &ScarcityNft| expected.is_none_or(|instance| held.instance == instance);
	let held = view.source.filter(is_expected);
	let Some(held) = held else { return TransferDecision::Wait };
	if view.locked {
		return TransferDecision::Wait;
	}
	let ScarcityNft { instance, state_nonce, .. } = held;
	let destination = transfer.params.destination;
	TransferDecision::Build(NftTransfer { instance, state_nonce, destination })
}

fn policy_error(error: impl core::fmt::Display) -> PolicyError {
	PolicyError { reason: error.to_string() }
}

#[cfg(test)]
mod tests {
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
	const NOW: u64 = 1_700_000_000_000;
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

		async fn locks_at(
			&self,
			at: H256,
			keys: &[ItemKey],
		) -> Result<KeyLocks, StorageWatchError> {
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
		let slot =
			ItemSlot { purse: PurseId(0), installation: TEST_INSTALLATION, index: ItemIndex(0) };
		let source = item_keys_from_entropy(&TEST_ENTROPY, &[slot]).unwrap()[0];
		let params = TransferParams {
			source,
			signer: TransferSigner::Derived(slot),
			instance: Some(INSTANCE),
			destination: DESTINATION,
			signed_state_nonce: None,
			give_up_after,
		};
		scheduled_transfer(params, attempts)
	}

	/// A scheduled import of [`imported_test_key`] 0 to [`DESTINATION`], as the
	/// ledger registers it, that stops waiting at unix time `give_up_after`.
	fn registered_import_until(give_up_after: u64) -> RegisteredTransfer {
		let params = TransferParams {
			source: imported_test_key(0).key(),
			signer: TransferSigner::Imported,
			instance: None,
			destination: DESTINATION,
			signed_state_nonce: None,
			give_up_after,
		};
		scheduled_transfer(params, 0)
	}

	fn scheduled_transfer(params: TransferParams, attempts: u32) -> RegisteredTransfer {
		let source = params.source;
		let plan = SubmissionPlan { policy: PolicyId::new(TRANSFER_POLICY), params: params.into() };
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
		block_on(store.store(&[imported_test_key(0)])).unwrap();
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

	fn prepare_one(
		registered: &RegisteredTransfer,
		chain: ScriptedScarcity,
	) -> Option<Preparation> {
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
		let recorded = TransferParams::try_from(&params).unwrap().signed_state_nonce;
		assert_eq!(recorded, Some(held_nft().state_nonce));
	}

	/// A reorg can briefly show the source key without its NFT, so the transfer
	/// waits for it to come back rather than failing the operation.
	#[test]
	fn a_key_that_does_not_hold_the_instance_waits_for_it() {
		let registered = registered_transfer(0);
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
		let expected = TransferParams {
			instance: Some(INSTANCE),
			signed_state_nonce: Some(held_nft().state_nonce),
			..registered
		};
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
}
