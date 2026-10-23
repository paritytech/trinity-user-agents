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
			encode_v4_signed_with_info_to, encode_v5_general_with_info_to,
		},
		scale_decode::visitor::{IgnoreVisitor, decode_with_visitor},
		scale_encode::{EncodeAsFields, Error as ScaleEncodeError, FieldIter, TypeResolver},
	},
	utils::{AccountId32, MultiAddress},
};

use super::ExtrinsicBuilder;
use crate::chain::{
	EncodedExtrinsic, MortalExtrinsic, Mortality,
	extrinsic::{
		ExtrinsicBuildError, ExtrinsicVersion, InheritedImplication,
		extension::{SlotTypes, SuppliedExtension},
		signature::VerifySignatureExtension,
		version::{choose_version, pipeline_names},
		well_known::{ChainValues, SlotValues, unset_slot_value, well_known_values},
	},
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
	let signed = builder.signature.is_some();
	let version = choose_version(builder.version, host_extension_version, signed, &metadata)?;
	check_format_supported(version, &metadata)?;
	let (extensions, v4_signature) =
		place_signature(version, builder.extensions, builder.signature)?;
	check_supplied_extensions(&extensions, version.extension_version(), &metadata)?;
	let mortality = Mortality::new(builder.runtime.birth, builder.period)?;
	let chain = ChainValues {
		genesis: builder.runtime.genesis,
		spec_version: builder.runtime.spec_version,
		transaction_version: builder.runtime.transaction_version,
		mortality,
		nonce: builder.nonce.or(account_nonce).unwrap_or(0),
		restrict_origins: builder.restrict_origins,
	};
	let fold = PipelineFold { version, call: &call, chain, types: metadata.types() };
	let slots = fold.fill_pipeline(extensions, &metadata)?;
	let parts = EncodedParts::new(&call, &slots, version, &metadata)?;
	let bytes = match v4_signature {
		Some(signature) => parts.encode_v4(&signature, &fold.implication_after(&slots), &metadata),
		None => parts.encode_v5(version.extension_version(), &metadata),
	}?;
	Ok(MortalExtrinsic { extrinsic: EncodedExtrinsic::new(bytes), mortality })
}

/// Where the signature goes: a V5 extrinsic fills its `VerifyMultiSignature`
/// slot with it like any extension; a V4 one takes it out of the pipeline to
/// sign the preamble, and cannot be built without one.
fn place_signature<'a>(
	version: ExtrinsicVersion,
	mut extensions: Vec<SuppliedExtension<'a>>,
	signature: Option<VerifySignatureExtension<'a>>,
) -> Result<(Vec<SuppliedExtension<'a>>, Option<VerifySignatureExtension<'a>>), ExtrinsicBuildError>
{
	if let ExtrinsicVersion::V5 { .. } = version {
		extensions.extend(signature.map(SuppliedExtension::new));
		return Ok((extensions, None));
	}
	let signature = signature.ok_or(ExtrinsicBuildError::UnsignedV4)?;
	Ok((extensions, Some(signature)))
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
fn check_supplied_extensions(
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
			let implication = self.implication_after(slots.make_contiguous());
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
	fn implication_after(&self, later: &[Slot]) -> InheritedImplication {
		let mut encoded = match self.version {
			ExtrinsicVersion::V4 => Vec::new(),
			ExtrinsicVersion::V5 { extension_version } => vec![extension_version],
		};
		encoded.extend_from_slice(self.call);
		later.iter().for_each(|slot| encoded.extend_from_slice(&slot.values.explicit));
		later.iter().for_each(|slot| encoded.extend_from_slice(&slot.values.implicit));
		InheritedImplication::new(encoded, self.version)
	}

	/// A slot's values: the supplied extension's, else a well-known value,
	/// else the type's unset default.
	fn slot_values(
		&self,
		declared: &ExtrinsicExtensionInfoArg<'_, u32>,
		supplied: Option<SuppliedExtension<'_>>,
		implication: &InheritedImplication,
	) -> Result<SlotValues, ExtrinsicBuildError> {
		if let Some(extension) = supplied {
			let types = SlotTypes { explicit: declared.id, implicit: declared.implicit_id };
			return (extension.encode)(implication, types, self.types);
		}
		if let Some(values) = well_known_values(declared, &self.chain, self.types)? {
			return Ok(values);
		}
		unset_values(declared, self.types)
	}
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

impl<'p> EncodedParts<'p> {
	/// The call and the computed slots, ready for frame-decode to lay out in
	/// `version`'s format.
	fn new(
		call: &'p [u8],
		slots: &'p [Slot],
		version: ExtrinsicVersion,
		metadata: &'p Metadata,
	) -> Result<Self, ExtrinsicBuildError> {
		let (call_info, args) = split_call(call, metadata)?;
		let pipeline = extension_info(metadata, version.extension_version())?;
		Ok(Self { args: EncodedArgs(args), slots: ComputedSlots(slots), call_info, pipeline })
	}

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

	/// A V4 signed extrinsic, the preamble carrying `signature`'s signature
	/// over the whole pipeline's implication.
	fn encode_v4(
		&self,
		signature: &VerifySignatureExtension<'_>,
		implication: &InheritedImplication,
		metadata: &Metadata,
	) -> Result<Vec<u8>, ExtrinsicBuildError> {
		let address = MultiAddress::<AccountId32, u32>::Id(signature.account());
		let signature_info = metadata.extrinsic_signature_info().map_err(encode_error)?;
		let mut out = Vec::new();
		encode_v4_signed_with_info_to(
			&self.args,
			&self.slots,
			&address,
			&signature.sign(implication),
			metadata.types(),
			&self.call_info,
			&signature_info,
			&self.pipeline,
			&mut out,
		)
		.map_err(encode_error)?;
		Ok(out)
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
