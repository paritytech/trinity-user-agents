//! The transaction extension trait callers implement, and the implication its
//! explicit value may cover.

use scale_info::PortableRegistry;
use sp_crypto_hashing::blake2_256;
use subxt::ext::scale_encode::{EncodeAsType, Error as EncodeError, TypeResolver};

use super::{ExtrinsicBuildError, well_known::SlotValues};

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
}

/// The bytes an extension's explicit value may cover: for V5, the extension
/// version, the call, then the explicit and the implicit values of every
/// later extension in the pipeline. V4 has no version byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InheritedImplication {
	encoded: Vec<u8>,
}

impl InheritedImplication {
	/// Wraps the implication bytes the builder assembled.
	pub fn new(encoded: Vec<u8>) -> Self {
		Self { encoded }
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

/// An extension whose name and bytes are only known at run time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawExtension {
	name: String,
	explicit: Vec<u8>,
	implicit: Vec<u8>,
}

impl RawExtension {
	/// The extension `name` with already-encoded explicit and implicit values.
	pub fn new(name: impl Into<String>, explicit: Vec<u8>, implicit: Vec<u8>) -> Self {
		Self { name: name.into(), explicit, implicit }
	}
}

impl TransactionExtension for RawExtension {
	type Implicit = PreencodedValue;
	type Explicit = PreencodedValue;

	fn name(&self) -> &str {
		&self.name
	}

	fn implicit(&self) -> PreencodedValue {
		PreencodedValue(self.implicit.clone())
	}

	fn explicit(
		&self,
		_implication: &InheritedImplication,
	) -> Result<PreencodedValue, ExtensionError> {
		Ok(PreencodedValue(self.explicit.clone()))
	}
}

/// A value that is already encoded, written as it is whatever the type. The
/// builder still checks it decodes as that type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreencodedValue(pub Vec<u8>);

impl EncodeAsType for PreencodedValue {
	fn encode_as_type_to<R: TypeResolver>(
		&self,
		_type_id: R::TypeId,
		_types: &R,
		out: &mut Vec<u8>,
	) -> Result<(), EncodeError> {
		out.extend_from_slice(&self.0);
		Ok(())
	}
}

/// The metadata types of one pipeline slot.
#[derive(Clone, Copy, Debug)]
pub struct SlotTypes {
	/// The explicit value's type.
	pub explicit: u32,
	/// The implicit value's type.
	pub implicit: u32,
}

/// A supplied extension with its values not yet encoded, so the builder can
/// hold extensions of different types in one list. Both values encode once
/// the slot's types and the implication are known.
pub struct SuppliedExtension<'a> {
	/// The extension's name in metadata.
	pub name: String,
	/// Encodes both values for the slot.
	pub encode: EncodeSlot<'a>,
}

/// Encodes an extension's values against its slot's types, the explicit one
/// from the implication.
type EncodeSlot<'a> = Box<
	dyn FnOnce(
			&InheritedImplication,
			SlotTypes,
			&PortableRegistry,
		) -> Result<SlotValues, ExtrinsicBuildError>
		+ Send
		+ 'a,
>;

impl<'a> SuppliedExtension<'a> {
	/// Holds `extension` until the builder reaches its slot.
	pub fn new<E: TransactionExtension + 'a>(extension: E) -> Self {
		let name = extension.name().to_string();
		let encode: EncodeSlot<'a> = Box::new(move |implication, types, registry| {
			encode_slot(&extension, implication, types, registry)
		});
		Self { name, encode }
	}
}

fn encode_slot<E: TransactionExtension>(
	extension: &E,
	implication: &InheritedImplication,
	types: SlotTypes,
	registry: &PortableRegistry,
) -> Result<SlotValues, ExtrinsicBuildError> {
	let name = extension.name();
	let explicit_value = extension
		.explicit(implication)
		.map_err(|source| ExtrinsicBuildError::Extension { extension: name.to_string(), source })?;
	let explicit = encode_value(name, "explicit", &explicit_value, types.explicit, registry)?;
	let implicit_value = extension.implicit();
	let implicit = encode_value(name, "implicit", &implicit_value, types.implicit, registry)?;
	Ok(SlotValues { explicit, implicit })
}

fn encode_value(
	extension: &str,
	slot: &'static str,
	value: &impl EncodeAsType,
	type_id: u32,
	registry: &PortableRegistry,
) -> Result<Vec<u8>, ExtrinsicBuildError> {
	value
		.encode_as_type(type_id, registry)
		.map_err(|error| ExtrinsicBuildError::ExtensionEncode {
			extension: extension.to_string(),
			slot,
			type_id,
			reason: error.to_string(),
		})
}
