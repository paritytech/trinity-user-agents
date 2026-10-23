//! The transaction extension trait callers implement, and the implication its
//! explicit value may cover.

use scale_info::PortableRegistry;
use sp_crypto_hashing::blake2_256;
use subxt::ext::scale_encode::EncodeAsType;

use super::{
	ExtrinsicBuildError, ExtrinsicVersion, signature::SubstrateSigner,
	well_known::EncodedExtensionValues,
};

/// A transaction extension that a caller supplies to the builder.
///
/// The builder computes extensions from the last one in the runtime's
/// pipeline to the first, so `explicit` sees every later extension's bytes
/// and can prove or sign over them. Both values encode against the types the
/// runtime declares for them, matching variants and fields by name. Builds run
/// on any thread, so an extension is `Send`.
pub trait TransactionExtension: Send {
	/// The value only the signer payload carries.
	type Implicit: EncodeAsType;
	/// The value the extrinsic carries.
	type Explicit: EncodeAsType;

	/// The extension's name in metadata.
	fn name(&self) -> &str;

	/// The implicit value.
	fn implicit(&self) -> Self::Implicit;

	/// The explicit value, computed once every later extension is known.
	fn explicit(
		&self,
		implication: &InheritedImplication,
	) -> Result<Self::Explicit, ExtensionError>;

	/// The signer whose signature this extension carries, if any. A V4
	/// extrinsic takes it out of the pipeline and signs the preamble with it.
	fn signer(&self) -> Option<&SubstrateSigner> {
		None
	}
}

/// The bytes an extension's explicit value may cover: for V5, the extension
/// version, the call, then the explicit and the implicit values of every
/// later extension in the pipeline. V4 has no version byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InheritedImplication {
	encoded: Vec<u8>,
	version: ExtrinsicVersion,
}

impl InheritedImplication {
	/// Wraps the implication bytes the builder assembled for `version`.
	pub fn new(encoded: Vec<u8>, version: ExtrinsicVersion) -> Self {
		Self { encoded, version }
	}

	/// What a signature covers: the hash of the implication for V5; for V4
	/// the implication itself, hashed only when longer than 256 bytes.
	pub fn signer_payload(&self) -> Vec<u8> {
		let v4_short = matches!(self.version, ExtrinsicVersion::V4) && self.encoded.len() <= 256;
		if v4_short {
			return self.encoded.clone();
		}
		self.hash().to_vec()
	}

	/// The implication bytes.
	pub fn encoded(&self) -> &[u8] {
		&self.encoded
	}

	/// The Blake2-256 hash of the implication, which signatures and ring
	/// proofs cover.
	pub fn hash(&self) -> [u8; 32] {
		blake2_256(&self.encoded)
	}
}

/// Why an extension could not produce its explicit value.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct ExtensionError(Box<dyn core::error::Error + Send + Sync>);

impl ExtensionError {
	/// Wraps the error the extension ran into.
	pub fn new(error: impl core::error::Error + Send + Sync + 'static) -> Self {
		Self(Box::new(error))
	}

	/// The wrapped error as `E`, when it is one, so a caller gets its own
	/// error back; otherwise the error unchanged.
	pub fn downcast<E: core::error::Error + 'static>(self) -> Result<E, Self> {
		self.0.downcast::<E>().map(|error| *error).map_err(Self)
	}
}

/// The metadata types of one extension's values.
#[derive(Clone, Copy, Debug)]
pub struct ExtensionValueTypes {
	/// The explicit value's type.
	pub explicit: u32,
	/// The implicit value's type.
	pub implicit: u32,
}

/// A supplied extension of any type, so the builder holds extensions of
/// different types in one list and encodes each once the pipeline reaches it.
/// `TransactionExtension` cannot be boxed itself: its value types differ per
/// extension, and `EncodeAsType` is generic over the type resolver.
pub trait AnyExtension: Send {
	/// The extension's name in metadata.
	fn name(&self) -> &str;

	/// The signer whose signature the extension carries, if any.
	fn signer(&self) -> Option<&SubstrateSigner>;

	/// Encodes both values against their metadata types, the explicit one from
	/// the implication.
	fn encode(
		&self,
		implication: &InheritedImplication,
		types: ExtensionValueTypes,
		registry: &PortableRegistry,
	) -> Result<EncodedExtensionValues, ExtrinsicBuildError>;
}

impl<E: TransactionExtension> AnyExtension for E {
	fn name(&self) -> &str {
		TransactionExtension::name(self)
	}

	fn signer(&self) -> Option<&SubstrateSigner> {
		TransactionExtension::signer(self)
	}

	fn encode(
		&self,
		implication: &InheritedImplication,
		types: ExtensionValueTypes,
		registry: &PortableRegistry,
	) -> Result<EncodedExtensionValues, ExtrinsicBuildError> {
		encode_extension_values(self, implication, types, registry)
	}
}

/// An extension the caller or the builder supplied, held until the pipeline
/// reaches it.
pub type SuppliedExtension<'a> = Box<dyn AnyExtension + 'a>;

fn encode_extension_values<E: TransactionExtension>(
	extension: &E,
	implication: &InheritedImplication,
	types: ExtensionValueTypes,
	registry: &PortableRegistry,
) -> Result<EncodedExtensionValues, ExtrinsicBuildError> {
	let name = extension.name();
	let explicit_value = extension
		.explicit(implication)
		.map_err(|source| ExtrinsicBuildError::Extension { extension: name.to_string(), source })?;
	let explicit = encode_value(name, "explicit", &explicit_value, types.explicit, registry)?;
	let implicit_value = extension.implicit();
	let implicit = encode_value(name, "implicit", &implicit_value, types.implicit, registry)?;
	Ok(EncodedExtensionValues { explicit, implicit })
}

fn encode_value(
	extension: &str,
	value_name: &'static str,
	value: &impl EncodeAsType,
	type_id: u32,
	registry: &PortableRegistry,
) -> Result<Vec<u8>, ExtrinsicBuildError> {
	value
		.encode_as_type(type_id, registry)
		.map_err(|error| ExtrinsicBuildError::ExtensionEncode {
			extension: extension.to_string(),
			value: value_name,
			type_id,
			reason: error.to_string(),
		})
}
