// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer core/crates/brevity-ffi/src/coinage_transfer.rs tests.
// Copyright the Brevity contributors. See truapi-coinage/NOTICE and LICENSE.

use super::transaction::{AS_COINAGE, Snapshot, constant, mortal_era, replace};
use super::*;
use parity_scale_codec::{Compact, Decode, Encode};
use scale_info::{PortableRegistry, TypeDef, TypeDefPrimitive};
use serde_json::json;
use subxt::ext::scale_encode::{EncodeAsFields, Field};
use subxt::ext::scale_value::{Primitive, Value as ScaleValue};

const METADATA: &[u8] = include_bytes!("../../../tests/fixtures/paseo-next-v2-metadata-v16.scale");

fn snapshot() -> Snapshot {
    Snapshot::new(
        METADATA.to_vec(),
        [3; 32],
        1_025,
        [4; 32],
        3_000_000,
        1,
        Some(0),
    )
    .unwrap()
}
fn source() -> schnorrkel::Keypair {
    schnorrkel::MiniSecretKey::from_bytes(&[17; 32])
        .unwrap()
        .expand_to_keypair(schnorrkel::ExpansionMode::Ed25519)
}
fn request() -> ExternalCoinTransferRequest {
    let key = source();
    ExternalCoinTransferRequest {
        source_secret: truapi_coinage::MemoEntry(key.secret.to_bytes()),
        source_public: key.public.to_bytes(),
        recipient: [9; 32],
        exponent: 2,
        asset_unit: 10,
        amount_planks: 40,
    }
}

#[test]
fn preflight_rejects_denomination_drift_and_reused_destination() {
    let context = DenominationBreakdownContext {
        asset_unit: 10,
        min_exponent: 0,
        max_exponent: 4,
        precision: 2,
    };
    let live = Some(OnChainCoin {
        exponent: 2,
        age: 3,
    });
    let mut request = request();
    assert!(validate_live_transfer(&request, &context, &[live, None]).is_ok());
    request.asset_unit = 11;
    assert!(validate_live_transfer(&request, &context, &[live, None]).is_err());
    request.asset_unit = 10;
    request.amount_planks = 41;
    assert!(validate_live_transfer(&request, &context, &[live, None]).is_err());
    request.amount_planks = 40;
    assert!(validate_live_transfer(&request, &context, &[None, None]).is_err());
    assert!(validate_live_transfer(&request, &context, &[live, live]).is_err());
}

#[test]
fn finality_requires_consumed_source_and_exact_recipient_denomination() {
    let request = request();
    let coin = OnChainCoin {
        exponent: 2,
        age: 0,
    };
    assert_eq!(
        validate_finalized_transfer(&request, &[None, Some(coin)]).unwrap(),
        coin
    );
    assert!(validate_finalized_transfer(&request, &[Some(coin), Some(coin)]).is_err());
    assert!(validate_finalized_transfer(&request, &[None, None]).is_err());
    assert!(
        validate_finalized_transfer(
            &request,
            &[
                None,
                Some(OnChainCoin {
                    exponent: 1,
                    age: 0
                })
            ]
        )
        .is_err()
    );
}

#[test]
fn storage_snapshot_requires_explicit_rows_and_requested_finalized_hash() {
    let at = [7; 32];
    let keys = vec![vec![1], vec![2]];
    let good = json!([{"block":hex0x(&at),"changes":[["0x02",null],["0x01","0xab"]]}]);
    assert_eq!(
        rpc::decode_query(&good, &keys, at).unwrap(),
        vec![Some(vec![0xab]), None]
    );
    let missing = json!([{"block":hex0x(&at),"changes":[["0x01","0xab"]]}]);
    assert!(rpc::decode_query(&missing, &keys, at).is_err());
    assert!(rpc::decode_query(&good, &keys, [8; 32]).is_err());
    let duplicate =
        json!([{"block":hex0x(&at),"changes":[["0x01",null],["0x01",null],["0x02",null]]}]);
    assert!(rpc::decode_query(&duplicate, &keys, at).is_err());
}

#[test]
fn free_unload_allowance_rejects_an_ambiguous_same_width_runtime_type() {
    use crate::runtime::statement_allowance::rpc::testing::ScriptedRpc;
    let mut metadata = frame_metadata::RuntimeMetadataPrefixed::decode(&mut &METADATA[..]).unwrap();
    let frame_metadata::RuntimeMetadata::V16(runtime) = &mut metadata.1 else {
        panic!("expected V16 fixture");
    };
    let u64_type = runtime
        .types
        .types
        .iter()
        .find(|ty| matches!(ty.ty.type_def, TypeDef::Primitive(TypeDefPrimitive::U64)))
        .unwrap()
        .id;
    let function = runtime
        .pallets
        .iter_mut()
        .find(|p| p.name == "Coinage")
        .unwrap()
        .view_functions
        .iter_mut()
        .find(|function| function.name == "get_free_unload_token_info")
        .unwrap();
    // A u64 response has the same eight bytes as (u32, u32), but must not be
    // interpreted as two origin-specific spending allowances.
    function.output = u64_type.into();
    let snapshot = Snapshot::new(
        metadata.encode(),
        [3; 32],
        1_025,
        [4; 32],
        3_000_000,
        1,
        Some(0),
    )
    .unwrap();
    let rpc = subxt_rpcs::RpcClient::new(ScriptedRpc::default());
    assert!(futures::executor::block_on(rpc::free_unload_token_limits(&rpc, &snapshot)).is_err());
}

#[test]
fn coin_queries_follow_the_queried_blocks_storage_hashers() {
    use frame_metadata::v16::{StorageEntryType, StorageHasher};
    let owner = [17; 32];
    for hasher in [StorageHasher::Blake2_128Concat, StorageHasher::Twox64Concat] {
        let mut metadata =
            frame_metadata::RuntimeMetadataPrefixed::decode(&mut &METADATA[..]).unwrap();
        let frame_metadata::RuntimeMetadata::V16(runtime) = &mut metadata.1 else {
            panic!("V16 fixture")
        };
        let storage = runtime
            .pallets
            .iter_mut()
            .find(|p| p.name == "Coinage")
            .unwrap()
            .storage
            .as_mut()
            .unwrap();
        let entry = storage
            .entries
            .iter_mut()
            .find(|entry| entry.name == "CoinsByOwner")
            .unwrap();
        let StorageEntryType::Map { hashers, .. } = &mut entry.ty else {
            panic!("coin map")
        };
        hashers[0] = hasher.clone();
        let snapshot = Snapshot::new(
            metadata.encode(),
            [3; 32],
            1_025,
            [4; 32],
            3_000_000,
            1,
            Some(0),
        )
        .unwrap();
        let requested = snapshot
            .storage
            .coinage_key(&truapi_coinage::CoinageStorageKey::Coin(owner))
            .unwrap();
        let mut stored_key = sp_crypto_hashing::twox_128(b"Coinage").to_vec();
        stored_key.extend_from_slice(&sp_crypto_hashing::twox_128(b"CoinsByOwner"));
        match hasher {
            StorageHasher::Blake2_128Concat => {
                stored_key.extend_from_slice(&sp_crypto_hashing::blake2_128(&owner))
            }
            StorageHasher::Twox64Concat => {
                stored_key.extend_from_slice(&sp_crypto_hashing::twox_64(&owner))
            }
            _ => unreachable!(),
        }
        stored_key.extend_from_slice(&owner);
        // A storage backend returns None for an obsolete physical key. A live
        // coin must remain visible across this metadata-only runtime change.
        let row = (requested == stored_key).then(|| (3i8, 5u16).encode());
        assert_eq!(
            row.as_deref()
                .map(decode_exact::<(i8, u16)>)
                .transpose()
                .unwrap(),
            Some((3, 5))
        );
    }
}

#[test]
fn coin_queries_reject_incompatible_metadata_key_types() {
    let mut metadata = frame_metadata::RuntimeMetadataPrefixed::decode(&mut &METADATA[..]).unwrap();
    let frame_metadata::RuntimeMetadata::V16(runtime) = &mut metadata.1 else {
        panic!("V16 fixture")
    };
    let integer = runtime
        .types
        .types
        .iter()
        .find(|ty| matches!(ty.ty.type_def, TypeDef::Primitive(TypeDefPrimitive::U64)))
        .unwrap()
        .id;
    let entry = runtime
        .pallets
        .iter_mut()
        .find(|p| p.name == "Coinage")
        .unwrap()
        .storage
        .as_mut()
        .unwrap()
        .entries
        .iter_mut()
        .find(|entry| entry.name == "CoinsByOwner")
        .unwrap();
    let frame_metadata::v16::StorageEntryType::Map { key, .. } = &mut entry.ty else {
        panic!("coin map")
    };
    key.id = integer;
    assert!(
        Snapshot::new(
            metadata.encode(),
            [3; 32],
            1_025,
            [4; 32],
            3_000_000,
            1,
            Some(0)
        )
        .is_err()
    );
}

#[test]
fn instance_runtime_never_guesses_an_asset_and_rejects_other_assets() {
    assert!(
        Snapshot::new(
            METADATA.to_vec(),
            [3; 32],
            1_025,
            [4; 32],
            3_000_000,
            1,
            None
        )
        .is_err()
    );
    let snapshot = snapshot();
    assert!(
        snapshot.denomination_context().is_err(),
        "instance asset unit must come from storage at the queried block"
    );
    let key = truapi_coinage::CoinageStorageKey::Coin([7; 32]);
    let mut selected = (0u32, 3i8, 5u16).encode();
    snapshot
        .storage
        .normalize_value(&key, &mut selected)
        .unwrap();
    assert_eq!(decode_exact::<(i8, u16)>(&selected).unwrap(), (3, 5));
    let mut foreign = (1u32, 3i8, 5u16).encode();
    assert!(
        snapshot
            .storage
            .normalize_value(&key, &mut foreign)
            .is_err()
    );
    let recycler = truapi_coinage::CoinageStorageKey::Recycler([7; 32]);
    let mut selected = (0u32, 3i8).encode();
    snapshot
        .storage
        .normalize_value(&recycler, &mut selected)
        .unwrap();
    assert_eq!(decode_exact::<i8>(&selected).unwrap(), 3);
    let mut foreign = (1u32, 3i8).encode();
    assert!(
        snapshot
            .storage
            .normalize_value(&recycler, &mut foreign)
            .is_err()
    );
}

#[test]
fn read_only_denominations_use_the_selected_finalized_instance_and_reject_invalid_units() {
    use crate::test_support::{StubPlatform, test_spawner};
    use futures::executor::block_on;

    block_on(async {
        let snapshot = snapshot();
        let metadata = frame_metadata::RuntimeMetadataPrefixed::decode(&mut &METADATA[..]).unwrap();
        let frame_metadata::RuntimeMetadata::V16(runtime) = metadata.1 else {
            panic!("V16 fixture")
        };
        let entry = runtime
            .pallets
            .iter()
            .find(|p| p.name == "Coinage")
            .unwrap()
            .storage
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .find(|e| e.name == "Instances")
            .unwrap();
        let frame_metadata::v16::StorageEntryType::Map { value, .. } = &entry.ty else {
            panic!("instance map")
        };
        let TypeDef::Composite(instance_fields) =
            &runtime.types.resolve(value.id).unwrap().type_def
        else {
            panic!("instance record")
        };
        let instance_key = snapshot.instance_asset_key().unwrap().unwrap();
        let minimum: i8 = constant(&snapshot.metadata, "Coinage", "MinimumExponent").unwrap();
        let unit = 3u128
            .checked_shl(if minimum < 0 {
                u32::from(minimum.unsigned_abs())
            } else {
                1
            })
            .unwrap();
        let platform = |unit: u128| {
            // Encode the actual metadata's XCM asset and remaining fields, rather
            // than assuming an asset ID integer or a fixed record layout.
            let values =
                ScaleValue::unnamed_composite(instance_fields.fields.iter().map(|field| {
                    if field.name.as_deref() == Some("asset_unit") {
                        ScaleValue::u128(unit)
                    } else {
                        default_value(&runtime.types, field.ty.id)
                    }
                }));
            let mut fields = instance_fields
                .fields
                .iter()
                .map(|field| Field::new(field.ty.id, field.name.as_deref()));
            let mut row = Vec::new();
            values
                .encode_as_fields_to(&mut fields, &runtime.types, &mut row)
                .unwrap();
            Arc::new(StubPlatform {
                rpc_method_responses: vec![
                    ("chain_getBlockHash", json!(hex0x(&[4; 32])).to_string()),
                    ("chain_getFinalizedHead", json!(hex0x(&[3; 32])).to_string()),
                    ("chain_getHeader", json!({"number": "0x401"}).to_string()),
                    (
                        "state_getRuntimeVersion",
                        json!({"specVersion": 3_000_000, "transactionVersion": 1}).to_string(),
                    ),
                    (
                        "Metadata_metadata_at_version",
                        json!(hex0x(&Some(METADATA.to_vec()).encode())).to_string(),
                    ),
                    (
                        "state_queryStorageAt",
                        json!([{
                            "block": hex0x(&[3; 32]),
                            "changes": [[hex0x(&instance_key), hex0x(&row)]],
                        }])
                        .to_string(),
                    ),
                ],
                ..Default::default()
            })
        };
        let source = platform(unit);
        let denominations = HostCoinageChain::selected_denomination_context(
            source.as_ref(),
            [4; 32],
            Some(0),
            &|| true,
            test_spawner(),
        )
        .await
        .unwrap();
        assert_eq!(denominations.asset_unit, unit);
        assert_eq!(denominations.cash_cents_from_planks(unit * 123), Some(123));
        assert_eq!(denominations.cash_cents_from_planks(unit * 123 - 1), None);
        assert_eq!(denominations.cash_cents_to_planks(u128::MAX), None);
        // Selecting a different instance must not reuse instance zero's unit.
        assert!(
            HostCoinageChain::selected_denomination_context(
                source.as_ref(),
                [4; 32],
                Some(1),
                &|| true,
                test_spawner(),
            )
            .await
            .is_err()
        );
        for invalid_unit in [0, u128::MAX] {
            assert!(
                HostCoinageChain::selected_denomination_context(
                    platform(invalid_unit).as_ref(),
                    [4; 32],
                    Some(0),
                    &|| true,
                    test_spawner(),
                )
                .await
                .is_err()
            );
        }
        assert!(
            HostCoinageChain::selected_denomination_context(
                source.as_ref(),
                [4; 32],
                Some(0),
                &|| false,
                test_spawner(),
            )
            .await
            .is_err()
        );
    });
}

#[test]
fn presence_only_recycler_runtime_is_rejected_before_queries_or_spending() {
    // This older snapshot predates RecyclerAliasStates entirely. Treating its
    // missing lock-state map as empty would make unsafe recovery decisions.
    let metadata = include_bytes!("../../../tests/fixtures/paseo-next-v2-metadata.scale");
    assert!(
        Snapshot::new(
            metadata.to_vec(),
            [3; 32],
            1_025,
            [4; 32],
            1_000_032,
            1,
            None
        )
        .is_err()
    );
}

#[test]
fn coin_signature_binds_runtime_coin_origin_nonce_checkpoint_and_call() {
    let snapshot = snapshot();
    let source = source();
    let [pallet, index] = snapshot
        .metadata
        .call_indices("Coinage", "transfer")
        .unwrap();
    let call = truapi_coinage::pallet::transfer_call(pallet, index, &[9; 32]);
    let transaction = snapshot.signed(&source, &call, 7).unwrap();
    let mut extensions = snapshot.extensions(7).unwrap();
    replace(
        &mut extensions,
        AS_COINAGE,
        snapshot.as_coin().unwrap(),
        vec![],
    )
    .unwrap();
    let verify = extensions
        .iter()
        .position(|extension| extension.id == "VerifyMultiSignature")
        .unwrap();
    let mut body = transaction.as_slice();
    let size = Compact::<u32>::decode(&mut body).unwrap().0 as usize;
    assert_eq!(size, body.len());
    let offset = 2 + extensions[..verify]
        .iter()
        .map(|extension| extension.extra.len())
        .sum::<usize>();
    let signature = schnorrkel::Signature::from_bytes(&body[offset + 2..offset + 66]).unwrap();
    let mut implication = vec![snapshot.metadata.extension_version()];
    implication.extend_from_slice(&call);
    for extension in &extensions[verify + 1..] {
        implication.extend_from_slice(&extension.extra);
    }
    for extension in &extensions[verify + 1..] {
        implication.extend_from_slice(&extension.additional_signed);
    }
    source
        .public
        .verify_simple(
            b"substrate",
            &sp_crypto_hashing::blake2_256(&implication),
            &signature,
        )
        .unwrap();
    *implication.last_mut().unwrap() ^= 1;
    assert!(
        source
            .public
            .verify_simple(
                b"substrate",
                &sp_crypto_hashing::blake2_256(&implication),
                &signature
            )
            .is_err()
    );
    assert!(body.ends_with(&call));
}

#[test]
fn mortal_era_expires_before_the_wal_releases_its_inputs() {
    let snapshot = snapshot();
    let era = u16::from_le_bytes(mortal_era(snapshot.period, snapshot.number).unwrap());
    let period = 2u64 << (era & 15);
    let phase = u64::from(era >> 4);
    assert_eq!(period, snapshot.period);
    assert_eq!(phase, snapshot.number % period);
    assert!(snapshot.valid_until() <= snapshot.number + truapi_coinage::WAL_MORTALITY_BLOCKS);
    assert!(mortal_era(512, 1_025).is_err());
    assert!(mortal_era(3, 1_025).is_err());
}

#[test]
fn exact_scale_constant_decode_rejects_trailing_bytes() {
    assert_eq!(decode_exact::<u32>(&7u32.encode()).unwrap(), 7);
    let mut bytes = 7u32.encode();
    bytes.push(0);
    assert!(decode_exact::<u32>(&bytes).is_err());
    let snapshot = snapshot();
    let period: u32 = constant(
        &snapshot.metadata,
        "Coinage",
        "UnloadTokenTimePeriodPeopleLitePeople",
    )
    .unwrap();
    assert_ne!(period, 0);
    let key = transaction::storage_key(
        &snapshot.storage,
        "Coinage",
        "ConsumedFreeUnloadTokens",
        &[7u32.encode(), [8u8; 32].encode()],
    )
    .unwrap();
    let other = transaction::storage_key(
        &snapshot.storage,
        "Coinage",
        "ConsumedFreeUnloadTokens",
        &[8u32.encode(), [8u8; 32].encode()],
    )
    .unwrap();
    assert_ne!(key, other);
}

#[test]
fn only_explicit_success_at_the_submitted_extrinsic_index_completes() {
    let snapshot = snapshot();
    assert!(
        rpc::dispatch_success(
            &snapshot,
            1_026,
            events(&snapshot, "ExtrinsicSuccess", 3),
            3
        )
        .is_ok()
    );
    assert!(
        rpc::dispatch_success(
            &snapshot,
            1_026,
            events(&snapshot, "ExtrinsicSuccess", 2),
            3
        )
        .is_err()
    );
    assert!(
        rpc::dispatch_success(&snapshot, 1_026, events(&snapshot, "ExtrinsicFailed", 3), 3)
            .is_err()
    );
    assert!(rpc::dispatch_success(&snapshot, 1_026, Compact(0u32).encode(), 3).is_err());
}

#[test]
fn expired_session_cannot_derive_voucher_material_or_create_proofs() {
    let crypto = HostVoucherCryptography::new(Arc::new(|| false));
    let seed = truapi_coinage::VoucherSeed([1; 32]);
    assert!(crypto.member_key(&seed).is_err());
    assert!(crypto.sign(&seed, b"message").is_err());
    assert!(crypto.alias(&seed, b"context").is_err());
    assert!(
        crypto
            .ring_vrf_proof(&seed, 9, &[], b"context", b"message")
            .is_err()
    );
}

#[test]
fn voucher_ownership_signature_verifies_and_ring_proof_rejects_nonmembers() {
    use verifiable::{GenerateVerifiable, ring::bandersnatch::BandersnatchVrfVerifiable};
    let crypto = HostVoucherCryptography::new(Arc::new(|| true));
    let seed = truapi_coinage::VoucherSeed([7; 32]);
    let member = crypto.member_key(&seed).unwrap();
    let signature = crypto.sign(&seed, b"coin-owner").unwrap();
    assert!(BandersnatchVrfVerifiable::verify_signature(
        &signature,
        b"coin-owner",
        &member
    ));
    assert!(!BandersnatchVrfVerifiable::verify_signature(
        &signature,
        b"different-owner",
        &member
    ));
    let outsider = crypto
        .member_key(&truapi_coinage::VoucherSeed([8; 32]))
        .unwrap();
    assert!(
        crypto
            .ring_vrf_proof(&seed, 9, &[outsider], b"recycler-context", b"implication")
            .is_err()
    );
}

fn events(snapshot: &Snapshot, name: &str, index: u32) -> Vec<u8> {
    let system = snapshot.subxt.pallet_by_name("System").unwrap();
    let event = system
        .event_variants()
        .unwrap()
        .iter()
        .find(|event| event.name == name)
        .unwrap();
    let values = ScaleValue::unnamed_composite(
        event
            .fields
            .iter()
            .map(|field| default_value(snapshot.subxt.types(), field.ty.id)),
    );
    let mut fields = event
        .fields
        .iter()
        .map(|field| Field::new(field.ty.id, field.name.as_deref()));
    let mut bytes = Compact(1u32).encode();
    subxt::events::Phase::ApplyExtrinsic(index).encode_to(&mut bytes);
    system.event_index().encode_to(&mut bytes);
    event.index.encode_to(&mut bytes);
    values
        .encode_as_fields_to(&mut fields, snapshot.subxt.types(), &mut bytes)
        .unwrap();
    Vec::<[u8; 32]>::new().encode_to(&mut bytes);
    bytes
}

fn default_value(types: &PortableRegistry, id: u32) -> ScaleValue {
    match &types.resolve(id).unwrap().type_def {
        TypeDef::Composite(value) => ScaleValue::unnamed_composite(
            value
                .fields
                .iter()
                .map(|field| default_value(types, field.ty.id)),
        ),
        TypeDef::Tuple(value) => ScaleValue::unnamed_composite(
            value
                .fields
                .iter()
                .map(|field| default_value(types, field.id)),
        ),
        TypeDef::Variant(value) => {
            let variant = value.variants.first().unwrap();
            ScaleValue::unnamed_variant(
                variant.name.clone(),
                variant
                    .fields
                    .iter()
                    .map(|field| default_value(types, field.ty.id)),
            )
        }
        TypeDef::Sequence(_) => ScaleValue::unnamed_composite([]),
        TypeDef::Array(value) => ScaleValue::unnamed_composite(
            (0..value.len).map(|_| default_value(types, value.type_param.id)),
        ),
        TypeDef::Compact(_) => ScaleValue::u128(0),
        TypeDef::BitSequence(_) => ScaleValue::bit_sequence(subxt::ext::scale_bits::Bits::new()),
        TypeDef::Primitive(value) => match value {
            TypeDefPrimitive::Bool => ScaleValue::bool(false),
            TypeDefPrimitive::Char => ScaleValue::char('\0'),
            TypeDefPrimitive::Str => ScaleValue::string(""),
            TypeDefPrimitive::U8
            | TypeDefPrimitive::U16
            | TypeDefPrimitive::U32
            | TypeDefPrimitive::U64
            | TypeDefPrimitive::U128 => ScaleValue::u128(0),
            TypeDefPrimitive::I8
            | TypeDefPrimitive::I16
            | TypeDefPrimitive::I32
            | TypeDefPrimitive::I64
            | TypeDefPrimitive::I128 => ScaleValue::i128(0),
            TypeDefPrimitive::U256 => ScaleValue::primitive(Primitive::U256([0; 32])),
            TypeDefPrimitive::I256 => ScaleValue::primitive(Primitive::I256([0; 32])),
        },
    }
}
