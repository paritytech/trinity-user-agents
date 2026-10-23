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

use super::{ExtrinsicBuilder, RuntimeAtBirth};
use crate::chain::{
	EncodedExtrinsic, MortalExtrinsic, Mortality,
	extrinsic::{
		ExtrinsicBuildError, ExtrinsicVersion, InheritedImplication,
		extension::{ExtensionValueTypes, SuppliedExtension},
		signature::{SubstrateSigner, VerifySignatureExtension},
		version::{choose_extrinsic_version, pipeline_names},
		well_known::{
			ChainValues, EncodedExtensionValues, default_value_for_type, well_known_extension,
		},
	},
};

/// Builds the extrinsic `builder` describes.
///
/// What the caller left unset is read only when needed:
/// `host_extension_version` gives the host's pipeline for the chain when no
/// version is pinned, and `account_nonce` gives the signer's next index when
/// no nonce is set.
pub async fn assemble<HostRead, NonceRead>(
	builder: ExtrinsicBuilder<'_>,
	host_extension_version: impl FnOnce() -> HostRead,
	account_nonce: impl FnOnce(AccountId32) -> NonceRead,
) -> Result<MortalExtrinsic, ExtrinsicBuildError>
where
	HostRead: Future<Output = Result<Option<u8>, ExtrinsicBuildError>>,
	NonceRead: Future<Output = Result<u64, ExtrinsicBuildError>>,
{
	let metadata = builder.runtime.metadata.clone();
	let signer = builder.signer();
	let version = choose_extrinsic_version(
		builder.version,
		host_extension_version,
		signer.is_some(),
		&metadata,
	)
	.await?;
	let nonce = choose_nonce(builder.nonce, signer, account_nonce).await?;
	encode_extrinsic(builder, version, nonce)
}

/// The nonce `CheckNonce` carries: the caller's, else the signer's next index,
/// else 0 for a build nobody signs.
async fn choose_nonce<NonceRead>(
	pinned: Option<u64>,
	signer: Option<&SubstrateSigner>,
	account_nonce: impl FnOnce(AccountId32) -> NonceRead,
) -> Result<u64, ExtrinsicBuildError>
where
	NonceRead: Future<Output = Result<u64, ExtrinsicBuildError>>,
{
	if let Some(nonce) = pinned {
		return Ok(nonce);
	}
	let Some(signer) = signer else {
		return Ok(0);
	};
	account_nonce(signer.account_id()).await
}

/// Encodes the extrinsic once its version and nonce are settled.
fn encode_extrinsic(
	builder: ExtrinsicBuilder<'_>,
	version: ExtrinsicVersion,
	nonce: u64,
) -> Result<MortalExtrinsic, ExtrinsicBuildError> {
	let ExtrinsicBuilder { runtime, call, extensions, signature, period, .. } = builder;
	let metadata = runtime.metadata.clone();
	check_format_supported(version, &metadata)?;
	let pending_call = call.ok_or(ExtrinsicBuildError::NoCall)?;
	let call = pending_call(&metadata)?;
	let (extensions, v4_signer) = supplied_extensions(version, extensions, signature, &metadata)?;
	let mortality = Mortality::new(runtime.birth, period)?;
	let chain = chain_values(&runtime, mortality, nonce);
	let fold = PipelineFold { version, call: &call, chain, types: metadata.types() };
	let encoded = fold.fill_pipeline(extensions, &metadata)?;
	let bytes = fold.lay_out(&encoded, v4_signer, &metadata)?;
	Ok(MortalExtrinsic { extrinsic: EncodedExtrinsic::new(bytes), mortality })
}

fn chain_values(runtime: &RuntimeAtBirth, mortality: Mortality, nonce: u64) -> ChainValues {
	ChainValues {
		genesis: runtime.genesis,
		spec_version: runtime.spec_version,
		transaction_version: runtime.transaction_version,
		mortality,
		nonce,
	}
}

/// The supplied extensions the pipeline fills, with the signature placed for
/// `version`, and the V4 signer if there is one.
fn supplied_extensions<'a>(
	version: ExtrinsicVersion,
	extensions: Vec<SuppliedExtension<'a>>,
	signature: Option<VerifySignatureExtension<'a>>,
	metadata: &Metadata,
) -> Result<(Vec<SuppliedExtension<'a>>, Option<&'a SubstrateSigner>), ExtrinsicBuildError> {
	let (extensions, v4_signer) = place_signature(version, extensions, signature)?;
	check_supplied_extensions(&extensions, version.extension_version(), metadata)?;
	Ok((extensions, v4_signer))
}

/// Where the signature goes: a V5 extrinsic carries `VerifyMultiSignature` like
/// any extension; a V4 one takes its signer out of the pipeline to sign the
/// preamble, and cannot be built without one.
fn place_signature<'a>(
	version: ExtrinsicVersion,
	mut extensions: Vec<SuppliedExtension<'a>>,
	signature: Option<VerifySignatureExtension<'a>>,
) -> Result<(Vec<SuppliedExtension<'a>>, Option<&'a SubstrateSigner>), ExtrinsicBuildError> {
	if let ExtrinsicVersion::V5 { .. } = version {
		extensions.extend(signature.map(SuppliedExtension::new));
		return Ok((extensions, None));
	}
	let signer = signature.and_then(|signature| signature.signer());
	Ok((extensions, Some(signer.ok_or(ExtrinsicBuildError::UnsignedV4)?)))
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
/// twice, since an extension has only one value.
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

/// One pipeline extension's name and encoded values.
struct EncodedExtension {
	name: String,
	values: EncodedExtensionValues,
}

/// What filling the pipeline needs besides the supplied extensions.
struct PipelineFold<'f> {
	version: ExtrinsicVersion,
	call: &'f [u8],
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
			let position = supplied.iter().position(|extension| extension.name == declared.name);
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
		encoded.extend_from_slice(self.call);
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
		(extension.encode)(implication, types, self.types)
	}

	/// The extrinsic's bytes: V4 signed by `v4_signer` over the whole
	/// pipeline's implication, or V5 general.
	fn lay_out(
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
	encoded: EncodedExtensions<'p>,
	call_info: ExtrinsicCallInfo<'p, u32>,
	pipeline: ExtrinsicExtensionInfo<'p, u32>,
}

impl<'p> EncodedParts<'p> {
	/// The call and the encoded extensions, ready for frame-decode to lay out in
	/// `version`'s format.
	fn new(
		call: &'p [u8],
		encoded: &'p [EncodedExtension],
		version: ExtrinsicVersion,
		metadata: &'p Metadata,
	) -> Result<Self, ExtrinsicBuildError> {
		let (call_info, args) = split_call(call, metadata)?;
		let pipeline = extension_info(metadata, version.extension_version())?;
		Ok(Self {
			args: EncodedArgs(args),
			encoded: EncodedExtensions(encoded),
			call_info,
			pipeline,
		})
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
			&self.encoded,
			metadata.types(),
			&self.call_info,
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
			&self.args,
			&self.encoded,
			&address,
			&signer.sign(&implication.signer_payload()),
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
