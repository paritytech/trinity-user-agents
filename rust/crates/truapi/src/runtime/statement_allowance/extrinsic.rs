//! `Resources.set_statement_store_account` call + unsigned General (v5)
//! extrinsic assembly. Mirrors signing-bot `allocation.ts` / `extrinsic-submit.ts`.
//! Dispatch and variant indices are resolved by name from the fetched runtime
//! metadata, so a re-indexed runtime fails loudly instead of encoding a wrong
//! call.

use parity_scale_codec::{Decode, Encode};

use super::StatementAllowanceError;
use super::collection::PersonhoodCollection;
use super::extension::{AS_PGAS, AS_RESOURCES, ChainState, Metadata, MetadataError};

/// General-transaction preamble byte: `0b01` (General) | version 5.
const GENERAL_V5_PREAMBLE: u8 = 0x45;
/// `Option::Some` discriminant for the `AsResources` extension `extra`.
const OPTION_SOME: u8 = 0x01;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
struct SetStatementStoreAccountCallArgs {
    period: u32,
    seq: u32,
    target: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
struct ClaimLongTermStorageCallArgs {
    period: u32,
    counter: u8,
    account_id: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
struct ClaimPgasCallArgs {
    slot_index: u32,
    target: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
struct RegisterStatementStoreAllowanceInfo {
    proof: Vec<u8>,
    ring_index: u32,
    revision: u32,
    personhood: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
struct ClaimLongTermStorageInfo {
    proof: Vec<u8>,
    ring_index: u32,
    revision: u32,
    personhood: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
struct ClaimPgasInfo {
    proof: Vec<u8>,
    ring_index: u32,
    revision: u32,
    collection: u8,
    day: u32,
}

/// Encode `Resources.set_statement_store_account(period, seq, target)`:
/// `pallet ‖ call ‖ period_u32LE ‖ seq_u32LE ‖ target[32]`, with the dispatch
/// indices resolved from `metadata`.
pub fn build_set_statement_store_account_call(
    metadata: &Metadata,
    period: u32,
    seq: u32,
    target: &[u8; 32],
) -> Result<Vec<u8>, StatementAllowanceError> {
    let indices = metadata.call_indices("Resources", "set_statement_store_account")?;
    let mut call = Vec::with_capacity(2 + 4 + 4 + 32);
    call.extend_from_slice(&indices);
    SetStatementStoreAccountCallArgs {
        period,
        seq,
        target: *target,
    }
    .encode_to(&mut call);
    Ok(call)
}

/// Encode `Resources.claim_long_term_storage(period, counter, account_id)`:
/// `pallet ‖ call ‖ period_u32LE ‖ counter_u8 ‖ account_id[32]`, with the
/// dispatch indices resolved from `metadata`.
pub fn build_claim_long_term_storage_call(
    metadata: &Metadata,
    period: u32,
    counter: u8,
    account_id: &[u8; 32],
) -> Result<Vec<u8>, StatementAllowanceError> {
    let indices = metadata.call_indices("Resources", "claim_long_term_storage")?;
    let mut call = Vec::with_capacity(2 + 4 + 1 + 32);
    call.extend_from_slice(&indices);
    ClaimLongTermStorageCallArgs {
        period,
        counter,
        account_id: *account_id,
    }
    .encode_to(&mut call);
    Ok(call)
}

/// Encode the `AsResources` extension `extra` for a statement-store allowance:
/// `Some(RegisterStatementStoreAllowance { proof, ring_index, collection })`,
/// with the variant indices resolved from `metadata`.
pub fn build_as_resources_extra(
    metadata: &Metadata,
    proof: &[u8],
    ring_index: u32,
    revision: u32,
    collection: PersonhoodCollection,
) -> Result<Vec<u8>, StatementAllowanceError> {
    let (info_index, personhood) =
        metadata.as_resources_variant_indices("RegisterStatementStoreAllowance", collection)?;
    let mut extra = Vec::with_capacity(2 + 2 + proof.len() + 4 + 4 + 1);
    extra.push(OPTION_SOME);
    extra.push(info_index);
    RegisterStatementStoreAllowanceInfo {
        proof: proof.to_vec(),
        ring_index,
        revision,
        personhood,
    }
    .encode_to(&mut extra);
    Ok(extra)
}

/// Encode the `AsResources` extension `extra` for a long-term storage claim:
/// `Some(ClaimLongTermStorage { proof, ring_index, revision, collection })`,
/// with the variant indices resolved from `metadata`.
pub fn build_long_term_storage_extra(
    metadata: &Metadata,
    proof: &[u8],
    ring_index: u32,
    revision: u32,
    collection: PersonhoodCollection,
) -> Result<Vec<u8>, StatementAllowanceError> {
    let (info_index, personhood) =
        metadata.as_resources_variant_indices("ClaimLongTermStorage", collection)?;
    let mut extra = Vec::with_capacity(2 + 2 + proof.len() + 4 + 4 + 1);
    extra.push(OPTION_SOME);
    extra.push(info_index);
    ClaimLongTermStorageInfo {
        proof: proof.to_vec(),
        ring_index,
        revision,
        personhood,
    }
    .encode_to(&mut extra);
    Ok(extra)
}

/// Encode `Pgas.claim_pgas(slot_index, target)` on Asset Hub:
/// `pallet ‖ call ‖ slot_index_u32LE ‖ target[32]`, with the dispatch indices
/// resolved from `metadata`.
pub fn build_claim_pgas_call(
    metadata: &Metadata,
    slot_index: u32,
    target: &[u8; 32],
) -> Result<Vec<u8>, StatementAllowanceError> {
    let indices = metadata.call_indices("Pgas", "claim_pgas")?;
    let mut call = Vec::with_capacity(2 + 4 + 32);
    call.extend_from_slice(&indices);
    ClaimPgasCallArgs {
        slot_index,
        target: *target,
    }
    .encode_to(&mut call);
    Ok(call)
}

/// Encode the `AsPgas` extension `extra` for a PGAS claim:
/// `Some(Claim { proof, ring_index, revision, collection, day })`, with the
/// variant indices resolved from `metadata`.
///
/// The runtime verifies the proof against the collection this declares, so it
/// must be the collection whose ring the proof was built in.
///
/// `AsPgas` names its membership enum `PgasCollection` rather than
/// `MembershipCollection`, so the tier is resolved by variant name.
pub fn build_as_pgas_extra(
    metadata: &Metadata,
    proof: &[u8],
    ring_index: u32,
    revision: u32,
    day: u32,
    collection: PersonhoodCollection,
) -> Result<Vec<u8>, StatementAllowanceError> {
    let (info_index, collection_index) = metadata.extension_info_and_field_variant_indices(
        AS_PGAS,
        "Claim",
        collection.metadata_variant(),
    )?;
    let mut extra = Vec::with_capacity(2 + 2 + proof.len() + 4 + 4 + 1 + 4);
    extra.push(OPTION_SOME);
    extra.push(info_index);
    ClaimPgasInfo {
        proof: proof.to_vec(),
        ring_index,
        revision,
        collection: collection_index,
        day,
    }
    .encode_to(&mut extra);
    Ok(extra)
}

/// Assemble the unsigned General (v5) extrinsic:
/// `compact(len) ‖ 0x45 ‖ 0x00 ‖ Σ(all extra, AsResources = Some(info)) ‖ call`.
pub fn build_unsigned_extrinsic(
    metadata: &Metadata,
    state: &ChainState,
    call_data: &[u8],
    as_resources_extra: &[u8],
) -> Result<Vec<u8>, StatementAllowanceError> {
    build_unsigned_extrinsic_with_extra(
        metadata,
        state,
        call_data,
        AS_RESOURCES,
        as_resources_extra,
    )
}

/// Same, for any authorizing extension: every extension's `extra` in metadata
/// order, with `identifier`'s replaced by `extra`.
///
/// The version byte comes from metadata, so an extension added here encodes for
/// the pipeline the runtime declares rather than a compiled-in guess.
pub fn build_unsigned_extrinsic_with_extra(
    metadata: &Metadata,
    state: &ChainState,
    call_data: &[u8],
    identifier: &str,
    extra: &[u8],
) -> Result<Vec<u8>, StatementAllowanceError> {
    let all = metadata.encode_signed_extensions(state);
    let authorizing_index = metadata.extension_index(identifier).ok_or_else(|| {
        MetadataError::MissingExtension {
            identifier: identifier.to_string(),
        }
    })?;

    let mut body = vec![GENERAL_V5_PREAMBLE, metadata.extension_version()];
    for (i, ext) in all.iter().enumerate() {
        if i == authorizing_index {
            body.extend_from_slice(extra);
        } else {
            body.extend_from_slice(&ext.extra);
        }
    }
    body.extend_from_slice(call_data);

    Ok(body.encode())
}

#[cfg(test)]
mod tests {
    use parity_scale_codec::Compact;

    use super::super::test_fixtures;
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/paseo-next-v2-metadata.scale");

    fn fixture_state() -> ChainState {
        ChainState {
            spec_version: 1_000_000,
            transaction_version: 1,
            genesis_hash: [0xab; 32],
            nonce: 0,
            restrict_origins: false,
        }
    }

    #[test]
    fn call_layout_is_pallet_call_period_seq_target() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let call = build_set_statement_store_account_call(&metadata, 7, 0, &[0u8; 32]).unwrap();
        assert_eq!(
            call,
            [
                vec![0x3f, 0x0a],
                7u32.to_le_bytes().to_vec(),
                0u32.to_le_bytes().to_vec(),
                vec![0u8; 32],
            ]
            .concat()
        );
        assert_eq!(
            SetStatementStoreAccountCallArgs::decode(&mut &call[2..]).unwrap(),
            SetStatementStoreAccountCallArgs {
                period: 7,
                seq: 0,
                target: [0; 32],
            }
        );
    }

    #[test]
    fn long_term_storage_call_layout_is_pallet_call_period_counter_account() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let call = build_claim_long_term_storage_call(&metadata, 7, 3, &[0u8; 32]).unwrap();
        assert_eq!(
            call,
            [
                vec![0x3f, 0x0c],
                7u32.to_le_bytes().to_vec(),
                vec![3],
                vec![0u8; 32],
            ]
            .concat()
        );
        assert_eq!(
            ClaimLongTermStorageCallArgs::decode(&mut &call[2..]).unwrap(),
            ClaimLongTermStorageCallArgs {
                period: 7,
                counter: 3,
                account_id: [0; 32],
            }
        );
    }

    /// The collection the extension declares is what the runtime verifies the
    /// proof against, so each collection has to reach a distinct variant index
    /// resolved from metadata rather than a hardcoded one.
    ///
    /// `AsPgas` lives on Asset Hub and is covered separately, against that
    /// chain's own fixture, by `a_full_person_pgas_claim_names_its_own_collection`.
    #[test]
    fn each_collection_reaches_its_own_extension_variant() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let proof = vec![0xEE; 785];

        let mut encoded = Vec::new();
        for collection in PersonhoodCollection::ALL {
            let resources = build_as_resources_extra(&metadata, &proof, 3, 9, collection).unwrap();
            let long_term =
                build_long_term_storage_extra(&metadata, &proof, 3, 9, collection).unwrap();
            // The collection is the last byte of each `AsResources` payload.
            encoded.push((
                collection,
                *resources.last().unwrap(),
                *long_term.last().unwrap(),
            ));
        }

        let [people, lite] = <[_; 2]>::try_from(encoded).unwrap();
        assert_eq!(people.0, PersonhoodCollection::People);
        assert_eq!(lite.0, PersonhoodCollection::LitePeople);
        assert_eq!(
            (people.1, lite.1),
            (0x00, 0x01),
            "MembershipCollection declares People before LitePeople in the fixture"
        );
        assert_eq!((people.2, lite.2), (0x00, 0x01));
    }

    #[test]
    fn as_resources_extra_wraps_proof_as_bytes() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let proof = vec![0xEE; 785];
        let extra =
            build_as_resources_extra(&metadata, &proof, 3, 9, PersonhoodCollection::LitePeople)
                .unwrap();
        // Some(0x01) ‖ variant(0x02) ‖ compact(785)=0x45,0x0c ‖ 785 bytes
        // ‖ ringIndex LE ‖ revision LE ‖ LitePeople.
        assert_eq!(
            extra,
            [
                vec![0x01, 0x02],
                Compact(785u32).encode(),
                proof,
                3u32.to_le_bytes().to_vec(),
                9u32.to_le_bytes().to_vec(),
                vec![0x01],
            ]
            .concat()
        );
        assert_eq!(
            RegisterStatementStoreAllowanceInfo::decode(&mut &extra[2..]).unwrap(),
            RegisterStatementStoreAllowanceInfo {
                proof: vec![0xEE; 785],
                ring_index: 3,
                revision: 9,
                personhood: 1,
            }
        );
    }

    /// `AsPgas::Claim` carries a fifth field the `AsResources` claims do not, the
    /// day, and the variant indices come from Asset Hub rather than the relay.
    #[test]
    fn as_pgas_extra_wraps_proof_and_day() {
        let metadata = test_fixtures::asset_hub();
        let proof = vec![0xEE; 785];
        let extra =
            build_as_pgas_extra(metadata, &proof, 3, 9, 11, PersonhoodCollection::LitePeople)
                .unwrap();
        // Some(0x01) ‖ variant(0x00) ‖ compact(785)=0x45,0x0c ‖ 785 bytes
        // ‖ ringIndex LE ‖ revision LE ‖ LitePeople ‖ day LE.
        assert_eq!(
            extra,
            [
                vec![0x01, 0x00],
                Compact(785u32).encode(),
                proof,
                3u32.to_le_bytes().to_vec(),
                9u32.to_le_bytes().to_vec(),
                vec![0x01],
                11u32.to_le_bytes().to_vec(),
            ]
            .concat()
        );
        assert_eq!(
            ClaimPgasInfo::decode(&mut &extra[2..]).unwrap(),
            ClaimPgasInfo {
                proof: vec![0xEE; 785],
                ring_index: 3,
                revision: 9,
                collection: 1,
                day: 11,
            }
        );
    }

    /// A full person proves PGAS against their own ring, so the claim has to name
    /// that collection. Until Asset Hub had a fixture this was only reachable
    /// live, which left the encoding of the full-person claim untested offline.
    #[test]
    fn a_full_person_pgas_claim_names_its_own_collection() {
        let metadata = test_fixtures::asset_hub();
        let proof = vec![0xEE; 785];

        let people =
            build_as_pgas_extra(metadata, &proof, 3, 9, 11, PersonhoodCollection::People).unwrap();
        let lite =
            build_as_pgas_extra(metadata, &proof, 3, 9, 11, PersonhoodCollection::LitePeople)
                .unwrap();

        assert_eq!(
            ClaimPgasInfo::decode(&mut &people[2..]).unwrap().collection,
            0,
            "Asset Hub declares People before LitePeople",
        );
        assert_eq!(
            ClaimPgasInfo::decode(&mut &lite[2..]).unwrap().collection,
            1
        );
        assert_ne!(people, lite, "the collection has to reach the payload");
    }

    /// The dispatch indices come from Asset Hub's metadata, so the call the claim
    /// submits is only as right as the pallet the fixture declares.
    #[test]
    fn pgas_call_layout_is_pallet_call_slot_target() {
        let metadata = test_fixtures::asset_hub();
        let target = [0x33; 32];

        let call = build_claim_pgas_call(metadata, 5, &target).unwrap();

        assert_eq!(
            call,
            [
                vec![0x63, 0x00],
                5u32.to_le_bytes().to_vec(),
                target.to_vec()
            ]
            .concat()
        );
    }

    /// The `AsPgas::Claim` payload the live runtime declares. Encoding fewer
    /// fields than the runtime expects is accepted locally and then panics inside
    /// `validate_transaction`, which is how the missing `revision` on
    /// `RegisterStatementStoreAllowance` went unnoticed until a live submission.
    #[test]
    fn pgas_info_carries_the_five_declared_fields() {
        let info = ClaimPgasInfo {
            proof: vec![0xaa, 0xbb],
            ring_index: 7,
            revision: 9,
            collection: 1,
            day: 11,
        };

        assert_eq!(
            ClaimPgasInfo::decode(&mut &info.encode()[..]).unwrap(),
            info
        );
        // proof is length-prefixed; the four u32/u8 tail fields follow in order.
        assert_eq!(
            info.encode(),
            [
                vec![0x08, 0xaa, 0xbb],
                7u32.encode(),
                9u32.encode(),
                vec![1],
                11u32.encode()
            ]
            .concat(),
        );
    }

    #[test]
    fn pgas_call_encodes_slot_then_target() {
        let target = [0x33; 32];
        let args = ClaimPgasCallArgs {
            slot_index: 5,
            target,
        };

        assert_eq!(args.encode(), [5u32.encode(), target.to_vec()].concat());
    }

    #[test]
    fn long_term_storage_extra_wraps_revision() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let proof = vec![0xEE; 785];
        let extra = build_long_term_storage_extra(
            &metadata,
            &proof,
            3,
            9,
            PersonhoodCollection::LitePeople,
        )
        .unwrap();
        // Some(0x01) ‖ variant(0x03) ‖ compact(785)=0x45,0x0c ‖ proof
        // ‖ ringIndex LE ‖ revision LE ‖ LitePeople.
        assert_eq!(
            extra,
            [
                vec![0x01, 0x03],
                Compact(785u32).encode(),
                proof,
                3u32.to_le_bytes().to_vec(),
                9u32.to_le_bytes().to_vec(),
                vec![0x01],
            ]
            .concat()
        );
        assert_eq!(
            ClaimLongTermStorageInfo::decode(&mut &extra[2..]).unwrap(),
            ClaimLongTermStorageInfo {
                proof: vec![0xEE; 785],
                ring_index: 3,
                revision: 9,
                personhood: 1,
            }
        );
    }

    /// The extrinsic declares an extension-pipeline version and the ring-VRF proof
    /// is signed over a message beginning with the same byte. The runtime rebuilds
    /// that message to verify, so the two must never diverge.
    #[test]
    fn the_body_and_the_proof_message_declare_the_same_extension_version() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let call = build_set_statement_store_account_call(&metadata, 7, 0, &[0u8; 32]).unwrap();
        let extra = build_as_resources_extra(
            &metadata,
            &[0xEE; 785],
            0,
            0,
            PersonhoodCollection::LitePeople,
        )
        .unwrap();
        let xt = build_unsigned_extrinsic(&metadata, &fixture_state(), &call, &extra).unwrap();

        let body = &xt[compact_prefix_len(&xt)..];
        assert_eq!(body[1], metadata.extension_version());
        // `build_proof_message` prefixes the same byte; recomputing it with a
        // different version would change the hash, so the frozen known-answer
        // test alongside this one is what pins the pairing.
        assert_eq!(
            metadata.extension_version(),
            0,
            "the V14 fixture has no version map"
        );
    }

    #[test]
    fn extrinsic_has_general_v5_preamble_and_embeds_call() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let call = build_set_statement_store_account_call(&metadata, 7, 0, &[0u8; 32]).unwrap();
        let extra = build_as_resources_extra(
            &metadata,
            &[0xEE; 785],
            0,
            0,
            PersonhoodCollection::LitePeople,
        )
        .unwrap();
        let xt = build_unsigned_extrinsic(&metadata, &fixture_state(), &call, &extra).unwrap();

        // Strip the compact length prefix and check the body head + tail.
        let body = &xt[compact_prefix_len(&xt)..];
        assert_eq!(
            &body[..2],
            &[GENERAL_V5_PREAMBLE, metadata.extension_version()]
        );
        assert_eq!(&body[body.len() - call.len()..], &call[..]);
        // The Some(info) extra appears verbatim in the body.
        assert!(
            body.windows(extra.len()).any(|w| w == extra),
            "AsResources Some(info) extra should appear in the body",
        );
    }

    /// Length of the SCALE compact prefix at the head of `xt`.
    fn compact_prefix_len(xt: &[u8]) -> usize {
        match xt[0] & 0b11 {
            0b00 => 1,
            0b01 => 2,
            0b10 => 4,
            _ => 5,
        }
    }
}
