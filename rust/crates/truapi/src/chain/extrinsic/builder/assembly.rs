//! Turns a finished [`ExtrinsicBuilder`] into extrinsic bytes.
//!
//! The pipeline is filled from its last extension to its first, so each
//! extension's explicit value can cover every later one. frame-decode then
//! lays out the final bytes from the computed values.

use std::collections::{BTreeSet, VecDeque};

use scale_info::PortableRegistry;
use subxt::{
	Metadata,
	ext::{
		frame_decode::extrinsics::{
			ExtrinsicCallInfo, ExtrinsicExtensionInfo, ExtrinsicExtensionInfoArg,
			ExtrinsicTypeInfo, TransactionExtensions, TransactionExtensionsError,
			encode_v4_signed_with_info_to, encode_v4_signer_payload_with_info,
			encode_v5_general_with_info_to,
		},
		scale_decode::visitor::{IgnoreVisitor, decode_with_visitor},
		scale_encode::{EncodeAsFields, Error as ScaleEncodeError, FieldIter, TypeResolver},
	},
	tx::Signer,
	utils::{AccountId32, MultiAddress, MultiSignature},
};

use super::ExtrinsicBuilder;
use crate::{
	chain::{
		EncodedExtrinsic, MortalExtrinsic, Mortality,
		extrinsic::{
			ExtrinsicBuildError, ExtrinsicVersion, InheritedImplication,
			extension::SuppliedExtension,
			version::{choose_version, declares, pipeline_names},
			well_known::{
				ChainValues, SlotValues, VERIFY_MULTI_SIGNATURE, unset_slot_value,
				verify_signature_value, well_known_values,
			},
		},
	},
	host_internal::extrinsic::Sr25519Signer,
};

/// Builds the extrinsic `builder` describes.
///
/// `host_extension_version` is the host's pipeline for this chain, if it
/// builds the chain as V5. `account_nonce` is the signer's next index, read
/// when the caller set no nonce.
pub fn assemble(
	builder: ExtrinsicBuilder<'_>,
	host_extension_version: Option<u8>,
	account_nonce: Option<u64>,
) -> Result<MortalExtrinsic, ExtrinsicBuildError> {
	let call = builder.call.ok_or(ExtrinsicBuildError::NoCall)??;
	let metadata = builder.runtime.metadata.clone();
	let signer = builder.signer;
	let version =
		choose_version(builder.version, host_extension_version, signer.is_some(), &metadata)?;
	check_version_fits(version, signer.is_some(), &builder.extensions, &metadata)?;
	let mortality = Mortality::new(builder.runtime.birth, builder.period)?;
	let chain = ChainValues {
		genesis: builder.runtime.genesis,
		spec_version: builder.runtime.spec_version,
		transaction_version: builder.runtime.transaction_version,
		mortality,
		nonce: builder.nonce.or(account_nonce).unwrap_or(0),
	};
	let fold = PipelineFold { version, call: &call, signer, chain, types: metadata.types() };
	let slots = fold.fill_pipeline(builder.extensions, &metadata)?;
	let bytes = encode_extrinsic(version, &call, &slots, signer, &metadata)?;
	Ok(MortalExtrinsic { extrinsic: EncodedExtrinsic::new(bytes), mortality })
}

/// Rejects a version the runtime or the supplied extensions cannot be built
/// with, and a signed V5 build whose pipeline cannot carry the signature.
fn check_version_fits(
	version: ExtrinsicVersion,
	signed: bool,
	supplied: &[SuppliedExtension<'_>],
	metadata: &Metadata,
) -> Result<(), ExtrinsicBuildError> {
	check_format_supported(version, metadata)?;
	let extension_version = version.extension_version();
	let is_v5 = matches!(version, ExtrinsicVersion::V5 { .. });
	if is_v5 && signed && !declares(metadata, extension_version, VERIFY_MULTI_SIGNATURE)? {
		return Err(ExtrinsicBuildError::NoSignatureSlot { extension_version });
	}
	check_supplied_declared(supplied, extension_version, metadata)
}

fn check_format_supported(
	version: ExtrinsicVersion,
	metadata: &Metadata,
) -> Result<(), ExtrinsicBuildError> {
	let format = match version {
		ExtrinsicVersion::V4 => 4,
		ExtrinsicVersion::V5 { .. } => 5,
	};
	let supported = metadata.extrinsic().supported_versions();
	if supported.contains(&format) {
		return Ok(());
	}
	Err(ExtrinsicBuildError::UnsupportedVersion { version, supported: supported.to_vec() })
}

/// Rejects a supplied extension the pipeline does not list, or one supplied
/// twice, since only one value can fill a slot.
fn check_supplied_declared(
	supplied: &[SuppliedExtension<'_>],
	extension_version: u8,
	metadata: &Metadata,
) -> Result<(), ExtrinsicBuildError> {
	let declared = pipeline_names(metadata, extension_version)?;
	let mut seen = BTreeSet::new();
	for extension in supplied {
		if !seen.insert(extension.name.as_str()) {
			return Err(ExtrinsicBuildError::DuplicateExtension {
				extension: extension.name.clone(),
			});
		}
		if !declared.contains(&extension.name) {
			return Err(ExtrinsicBuildError::UndeclaredExtension {
				extension: extension.name.clone(),
				extension_version,
				declared,
			});
		}
	}
	Ok(())
}

/// One pipeline slot's name and computed values.
struct Slot {
	name: String,
	values: SlotValues,
}

/// What filling the pipeline needs besides the supplied extensions.
struct PipelineFold<'f> {
	version: ExtrinsicVersion,
	call: &'f [u8],
	signer: Option<&'f Sr25519Signer>,
	chain: ChainValues,
	types: &'f PortableRegistry,
}

impl PipelineFold<'_> {
	/// Fills every slot of the pipeline, from the last to the first.
	fn fill_pipeline(
		&self,
		mut supplied: Vec<SuppliedExtension<'_>>,
		metadata: &Metadata,
	) -> Result<Vec<Slot>, ExtrinsicBuildError> {
		let pipeline = extension_info(metadata, self.version.extension_version())?;
		let mut slots = VecDeque::with_capacity(pipeline.extension_ids.len());
		for declared in pipeline.extension_ids.iter().rev() {
			let implication = self.implication_after(&slots);
			let position = supplied.iter().position(|extension| extension.name == declared.name);
			let extension = position.map(|index| supplied.swap_remove(index));
			let values = self.slot_values(declared, extension, &implication)?;
			check_slot_shape(declared, &values, self.types)?;
			slots.push_front(Slot { name: declared.name.to_string(), values });
		}
		Ok(slots.into())
	}

	/// The implication an extension sees: for V5 the extension version, then
	/// the call, then every later extension's explicit and implicit values.
	fn implication_after(&self, later: &VecDeque<Slot>) -> InheritedImplication {
		let mut encoded = match self.version {
			ExtrinsicVersion::V4 => Vec::new(),
			ExtrinsicVersion::V5 { extension_version } => vec![extension_version],
		};
		encoded.extend_from_slice(self.call);
		later.iter().for_each(|slot| encoded.extend_from_slice(&slot.values.explicit));
		later.iter().for_each(|slot| encoded.extend_from_slice(&slot.values.implicit));
		InheritedImplication::new(encoded)
	}

	/// A slot's values: the supplied extension's, else the signature's,
	/// else a well-known value, else the type's unset default.
	fn slot_values(
		&self,
		declared: &ExtrinsicExtensionInfoArg<'_, u32>,
		supplied: Option<SuppliedExtension<'_>>,
		implication: &InheritedImplication,
	) -> Result<SlotValues, ExtrinsicBuildError> {
		if let Some(extension) = supplied {
			return supplied_values(extension, implication);
		}
		if declared.name == VERIFY_MULTI_SIGNATURE {
			return self.signature_values(declared.id, implication);
		}
		if let Some(values) = well_known_values(&declared.name, &self.chain) {
			return Ok(values);
		}
		unset_values(declared, self.types)
	}

	/// `VerifyMultiSignature`'s values: a signature over the implication in a
	/// signed V5 build, else `Disabled`.
	fn signature_values(
		&self,
		type_id: u32,
		implication: &InheritedImplication,
	) -> Result<SlotValues, ExtrinsicBuildError> {
		let signature = match (self.version, self.signer) {
			(ExtrinsicVersion::V5 { .. }, Some(signer)) => {
				Some((signer.account_id(), signer.sign(&implication.hash())))
			},
			_ => None,
		};
		let explicit = verify_signature_value(signature, type_id, self.types)?;
		Ok(SlotValues { explicit, implicit: Vec::new() })
	}
}

fn supplied_values(
	extension: SuppliedExtension<'_>,
	implication: &InheritedImplication,
) -> Result<SlotValues, ExtrinsicBuildError> {
	let explicit = (extension.explicit)(implication).map_err(|source| {
		ExtrinsicBuildError::Extension { extension: extension.name.clone(), source }
	})?;
	Ok(SlotValues { explicit, implicit: extension.implicit })
}

fn unset_values(
	declared: &ExtrinsicExtensionInfoArg<'_, u32>,
	types: &PortableRegistry,
) -> Result<SlotValues, ExtrinsicBuildError> {
	let missing = || ExtrinsicBuildError::MissingExtension { extension: declared.name.to_string() };
	let explicit = unset_slot_value(declared.id, types).ok_or_else(missing)?;
	let implicit = unset_slot_value(declared.implicit_id, types).ok_or_else(missing)?;
	Ok(SlotValues { explicit, implicit })
}

/// Checks that both of a slot's values decode as their metadata types and
/// leave nothing over.
fn check_slot_shape(
	declared: &ExtrinsicExtensionInfoArg<'_, u32>,
	values: &SlotValues,
	types: &PortableRegistry,
) -> Result<(), ExtrinsicBuildError> {
	check_value_shape(&declared.name, "explicit", &values.explicit, declared.id, types)?;
	check_value_shape(&declared.name, "implicit", &values.implicit, declared.implicit_id, types)
}

fn check_value_shape(
	extension: &str,
	slot: &'static str,
	bytes: &[u8],
	type_id: u32,
	types: &PortableRegistry,
) -> Result<(), ExtrinsicBuildError> {
	let shape_error = |reason: String| ExtrinsicBuildError::ExtensionShape {
		extension: extension.to_string(),
		slot,
		bytes: hex::encode(bytes),
		reason,
	};
	let mut cursor = bytes;
	decode_with_visitor(&mut cursor, type_id, types, IgnoreVisitor::new())
		.map_err(|error| shape_error(error.to_string()))?;
	if !cursor.is_empty() {
		return Err(shape_error(format!("{} trailing byte(s)", cursor.len())));
	}
	Ok(())
}

fn extension_info(
	metadata: &Metadata,
	extension_version: u8,
) -> Result<ExtrinsicExtensionInfo<'_, u32>, ExtrinsicBuildError> {
	metadata.extrinsic_extension_info(Some(extension_version)).map_err(|error| {
		ExtrinsicBuildError::UnknownPipeline { extension_version, reason: error.to_string() }
	})
}

/// Lays out the extrinsic with frame-decode from the call and the computed
/// slots, signing the V4 payload when the format is V4.
fn encode_extrinsic(
	version: ExtrinsicVersion,
	call: &[u8],
	slots: &[Slot],
	signer: Option<&Sr25519Signer>,
	metadata: &Metadata,
) -> Result<Vec<u8>, ExtrinsicBuildError> {
	let (call_info, args) = split_call(call, metadata)?;
	let pipeline = extension_info(metadata, version.extension_version())?;
	let parts =
		EncodedParts { args: EncodedArgs(args), slots: ComputedSlots(slots), call_info, pipeline };
	match (version, signer) {
		(ExtrinsicVersion::V5 { extension_version }, _) => {
			parts.encode_v5(extension_version, metadata)
		},
		(ExtrinsicVersion::V4, Some(signer)) => parts.encode_v4(signer, metadata),
		(ExtrinsicVersion::V4, None) => Err(ExtrinsicBuildError::UnsignedV4),
	}
}

/// The call's metadata and its argument bytes, after the pallet and call
/// indices.
fn split_call<'c>(
	call: &'c [u8],
	metadata: &'c Metadata,
) -> Result<(ExtrinsicCallInfo<'c, u32>, &'c [u8]), ExtrinsicBuildError> {
	let call_bytes_error =
		|reason: String| ExtrinsicBuildError::CallBytes { call_bytes: hex::encode(call), reason };
	let [pallet_index, call_index, args @ ..] = call else {
		return Err(call_bytes_error(format!("only {} byte(s)", call.len())));
	};
	let call_info = metadata
		.extrinsic_call_info_by_index(*pallet_index, *call_index)
		.map_err(|error| call_bytes_error(error.to_string()))?;
	Ok((call_info, args))
}

/// Everything frame-decode lays out, already computed.
struct EncodedParts<'p> {
	args: EncodedArgs<'p>,
	slots: ComputedSlots<'p>,
	call_info: ExtrinsicCallInfo<'p, u32>,
	pipeline: ExtrinsicExtensionInfo<'p, u32>,
}

impl EncodedParts<'_> {
	fn encode_v5(
		&self,
		extension_version: u8,
		metadata: &Metadata,
	) -> Result<Vec<u8>, ExtrinsicBuildError> {
		let mut out = Vec::new();
		encode_v5_general_with_info_to(
			&self.args,
			extension_version,
			&self.slots,
			metadata.types(),
			&self.call_info,
			&self.pipeline,
			&mut out,
		)
		.map_err(encode_error)?;
		Ok(out)
	}

	fn encode_v4(
		&self,
		signer: &Sr25519Signer,
		metadata: &Metadata,
	) -> Result<Vec<u8>, ExtrinsicBuildError> {
		let signature = self.sign_v4_payload(signer, metadata)?;
		let address = MultiAddress::<AccountId32, u32>::Id(signer.account_id());
		let signature_info = metadata.extrinsic_signature_info().map_err(encode_error)?;
		let mut out = Vec::new();
		encode_v4_signed_with_info_to(
			&self.args,
			&self.slots,
			&address,
			&signature,
			metadata.types(),
			&self.call_info,
			&signature_info,
			&self.pipeline,
			&mut out,
		)
		.map_err(encode_error)?;
		Ok(out)
	}

	/// The signature over the V4 signer payload, which frame-decode hashes
	/// when it is longer than 256 bytes.
	fn sign_v4_payload(
		&self,
		signer: &Sr25519Signer,
		metadata: &Metadata,
	) -> Result<MultiSignature, ExtrinsicBuildError> {
		let payload = encode_v4_signer_payload_with_info(
			&self.args,
			&self.slots,
			metadata.types(),
			&self.call_info,
			&self.pipeline,
		)
		.map_err(encode_error)?;
		Ok(signer.sign(&payload))
	}
}

fn encode_error(error: impl core::fmt::Display) -> ExtrinsicBuildError {
	ExtrinsicBuildError::Encode(error.to_string())
}

/// Call arguments that are already encoded, written as they are.
struct EncodedArgs<'a>(&'a [u8]);

impl EncodeAsFields for EncodedArgs<'_> {
	fn encode_as_fields_to<R: TypeResolver>(
		&self,
		_fields: &mut dyn FieldIter<'_, R::TypeId>,
		_types: &R,
		out: &mut Vec<u8>,
	) -> Result<(), ScaleEncodeError> {
		out.extend_from_slice(self.0);
		Ok(())
	}
}

/// The computed slots, handed to frame-decode by name.
struct ComputedSlots<'s>(&'s [Slot]);

impl ComputedSlots<'_> {
	fn slot(&self, name: &str) -> Result<&SlotValues, TransactionExtensionsError> {
		self.0
			.iter()
			.find(|slot| slot.name == name)
			.map(|slot| &slot.values)
			.ok_or_else(|| TransactionExtensionsError::NotFound(name.to_string()))
	}
}

impl TransactionExtensions<PortableRegistry> for ComputedSlots<'_> {
	fn contains_extension(&self, name: &str) -> bool {
		self.0.iter().any(|slot| slot.name == name)
	}

	fn encode_extension_value_to(
		&self,
		name: &str,
		_type_id: u32,
		_types: &PortableRegistry,
		out: &mut Vec<u8>,
	) -> Result<(), TransactionExtensionsError> {
		out.extend_from_slice(&self.slot(name)?.explicit);
		Ok(())
	}

	fn encode_extension_implicit_to(
		&self,
		name: &str,
		_type_id: u32,
		_types: &PortableRegistry,
		out: &mut Vec<u8>,
	) -> Result<(), TransactionExtensionsError> {
		out.extend_from_slice(&self.slot(name)?.implicit);
		Ok(())
	}
}
