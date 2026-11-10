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
/// that key sits, the key it goes to, the attempt it submitted, and when it
/// stops waiting. Everything a build needs, so it reads no table.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct TransferParams {
	/// The key the NFT leaves, which signs the transfer.
	pub source: ItemKey,
	/// How the secret of `source` is found.
	pub signer: TransferSigner,
	/// The key it goes to.
	pub destination: ItemKey,
	/// The live attempt. `None` until the first build, which moves whatever
	/// instance `source` holds then.
	pub submitted_attempt: Option<TransferAttempt>,
	/// Unix time, in seconds of the host's clock, after which a transfer that
	/// still waits is given up. Fixed at registration, so a restart never
	/// extends it.
	pub give_up_after: u64,
}

/// The live attempt of a transfer. Later attempts move the same instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub struct TransferAttempt {
	/// The instance the attempt moves.
	pub instance: u64,
	/// The `state_nonce` the attempt signed over, which the oracle proves its
	/// landing by.
	pub state_nonce: u64,
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
#[derive(Clone)]
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
#[derive(Clone)]
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
		match self.sign_transfer(transfer, &nft_transfer, build).await {
			SignedTransfer::Ready(ready) => Some(ready),
			SignedTransfer::Unsignable => Some(Preparation::GiveUp),
			SignedTransfer::Unavailable => give_up_after_limit(transfer, build.now),
		}
	}

	/// What signing `nft_transfer` for `transfer` came to: ready, with the
	/// instance and revision it signed recorded in its params, or why not.
	async fn sign_transfer(
		&self,
		transfer: &PlannedTransfer,
		nft_transfer: &NftTransfer,
		build: &BatchBuild,
	) -> SignedTransfer {
		let signer = match self.signer_for(&transfer.params).await {
			Ok(Some(signer)) => signer,
			Ok(None) => return SignedTransfer::Unsignable,
			Err(error) => return log_unsigned_transfer(error),
		};
		build
			.extrinsics
			.sign_nft_transfer(self.deps.genesis, &signer, nft_transfer)
			.await
			.map(|extrinsic| ready_preparation(transfer, nft_transfer, extrinsic))
			.map_or_else(log_unsigned_transfer, SignedTransfer::Ready)
	}

	/// The signer of the source key of `params`, or `None` when it is an
	/// imported key whose secret is not stored. Fails without a session or
	/// when the store cannot be read.
	async fn signer_for(
		&self,
		params: &TransferParams,
	) -> Result<Option<Sr25519Signer>, PolicyError> {
		match params.signer {
			TransferSigner::Derived(slot) => {
				self.deps.signer.item_signer(slot).map(Some).map_err(policy_error)
			},
			TransferSigner::Imported => self.imported_signer(&params.source).await,
		}
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
	let NftTransfer { instance, state_nonce, .. } = *nft_transfer;
	let submitted_attempt = Some(TransferAttempt { instance, state_nonce });
	let params = TransferParams { submitted_attempt, ..transfer.params };
	Preparation::Ready { extrinsic, params: Some(params.into()) }
}

/// [`SignedTransfer::Unavailable`], after logging the `error` that kept the
/// transfer from being signed.
fn log_unsigned_transfer(error: impl core::fmt::Display) -> SignedTransfer {
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
/// instance waits, since a reorg can put it back, and a transfer with no
/// submitted attempt yet waits for its source to hold any. An occupied destination
/// fails at once.
fn decide_from_chain(transfer: &PlannedTransfer, view: TransferView) -> TransferDecision {
	if transfer.attempts >= MAX_ATTEMPTS || view.destination.is_some() {
		return TransferDecision::GiveUp;
	}
	let expected = transfer.params.submitted_attempt.map(|attempt| attempt.instance);
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
mod tests;
