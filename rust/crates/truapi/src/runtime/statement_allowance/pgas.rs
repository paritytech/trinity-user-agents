//! Asset Hub PGAS allowance claims.
//!
//! Mirrors the mobile wallet flow: pick the first unclaimed daily PGAS slot for
//! the target, prove membership in the claiming collection with the `AsPgas`
//! transaction extension, and submit `Pgas.claim_pgas` on Asset Hub. The slot
//! budget and the alias space are both per collection, so the claim is bounded by
//! whichever collection it proves.
//!
//! Two chains are involved. The ring and its revision come from the People
//! chain, where membership lives; the claim is submitted on Asset Hub, which
//! learns about People's rings through `MembersSubscriber`. A claim can only be
//! authorized against a revision Asset Hub has already imported, so the flow
//! waits for it rather than submitting a proof the runtime cannot verify.

use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use parity_scale_codec::{Decode, DecodeAll};
use scale_decode::DecodeAsType;
use sp_crypto_hashing::twox_128;
use thiserror::Error;

use super::{
	StatementAllowanceError,
	collection::PersonhoodCollection,
	duplicate_submit_error,
	extension::{Metadata, MetadataError},
	extrinsic,
	key_hash::{blake2_128_concat, twox_64_concat},
	ring::{self, RingParams},
	rpc::RpcClient,
	slot,
};

/// How long to wait for Asset Hub to import the ring revision a proof is built
/// against before giving up.
const RING_REVISION_WAIT: Duration = Duration::from_secs(60);

/// Longest a single poll sleeps while waiting for a revision to land.
const RING_REVISION_POLL: Duration = Duration::from_secs(1);

/// Error while claiming a PGAS allowance.
#[derive(Debug, Error)]
pub enum PgasError {
	/// Asset Hub has moved past the revision the proof was built against, so it
	/// can never verify it. Resolve the ring again and rebuild the proof.
	#[error("Asset Hub pruned ring {ring_index} revision {revision}")]
	RingRevisionPruned {
		/// Ring the proof was built against.
		ring_index: u32,
		/// Revision the proof was built against.
		revision: u32,
	},
	/// Asset Hub had not imported the revision within the wait window.
	#[error("Asset Hub has not imported ring {ring_index} revision {revision}")]
	RingRevisionTimeout {
		/// Ring the proof was built against.
		ring_index: u32,
		/// Revision the proof was built against.
		revision: u32,
	},
	/// `MembersSubscriber.CurrentGeneration` was not a SCALE-encoded `u32`.
	#[error("MembersSubscriber.CurrentGeneration: {0}")]
	GenerationDecode(#[source] parity_scale_codec::Error),
	/// The asset account's leading balance failed to decode.
	#[error("PGAS balance: {0}")]
	BalanceDecode(#[source] parity_scale_codec::Error),
	/// The claim reached a block but the slot is not recorded as claimed, so the
	/// call dispatch-errored and nothing was minted.
	#[error(
		"claim reached block {block_hash} but PGAS slot (day {day}, slot {slot_index}) is not claimed"
	)]
	ClaimNotRecorded {
		/// Block the claim reached.
		block_hash: String,
		/// Day claimed for.
		day: u32,
		/// Slot claimed.
		slot_index: u32,
	},
}

/// One `MembersSubscriber.RingRoots` record, projected to the field that decides
/// whether a proof can be verified.
#[derive(DecodeAsType)]
struct RingCommitmentRecord {
	revision: u32,
}

/// Outcome of a PGAS claim.
pub struct PgasClaimOutcome {
	/// Asset Hub block containing the claim.
	pub block_hash: String,
	/// UTC day the claim was made for.
	pub day: u32,
	/// Slot claimed within that day.
	pub slot_index: u32,
	/// People ring index that authorized the claim.
	pub ring_index: u32,
}

fn current_generation_key() -> Vec<u8> {
	[twox_128(b"MembersSubscriber").as_slice(), twox_128(b"CurrentGeneration").as_slice()].concat()
}

async fn read_current_generation(
	rpc: &RpcClient,
	metadata: &Metadata,
) -> Result<u32, StatementAllowanceError> {
	// Missing entries and absent values both read as `None`, so distinguish them
	// before accepting the runtime's default generation.
	if metadata.storage_value_type("MembersSubscriber", "CurrentGeneration").is_none() {
		return Err(MetadataError::MissingStorageType {
			pallet: "MembersSubscriber",
			entry: "CurrentGeneration",
		}
		.into());
	}
	match rpc.get_storage(&current_generation_key()).await? {
		Some(bytes) => u32::decode_all(&mut &bytes[..])
			.map_err(PgasError::GenerationDecode)
			.map_err(Into::into),
		None => Ok(0),
	}
}

fn ring_roots_key(generation: u32, collection: PersonhoodCollection, ring_index: u32) -> Vec<u8> {
	[
		twox_128(b"MembersSubscriber").as_slice(),
		twox_128(b"RingRoots").as_slice(),
		&twox_64_concat(&generation.to_le_bytes()),
		&blake2_128_concat(collection.identifier()),
		&blake2_128_concat(&ring_index.to_le_bytes()),
	]
	.concat()
}

/// Everything one PGAS claim needs. Grouped rather than passed loose, matching
/// `RegistrationParams` in the statement-store path.
pub struct PgasClaim<'a> {
	/// Asset Hub connection the claim is submitted on.
	pub asset_hub_rpc: &'a RpcClient,
	/// Asset Hub metadata.
	pub asset_hub_metadata: &'a Metadata,
	/// Where the claim extrinsic is built on Asset Hub.
	pub asset_hub_target: super::ExtrinsicTarget<'a>,
	/// People-chain connection the ring revision is read from.
	pub people_rpc: &'a RpcClient,
	/// People-chain metadata.
	pub people_metadata: &'a Metadata,
	/// Authorizes personhood operations for the selected collection.
	pub signer: &'a dyn super::PersonhoodSigner,
	/// Asset Hub suffix used for the product-scoped alias and proof.
	pub network_suffix: &'a [u8],
	/// Account the claim credits.
	pub target: &'a [u8; 32],
	/// Ring the membership proof is built against, already located on People.
	pub ring: &'a RingParams,
}

/// `Assets.Account[(asset_id, who)]` storage key on Asset Hub.
fn pgas_balance_key(asset_id: u32, who: &[u8; 32]) -> Vec<u8> {
	[
		twox_128(b"Assets").as_slice(),
		twox_128(b"Account").as_slice(),
		&blake2_128_concat(&asset_id.to_le_bytes()),
		&blake2_128_concat(who),
	]
	.concat()
}

/// Whether `target` already holds a full claim's worth of PGAS.
///
/// A claim spends one of the day's slots, so a caller that asked to leave an
/// existing allowance alone should not burn one topping up an account that is
/// already warm. "Warm" is a whole `PgasClaimAmount` rather than any balance at
/// all: a dust balance is not what the claim would have provided.
pub async fn holds_a_full_claim(
	rpc: &RpcClient,
	metadata: &Metadata,
	target: &[u8; 32],
) -> Result<bool, StatementAllowanceError> {
	let claim_amount = metadata.constant_u128("Pgas", "PgasClaimAmount")?;
	Ok(pgas_balance(rpc, metadata, target).await? >= claim_amount)
}

/// `who`'s PGAS balance, 0 when the account holds none.
///
/// `AssetAccount` leads with its `u128` balance, which is all this needs.
pub async fn pgas_balance(
	rpc: &RpcClient,
	metadata: &Metadata,
	who: &[u8; 32],
) -> Result<u128, StatementAllowanceError> {
	let asset_id = metadata.constant_u32("Pgas", "PgasAssetId")?;
	let Some(bytes) = rpc.get_storage(&pgas_balance_key(asset_id, who)).await? else {
		return Ok(0);
	};
	Ok(u128::decode(&mut &bytes[..]).map_err(PgasError::BalanceDecode)?)
}

/// Claim one PGAS allowance for `target`, proving membership in the
/// already-located People `ring`.
///
/// A duplicate submission retries against a different slot, as the statement-store
/// and long-term-storage claims do: two hosts racing the same day both scan a free
/// slot before either lands.
pub async fn claim_pgas(
	params: PgasClaim<'_>,
) -> Result<PgasClaimOutcome, StatementAllowanceError> {
	let PgasClaim {
		asset_hub_rpc,
		asset_hub_metadata,
		asset_hub_target,
		people_rpc,
		people_metadata,
		signer,
		network_suffix,
		target,
		ring,
	} = params;
	// The day is Asset Hub's, because its runtime checks the claim against its
	// own clock. Periods are a UTC day here as they are for statement store.
	let day = slot::current_period(slot::read_chain_now_seconds(asset_hub_rpc).await?);
	let revision = ring::read_ring_revision(
		people_rpc,
		people_metadata,
		ring.collection,
		ring.ring_index,
		&ring.block_hash,
	)
	.await?;
	await_ring_revision(
		asset_hub_rpc,
		asset_hub_metadata,
		ring.collection,
		ring.ring_index,
		revision,
	)
	.await?;

	let mut skipped_duplicate_slots = Vec::new();
	loop {
		let slot_index = slot::scan_pgas_slot_excluding(
			asset_hub_rpc,
			asset_hub_metadata,
			ring.collection,
			signer,
			network_suffix,
			day,
			&skipped_duplicate_slots,
		)
		.await?;
		let context = slot::derive_pgas_context(network_suffix, day, slot_index);
		let alias = slot::pgas_alias(signer, ring.collection, network_suffix, day, slot_index)?;
		let proof = super::RingProof { signer, ring, context, revision };
		let call = extrinsic::ClaimPgasCallArgs { slot_index, target: *target };
		let claim = asset_hub_target
			.extrinsics
			.build_mortal_extrinsic(asset_hub_target.genesis)
			.with_call_args("Pgas", "claim_pgas", call)
			.with_extension(super::AsPgasExtension::new(proof, day))
			.build()
			.await?;

		match asset_hub_rpc.submit_and_watch(claim.extrinsic.bytes()).await {
			Ok(block_hash) => {
				// Inclusion is not success. `Pgas.claim_pgas` can dispatch-error —
				// `AlreadyClaimed`, `PgasMintFailed` — and the extrinsic still lands,
				// so reporting an allowance now would promise PGAS that was never
				// minted. The pallet marks the alias spent on success; check that.
				if !slot::pgas_slot_is_claimed_at(asset_hub_rpc, &alias, day, &block_hash).await? {
					return Err(PgasError::ClaimNotRecorded { block_hash, day, slot_index }.into());
				}
				return Ok(PgasClaimOutcome {
					block_hash,
					day,
					slot_index,
					ring_index: ring.ring_index,
				});
			},
			Err(err) if duplicate_submit_error(&err.to_string()) => {
				skipped_duplicate_slots.push(slot_index);
			},
			Err(err) => return Err(err),
		}
	}
}

/// What Asset Hub's held ring roots say about the revision a proof was built
/// against.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RevisionStatus {
	/// Asset Hub holds it, so the proof can be verified.
	Imported,
	/// Asset Hub holds a newer root, so it will never verify this one.
	Pruned,
	/// Not held, and nothing newer is held either.
	Pending,
}

/// Classify `revision` against the roots Asset Hub currently holds for a ring.
///
/// The newest held root is the test, not the oldest: the window drops revisions
/// off the front but can also skip one entirely, and a skipped revision is just
/// as unreachable as an evicted one.
fn revision_status(held: &[u32], revision: u32) -> RevisionStatus {
	if held.contains(&revision) {
		return RevisionStatus::Imported;
	}
	match held.iter().max() {
		Some(&newest) if newest > revision => RevisionStatus::Pruned,
		_ => RevisionStatus::Pending,
	}
}

/// Wait until Asset Hub has imported `revision` of `ring_index`.
///
/// Public so a host can check propagation before offering a claim, and so the
/// live tests can exercise it without submitting.
///
/// Asset Hub keeps only the most recent roots per ring, so a revision can be
/// absent because it has not arrived yet or because it never will. Those need
/// different answers: the first is worth waiting for, the second means the proof
/// has to be rebuilt against a newer ring.
pub async fn await_ring_revision(
	rpc: &RpcClient,
	metadata: &Metadata,
	collection: PersonhoodCollection,
	ring_index: u32,
	revision: u32,
) -> Result<(), StatementAllowanceError> {
	let value_type = metadata.storage_value_type("MembersSubscriber", "RingRoots").ok_or(
		MetadataError::MissingStorageType { pallet: "MembersSubscriber", entry: "RingRoots" },
	)?;
	let started = Instant::now();
	loop {
		// A rebuild can land during this wait, leaving cached-generation polls
		// on roots that will never return.
		let generation = read_current_generation(rpc, metadata).await?;
		if let Some(bytes) =
			rpc.get_storage(&ring_roots_key(generation, collection, ring_index)).await?
		{
			let mut input = bytes.as_slice();
			let records = Vec::<RingCommitmentRecord>::decode_as_type(
				&mut input,
				value_type,
				metadata.registry(),
			)
			.map_err(|source| ring::RingError::DecodeAsType {
				context: "subscriber ring roots",
				source,
			})?;
			let held: Vec<u32> = records.iter().map(|record| record.revision).collect();
			match revision_status(&held, revision) {
				RevisionStatus::Imported => return Ok(()),
				RevisionStatus::Pruned => {
					return Err(PgasError::RingRevisionPruned { ring_index, revision }.into());
				},
				RevisionStatus::Pending => {},
			}
		}
		wait_before_next_ring_revision_poll(started, ring_index, revision).await?;
	}
}

/// Sleep before the next revision poll, or report the wait as exhausted.
async fn wait_before_next_ring_revision_poll(
	started: Instant,
	ring_index: u32,
	revision: u32,
) -> Result<(), StatementAllowanceError> {
	let Some(remaining) = RING_REVISION_WAIT.checked_sub(started.elapsed()) else {
		return Err(PgasError::RingRevisionTimeout { ring_index, revision }.into());
	};
	futures_timer::Delay::new(remaining.min(RING_REVISION_POLL)).await;
	Ok(())
}

#[cfg(test)]
mod tests;
