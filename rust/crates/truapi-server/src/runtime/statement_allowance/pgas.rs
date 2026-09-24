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

use parity_scale_codec::{Decode, DecodeAll};
use scale_decode::DecodeAsType;
use sp_crypto_hashing::twox_128;
use thiserror::Error;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use super::collection::PersonhoodCollection;
use super::extension::{AS_PGAS, Metadata, MetadataError};
use super::ring::{self, RingParams, blake2_128_concat, twox_64_concat};
use super::rpc::RpcClient;
use super::{
    ChainContext, StatementAllowanceError, duplicate_submit_error, extension, extrinsic, proof,
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
#[derive(Debug, DecodeAsType)]
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
    [
        twox_128(b"MembersSubscriber").as_slice(),
        twox_128(b"CurrentGeneration").as_slice(),
    ]
    .concat()
}

async fn read_current_generation(
    rpc: &RpcClient,
    metadata: &Metadata,
) -> Result<u32, StatementAllowanceError> {
    // Missing entries and absent values both read as `None`, so distinguish them
    // before accepting the runtime's default generation.
    if metadata
        .storage_value_type("MembersSubscriber", "CurrentGeneration")
        .is_none()
    {
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
    /// Asset Hub metadata and signed-extension state, from the per-chain cache.
    pub asset_hub: &'a ChainContext,
    /// People-chain connection the ring revision is read from.
    pub people_rpc: &'a RpcClient,
    /// People-chain metadata.
    pub people_metadata: &'a Metadata,
    /// Our ring-VRF entropy for the collection `ring` names.
    pub entropy: [u8; 32],
    /// Asset Hub suffix used for the product-scoped alias and proof.
    pub network_suffix: &'a [u8],
    /// Account the claim credits.
    pub target: &'a [u8; 32],
    /// Ring the membership proof is built against, already located on People.
    pub ring: &'a RingParams,
}

/// `Assets.Account[(asset_id, who)]` storage key on Asset Hub.
pub(super) fn pgas_balance_key(asset_id: u32, who: &[u8; 32]) -> Vec<u8> {
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
///
/// `AssetAccount` leads with its `u128` balance, which is all this needs.
pub async fn holds_a_full_claim(
    rpc: &RpcClient,
    metadata: &Metadata,
    target: &[u8; 32],
) -> Result<bool, StatementAllowanceError> {
    let asset_id = metadata.constant_u32("Pgas", "PgasAssetId")?;
    let claim_amount = metadata.constant_u128("Pgas", "PgasClaimAmount")?;
    let Some(bytes) = rpc.get_storage(&pgas_balance_key(asset_id, target)).await? else {
        return Ok(false);
    };
    let balance = u128::decode(&mut &bytes[..]).map_err(PgasError::BalanceDecode)?;
    Ok(balance >= claim_amount)
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
        asset_hub,
        people_rpc,
        people_metadata,
        entropy,
        network_suffix,
        target,
        ring,
    } = params;
    let asset_hub_metadata = asset_hub.metadata.as_ref();
    let asset_hub_state = &asset_hub.state;
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
            entropy,
            network_suffix,
            day,
            &skipped_duplicate_slots,
        )
        .await?;
        let context = slot::derive_pgas_context(network_suffix, day, slot_index);
        let call = extrinsic::build_claim_pgas_call(asset_hub_metadata, slot_index, target)?;
        let message = extension::build_proof_message_after_extension(
            asset_hub_metadata,
            &call,
            asset_hub_state,
            AS_PGAS,
        )?;
        let domain = proof::domain_for_ring_exponent(ring.exponent)?;
        let ring_proof = proof::ring_vrf_proof(domain, entropy, &ring.members, &context, &message)?;
        let extra = extrinsic::build_as_pgas_extra(
            asset_hub_metadata,
            &ring_proof,
            ring.ring_index,
            revision,
            day,
            ring.collection,
        )?;
        let claim = extrinsic::build_unsigned_extrinsic_with_extra(
            asset_hub_metadata,
            asset_hub_state,
            &call,
            AS_PGAS,
            &extra,
        )?;

        match asset_hub_rpc.submit_and_watch(&claim).await {
            Ok(block_hash) => {
                // Inclusion is not success. `Pgas.claim_pgas` can dispatch-error —
                // `AlreadyClaimed`, `PgasMintFailed` — and the extrinsic still lands,
                // so reporting an allowance now would promise PGAS that was never
                // minted. The pallet marks the alias spent on success; check that.
                if !slot::pgas_slot_is_claimed_at(
                    asset_hub_rpc,
                    entropy,
                    network_suffix,
                    day,
                    slot_index,
                    &block_hash,
                )
                .await?
                {
                    return Err(PgasError::ClaimNotRecorded {
                        block_hash,
                        day,
                        slot_index,
                    }
                    .into());
                }
                return Ok(PgasClaimOutcome {
                    block_hash,
                    day,
                    slot_index,
                    ring_index: ring.ring_index,
                });
            }
            Err(err) if duplicate_submit_error(&err.to_string()) => {
                skipped_duplicate_slots.push(slot_index);
            }
            Err(err) => return Err(err),
        }
    }
}

/// What Asset Hub's held ring roots say about the revision a proof was built
/// against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    let value_type = metadata
        .storage_value_type("MembersSubscriber", "RingRoots")
        .ok_or(MetadataError::MissingStorageType {
            pallet: "MembersSubscriber",
            entry: "RingRoots",
        })?;
    let started = Instant::now();
    loop {
        // A rebuild can land during this wait, leaving cached-generation polls
        // on roots that will never return.
        let generation = read_current_generation(rpc, metadata).await?;
        if let Some(bytes) = rpc
            .get_storage(&ring_roots_key(generation, collection, ring_index))
            .await?
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
                    return Err(PgasError::RingRevisionPruned {
                        ring_index,
                        revision,
                    }
                    .into());
                }
                RevisionStatus::Pending => {}
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
        return Err(PgasError::RingRevisionTimeout {
            ring_index,
            revision,
        }
        .into());
    };
    futures_timer::Delay::new(remaining.min(RING_REVISION_POLL)).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use subxt_rpcs::RpcClient as HostRpcClient;

    use super::super::rpc::testing::ScriptedRpc;
    use super::super::test_fixtures;
    use super::*;

    /// The collection the captured roots were read from. The fixture only means
    /// anything paired with this identifier.
    const CAPTURED_COLLECTION: PersonhoodCollection = PersonhoodCollection::LitePeople;
    const TEST_GENERATION: u32 = 7;

    /// The captured ring-5 roots as a scripted `state_getStorage` result, with the
    /// transport handle so the key that was read can be checked.
    fn scripted_ring_5_roots() -> (RpcClient, ScriptedRpc) {
        let value = format!(
            r#""0x{}""#,
            hex::encode(test_fixtures::ASSET_HUB_RING_5_ROOTS)
        );
        let generation = format!(r#""0x{}""#, hex::encode(TEST_GENERATION.to_le_bytes()));
        let scripted = ScriptedRpc::new([generation.as_str(), value.as_str()]);
        (
            RpcClient::new(HostRpcClient::new(scripted.clone())),
            scripted,
        )
    }

    /// `ScriptedRpc` replays by position and ignores the key, so without this the
    /// collection paired with the captured blob would be a comment rather than a
    /// fact: re-capturing the blob against another collection, or editing the
    /// constant, would leave every test in this module green.
    ///
    /// The expected key is built from the identifier literal the fixtures README
    /// records the capture under, not from `CAPTURED_COLLECTION`. Deriving both
    /// sides from the constant would move them together and assert nothing.
    fn assert_read_ring_5_of(scripted: &ScriptedRpc) {
        const CAPTURED_UNDER: &[u8; 32] = b"pop:polkadot.network/people-lite";
        assert_eq!(
            CAPTURED_COLLECTION.identifier(),
            CAPTURED_UNDER,
            "the committed blob was read under the lite-people identifier",
        );
        let current_generation = format!(
            r#"["0x{}"]"#,
            hex::encode(
                [
                    twox_128(b"MembersSubscriber").as_slice(),
                    twox_128(b"CurrentGeneration").as_slice(),
                ]
                .concat()
            )
        );
        let generation = twox_64_concat(&TEST_GENERATION.to_le_bytes());
        let ring_roots = format!(
            r#"["0x{}"]"#,
            hex::encode(
                [
                    twox_128(b"MembersSubscriber").as_slice(),
                    twox_128(b"RingRoots").as_slice(),
                    &generation,
                    &blake2_128_concat(CAPTURED_UNDER),
                    &blake2_128_concat(&5u32.to_le_bytes()),
                ]
                .concat()
            )
        );
        let reads: Vec<String> = scripted
            .calls()
            .into_iter()
            .filter(|(method, _)| method == "state_getStorage")
            .map(|(_, params)| params)
            .collect();
        assert_eq!(
            reads,
            vec![current_generation, ring_roots],
            "the roots read must use the current generation and the fixture collection"
        );
    }

    /// `RingCommitmentRecord` takes one field out of a much wider runtime struct,
    /// so decode a captured value rather than a hand-built one: the projection is
    /// the part that a changed record layout would silently break.
    #[test]
    fn captured_ring_roots_project_to_their_revisions() {
        let metadata = test_fixtures::asset_hub();
        let value_type = metadata
            .storage_value_type("MembersSubscriber", "RingRoots")
            .expect(
                "the metadata fixture declares MembersSubscriber; \
                 re-capture it and the roots blob together",
            );

        let records = Vec::<RingCommitmentRecord>::decode_as_type(
            &mut &test_fixtures::ASSET_HUB_RING_5_ROOTS[..],
            value_type,
            metadata.registry(),
        )
        .unwrap();

        assert_eq!(
            records
                .iter()
                .map(|record| record.revision)
                .collect::<Vec<_>>(),
            vec![105, 106, 108],
        );
    }

    /// End to end over the real path, against roots Asset Hub actually served: a
    /// revision the window skipped reports as pruned instead of being waited out.
    ///
    /// Only the two terminating outcomes are reachable here. Pending keeps
    /// polling until `RING_REVISION_WAIT` expires, so it is covered by the
    /// `revision_status` table instead.
    #[test]
    fn a_skipped_revision_reports_as_pruned_against_captured_roots() {
        let (rpc, scripted) = scripted_ring_5_roots();
        let err = futures::executor::block_on(await_ring_revision(
            &rpc,
            test_fixtures::asset_hub(),
            CAPTURED_COLLECTION,
            5,
            107,
        ))
        .expect_err("revision 107 is missing from the captured window");
        assert_read_ring_5_of(&scripted);

        assert!(
            err.to_string().contains("pruned"),
            "a skipped revision should not be waited out: {err}"
        );
    }

    /// And one the window holds returns without a second poll.
    #[test]
    fn a_held_revision_returns_against_captured_roots() {
        let (rpc, scripted) = scripted_ring_5_roots();
        futures::executor::block_on(await_ring_revision(
            &rpc,
            test_fixtures::asset_hub(),
            CAPTURED_COLLECTION,
            5,
            106,
        ))
        .expect("revision 106 is in the captured window");
        assert_read_ring_5_of(&scripted);
    }

    /// A wrong key hasher makes imported roots look absent until timeout.
    #[test]
    fn subscriber_ring_key_matches_the_runtime_layout() {
        assert_eq!(
            ring_roots_key(7, PersonhoodCollection::LitePeople, 136),
            hex::decode(
                "c8d053ab324196afc756c5ae3fbd29178034a4ffd558299b9ac8097424772405\
                 0e0d969b0e48cab7070000000bf6e234fafa085fd6710b952081ed8d706f703a\
                 706f6c6b61646f742e6e6574776f726b2f70656f706c652d6c697465751c83\
                 d291ed9a90887239b8a92828a688000000"
            )
            .unwrap(),
        );
    }

    #[test]
    fn missing_current_generation_uses_the_runtime_default() {
        let scripted = ScriptedRpc::new(["null"]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

        let generation =
            futures::executor::block_on(read_current_generation(&rpc, test_fixtures::asset_hub()))
                .unwrap();

        assert_eq!(
            (generation, scripted.calls()),
            (
                0,
                vec![(
                    "state_getStorage".to_string(),
                    r#"["0xc8d053ab324196afc756c5ae3fbd2917c2dbc4fc2f665a39ada06f0965cccf86"]"#
                        .to_string(),
                )],
            ),
        );
    }

    /// Caching the generation would leave later polls on abandoned roots.
    #[test]
    fn a_generation_change_during_wait_moves_the_ring_root_read() {
        let first_generation = format!(r#""0x{}""#, hex::encode(7u32.to_le_bytes()));
        let second_generation = format!(r#""0x{}""#, hex::encode(8u32.to_le_bytes()));
        let roots = format!(
            r#""0x{}""#,
            hex::encode(test_fixtures::ASSET_HUB_RING_5_ROOTS)
        );
        let scripted = ScriptedRpc::new([
            first_generation.as_str(),
            "null",
            second_generation.as_str(),
            roots.as_str(),
        ]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

        futures::executor::block_on(await_ring_revision(
            &rpc,
            test_fixtures::asset_hub(),
            CAPTURED_COLLECTION,
            5,
            106,
        ))
        .unwrap();

        let storage_call = |key: Vec<u8>| {
            (
                "state_getStorage".to_string(),
                format!(r#"["0x{}"]"#, hex::encode(key)),
            )
        };
        assert_eq!(
            scripted.calls(),
            vec![
                storage_call(current_generation_key()),
                storage_call(ring_roots_key(7, CAPTURED_COLLECTION, 5)),
                storage_call(current_generation_key()),
                storage_call(ring_roots_key(8, CAPTURED_COLLECTION, 5)),
            ],
        );
    }

    /// A trailing byte means the entry is no longer a bare `u32`, which is the
    /// same layout drift the three-key ring-root key exists to track. Taking the
    /// first four bytes would build keys for a generation nothing answers, and
    /// the wait would read as "the ring never arrived".
    #[test]
    fn a_current_generation_that_is_not_a_bare_u32_is_rejected() {
        let overlong = format!(r#""0x{}""#, hex::encode([7u8, 0, 0, 0, 0]));
        let scripted = ScriptedRpc::new([overlong.as_str()]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted));

        let err =
            futures::executor::block_on(read_current_generation(&rpc, test_fixtures::asset_hub()))
                .expect_err("a five-byte value is not a u32");

        assert_eq!(
            err.to_string(),
            "MembersSubscriber.CurrentGeneration: Input buffer has still data left after decoding!"
        );
    }

    /// An absent value is only the `ValueQuery` default if the runtime declares
    /// the entry at all. A renamed pallet or item reads as absent too, and
    /// defaulting there would key every ring-root read at generation 0.
    #[test]
    fn a_runtime_without_current_generation_is_named_rather_than_defaulted() {
        // Real metadata, because the regression depends on the pallet being absent.
        let people = test_fixtures::people();
        let scripted = ScriptedRpc::new(["null"]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

        let err = futures::executor::block_on(read_current_generation(&rpc, people))
            .expect_err("the People runtime declares no MembersSubscriber");

        assert_eq!(
            (err.to_string(), scripted.calls()),
            (
                "MembersSubscriber.CurrentGeneration type not in metadata".to_string(),
                vec![],
            ),
            "the metadata check comes before the read, so nothing is asked of the chain",
        );
    }

    /// `holds_a_full_claim` reads both of these from the runtime, and compares a
    /// balance against the claim amount. A missing constant would make the warm
    /// check answer the same way for every account.
    #[test]
    fn the_asset_hub_fixture_declares_the_pgas_asset_and_claim_amount() {
        let metadata = test_fixtures::asset_hub();

        assert_eq!(
            metadata.constant_u32("Pgas", "PgasAssetId").unwrap(),
            2_000_000_000
        );
        assert_eq!(
            metadata.constant_u128("Pgas", "PgasClaimAmount").unwrap(),
            50_000_000_000,
        );
    }

    /// The window holds it, so the proof can be verified.
    #[test]
    fn a_held_revision_is_imported() {
        assert_eq!(
            revision_status(&[105, 106, 108], 106),
            RevisionStatus::Imported
        );
        assert_eq!(revision_status(&[106], 106), RevisionStatus::Imported);
    }

    /// Nothing newer is held, so ours may still be on its way. An empty window
    /// reaches this if the entry exists but holds no roots; an absent entry
    /// skips the classification entirely and goes straight to the next poll.
    #[test]
    fn a_revision_newer_than_the_window_is_pending() {
        assert_eq!(revision_status(&[], 106), RevisionStatus::Pending);
        assert_eq!(
            revision_status(&[103, 104, 105], 106),
            RevisionStatus::Pending
        );
    }

    /// A revision the window skipped is as unreachable as one that fell off the
    /// front. Testing the oldest held root instead would wait this one out.
    #[test]
    fn a_skipped_revision_is_pruned_rather_than_pending() {
        assert_eq!(
            revision_status(&[105, 106, 108], 107),
            RevisionStatus::Pruned
        );
    }

    /// Evicted off the front of the window.
    #[test]
    fn a_revision_older_than_the_window_is_pruned() {
        assert_eq!(
            revision_status(&[105, 106, 108], 42),
            RevisionStatus::Pruned
        );
    }

    /// Storage does not promise an order, so the rule cannot depend on one.
    #[test]
    fn the_held_order_does_not_change_the_answer() {
        assert_eq!(
            revision_status(&[108, 105, 106], 107),
            RevisionStatus::Pruned
        );
        assert_eq!(
            revision_status(&[108, 105, 106], 106),
            RevisionStatus::Imported
        );
    }

    /// A claim is authorized against one revision, so the day it is built for and
    /// the revision it proves must both reach the runtime unchanged.
    #[test]
    fn a_pruned_revision_is_distinguished_from_one_still_arriving() {
        let pruned = PgasError::RingRevisionPruned {
            ring_index: 2,
            revision: 100,
        };
        let waiting = PgasError::RingRevisionTimeout {
            ring_index: 2,
            revision: 100,
        };

        assert!(pruned.to_string().contains("pruned"));
        assert!(waiting.to_string().contains("has not imported"));
    }
}
