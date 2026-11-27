//! [`build_claim_in_free_slot`]: a claim's extrinsic, built from Asset Hub
//! and People reads in the first free slot of the day that the policy has
//! not excluded.

use std::collections::BTreeSet;

use crate::{
	chain::MortalExtrinsic,
	runtime::statement_allowance::{
		StatementAllowanceError,
		pgas::{PgasClaim, await_ring_revision, build_pgas_claim_extrinsic},
		ring::read_ring_revision,
		slot::{current_period, read_chain_now_seconds, scan_pgas_slot_excluding},
	},
};

/// One PGAS claim slot: a slot index within a UTC day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClaimSlot {
	/// UTC day the slot belongs to, by Asset Hub's clock.
	pub day: u32,
	/// Slot index within that day.
	pub slot_index: u32,
}

/// A claim's fee-less `Pgas.claim_pgas` extrinsic, with the slot it spends.
pub struct BuiltClaim {
	/// The extrinsic, not yet submitted.
	pub extrinsic: MortalExtrinsic,
	/// The slot it claims.
	pub slot: ClaimSlot,
}

/// Builds a claim into `params.target` in the first free slot of Asset
/// Hub's current day that `excluded` does not list.
///
/// The proof is built against the claim ring's current revision on People,
/// once Asset Hub has imported it. Fails when no slot of the day is free,
/// when the revision is pruned or does not arrive within the wait, or when
/// a read, the proof or the extrinsic fails.
pub async fn build_claim_in_free_slot(
	params: &PgasClaim<'_>,
	excluded: &BTreeSet<ClaimSlot>,
) -> Result<BuiltClaim, StatementAllowanceError> {
	// The day is Asset Hub's, because its runtime checks the claim against
	// its own clock.
	let chain_now = read_chain_now_seconds(params.asset_hub_rpc).await?;
	let day = current_period(chain_now);
	let revision = imported_ring_revision(params).await?;
	let excluded_today = slot_indices_on_day(excluded, day);
	let slot_index = free_slot_index(params, day, &excluded_today).await?;
	let extrinsic = build_pgas_claim_extrinsic(params, day, slot_index, revision).await?;
	Ok(BuiltClaim { extrinsic, slot: ClaimSlot { day, slot_index } })
}

/// The revision of the claim's ring on People, once Asset Hub has imported
/// it.
async fn imported_ring_revision(params: &PgasClaim<'_>) -> Result<u32, StatementAllowanceError> {
	let PgasClaim { asset_hub_rpc, asset_hub_metadata, people_rpc, people_metadata, ring, .. } =
		*params;
	let revision = read_ring_revision(
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
	Ok(revision)
}

/// The first unclaimed slot index of `day` that `excluded` does not list.
async fn free_slot_index(
	params: &PgasClaim<'_>,
	day: u32,
	excluded: &[u32],
) -> Result<u32, StatementAllowanceError> {
	let PgasClaim { asset_hub_rpc, asset_hub_metadata, signer, network_suffix, ring, .. } = *params;
	scan_pgas_slot_excluding(
		asset_hub_rpc,
		asset_hub_metadata,
		ring.collection,
		signer,
		network_suffix,
		day,
		excluded,
	)
	.await
}

/// The slot indices of `slots` that belong to `day`. A slot of another day
/// does not stand in the way of a claim today.
fn slot_indices_on_day(slots: &BTreeSet<ClaimSlot>, day: u32) -> Vec<u32> {
	slots
		.iter()
		.filter(|slot| slot.day == day)
		.map(|slot| slot.slot_index)
		.collect()
}

#[cfg(test)]
mod tests;
