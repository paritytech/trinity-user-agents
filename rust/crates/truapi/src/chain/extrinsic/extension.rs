//! The transaction extension trait callers implement, and the implication its
//! explicit value may cover.

use parity_scale_codec::Encode;
use sp_crypto_hashing::blake2_256;
use subxt::utils::Encoded;

/// A transaction extension that a caller supplies to the builder.
///
/// The builder computes extensions from the last one in the runtime's
/// pipeline to the first, so `explicit` sees every later extension's bytes
/// and can prove or sign over them. Builds run on any thread, so an extension
/// is `Send`.
pub trait TransactionExtension: Send {
	/// The value only the signer payload carries.
	type Implicit: Encode;
	/// The value the extrinsic carries.
	type Explicit: Encode;

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

	/// The wrapped error, for a caller that wants its own type back.
	pub fn into_inner(self) -> Box<dyn core::error::Error + Send + Sync> {
		self.0
	}
}

/// An extension whose name and bytes are only known at run time.
///
/// Its values encode as the bytes given, without a length prefix.
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
	type Implicit = Encoded;
	type Explicit = Encoded;

	fn name(&self) -> &str {
		&self.name
	}

	fn implicit(&self) -> Encoded {
		Encoded(self.implicit.clone())
	}

	fn explicit(&self, _implication: &InheritedImplication) -> Result<Encoded, ExtensionError> {
		Ok(Encoded(self.explicit.clone()))
	}
}

/// A supplied extension with its values encoded, so the builder can hold
/// extensions of different types in one list. The explicit value waits for
/// the implication, which exists only once every later extension is known.
pub struct SuppliedExtension<'a> {
	/// The extension's name in metadata.
	pub name: String,
	/// The encoded implicit value.
	pub implicit: Vec<u8>,
	/// Computes the encoded explicit value from the implication.
	pub explicit: ComputeExplicit<'a>,
}

/// Computes an extension's encoded explicit value from its implication.
type ComputeExplicit<'a> =
	Box<dyn FnOnce(&InheritedImplication) -> Result<Vec<u8>, ExtensionError> + Send + 'a>;

impl<'a> SuppliedExtension<'a> {
	/// Encodes `extension`'s name and implicit value now and its explicit
	/// value when the builder calls for it.
	pub fn new<E: TransactionExtension + 'a>(extension: E) -> Self {
		let name = extension.name().to_string();
		let implicit = extension.implicit().encode();
		let explicit = Box::new(move |implication: &InheritedImplication| {
			extension.explicit(implication).map(|explicit| explicit.encode())
		});
		Self { name, implicit, explicit }
	}
}
