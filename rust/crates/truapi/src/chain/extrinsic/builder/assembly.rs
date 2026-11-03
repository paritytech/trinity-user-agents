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
			encode_call_data_with_info_to, encode_v4_signed_with_info_to,
			encode_v5_general_with_info_to,
		},
		scale_decode::visitor::{IgnoreVisitor, decode_with_visitor},
		scale_encode::{EncodeAsFields, Error as ScaleEncodeError, FieldIter, TypeResolver},
	},
	utils::{AccountId32, H256, MultiAddress},
};

use super::{AnyCall, BuildSource, ExtrinsicBuilder, RuntimeAtBirth};
use crate::chain::{
	EncodedExtrinsic, MortalExtrinsic, Mortality,
	extrinsic::{
		ExtrinsicBuildError, ExtrinsicVersion, InheritedImplication,
		extension::{AnyExtension, ExtensionValueTypes, SuppliedExtension},
		signature::SubstrateSigner,
		version::{choose_extrinsic_version, pipeline_names},
		well_known::{
			CHECK_MORTALITY, ChainValues, EncodedExtensionValues, VERIFY_MULTI_SIGNATURE,
			default_value_for_type, well_known_extension,
		},
	},
};

/// Builds the extrinsic `builder` describes, born at `runtime`.
///
/// It reads from the builder's source only what the caller left unset: the
/// host's pipeline when no version is pinned, and the signer's next nonce
/// when no nonce is set.
pub async fn assemble(
	builder: ExtrinsicBuilder<'_>,
	runtime: RuntimeAtBirth,
) -> Result<MortalExtrinsic, ExtrinsicBuildError> {
	let (source, genesis, signer) = (builder.source, builder.genesis, builder.signer());
	let signed = signer.is_some();
	let version =
		choose_extrinsic_version(builder.version, source, genesis, signed, &runtime.metadata)
			.await?;
	let nonce = choose_nonce(builder.nonce, signer, source, genesis).await?;
	encode_extrinsic(builder, runtime, version, nonce)
}

/// The nonce `CheckNonce` carries: the caller's, else the signer's next index
/// read from `source`, else 0 for a build nobody signs.
async fn choose_nonce(
	pinned: Option<u64>,
	signer: Option<&SubstrateSigner>,
	source: &dyn BuildSource,
	genesis: H256,
) -> Result<u64, ExtrinsicBuildError> {
	if let Some(nonce) = pinned {
		return Ok(nonce);
	}
	let Some(signer) = signer else {
		return Ok(0);
	};
	source.account_nonce(genesis, signer.account_id()).await
}

/// Encodes the extrinsic once its version and nonce are settled.
fn encode_extrinsic(
	builder: ExtrinsicBuilder<'_>,
	runtime: RuntimeAtBirth,
	version: ExtrinsicVersion,
	nonce: u64,
) -> Result<MortalExtrinsic, ExtrinsicBuildError> {
	let ExtrinsicBuilder { genesis, call, extensions, period, .. } = builder;
	let metadata = runtime.metadata.clone();
	check_format_supported(version, &metadata)?;
	let pending_call = call.ok_or(ExtrinsicBuildError::NoCall)?;
	let call = EncodedCall::new(pending_call.as_ref(), &metadata)?;
	let (extensions, v4_signature) = supplied_extensions(version, extensions, &metadata)?;
	let v4_signer = v4_signature.as_deref().and_then(AnyExtension::signer);
	let mortality = Mortality::new(runtime.birth, period)?;
	let chain = chain_values(genesis, &runtime, mortality, nonce);
	let fold = PipelineFold { version, call: &call, chain, types: metadata.types() };
	let encoded = fold.fill_pipeline(extensions, &metadata)?;
	let bytes = fold.extrinsic_bytes(&encoded, v4_signer, &metadata)?;
	Ok(MortalExtrinsic { extrinsic: EncodedExtrinsic::new(bytes), mortality })
}

fn chain_values(
	genesis: H256,
	runtime: &RuntimeAtBirth,
	mortality: Mortality,
	nonce: u64,
) -> ChainValues {
	ChainValues {
		genesis,
		spec_version: runtime.spec_version,
		transaction_version: runtime.transaction_version,
		mortality,
		nonce,
	}
}

/// The supplied extensions the pipeline fills and, for V4, the signature
/// taken out of them.
fn supplied_extensions<'a>(
	version: ExtrinsicVersion,
	extensions: Vec<SuppliedExtension<'a>>,
	metadata: &Metadata,
) -> Result<(Vec<SuppliedExtension<'a>>, Option<SuppliedExtension<'a>>), ExtrinsicBuildError> {
	let (extensions, v4_signature) = take_v4_signature(version, extensions)?;
	check_supplied_extensions(&extensions, version.extension_version(), metadata)?;
	Ok((extensions, v4_signature))
}

/// A V5 extrinsic carries `VerifyMultiSignature` like any other extension. A
/// V4 one takes it out of the pipeline to sign the preamble, which leaves the
/// pipeline's own entry, if it declares one, disabled; it cannot be built
/// without a signer.
fn take_v4_signature(
	version: ExtrinsicVersion,
	mut extensions: Vec<SuppliedExtension<'_>>,
) -> Result<(Vec<SuppliedExtension<'_>>, Option<SuppliedExtension<'_>>), ExtrinsicBuildError> {
	if let ExtrinsicVersion::V5 { .. } = version {
		return Ok((extensions, None));
	}
	let position = extensions
		.iter()
		.position(|extension| extension.name() == VERIFY_MULTI_SIGNATURE);
	let signature = position.map(|index| extensions.remove(index));
	if signature.as_ref().and_then(|signature| signature.signer()).is_none() {
		return Err(ExtrinsicBuildError::UnsignedV4);
	}
	Ok((extensions, signature))
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

/// Rejects a supplied `CheckMortality`, an extension the pipeline does not
/// list, or one supplied twice, since an extension has only one value.
fn check_supplied_extensions(
	supplied: &[SuppliedExtension<'_>],
	extension_version: u8,
	metadata: &Metadata,
) -> Result<(), ExtrinsicBuildError> {
	let declared = pipeline_names(metadata, extension_version)?;
	let mut seen = BTreeSet::new();
	for extension in supplied {
		let name = extension.name();
		if name == CHECK_MORTALITY {
			return Err(ExtrinsicBuildError::SuppliedMortality);
		}
		if !seen.insert(name) {
			return Err(ExtrinsicBuildError::DuplicateExtension { extension: name.to_string() });
		}
		if !declared.iter().any(|declared_name| declared_name == name) {
			return Err(ExtrinsicBuildError::UndeclaredExtension {
				extension: name.to_string(),
				extension_version,
				declared,
			});
		}
	}
	Ok(())
}

/// One pipeline extension's name and encoded values.
struct EncodedExtension {
	name: String,
	values: EncodedExtensionValues,
}

/// What filling the pipeline needs besides the supplied extensions.
struct PipelineFold<'f> {
	version: ExtrinsicVersion,
	call: &'f EncodedCall<'f>,
	chain: ChainValues,
	types: &'f PortableRegistry,
}

impl PipelineFold<'_> {
	/// Encodes every extension of the pipeline, from the last to the first.
	fn fill_pipeline(
		&self,
		mut supplied: Vec<SuppliedExtension<'_>>,
		metadata: &Metadata,
	) -> Result<Vec<EncodedExtension>, ExtrinsicBuildError> {
		let pipeline = extension_info(metadata, self.version.extension_version())?;
		let mut encoded = VecDeque::with_capacity(pipeline.extension_ids.len());
		for declared in pipeline.extension_ids.iter().rev() {
			let implication = self.implication_after(encoded.make_contiguous());
			let position = supplied.iter().position(|extension| extension.name() == declared.name);
			let extension = position.map(|index| supplied.swap_remove(index));
			let values = self.extension_values(declared, extension, &implication)?;
			check_value_shapes(declared, &values, self.types)?;
			encoded.push_front(EncodedExtension { name: declared.name.to_string(), values });
		}
		Ok(encoded.into())
	}

	/// The implication an extension sees: for V5 the extension version, then
	/// the call, then every later extension's explicit and implicit values.
	///
	/// frame-decode's signer payload encoders return only the hashed payload a
	/// signature covers, while an extension may need the implication's
	/// preimage, so the bytes are put together here.
	fn implication_after(&self, later: &[EncodedExtension]) -> InheritedImplication {
		let mut encoded = match self.version {
			ExtrinsicVersion::V4 => Vec::new(),
			ExtrinsicVersion::V5 { extension_version } => vec![extension_version],
		};
		encoded.extend_from_slice(&self.call.bytes);
		later
			.iter()
			.for_each(|extension| encoded.extend_from_slice(&extension.values.explicit));
		later
			.iter()
			.for_each(|extension| encoded.extend_from_slice(&extension.values.implicit));
		InheritedImplication::new(encoded, self.version)
	}

	/// An extension's values: the supplied extension's, else its well-known
	/// default, else the type's unset default.
	fn extension_values(
		&self,
		declared: &ExtrinsicExtensionInfoArg<'_, u32>,
		supplied: Option<SuppliedExtension<'_>>,
		implication: &InheritedImplication,
	) -> Result<EncodedExtensionValues, ExtrinsicBuildError> {
		let Some(extension) =
			supplied.or_else(|| well_known_extension(&declared.name, &self.chain))
		else {
			return use_default_values_for_unset_extension(declared, self.types);
		};
		let types = ExtensionValueTypes { explicit: declared.id, implicit: declared.implicit_id };
		extension.encode(implication, types, self.types)
	}

	/// The extrinsic's bytes: V4 signed by `v4_signer` over the whole
	/// pipeline's implication, or V5 general.
	fn extrinsic_bytes(
		&self,
		encoded: &[EncodedExtension],
		v4_signer: Option<&SubstrateSigner>,
		metadata: &Metadata,
	) -> Result<Vec<u8>, ExtrinsicBuildError> {
		let parts = EncodedParts::new(self.call, encoded, self.version, metadata)?;
		match v4_signer {
			Some(signer) => parts.encode_v4(signer, &self.implication_after(encoded), metadata),
			None => parts.encode_v5(self.version.extension_version(), metadata),
		}
	}
}

fn use_default_values_for_unset_extension(
	declared: &ExtrinsicExtensionInfoArg<'_, u32>,
	types: &PortableRegistry,
) -> Result<EncodedExtensionValues, ExtrinsicBuildError> {
	let missing = || ExtrinsicBuildError::MissingExtension { extension: declared.name.to_string() };
	let explicit = default_value_for_type(declared.id, types).ok_or_else(missing)?;
	let implicit = default_value_for_type(declared.implicit_id, types).ok_or_else(missing)?;
	Ok(EncodedExtensionValues { explicit, implicit })
}

/// Checks that both of an extension's values decode as their metadata types and
/// leave nothing over.
fn check_value_shapes(
	declared: &ExtrinsicExtensionInfoArg<'_, u32>,
	values: &EncodedExtensionValues,
	types: &PortableRegistry,
) -> Result<(), ExtrinsicBuildError> {
	check_value_shape(&declared.name, "explicit", &values.explicit, declared.id, types)?;
	check_value_shape(&declared.name, "implicit", &values.implicit, declared.implicit_id, types)
}

fn check_value_shape(
	extension: &str,
	value_name: &'static str,
	bytes: &[u8],
	type_id: u32,
	types: &PortableRegistry,
) -> Result<(), ExtrinsicBuildError> {
	let shape_error = |reason: String| ExtrinsicBuildError::ExtensionShape {
		extension: extension.to_string(),
		value: value_name,
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

/// The call with its metadata entry, its encoded arguments and its full
/// encoding, which the implication starts with.
struct EncodedCall<'m> {
	info: ExtrinsicCallInfo<'m, u32>,
	args: Vec<u8>,
	bytes: Vec<u8>,
}

impl<'m> EncodedCall<'m> {
	fn new(call: &dyn AnyCall, metadata: &'m Metadata) -> Result<Self, ExtrinsicBuildError> {
		let info = call.call_info(metadata)?;
		let args = call.encode_args(&info, metadata.types())?;
		let mut bytes = Vec::new();
		encode_call_data_with_info_to(&EncodedArgs(&args), &info, metadata.types(), &mut bytes)
			.map_err(encode_error)?;
		Ok(Self { info, args, bytes })
	}
}

/// Everything frame-decode lays out, already computed.
struct EncodedParts<'p> {
	call: &'p EncodedCall<'p>,
	encoded: EncodedExtensions<'p>,
	pipeline: ExtrinsicExtensionInfo<'p, u32>,
}

impl<'p> EncodedParts<'p> {
	/// The call and the encoded extensions, ready for frame-decode to lay out in
	/// `version`'s format.
	fn new(
		call: &'p EncodedCall<'p>,
		encoded: &'p [EncodedExtension],
		version: ExtrinsicVersion,
		metadata: &'p Metadata,
	) -> Result<Self, ExtrinsicBuildError> {
		let pipeline = extension_info(metadata, version.extension_version())?;
		Ok(Self { call, encoded: EncodedExtensions(encoded), pipeline })
	}

	fn encode_v5(
		&self,
		extension_version: u8,
		metadata: &Metadata,
	) -> Result<Vec<u8>, ExtrinsicBuildError> {
		let mut out = Vec::new();
		encode_v5_general_with_info_to(
			&EncodedArgs(&self.call.args),
			extension_version,
			&self.encoded,
			metadata.types(),
			&self.call.info,
			&self.pipeline,
			&mut out,
		)
		.map_err(encode_error)?;
		Ok(out)
	}

	/// A V4 signed extrinsic, the preamble carrying `signer`'s signature over
	/// the whole pipeline's implication.
	fn encode_v4(
		&self,
		signer: &SubstrateSigner,
		implication: &InheritedImplication,
		metadata: &Metadata,
	) -> Result<Vec<u8>, ExtrinsicBuildError> {
		let address = MultiAddress::<AccountId32, u32>::Id(signer.account_id());
		let signature_info = metadata.extrinsic_signature_info().map_err(encode_error)?;
		let mut out = Vec::new();
		encode_v4_signed_with_info_to(
			&EncodedArgs(&self.call.args),
			&self.encoded,
			&address,
			&signer.sign(&implication.signer_payload()),
			metadata.types(),
			&self.call.info,
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

/// The encoded extensions, handed to frame-decode by name.
struct EncodedExtensions<'s>(&'s [EncodedExtension]);

impl EncodedExtensions<'_> {
	fn values_of(&self, name: &str) -> Result<&EncodedExtensionValues, TransactionExtensionsError> {
		self.0
			.iter()
			.find(|extension| extension.name == name)
			.map(|extension| &extension.values)
			.ok_or_else(|| TransactionExtensionsError::NotFound(name.to_string()))
	}
}

impl TransactionExtensions<PortableRegistry> for EncodedExtensions<'_> {
	fn contains_extension(&self, name: &str) -> bool {
		self.0.iter().any(|extension| extension.name == name)
	}

	fn encode_extension_value_to(
		&self,
		name: &str,
		_type_id: u32,
		_types: &PortableRegistry,
		out: &mut Vec<u8>,
	) -> Result<(), TransactionExtensionsError> {
		out.extend_from_slice(&self.values_of(name)?.explicit);
		Ok(())
	}

	fn encode_extension_implicit_to(
		&self,
		name: &str,
		_type_id: u32,
		_types: &PortableRegistry,
		out: &mut Vec<u8>,
	) -> Result<(), TransactionExtensionsError> {
		out.extend_from_slice(&self.values_of(name)?.implicit);
		Ok(())
	}
}
