// SPDX-License-Identifier: AGPL-3.0-only
// Metadata layout and bounded SCALE decoder adapted from polkavm-app-kit
// polkavm-chat-v2/src/recipient.rs (57b236fe9e740c83d0ead3d22cc7ca5a85e4ad17).

use super::rpc::MAX_METADATA_BYTES;
use frame_metadata::v14::{StorageEntryType, StorageHasher};
use frame_metadata::{META_RESERVED, RuntimeMetadata, RuntimeMetadataPrefixed};
use parity_scale_codec::{Compact, Decode, Encode, Input};
use scale_info::{PortableRegistry, TypeDef, TypeDefPrimitive, form::PortableForm};
use sp_crypto_hashing::{blake2_128, blake2_256, twox_64, twox_128, twox_256};

pub(super) struct ConsumerSchema {
    map: StorageMap,
    identifier: ByteEncoding,
}

impl ConsumerSchema {
    pub(super) fn new(bytes: &[u8]) -> Result<Self, String> {
        let metadata = PalletSchema::new(bytes, "Resources")?;
        let StorageEntryType::Map {
            hashers,
            key,
            value,
        } = metadata.entry("Consumers")?
        else {
            return Err("Resources.Consumers is not a map".into());
        };
        if hashers.len() != 1
            || ByteEncoding::from_type(&metadata.types, key.id)? != ByteEncoding::Fixed(32)
        {
            return Err("Resources.Consumers must declare a single AccountId32 key".into());
        }
        let identifier = identifier_encoding(&metadata.types, value.id)?;
        Ok(Self {
            map: StorageMap::new(&metadata.prefix, "Consumers", hashers[0].clone()),
            identifier,
        })
    }

    pub(super) fn key(&self, account: &[u8; 32]) -> Vec<u8> {
        self.map.key(account)
    }

    pub(super) fn identifier(&self, value: &[u8]) -> Result<[u8; 32], String> {
        self.identifier.decode_identifier(value)
    }
}

/// Older People runtimes retained a username -> AccountId32 index. It is only
/// a candidate source; Asset Hub's finalized dotNS registry remains authority.
pub(super) struct UsernameOwnerSchema {
    map: StorageMap,
    label: ByteEncoding,
}

impl UsernameOwnerSchema {
    pub(super) fn new(bytes: &[u8]) -> Result<Option<Self>, String> {
        let metadata = PalletSchema::new(bytes, "Resources")?;
        if !metadata
            .entries
            .iter()
            .any(|(name, _)| name == "UsernameOwnerOf")
        {
            return Ok(None);
        }
        let StorageEntryType::Map {
            hashers,
            key,
            value,
        } = metadata.entry("UsernameOwnerOf")?
        else {
            return Err("Resources.UsernameOwnerOf is not a map".into());
        };
        if hashers.len() != 1
            || ByteEncoding::from_type(&metadata.types, value.id)? != ByteEncoding::Fixed(32)
        {
            return Err(
                "Resources.UsernameOwnerOf must declare AccountId32 owners and one key".into(),
            );
        }
        Ok(Some(Self {
            map: StorageMap::new(&metadata.prefix, "UsernameOwnerOf", hashers[0].clone()),
            label: ByteEncoding::from_type(&metadata.types, key.id)?,
        }))
    }

    pub(super) fn key(&self, username: &str) -> Result<Vec<u8>, String> {
        let encoded = match self.label {
            ByteEncoding::Sequence => username.as_bytes().encode(),
            ByteEncoding::Fixed(length) if length as usize == username.len() => {
                username.as_bytes().to_vec()
            }
            ByteEncoding::Fixed(_) => {
                return Err("username length differs from legacy directory key".into());
            }
        };
        Ok(self.map.key(&encoded))
    }
}

pub(super) struct GatewaySchema {
    lite_owner: Option<(StorageMap, ByteEncoding)>,
}

impl GatewaySchema {
    pub(super) fn lite_owner_key(&self, label: &str) -> Result<Option<Vec<u8>>, String> {
        let Some((map, encoding)) = &self.lite_owner else {
            return Ok(None);
        };
        let encoded = match encoding {
            ByteEncoding::Sequence => label.as_bytes().encode(),
            ByteEncoding::Fixed(length) if *length as usize == label.len() => {
                label.as_bytes().to_vec()
            }
            ByteEncoding::Fixed(_) => {
                return Err("lite username length does not match declared directory key".into());
            }
        };
        Ok(Some(map.key(&encoded)))
    }
}

/// The shared dotNS discovery helper uses this declared plain H160 layout.
/// Refuse a renamed/retyped entry instead of interpreting arbitrary bytes as
/// the configured directory contract address.
pub(super) fn validate_gateway(bytes: &[u8]) -> Result<GatewaySchema, String> {
    let metadata = PalletSchema::new(bytes, "DotnsGateway")?;
    let StorageEntryType::Plain(value) = metadata.entry("DispatcherAddress")? else {
        return Err("DotnsGateway.DispatcherAddress is not plain storage".into());
    };
    if metadata.prefix != "DotnsGateway"
        || ByteEncoding::from_type(&metadata.types, value.id)? != ByteEncoding::Fixed(20)
    {
        return Err(
            "DotnsGateway.DispatcherAddress does not declare the deployed H160 layout".into(),
        );
    }
    let lite_owner = if metadata
        .entries
        .iter()
        .any(|(name, _)| name == "LiteLabelOwner")
    {
        let StorageEntryType::Map {
            hashers,
            key,
            value,
        } = metadata.entry("LiteLabelOwner")?
        else {
            return Err("DotnsGateway.LiteLabelOwner is not a map".into());
        };
        if hashers.len() != 1
            || ByteEncoding::from_type(&metadata.types, value.id)? != ByteEncoding::Fixed(32)
        {
            return Err(
                "DotnsGateway.LiteLabelOwner must declare AccountId32 owners and one key".into(),
            );
        }
        Some((
            StorageMap::new(&metadata.prefix, "LiteLabelOwner", hashers[0].clone()),
            ByteEncoding::from_type(&metadata.types, key.id)?,
        ))
    } else {
        None
    };
    Ok(GatewaySchema { lite_owner })
}

struct PalletSchema {
    types: PortableRegistry,
    prefix: String,
    entries: Vec<(String, StorageEntryType<PortableForm>)>,
}

impl PalletSchema {
    fn new(bytes: &[u8], name: &str) -> Result<Self, String> {
        let metadata: RuntimeMetadataPrefixed = decode_all(bytes)?;
        if metadata.0 != META_RESERVED {
            return Err("invalid Chat runtime metadata prefix".into());
        }
        macro_rules! pallet {
            ($metadata:expr) => {{
                let mut pallets = $metadata
                    .pallets
                    .into_iter()
                    .filter(|pallet| pallet.name == name);
                let storage = pallets
                    .next()
                    .and_then(|pallet| pallet.storage)
                    .ok_or_else(|| format!("configured chain has no {name} directory pallet"))?;
                if pallets.next().is_some() {
                    return Err("duplicate Chat directory pallet".into());
                }
                Self {
                    types: $metadata.types,
                    prefix: storage.prefix,
                    entries: storage
                        .entries
                        .into_iter()
                        .map(|entry| (entry.name, entry.ty))
                        .collect(),
                }
            }};
        }
        Ok(match metadata.1 {
            RuntimeMetadata::V14(metadata) => pallet!(metadata),
            RuntimeMetadata::V15(metadata) => pallet!(metadata),
            RuntimeMetadata::V16(metadata) => pallet!(metadata),
            _ => return Err("Chat directory requires metadata V14, V15 or V16".into()),
        })
    }

    fn entry(&self, name: &str) -> Result<&StorageEntryType<PortableForm>, String> {
        let mut entries = self.entries.iter().filter(|(entry, _)| entry == name);
        let (_, entry) = entries
            .next()
            .ok_or_else(|| format!("Chat directory entry {name} is absent"))?;
        if entries.next().is_some() {
            return Err("duplicate Chat directory storage entry".into());
        }
        Ok(entry)
    }
}

struct StorageMap {
    prefix: [u8; 32],
    hasher: StorageHasher,
}

impl StorageMap {
    fn new(pallet: &str, entry: &str, hasher: StorageHasher) -> Self {
        let mut prefix = [0; 32];
        prefix[..16].copy_from_slice(&twox_128(pallet.as_bytes()));
        prefix[16..].copy_from_slice(&twox_128(entry.as_bytes()));
        Self { prefix, hasher }
    }

    fn key(&self, encoded: &[u8]) -> Vec<u8> {
        let mut key = Vec::with_capacity(64 + encoded.len());
        key.extend_from_slice(&self.prefix);
        match self.hasher {
            StorageHasher::Blake2_128 => key.extend_from_slice(&blake2_128(encoded)),
            StorageHasher::Blake2_256 => key.extend_from_slice(&blake2_256(encoded)),
            StorageHasher::Blake2_128Concat => {
                key.extend_from_slice(&blake2_128(encoded));
                key.extend_from_slice(encoded);
            }
            StorageHasher::Twox128 => key.extend_from_slice(&twox_128(encoded)),
            StorageHasher::Twox256 => key.extend_from_slice(&twox_256(encoded)),
            StorageHasher::Twox64Concat => {
                key.extend_from_slice(&twox_64(encoded));
                key.extend_from_slice(encoded);
            }
            StorageHasher::Identity => key.extend_from_slice(encoded),
        }
        key
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ByteEncoding {
    Fixed(u32),
    Sequence,
}

impl ByteEncoding {
    fn from_type(types: &PortableRegistry, mut id: u32) -> Result<Self, String> {
        for _ in 0..16 {
            let ty = types
                .resolve(id)
                .ok_or("missing Chat directory metadata type")?;
            match &ty.type_def {
                TypeDef::Array(array) if is_u8(types, array.type_param.id) => {
                    return Ok(Self::Fixed(array.len));
                }
                TypeDef::Sequence(sequence) if is_u8(types, sequence.type_param.id) => {
                    return Ok(Self::Sequence);
                }
                TypeDef::Composite(composite) if composite.fields.len() == 1 => {
                    id = composite.fields[0].ty.id
                }
                TypeDef::Tuple(tuple) if tuple.fields.len() == 1 => id = tuple.fields[0].id,
                _ => return Err("Chat directory type is not a byte array or vector".into()),
            }
        }
        Err("Chat directory metadata exceeds type nesting bound".into())
    }

    fn decode_identifier(self, value: &[u8]) -> Result<[u8; 32], String> {
        let mut input = value;
        let length = match self {
            Self::Fixed(length) => length,
            Self::Sequence => {
                Compact::<u32>::decode(&mut input)
                    .map_err(|_| "malformed Chat identifier length")?
                    .0
            }
        };
        if !matches!(length, 32 | 65) {
            return Err("Chat identifier must declare raw32 or typed65 bytes".into());
        }
        let identifier = input
            .get(..length as usize)
            .ok_or("truncated Chat identifier")?;
        let key = if length == 65 {
            if identifier[0] != 0 {
                return Err("Chat identifier is not type-0 X25519".into());
            }
            // Native iOS intentionally ignores all 32 reserved container bytes.
            &identifier[1..33]
        } else {
            identifier
        };
        let key: [u8; 32] = key.try_into().map_err(|_| "invalid Chat key length")?;
        validate_public_key(&key)?;
        Ok(key)
    }
}

fn is_u8(types: &PortableRegistry, id: u32) -> bool {
    types
        .resolve(id)
        .is_some_and(|ty| matches!(ty.type_def, TypeDef::Primitive(TypeDefPrimitive::U8)))
}

fn identifier_encoding(types: &PortableRegistry, mut id: u32) -> Result<ByteEncoding, String> {
    for _ in 0..16 {
        let ty = types
            .resolve(id)
            .ok_or("missing Resources consumer metadata type")?;
        match &ty.type_def {
            TypeDef::Composite(composite) => {
                let field = composite
                    .fields
                    .first()
                    .ok_or("consumer has no identifier_key")?;
                if field.name.as_deref() == Some("identifier_key") {
                    let encoding = ByteEncoding::from_type(types, field.ty.id)?;
                    if matches!(encoding, ByteEncoding::Fixed(length) if !matches!(length, 32 | 65))
                    {
                        return Err("unsupported fixed Chat identifier size".into());
                    }
                    return Ok(encoding);
                }
                if composite.fields.len() == 1 && field.name.is_none() {
                    id = field.ty.id;
                } else {
                    return Err("consumer must declare identifier_key as first field".into());
                }
            }
            TypeDef::Tuple(tuple) if tuple.fields.len() == 1 => id = tuple.fields[0].id,
            _ => return Err("unsupported Resources consumer metadata type".into()),
        }
    }
    Err("consumer metadata exceeds type nesting bound".into())
}

fn validate_public_key(key: &[u8; 32]) -> Result<(), String> {
    // X25519 accepts masked/reduced aliases; identities must not. Compare the
    // little-endian u-coordinate with p = 2^255 - 19 before scalar multiplication.
    const P: [u8; 32] = [
        0xed, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0x7f,
    ];
    if key.iter().rev().cmp(P.iter().rev()) != core::cmp::Ordering::Less {
        return Err("noncanonical X25519 Chat identifier".into());
    }
    // This fixed, non-secret scalar only tests contributory behavior; it is not
    // the wallet's encryption key and no shared secret escapes the Host.
    let probe = x25519_dalek::StaticSecret::from([0x42; 32]);
    if !probe
        .diffie_hellman(&x25519_dalek::PublicKey::from(*key))
        .was_contributory()
    {
        return Err("noncontributory X25519 Chat identifier".into());
    }
    Ok(())
}

struct BudgetInput<'a> {
    bytes: &'a [u8],
    allocated: usize,
    depth: usize,
}

impl Input for BudgetInput<'_> {
    fn remaining_len(&mut self) -> Result<Option<usize>, parity_scale_codec::Error> {
        Ok(Some(self.bytes.len()))
    }
    fn read(&mut self, into: &mut [u8]) -> Result<(), parity_scale_codec::Error> {
        self.bytes.read(into)
    }
    fn descend_ref(&mut self) -> Result<(), parity_scale_codec::Error> {
        self.depth += 1;
        if self.depth > 64 {
            return Err("Chat metadata nesting bound exceeded".into());
        }
        Ok(())
    }
    fn ascend_ref(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
    fn on_before_alloc_mem(&mut self, size: usize) -> Result<(), parity_scale_codec::Error> {
        self.allocated = self.allocated.saturating_add(size);
        if self.allocated > 16 * 1024 * 1024 {
            return Err("Chat metadata allocation bound exceeded".into());
        }
        Ok(())
    }
}

fn decode_all<T: Decode>(bytes: &[u8]) -> Result<T, String> {
    if bytes.len() > MAX_METADATA_BYTES {
        return Err("Chat metadata exceeds wire bound".into());
    }
    let mut input = BudgetInput {
        bytes,
        allocated: 0,
        depth: 0,
    };
    let value = T::decode(&mut input).map_err(|_| "malformed or oversized Chat metadata")?;
    if !input.bytes.is_empty() {
        return Err("trailing Chat metadata bytes".into());
    }
    Ok(value)
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
