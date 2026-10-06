//! Signed-extension encoding for the unsigned General (v5) `AsResources`
//! extrinsic, driven by live chain metadata.
//!
//! The extension **order** and per-extension type ids come from the runtime
//! metadata (`state_getMetadata`, V14/V15/V16); the per-extension `extra` /
//! `additional_signed` bytes come from a name-keyed encoder mirroring
//! signing-bot `src/core/create-transaction.ts` `encodeSignedExtensions`, with a
//! generic default for the personhood extensions (all `Option`/void).
//!
//! Two concatenations are derived from the same encoded list:
//! - the ring-VRF proof message (`build_proof_message`) over the extensions
//!   strictly *after* `AsResources` (host-spec inherited implication), and
//! - the full extrinsic body's `Σ extra` (see `extrinsic.rs`), over *all*
//!   extensions with `AsResources` carrying `Some(AsResourcesInfo)`.

use std::collections::HashMap;
use std::sync::Mutex;

use frame_metadata::RuntimeMetadata;
use frame_metadata::RuntimeMetadataPrefixed;
use parity_scale_codec::{Compact, Decode, Encode};
use scale_info::form::PortableForm;
use scale_info::{PortableRegistry, TypeDef, TypeDefPrimitive, TypeDefVariant};
use thiserror::Error;

use super::StatementAllowanceError;
use super::collection::PersonhoodCollection;
use super::view_cache::{MetadataViewCache, ViewFunctionDef};

/// Signed-extension identifier that carries the `AsPgas` authorization on Asset Hub.
pub const AS_PGAS: &str = "AsPgas";

/// Signed-extension identifier that carries the `AsResources` authorization.
pub const AS_RESOURCES: &str = "AsResources";

/// Signed-extension identifier that carries the `AsDotnsGateway`
/// authorization on Asset Hub.
pub const AS_DOTNS_GATEWAY: &str = "AsDotnsGateway";

/// Error while decoding runtime metadata or resolving allowance-specific
/// metadata entries.
#[derive(Debug, Error)]
pub enum MetadataError {
    /// `state_getMetadata` did not return a hex string.
    #[error("state_getMetadata returned non-string")]
    MetadataResultNotString,
    /// Metadata hex payload was invalid.
    #[error("metadata hex: {0}")]
    MetadataHex(#[source] hex::FromHexError),
    /// Opaque metadata wrapper could not be decoded.
    #[error("opaque metadata: {0}")]
    OpaqueMetadata(#[source] parity_scale_codec::Error),
    /// Runtime metadata prefix could not be decoded.
    #[error("metadata decode failed: {0}")]
    Decode(#[source] parity_scale_codec::Error),
    /// Runtime metadata version is not supported by this encoder.
    #[error("unsupported metadata version {version}")]
    UnsupportedVersion {
        /// Runtime metadata version.
        version: u32,
    },
    /// Pallet has no call enum.
    #[error("pallet `{pallet}` has no calls in metadata")]
    MissingPalletCalls {
        /// Pallet name.
        pallet: String,
    },
    /// Named pallet call was not found.
    #[error("call `{pallet}.{call}` not found in metadata")]
    MissingCall {
        /// Pallet name.
        pallet: String,
        /// Call name.
        call: String,
    },
    /// Named transaction extension is absent from metadata.
    #[error("{identifier} extension not found in metadata")]
    MissingExtension {
        /// Extension identifier.
        identifier: String,
    },
    /// Extension's extra type did not contain the expected `Option`.
    #[error("{identifier} extra is not an Option")]
    ExtensionExtraNotOption {
        /// Extension whose extra was inspected.
        identifier: String,
    },
    /// Named authorization variant was not found on the extension's info enum.
    #[error("{identifier} info variant {variant} not found in metadata")]
    MissingExtensionVariant {
        /// Extension whose info enum was inspected.
        identifier: String,
        /// Variant name.
        variant: String,
    },
    /// `AsDotnsGatewayInfo::RegisterFullName` did not have the expected field shape.
    #[error(
        "AsDotnsGatewayInfo::RegisterFullName fields are [{actual}], expected \
         [proof, ring_index, revision, signature]; the runtime shape drifted"
    )]
    RegisterFullNameShapeDrift {
        /// Actual comma-separated field names.
        actual: String,
    },
    /// Info variant carried no enum field holding the requested variant.
    #[error(
        "{identifier} info variant {variant} carries no field enum with a {field_variant} variant"
    )]
    MissingExtensionFieldVariant {
        /// Extension whose info enum was inspected.
        identifier: String,
        /// Info variant inspected.
        variant: String,
        /// Nested variant that was not found.
        field_variant: String,
    },
    /// Type id did not resolve in the portable registry.
    #[error("unknown type id {type_id}")]
    UnknownTypeId {
        /// Missing type id.
        type_id: u32,
    },
    /// Type id did not resolve to an enum.
    #[error("type {type_id} is not an enum")]
    TypeNotEnum {
        /// Type id.
        type_id: u32,
    },
    /// Type id did not resolve to a composite.
    #[error("type {type_id} is not a composite")]
    TypeNotComposite {
        /// Type id.
        type_id: u32,
    },
    /// Composite type had the wrong field count.
    #[error("type {type_id} has {actual} fields, expected 1")]
    CompositeFieldCount {
        /// Type id.
        type_id: u32,
        /// Actual field count.
        actual: usize,
    },
    /// Storage value type was missing from metadata.
    #[error("{pallet}.{entry} type not in metadata")]
    MissingStorageType {
        /// Pallet name.
        pallet: &'static str,
        /// Storage entry name.
        entry: &'static str,
    },
    /// Pallet constant was missing from metadata.
    #[error("{pallet}.{constant} constant missing")]
    MissingConstant {
        /// Pallet name.
        pallet: &'static str,
        /// Constant name.
        constant: &'static str,
    },
}

/// Chain state needed to fill the standard signed extensions.
#[derive(Debug, Clone, Copy)]
pub struct ChainState {
    /// Runtime `specVersion` (CheckSpecVersion implicit).
    pub spec_version: u32,
    /// Runtime `transactionVersion` (CheckTxVersion implicit).
    pub transaction_version: u32,
    /// Genesis block hash (CheckGenesis / CheckMortality implicit).
    pub genesis_hash: [u8; 32],
    /// Account nonce (CheckNonce extra); ignored by the unsigned path.
    pub nonce: u32,
    /// `RestrictOrigins` extra value. `false` for statement-store allowance
    /// calls on People. `true` for restricted-origin dotNS gateway registrations
    /// on Asset Hub. It is part of both the signed digest and the extrinsic
    /// body, so it lives here to keep the two in lockstep.
    pub restrict_origins: bool,
}

/// A signed extension's identifier plus the type ids of its `extra` and
/// `additional_signed` fields, in metadata order.
struct ExtensionDef {
    identifier: String,
    extra_type: u32,
    additional_signed_type: u32,
}

/// A signed extension encoded to its `extra` and `additional_signed` bytes.
pub struct EncodedExtension {
    /// SCALE-encoded `extra` (goes into the extrinsic body).
    pub extra: Vec<u8>,
    /// SCALE-encoded `additional_signed` (the implicit, part of the signed data).
    pub additional_signed: Vec<u8>,
}

/// Decoded metadata: the ordered signed-extension defs, the type registry,
/// each storage entry's value type id (`(pallet, entry) -> type id`), pallet
/// constants, and each pallet's `(index, call enum type id)`.
pub struct Metadata {
    extensions: Vec<ExtensionDef>,
    metadata_version: u32,
    extension_version: u8,
    registry: PortableRegistry,
    storage_values: HashMap<(String, String), u32>,
    constants: HashMap<(String, String), Vec<u8>>,
    calls: HashMap<String, (u8, u32)>,
    view_functions: HashMap<(String, String), ViewFunctionDef>,
    view_values: Mutex<HashMap<[u8; 32], u32>>,
}

impl MetadataViewCache for Metadata {
    fn view_function(&self, pallet: &str, function: &str) -> Option<ViewFunctionDef> {
        self.view_functions
            .get(&(pallet.to_string(), function.to_string()))
            .copied()
    }

    #[cfg(test)]
    fn insert_view_function(&mut self, pallet: &str, function: &str, definition: ViewFunctionDef) {
        self.view_functions
            .insert((pallet.to_string(), function.to_string()), definition);
    }

    fn cached_view_u32(&self, id: &[u8; 32]) -> Option<u32> {
        self.view_values
            .lock()
            .expect("view function cache mutex poisoned")
            .get(id)
            .copied()
    }

    fn cache_view_u32(&self, id: [u8; 32], value: u32) {
        self.view_values
            .lock()
            .expect("view function cache mutex poisoned")
            .insert(id, value);
    }
}

/// The transaction-extension version to encode with: the highest the runtime
/// declares.
///
/// This mirrors Subxt's `transaction_extension_version_to_use_for_encoding`, so
/// the two extrinsic builders in this crate agree on which pipeline they are
/// encoding for. Metadata versions before V16 carry no version map at all; there
/// the only pipeline is version 0.
fn encoding_extension_version<'a>(versions: impl Iterator<Item = &'a u8>) -> u8 {
    versions.copied().max().unwrap_or(0)
}

/// The extension indices to encode for `version`, in the order the runtime lists
/// them for that pipeline.
///
/// A pipeline is not required to be a prefix of the declared extensions, nor to
/// list them in declaration order, so the map's order is the encoding order.
/// Metadata that declares no entry for `version` has one implicit pipeline: every
/// declared extension, in declaration order.
fn encoding_extension_indexes(
    by_version: &std::collections::BTreeMap<u8, Vec<Compact<u32>>>,
    version: u8,
    declared: usize,
) -> Vec<usize> {
    by_version.get(&version).map_or_else(
        || (0..declared).collect(),
        |indexes| {
            indexes
                .iter()
                .map(|Compact(index)| *index as usize)
                .collect()
        },
    )
}

/// Collect the pallet surface shared by every supported metadata version.
macro_rules! collect_pallets {
    ($m:expr, $set:path) => {{
        let mut storage_values = HashMap::new();
        let mut constants = HashMap::new();
        let mut calls = HashMap::new();
        for pallet in &$m.pallets {
            if let Some(pallet_calls) = &pallet.calls {
                calls.insert(pallet.name.clone(), (pallet.index, pallet_calls.ty.id));
            }
            for constant in &pallet.constants {
                constants.insert(
                    (pallet.name.clone(), constant.name.clone()),
                    constant.value.clone(),
                );
            }
            let Some(storage) = &pallet.storage else {
                continue;
            };
            for entry in &storage.entries {
                use $set as EntryType;
                let value_type = match &entry.ty {
                    EntryType::Plain(ty) => ty.id,
                    EntryType::Map { value, .. } => value.id,
                };
                storage_values.insert((pallet.name.clone(), entry.name.clone()), value_type);
            }
        }
        (storage_values, constants, calls)
    }};
}

/// Collect extensions, type registry, storage value types, and pallet constants
/// from decoded metadata; `$set` is the version's `StorageEntryType`.
macro_rules! collect_metadata {
    ($m:expr, $set:path) => {{
        let extensions = $m
            .extrinsic
            .signed_extensions
            .iter()
            .map(|e| ExtensionDef {
                identifier: e.identifier.clone(),
                extra_type: e.ty.id,
                additional_signed_type: e.additional_signed.id,
            })
            .collect();
        let (storage_values, constants, calls) = collect_pallets!($m, $set);
        (
            extensions,
            0u8,
            $m.types,
            storage_values,
            constants,
            calls,
            HashMap::new(),
        )
    }};
}

macro_rules! collect_metadata_v16 {
    ($m:expr) => {{
        // The extension-pipeline version, not the extrinsic format version: a
        // runtime may declare several pipelines, and the encoded transaction has
        // to name the one it was built for.
        let extension_version =
            encoding_extension_version($m.extrinsic.transaction_extensions_by_version.keys());
        let extension_indexes = encoding_extension_indexes(
            &$m.extrinsic.transaction_extensions_by_version,
            extension_version,
            $m.extrinsic.transaction_extensions.len(),
        );
        let extensions = extension_indexes
            .into_iter()
            .filter_map(|index| $m.extrinsic.transaction_extensions.get(index))
            .map(|e| ExtensionDef {
                identifier: e.identifier.clone(),
                extra_type: e.ty.id,
                additional_signed_type: e.implicit.id,
            })
            .collect();
        let (storage_values, constants, calls) =
            collect_pallets!($m, frame_metadata::v16::StorageEntryType);
        let mut view_functions = HashMap::new();
        for pallet in &$m.pallets {
            for function in &pallet.view_functions {
                view_functions.insert(
                    (pallet.name.clone(), function.name.clone()),
                    ViewFunctionDef {
                        id: function.id,
                        inputs: function.inputs.len(),
                        output_type: function.output.id,
                    },
                );
            }
        }
        (
            extensions,
            extension_version,
            $m.types,
            storage_values,
            constants,
            calls,
            view_functions,
        )
    }};
}

impl Metadata {
    /// Decode `state_getMetadata` bytes (a `RuntimeMetadataPrefixed`, V14
    /// through V16) into the ordered signed-extension defs, type registry,
    /// storage value types, constants, and call enums.
    pub fn decode(bytes: &[u8]) -> Result<Self, StatementAllowanceError> {
        let prefixed =
            RuntimeMetadataPrefixed::decode(&mut &bytes[..]).map_err(MetadataError::Decode)?;
        let metadata_version = prefixed.1.version();
        let (
            extensions,
            extension_version,
            registry,
            storage_values,
            constants,
            calls,
            view_functions,
        ) = match prefixed.1 {
            RuntimeMetadata::V14(m) => {
                collect_metadata!(m, frame_metadata::v14::StorageEntryType)
            }
            RuntimeMetadata::V15(m) => {
                collect_metadata!(m, frame_metadata::v15::StorageEntryType)
            }
            RuntimeMetadata::V16(m) => collect_metadata_v16!(m),
            other => {
                return Err(MetadataError::UnsupportedVersion {
                    version: other.version(),
                }
                .into());
            }
        };
        Ok(Self {
            extensions,
            metadata_version,
            extension_version,
            registry,
            storage_values,
            constants,
            calls,
            view_functions,
            view_values: Mutex::new(HashMap::new()),
        })
    }

    /// The `RuntimeMetadata` version this was decoded from (14, 15 or 16).
    ///
    /// Only V16 declares a transaction-extension pipeline map, so this is how a
    /// caller tells a real V16 fetch from a fallback that resolved the same
    /// pipeline version by default.
    pub fn metadata_version(&self) -> u32 {
        self.metadata_version
    }

    /// The transaction-extension pipeline version this metadata encodes for.
    ///
    /// Written as the General-transaction extension-version byte, and prefixed to
    /// the ring-VRF proof message, which the runtime rebuilds to verify the proof.
    /// Both must carry the same value.
    pub fn extension_version(&self) -> u8 {
        self.extension_version
    }

    /// The type registry, for dynamic decoding of storage values.
    pub fn registry(&self) -> &PortableRegistry {
        &self.registry
    }

    /// The value type id of storage entry `pallet::entry`, if present.
    pub fn storage_value_type(&self, pallet: &str, entry: &str) -> Option<u32> {
        self.storage_values
            .get(&(pallet.to_string(), entry.to_string()))
            .copied()
    }

    /// The SCALE-encoded value bytes of pallet constant `pallet::name`.
    pub fn constant(&self, pallet: &str, name: &str) -> Option<&[u8]> {
        self.constants
            .get(&(pallet.to_string(), name.to_string()))
            .map(Vec::as_slice)
    }

    /// Resolve `pallet::call` by name to its `[pallet_index, call_index]`
    /// dispatch bytes.
    pub fn call_indices(
        &self,
        pallet: &str,
        call: &str,
    ) -> Result<[u8; 2], StatementAllowanceError> {
        let (pallet_index, call_type) =
            self.calls
                .get(pallet)
                .copied()
                .ok_or_else(|| MetadataError::MissingPalletCalls {
                    pallet: pallet.to_string(),
                })?;
        let variants = self.resolve_variant(call_type)?;
        let variant = variants
            .variants
            .iter()
            .find(|v| v.name == call)
            .ok_or_else(|| MetadataError::MissingCall {
                pallet: pallet.to_string(),
                call: call.to_string(),
            })?;
        Ok([pallet_index, variant.index])
    }

    /// Resolve `AsResourcesInfo::<info_variant>` and the `MembershipCollection`
    /// index naming `collection`, by name, from the `AsResources` extension type.
    pub fn as_resources_variant_indices(
        &self,
        info_variant: &str,
        collection: PersonhoodCollection,
    ) -> Result<(u8, u8), StatementAllowanceError> {
        self.extension_info_and_field_variant_indices(
            AS_RESOURCES,
            info_variant,
            collection.metadata_variant(),
        )
    }

    /// Resolve `(info variant index, nested field variant index)` for an
    /// extension shaped as `Wrapper(Option<Info>)` whose named info variant
    /// carries an enum field.
    ///
    /// The field enum is located by the variant name it contains rather than by
    /// its type name, because each extension names its own collection type
    /// (`MembershipCollection` for `AsResources`, `PgasCollection` for `AsPgas`)
    /// while the membership tier inside them is named the same.
    pub fn extension_info_and_field_variant_indices(
        &self,
        identifier: &str,
        info_variant: &str,
        field_variant: &str,
    ) -> Result<(u8, u8), StatementAllowanceError> {
        let variant = self.extension_info_variant(identifier, info_variant)?;
        let nested = variant
            .fields
            .iter()
            .rev()
            .find_map(|field| {
                self.resolve_variant(field.ty.id)
                    .ok()
                    .and_then(|enumeration| {
                        enumeration
                            .variants
                            .iter()
                            .find(|candidate| candidate.name == field_variant)
                    })
            })
            .ok_or_else(|| MetadataError::MissingExtensionFieldVariant {
                identifier: identifier.to_string(),
                variant: info_variant.to_string(),
                field_variant: field_variant.to_string(),
            })?;
        Ok((variant.index, nested.index))
    }

    /// A pallet constant decoded as `u32`.
    ///
    /// Runtime constants are SCALE-encoded at their declared width, which for
    /// these is `u32` or narrower, so short values are zero-extended rather than
    /// rejected: a `u8` slot count is a valid count, not a decode failure.
    pub fn constant_u32(
        &self,
        pallet: &'static str,
        name: &'static str,
    ) -> Result<u32, StatementAllowanceError> {
        let bytes = self
            .constant(pallet, name)
            .ok_or(MetadataError::MissingConstant {
                pallet,
                constant: name,
            })?;
        let mut buf = [0u8; 4];
        let n = bytes.len().min(4);
        buf[..n].copy_from_slice(&bytes[..n]);
        Ok(u32::from_le_bytes(buf))
    }

    /// A pallet constant decoded as `u128`, zero-extended like [`Self::constant_u32`].
    pub fn constant_u128(
        &self,
        pallet: &'static str,
        name: &'static str,
    ) -> Result<u128, StatementAllowanceError> {
        let bytes = self
            .constant(pallet, name)
            .ok_or(MetadataError::MissingConstant {
                pallet,
                constant: name,
            })?;
        let mut buf = [0u8; 16];
        let n = bytes.len().min(16);
        buf[..n].copy_from_slice(&bytes[..n]);
        Ok(u128::from_le_bytes(buf))
    }

    /// Number of fields the runtime declares for one `AsResourcesInfo` variant.
    ///
    /// The encoded payload has to match it exactly: a short payload is accepted
    /// locally and then panics the runtime inside `validate_transaction`, so this
    /// is the offline guard against drifting out of step with the pallet.
    pub fn as_resources_info_field_count(
        &self,
        info_variant: &str,
    ) -> Result<usize, StatementAllowanceError> {
        self.extension_info_field_count(AS_RESOURCES, info_variant)
    }

    /// Same, for any authorizing extension.
    pub fn extension_info_field_count(
        &self,
        identifier: &str,
        info_variant: &str,
    ) -> Result<usize, StatementAllowanceError> {
        Ok(self
            .extension_info_variant(identifier, info_variant)?
            .fields
            .len())
    }

    /// Position of a transaction extension in the runtime's implication
    /// pipeline.
    pub fn extension_index(&self, identifier: &str) -> Option<usize> {
        self.extensions
            .iter()
            .position(|extension| extension.identifier == identifier)
    }

    /// Enum index of a named authorization carried by an extension shaped as
    /// `Wrapper(Option<Info>)`.
    pub fn extension_info_variant_index(
        &self,
        identifier: &str,
        variant: &str,
    ) -> Result<u8, StatementAllowanceError> {
        Ok(self.extension_info_variant(identifier, variant)?.index)
    }

    /// Resolve `AsDotnsGatewayInfo::RegisterFullName` to its variant index.
    ///
    /// Asserts the exact `{proof, ring_index, revision, signature}` field shape
    /// (the People-collection root revision the proof was built against). A
    /// runtime that changes the variant then fails loudly instead of
    /// mis-encoding.
    pub fn dotns_register_full_name_variant(&self) -> Result<u8, StatementAllowanceError> {
        let variant = self.extension_info_variant(AS_DOTNS_GATEWAY, "RegisterFullName")?;
        let fields: Vec<&str> = variant
            .fields
            .iter()
            .map(|field| field.name.as_deref().unwrap_or("<unnamed>"))
            .collect();
        if fields != ["proof", "ring_index", "revision", "signature"] {
            return Err(MetadataError::RegisterFullNameShapeDrift {
                actual: fields.join(", "),
            }
            .into());
        }
        Ok(variant.index)
    }

    /// Resolve one named authorization variant on an extension shaped as
    /// `Wrapper(Option<Info>)`, with or without the struct wrapper.
    fn extension_info_variant(
        &self,
        identifier: &str,
        variant: &str,
    ) -> Result<&scale_info::Variant<PortableForm>, StatementAllowanceError> {
        let extension = self
            .extensions
            .iter()
            .find(|extension| extension.identifier == identifier)
            .ok_or_else(|| MetadataError::MissingExtension {
                identifier: identifier.to_string(),
            })?;
        let option_type = match &self.resolve_type(extension.extra_type)?.type_def {
            TypeDef::Composite(_) => self.single_field_type(extension.extra_type)?,
            _ => extension.extra_type,
        };
        let info_type = self
            .resolve_variant(option_type)?
            .variants
            .iter()
            .find(|candidate| candidate.name == "Some")
            .and_then(|some| match some.fields.as_slice() {
                [field] => Some(field.ty.id),
                _ => None,
            })
            .ok_or_else(|| MetadataError::ExtensionExtraNotOption {
                identifier: identifier.to_string(),
            })?;
        self.resolve_variant(info_type)?
            .variants
            .iter()
            .find(|candidate| candidate.name == variant)
            .ok_or_else(|| {
                MetadataError::MissingExtensionVariant {
                    identifier: identifier.to_string(),
                    variant: variant.to_string(),
                }
                .into()
            })
    }

    /// Resolve a type id in the registry.
    fn resolve_type(
        &self,
        type_id: u32,
    ) -> Result<&scale_info::Type<PortableForm>, StatementAllowanceError> {
        self.registry
            .resolve(type_id)
            .ok_or(MetadataError::UnknownTypeId { type_id }.into())
    }

    /// Resolve `type_id` as an enum definition.
    fn resolve_variant(
        &self,
        type_id: u32,
    ) -> Result<&TypeDefVariant<PortableForm>, StatementAllowanceError> {
        match &self.resolve_type(type_id)?.type_def {
            TypeDef::Variant(variant) => Ok(variant),
            _ => Err(MetadataError::TypeNotEnum { type_id }.into()),
        }
    }

    /// The field type of a one-field composite.
    fn single_field_type(&self, type_id: u32) -> Result<u32, StatementAllowanceError> {
        let TypeDef::Composite(composite) = &self.resolve_type(type_id)?.type_def else {
            return Err(MetadataError::TypeNotComposite { type_id }.into());
        };
        match composite.fields.as_slice() {
            [field] => Ok(field.ty.id),
            fields => Err(MetadataError::CompositeFieldCount {
                type_id,
                actual: fields.len(),
            }
            .into()),
        }
    }

    /// Encode every signed extension in metadata order.
    pub fn encode_signed_extensions(&self, state: &ChainState) -> Vec<EncodedExtension> {
        self.extensions
            .iter()
            .map(|ext| {
                let (extra, additional_signed) = self.encode_one(ext, state);
                EncodedExtension {
                    extra,
                    additional_signed,
                }
            })
            .collect()
    }

    /// The signed-extension identifiers, in metadata order.
    #[cfg(test)]
    pub fn extension_ids(&self) -> Vec<&str> {
        self.extensions
            .iter()
            .map(|e| e.identifier.as_str())
            .collect()
    }

    /// Encode a single extension's `(extra, additional_signed)`, mirroring the
    /// signing-bot switch; unknown personhood extensions fall back to the
    /// metadata type default (`Option` -> None, void -> empty).
    fn encode_one(&self, ext: &ExtensionDef, state: &ChainState) -> (Vec<u8>, Vec<u8>) {
        match ext.identifier.as_str() {
            "CheckNonce" => (Compact(state.nonce).encode(), Vec::new()),
            "CheckSpecVersion" => (Vec::new(), state.spec_version.to_le_bytes().to_vec()),
            "CheckTxVersion" => (Vec::new(), state.transaction_version.to_le_bytes().to_vec()),
            "CheckGenesis" => (Vec::new(), state.genesis_hash.to_vec()),
            // extra = Era::Immortal (0x00); implicit = genesis hash.
            "CheckMortality" => (vec![0x00], state.genesis_hash.to_vec()),
            // extra = first variant `Disabled` (void) = 0x00.
            "VerifyMultiSignature" => (vec![0x00], Vec::new()),
            // extra = { tip: compact(0), asset_id: None } = 0x00 0x00.
            "ChargeAssetTxPayment" => (vec![0x00, 0x00], Vec::new()),
            // extra = bool. See `ChainState::restrict_origins`.
            "RestrictOrigins" => (vec![state.restrict_origins as u8], Vec::new()),
            _ => (
                self.encode_default(ext.extra_type),
                self.encode_default(ext.additional_signed_type),
            ),
        }
    }

    /// Encode the "disabled" default value for a metadata type: `Option` -> None
    /// (`0x00`), void/empty tuple -> empty, enums -> first variant, primitives
    /// -> zero. Matches signing-bot `defaultValueForType`.
    fn encode_default(&self, type_id: u32) -> Vec<u8> {
        let Some(ty) = self.registry.resolve(type_id) else {
            return Vec::new();
        };
        match &ty.type_def {
            TypeDef::Composite(c) => c
                .fields
                .iter()
                .flat_map(|f| self.encode_default(f.ty.id))
                .collect(),
            TypeDef::Tuple(t) => t
                .fields
                .iter()
                .flat_map(|f| self.encode_default(f.id))
                .collect(),
            TypeDef::Variant(v) => {
                // Option<T> encodes None as 0x00.
                if ty.path.segments.last().map(String::as_str) == Some("Option") {
                    return vec![0x00];
                }
                match v.variants.iter().min_by_key(|var| var.index) {
                    None => Vec::new(),
                    Some(first) => {
                        let mut out = vec![first.index];
                        for field in &first.fields {
                            out.extend(self.encode_default(field.ty.id));
                        }
                        out
                    }
                }
            }
            TypeDef::Array(a) => {
                let elem = self.encode_default(a.type_param.id);
                elem.repeat(a.len as usize)
            }
            // Sequences / strings / bit-sequences encode an empty run as compact(0).
            TypeDef::Sequence(_) | TypeDef::BitSequence(_) => vec![0x00],
            TypeDef::Compact(_) => vec![0x00],
            TypeDef::Primitive(p) => match p {
                TypeDefPrimitive::Bool | TypeDefPrimitive::U8 | TypeDefPrimitive::I8 => vec![0],
                TypeDefPrimitive::Char | TypeDefPrimitive::U32 | TypeDefPrimitive::I32 => {
                    vec![0; 4]
                }
                TypeDefPrimitive::U16 | TypeDefPrimitive::I16 => vec![0; 2],
                TypeDefPrimitive::U64 | TypeDefPrimitive::I64 => vec![0; 8],
                TypeDefPrimitive::U128 | TypeDefPrimitive::I128 => vec![0; 16],
                TypeDefPrimitive::U256 | TypeDefPrimitive::I256 => vec![0; 32],
                // Length-prefixed string: empty = compact(0).
                TypeDefPrimitive::Str => vec![0x00],
            },
        }
    }

    /// Index of `AsResources` in the extension list, if present.
    pub fn as_resources_index(&self) -> Option<usize> {
        self.extension_index(AS_RESOURCES)
    }
}

/// Build the ring-VRF proof message for an `AsResources`-authorized call:
/// `blake2b256(0x00 ‖ call ‖ Σ tail.extra ‖ Σ tail.additional_signed)`, where
/// the tail is the extensions ordered strictly after `AsResources`. The leading
/// byte is the General-transaction extension-version, taken from metadata so it
/// matches the byte the extrinsic declares.
pub fn build_proof_message(
    metadata: &Metadata,
    call_data: &[u8],
    state: &ChainState,
) -> Result<[u8; 32], StatementAllowanceError> {
    build_proof_message_after_extension(metadata, call_data, state, AS_RESOURCES)
}

/// Same, for any authorizing extension: the tail is the extensions ordered
/// strictly after `identifier`.
pub fn build_proof_message_after_extension(
    metadata: &Metadata,
    call_data: &[u8],
    state: &ChainState,
    identifier: &str,
) -> Result<[u8; 32], StatementAllowanceError> {
    let all = metadata.encode_signed_extensions(state);
    let tail_start = metadata
        .extension_index(identifier)
        .map(|i| i + 1)
        .ok_or_else(|| MetadataError::MissingExtension {
            identifier: identifier.to_string(),
        })?;
    let tail = &all[tail_start..];

    let mut payload = Vec::with_capacity(1 + call_data.len());
    payload.push(metadata.extension_version());
    payload.extend_from_slice(call_data);
    for ext in tail {
        payload.extend_from_slice(&ext.extra);
    }
    for ext in tail {
        payload.extend_from_slice(&ext.additional_signed);
    }
    Ok(blake2b256(&payload))
}

/// BLAKE2b-256 of `message`.
pub fn blake2b256(message: &[u8]) -> [u8; 32] {
    blake2b_simd::Params::new()
        .hash_length(32)
        .hash(message)
        .as_bytes()
        .try_into()
        .expect("hash_length(32) configures BLAKE2b output to exactly 32 bytes; qed")
}

#[cfg(test)]
mod tests {
    use super::super::test_fixtures;
    use super::*;

    /// Fixture metadata captured from paseo-next-v2 (raw `RuntimeMetadataPrefixed`).
    const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/paseo-next-v2-metadata.scale");

    /// The known-answer chain state frozen alongside the fixture.
    fn fixture_state() -> ChainState {
        ChainState {
            spec_version: 1_000_000,
            transaction_version: 1,
            genesis_hash: [0xab; 32],
            nonce: 0,
            restrict_origins: false,
        }
    }

    /// `Resources.set_statement_store_account(period=7, seq=0, target=0)`.
    fn fixture_call() -> Vec<u8> {
        let mut call = vec![0x3f, 0x0a];
        call.extend_from_slice(&7u32.to_le_bytes());
        call.extend_from_slice(&0u32.to_le_bytes());
        call.extend_from_slice(&[0u8; 32]);
        call
    }

    /// V16 metadata captured from paseo-next-v2 (spec 3000000), the version the
    /// runtime API serves. Distinct from `FIXTURE`, which is the V14 the legacy
    /// RPC answers with and predates the `revision` field.
    const FIXTURE_V16: &[u8] =
        include_bytes!("../../../tests/fixtures/paseo-next-v2-metadata-v16.scale");

    /// Preferring V16 makes this decode path load-bearing, so cover it: it has to
    /// yield a usable `Metadata`, not merely decode.
    #[test]
    fn v16_metadata_decodes_into_a_usable_metadata() {
        let metadata = Metadata::decode(FIXTURE_V16).unwrap();

        assert_eq!(metadata.metadata_version(), 16);
        // Resolved from the version map rather than assumed.
        assert_eq!(metadata.extension_version(), 0);
        // The extension pipeline is populated and carries the one we authorize with.
        assert!(metadata.extension_index(AS_RESOURCES).is_some());
        assert_eq!(
            (
                metadata
                    .as_resources_variant_indices(
                        "RegisterStatementStoreAllowance",
                        PersonhoodCollection::LitePeople,
                    )
                    .unwrap(),
                metadata
                    .as_resources_variant_indices(
                        "ClaimLongTermStorage",
                        PersonhoodCollection::LitePeople
                    )
                    .unwrap(),
            ),
            ((0x02, 0x01), (0x03, 0x01)),
        );
    }

    /// PGAS authorizes with a different extension from the statement-store
    /// claims, and only Asset Hub declares it. A re-captured fixture that changed
    /// the claim arity would otherwise surface as a runtime panic inside
    /// `validate_transaction` rather than a failing test.
    #[test]
    fn the_asset_hub_fixture_declares_the_pgas_claim_shape() {
        let metadata = test_fixtures::asset_hub();

        assert!(metadata.extension_index(AS_PGAS).is_some());
        assert_eq!(
            metadata
                .extension_info_field_count(AS_PGAS, "Claim")
                .unwrap(),
            5,
        );
        // `ClaimPgasInfo` encodes positionally, so the order is what the payload
        // depends on. Arity alone would still hold if the runtime swapped two
        // fields, and the collection is located by variant name rather than by
        // position, so nothing else would notice.
        assert_eq!(
            metadata
                .extension_info_variant(AS_PGAS, "Claim")
                .unwrap()
                .fields
                .iter()
                .map(|field| field.name.as_deref())
                .collect::<Vec<_>>(),
            [
                Some("proof"),
                Some("ring_index"),
                Some("revision"),
                Some("collection"),
                Some("day"),
            ],
        );
    }

    /// The V16 fixture is current, so it declares the four fields the live runtime
    /// wants. The V14 fixture still declares three, which is what hid the missing
    /// `revision` until a live submission failed.
    #[test]
    fn the_two_fixtures_disagree_about_the_allowance_arity() {
        let v14 = Metadata::decode(FIXTURE).unwrap();
        let v16 = Metadata::decode(FIXTURE_V16).unwrap();

        assert_eq!(
            v16.as_resources_info_field_count("RegisterStatementStoreAllowance")
                .unwrap(),
            4,
        );
        assert_eq!(
            v14.as_resources_info_field_count("RegisterStatementStoreAllowance")
                .unwrap(),
            3,
        );
    }

    /// The generalized builders must agree with the `AsResources` wrappers they
    /// now back, or the ring proof stops matching the extrinsic.
    #[test]
    fn the_as_resources_wrappers_match_the_general_forms() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let state = fixture_state();
        let call = fixture_call();

        assert_eq!(
            build_proof_message(&metadata, &call, &state).unwrap(),
            build_proof_message_after_extension(&metadata, &call, &state, AS_RESOURCES).unwrap(),
        );
        assert_eq!(
            metadata
                .as_resources_variant_indices(
                    "RegisterStatementStoreAllowance",
                    PersonhoodCollection::LitePeople,
                )
                .unwrap(),
            metadata
                .extension_info_and_field_variant_indices(
                    AS_RESOURCES,
                    "RegisterStatementStoreAllowance",
                    "LitePeople",
                )
                .unwrap(),
        );
    }

    /// An unknown extension, info variant, or nested variant each fail rather
    /// than resolving to something arbitrary.
    #[test]
    fn the_general_resolver_rejects_names_it_cannot_find() {
        let metadata = Metadata::decode(FIXTURE).unwrap();

        for (identifier, variant, field) in [
            (
                "NoSuchExtension",
                "RegisterStatementStoreAllowance",
                "LitePeople",
            ),
            (AS_RESOURCES, "NoSuchVariant", "LitePeople"),
            (
                AS_RESOURCES,
                "RegisterStatementStoreAllowance",
                "NoSuchTier",
            ),
        ] {
            assert!(
                metadata
                    .extension_info_and_field_variant_indices(identifier, variant, field)
                    .is_err(),
                "{identifier}/{variant}/{field} should not resolve"
            );
        }
    }

    /// Mainnet keeps more than one transaction-extension pipeline, and a
    /// transaction has to declare the one it was encoded for. Encoding for the
    /// newest matches Subxt, so both builders in this crate agree.
    #[test]
    fn the_encoding_version_is_the_highest_the_runtime_declares() {
        assert_eq!(
            encoding_extension_version([].iter()),
            0,
            "metadata with no version map has only pipeline 0"
        );
        assert_eq!(encoding_extension_version([0u8].iter()), 0);
        assert_eq!(
            encoding_extension_version([0u8, 1].iter()),
            1,
            "two pipelines: encode for the newer"
        );
        assert_eq!(encoding_extension_version([2u8, 0, 1].iter()), 2);
    }

    /// Constants are encoded at their declared width, so a `u8` count has to read
    /// as that count rather than failing. A constant the pallet does not declare is
    /// still an error.
    #[test]
    fn constants_read_as_u32_whatever_width_they_declare() {
        let metadata = Metadata::decode(FIXTURE).unwrap();

        assert_eq!(
            metadata
                .constant_u32("Resources", "LiteStmtStoreSlotsPerPeriod")
                .unwrap(),
            10,
            "a u32 constant"
        );
        assert_eq!(
            metadata
                .constant_u32("Resources", "LongTermStorageClaimsPerPeriod")
                .unwrap(),
            10,
            "a u8 constant zero-extends rather than erroring"
        );
        assert!(
            metadata
                .constant_u32("Resources", "NoSuchConstant")
                .is_err()
        );
    }

    /// A pipeline may list a subset, in its own order, so the map decides both
    /// which extensions are encoded and in what order. Declaration order is only
    /// the fallback for metadata that declares no pipeline at all.
    #[test]
    fn a_pipeline_selects_its_own_extensions_in_its_own_order() {
        let mut by_version = std::collections::BTreeMap::new();
        by_version.insert(0u8, vec![Compact(0u32), Compact(1), Compact(2)]);
        by_version.insert(1u8, vec![Compact(2u32), Compact(0)]);

        assert_eq!(
            encoding_extension_indexes(&by_version, 1, 3),
            vec![2, 0],
            "the newer pipeline's own list, in its own order"
        );
        assert_eq!(encoding_extension_indexes(&by_version, 0, 3), vec![0, 1, 2]);
        assert_eq!(
            encoding_extension_indexes(&std::collections::BTreeMap::new(), 0, 3),
            vec![0, 1, 2],
            "no map declared: every extension, in declaration order"
        );
        assert_eq!(
            encoding_extension_indexes(&by_version, 7, 3),
            vec![0, 1, 2],
            "an undeclared pipeline falls back rather than encoding nothing"
        );
    }

    /// The fixture is V14, which carries no version map, so it must resolve to 0.
    /// The frozen proof-message answer below depends on this.
    #[test]
    fn pre_v16_metadata_resolves_to_pipeline_zero() {
        let metadata = Metadata::decode(FIXTURE).unwrap();

        assert_eq!(metadata.extension_version(), 0);
        assert_eq!(metadata.metadata_version(), 14);
    }

    #[test]
    fn proof_message_matches_frozen_known_answer() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let msg = build_proof_message(&metadata, &fixture_call(), &fixture_state()).unwrap();
        assert_eq!(
            hex::encode(msg),
            "1d2e6d8d8f421b0857097c6076115507432d66fea47ebe0c3be282a369f6743c",
        );
    }

    #[test]
    fn as_resources_tail_is_indices_10_through_20() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let idx = metadata.as_resources_index().unwrap();
        // AsResources sits at index 9; the proof tail is everything after it.
        assert_eq!(idx, 9);
        let ids = metadata.extension_ids();
        assert_eq!(
            ids[idx + 1..].to_vec(),
            vec![
                "AuthorizeCall",
                "RestrictOrigins",
                "CheckNonZeroSender",
                "CheckSpecVersion",
                "CheckTxVersion",
                "CheckGenesis",
                "CheckMortality",
                "CheckNonce",
                "CheckWeight",
                "ChargeAssetTxPayment",
                "StorageWeightReclaim",
            ],
        );
    }

    #[test]
    fn call_and_variant_indices_resolve_by_name() {
        let metadata = Metadata::decode(FIXTURE).unwrap();

        assert_eq!(
            (
                metadata
                    .call_indices("Resources", "set_statement_store_account")
                    .unwrap(),
                metadata
                    .call_indices("Resources", "claim_long_term_storage")
                    .unwrap(),
                metadata
                    .as_resources_variant_indices(
                        "RegisterStatementStoreAllowance",
                        PersonhoodCollection::LitePeople,
                    )
                    .unwrap(),
                metadata
                    .as_resources_variant_indices(
                        "ClaimLongTermStorage",
                        PersonhoodCollection::LitePeople
                    )
                    .unwrap(),
            ),
            ([0x3f, 0x0a], [0x3f, 0x0c], (0x02, 0x01), (0x03, 0x01)),
        );
    }

    /// The `AsResources` helpers are thin wrappers over the generic extension
    /// lookups, so the two must not drift apart.
    #[test]
    fn the_generic_lookups_agree_with_the_as_resources_wrappers() {
        let metadata = Metadata::decode(FIXTURE).unwrap();

        assert_eq!(
            metadata.extension_index(AS_RESOURCES),
            metadata.as_resources_index(),
        );
        for variant in ["RegisterStatementStoreAllowance", "ClaimLongTermStorage"] {
            assert_eq!(
                metadata
                    .extension_info_variant_index(AS_RESOURCES, variant)
                    .unwrap(),
                metadata
                    .as_resources_variant_indices(variant, PersonhoodCollection::LitePeople)
                    .unwrap()
                    .0,
                "{variant}",
            );
        }
    }

    #[test]
    fn the_generic_lookups_reject_unknown_extensions_and_variants() {
        let metadata = Metadata::decode(FIXTURE).unwrap();

        assert_eq!(metadata.extension_index("NoSuchExtension"), None);
        assert!(
            metadata
                .extension_info_variant_index("NoSuchExtension", "RegisterStatementStoreAllowance")
                .is_err()
        );
        assert!(
            metadata
                .extension_info_variant_index(AS_RESOURCES, "NoSuchVariant")
                .is_err()
        );
    }

    #[test]
    fn index_resolution_fails_for_unknown_names() {
        let metadata = Metadata::decode(FIXTURE).unwrap();

        assert_eq!(
            (
                metadata.call_indices("Resources", "no_such_call").is_err(),
                metadata.call_indices("NoSuchPallet", "transfer").is_err(),
                metadata
                    .as_resources_variant_indices("NoSuchVariant", PersonhoodCollection::LitePeople)
                    .is_err(),
            ),
            (true, true, true),
        );
    }

    /// Asset Hub fixture metadata (V16, spec 2000036; previewnet serves the
    /// same runtime). The dotNS gateway flows are validated against it.
    const AH_FIXTURE: &[u8] =
        include_bytes!("../../../tests/fixtures/paseo-next-asset-hub-metadata.scale");

    #[test]
    fn dotns_gateway_pipeline_matches_the_reference_transaction_layout() {
        let metadata = Metadata::decode(AH_FIXTURE).unwrap();
        let state = ChainState {
            restrict_origins: true,
            ..fixture_state()
        };

        // RegisterFullName resolves with the asserted field shape.
        assert_eq!(metadata.dotns_register_full_name_variant().unwrap(), 0);
        assert!(
            metadata
                .call_indices("DotnsGateway", "register_name")
                .is_ok()
        );

        // The extension tail after AsDotnsGateway encodes exactly the bytes the
        // reference implementation registerTx.ts hand-builds. Extras are
        // RestrictOrigin(true) ‖ CheckEra(Immortal) ‖ CheckNonce(0) ‖
        // ChargePGAS(tip 0, asset None) ‖ CheckMetadataHash(Disabled). The
        // implicit half is specVersion ‖ txVersion ‖ genesis ‖ genesis ‖
        // metadata-hash None.
        let all = metadata.encode_signed_extensions(&state);
        let tail_start = metadata.extension_index(AS_DOTNS_GATEWAY).unwrap() + 1;
        let tail = &all[tail_start..];
        let extras: Vec<u8> = tail.iter().flat_map(|e| e.extra.clone()).collect();
        assert_eq!(
            hex::encode(extras),
            "010000000000",
            "tail extras: RestrictOrigin true, era, nonce, tip, asset, metadata hash"
        );
        let implicit: Vec<u8> = tail
            .iter()
            .flat_map(|e| e.additional_signed.clone())
            .collect();
        assert_eq!(
            implicit,
            [
                state.spec_version.to_le_bytes().to_vec(),
                state.transaction_version.to_le_bytes().to_vec(),
                state.genesis_hash.to_vec(),
                state.genesis_hash.to_vec(),
                vec![0x00],
            ]
            .concat()
        );
    }

    #[test]
    fn dropping_the_version_byte_changes_the_hash() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let state = fixture_state();
        let call = fixture_call();
        let all = metadata.encode_signed_extensions(&state);
        let tail = &all[metadata.as_resources_index().unwrap() + 1..];
        let mut without = call.clone();
        for e in tail {
            without.extend_from_slice(&e.extra);
        }
        for e in tail {
            without.extend_from_slice(&e.additional_signed);
        }
        assert_ne!(
            build_proof_message(&metadata, &call, &state).unwrap(),
            blake2b256(&without),
        );
    }
}
