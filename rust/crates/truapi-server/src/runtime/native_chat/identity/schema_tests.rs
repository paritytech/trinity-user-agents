// SPDX-License-Identifier: AGPL-3.0-only
// Metadata fixtures adapted from the deployed polkavm-chat-v2 recipient tests.

use super::*;
use frame_metadata::v14::{
    ExtrinsicMetadata, PalletMetadata, PalletStorageMetadata, RuntimeMetadataV14,
    StorageEntryMetadata, StorageEntryModifier,
};
use parity_scale_codec::Encode;
use scale_info::{
    Path, PortableRegistryBuilder, Type, TypeDefArray, TypeDefSequence, build::Fields,
};

fn metadata(encoding: ByteEncoding, first_field: &str, account_len: u32) -> Vec<u8> {
    let mut types = PortableRegistryBuilder::new();
    let byte = types.register_type(Type::new(
        Path::default(),
        vec![],
        TypeDefPrimitive::U8,
        vec![],
    ));
    let account = types.register_type(Type::new(
        Path::default(),
        vec![],
        TypeDefArray::new(account_len, byte.into()),
        vec![],
    ));
    let sequence = types.register_type(Type::new(
        Path::default(),
        vec![],
        TypeDefSequence::new(byte.into()),
        vec![],
    ));
    let identifier = match encoding {
        ByteEncoding::Fixed(length) => types.register_type(Type::new(
            Path::default(),
            vec![],
            TypeDefArray::new(length, byte.into()),
            vec![],
        )),
        ByteEncoding::Sequence => types.register_type(
            Type::builder_portable()
                .path(Path::from_segments_unchecked(["BoundedBytes".into()]))
                .composite(Fields::unnamed().field_portable(|field| field.ty(sequence))),
        ),
    };
    let consumer = types.register_type(
        Type::builder_portable()
            .path(Path::from_segments_unchecked(["ConsumerInfo".into()]))
            .composite(
                Fields::named()
                    .field_portable(|field| field.name(first_field.into()).ty(identifier))
                    .field_portable(|field| field.name("unrelated_tail".into()).ty(sequence)),
            ),
    );
    RuntimeMetadataPrefixed(
        META_RESERVED,
        RuntimeMetadata::V14(RuntimeMetadataV14 {
            types: types.finish(),
            pallets: vec![PalletMetadata {
                name: "Resources".into(),
                storage: Some(PalletStorageMetadata {
                    prefix: "Resources".into(),
                    entries: vec![StorageEntryMetadata {
                        name: "Consumers".into(),
                        modifier: StorageEntryModifier::Optional,
                        ty: StorageEntryType::Map {
                            hashers: vec![StorageHasher::Blake2_128Concat],
                            key: account.into(),
                            value: consumer.into(),
                        },
                        default: vec![0],
                        docs: vec![],
                    }],
                }),
                calls: None,
                event: None,
                constants: vec![],
                error: None,
                index: 10,
            }],
            extrinsic: ExtrinsicMetadata {
                ty: byte.into(),
                version: 4,
                signed_extensions: vec![],
            },
            ty: byte.into(),
        }),
    )
    .encode()
}

fn container() -> Vec<u8> {
    let mut result = vec![0; 65];
    result[1..33].copy_from_slice(&[0x37; 32]);
    result
}

#[test]
fn metadata_selects_raw_or_container_without_guessing_from_storage_length() {
    let fixed =
        ConsumerSchema::new(&metadata(ByteEncoding::Fixed(65), "identifier_key", 32)).unwrap();
    assert_eq!(fixed.identifier(&container()).unwrap(), [0x37; 32]);
    let vector =
        ConsumerSchema::new(&metadata(ByteEncoding::Sequence, "identifier_key", 32)).unwrap();
    assert_eq!(
        vector.identifier(&container().encode()).unwrap(),
        [0x37; 32]
    );
    assert!(vector.identifier(&container()).is_err());
    let raw =
        ConsumerSchema::new(&metadata(ByteEncoding::Fixed(32), "identifier_key", 32)).unwrap();
    let mut expected = [0x37; 32];
    expected[0] = 0;
    assert_eq!(raw.identifier(&container()).unwrap(), expected);
    let mut extended = vec![0x42; 32];
    extended.extend_from_slice(&[0; 40]);
    assert_eq!(raw.identifier(&extended).unwrap(), [0x42; 32]);
}

#[test]
fn undeclared_identifier_account_layout_and_malformed_keys_are_rejected() {
    assert!(ConsumerSchema::new(&metadata(ByteEncoding::Fixed(64), "identifier_key", 32)).is_err());
    assert!(ConsumerSchema::new(&metadata(ByteEncoding::Fixed(65), "unrelated", 32)).is_err());
    assert!(ConsumerSchema::new(&metadata(ByteEncoding::Fixed(65), "identifier_key", 20)).is_err());
    let schema =
        ConsumerSchema::new(&metadata(ByteEncoding::Fixed(65), "identifier_key", 32)).unwrap();
    let mut wrong_type = container();
    wrong_type[0] = 1;
    assert!(schema.identifier(&wrong_type).is_err());
    let mut reserved = container();
    reserved[64] = 1;
    assert_eq!(schema.identifier(&reserved).unwrap(), [0x37; 32]);
    assert!(schema.identifier(&container()[..64]).is_err());
    assert!(schema.identifier(&[0; 65]).is_err());
    assert!(
        ByteEncoding::Sequence
            .decode_identifier(&Compact(u32::MAX).encode())
            .is_err()
    );
    let mut malformed = metadata(ByteEncoding::Fixed(65), "identifier_key", 32);
    malformed.push(0);
    assert!(ConsumerSchema::new(&malformed).is_err());
    assert!(decode_all::<Vec<u8>>(&Compact(u32::MAX).encode()).is_err());
}

#[test]
fn zero_low_order_and_noncanonical_x25519_aliases_never_authenticate() {
    let mut basepoint = [0; 32];
    basepoint[0] = 9;
    assert!(validate_public_key(&basepoint).is_ok());
    let mut masked_alias = basepoint;
    masked_alias[31] = 0x80;
    assert!(validate_public_key(&masked_alias).is_err());
    let mut p = [0xff; 32];
    p[0] = 0xed;
    p[31] = 0x7f;
    assert!(validate_public_key(&p).is_err());
    p[0] += 9;
    assert!(validate_public_key(&p).is_err());
    let mut one = [0; 32];
    one[0] = 1;
    assert!(validate_public_key(&one).is_err());
    assert!(validate_public_key(&[0; 32]).is_err());
}

#[test]
fn legacy_username_index_requires_its_declared_account_layout() {
    let bytes = metadata(ByteEncoding::Fixed(65), "identifier_key", 32);
    assert!(UsernameOwnerSchema::new(&bytes).unwrap().is_none());
    let mut decoded: RuntimeMetadataPrefixed = decode_all(&bytes).unwrap();
    let RuntimeMetadata::V14(runtime) = &mut decoded.1 else {
        unreachable!()
    };
    let label = runtime.types.types.iter().find(|ty| {
        matches!(&ty.ty.type_def, TypeDef::Sequence(sequence) if is_u8(&runtime.types, sequence.type_param.id))
    }).unwrap().id;
    let entry = &mut runtime.pallets[0].storage.as_mut().unwrap().entries[0];
    let StorageEntryType::Map { key, value, .. } = &mut entry.ty else {
        unreachable!()
    };
    *value = *key;
    *key = label.into();
    entry.name = "UsernameOwnerOf".into();
    let schema = UsernameOwnerSchema::new(&decoded.encode())
        .unwrap()
        .unwrap();
    assert_ne!(
        schema.key("alice.01").unwrap(),
        schema.key("alice.1").unwrap()
    );
    let RuntimeMetadata::V14(runtime) = &mut decoded.1 else {
        unreachable!()
    };
    let entry = &mut runtime.pallets[0].storage.as_mut().unwrap().entries[0];
    let StorageEntryType::Map { value, .. } = &mut entry.ty else {
        unreachable!()
    };
    *value = label.into();
    assert!(UsernameOwnerSchema::new(&decoded.encode()).is_err());
}
