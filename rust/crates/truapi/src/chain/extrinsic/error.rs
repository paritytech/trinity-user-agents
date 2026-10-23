//! Why an extrinsic could not be built.

use super::{ExtensionError, ExtrinsicVersion};
use crate::{chain::MortalityError, chain_runtime::RuntimeFailure};

/// Why an extrinsic could not be built.
#[derive(Debug, thiserror::Error)]
pub enum ExtrinsicBuildError {
	/// The chain could not be read.
	#[error(transparent)]
	Chain(#[from] RuntimeFailure),
	/// The host could not say which chains it builds as V5.
	#[error("the host could not list its V5 chains: {0}")]
	HostVersions(String),
	/// The builder was given no call.
	#[error(
		"no call was given, expected one call through with_call, with_call_args or with_call_bytes"
	)]
	NoCall,
	/// A call could not be encoded against the runtime's metadata.
	#[error("call {pallet}.{call} does not encode against metadata: {reason}")]
	Call {
		/// Pallet name.
		pallet: String,
		/// Call name.
		call: String,
		/// The encoder's message.
		reason: String,
	},
	/// Call bytes do not start with a pallet and call index the runtime knows.
	#[error(
		"call bytes {call_bytes} do not name a known call, expected a pallet index and a call index: {reason}"
	)]
	CallBytes {
		/// The bytes, hex encoded.
		call_bytes: String,
		/// Why they were rejected.
		reason: String,
	},
	/// The metadata could not answer a question the build needs.
	#[error("metadata: {0}")]
	Metadata(String),
	/// No extension pipeline declares the extension the caller pinned to.
	#[error(
		"no extension pipeline declares {extension}, expected at least one V5 pipeline listing it"
	)]
	NoPipelineDeclares {
		/// The extension name.
		extension: String,
	},
	/// The chosen pipeline is not in metadata.
	#[error("extension pipeline {extension_version} is not declared: {reason}")]
	UnknownPipeline {
		/// The pipeline number.
		extension_version: u8,
		/// The metadata's message.
		reason: String,
	},
	/// The runtime does not accept the chosen extrinsic format.
	#[error("the runtime accepts extrinsic versions {supported:?}, not {version:?}")]
	UnsupportedVersion {
		/// The chosen version.
		version: ExtrinsicVersion,
		/// The extrinsic format versions the runtime accepts.
		supported: Vec<u8>,
	},
	/// A V4 extrinsic was requested without a signer.
	#[error("a V4 extrinsic needs a signer, expected with_signer or a V5 version")]
	UnsignedV4,
	/// A signed V5 extrinsic was requested on a pipeline with no signature slot.
	#[error(
		"extension pipeline {extension_version} has no VerifyMultiSignature, so a signer cannot sign this V5 extrinsic"
	)]
	NoSignatureSlot {
		/// The pipeline number.
		extension_version: u8,
	},
	/// The caller supplied an extension the chosen pipeline does not list.
	#[error(
		"extension pipeline {extension_version} does not list {extension}; it lists {declared:?}"
	)]
	UndeclaredExtension {
		/// The extension name.
		extension: String,
		/// The pipeline number.
		extension_version: u8,
		/// The extensions the pipeline lists.
		declared: Vec<String>,
	},
	/// The caller supplied the same extension twice.
	#[error("extension {extension} was supplied more than once, expected it once")]
	DuplicateExtension {
		/// The extension name.
		extension: String,
	},
	/// An extension the pipeline requires has no value and no default.
	#[error(
		"extension {extension} has no value, expected the caller to supply it because its type is neither empty nor an Option"
	)]
	MissingExtension {
		/// The extension name.
		extension: String,
	},
	/// An extension value does not match its metadata type.
	#[error("{extension} {slot} value 0x{bytes} does not match its metadata type: {reason}")]
	ExtensionShape {
		/// The extension name.
		extension: String,
		/// `explicit` or `implicit`.
		slot: &'static str,
		/// The bytes, hex encoded.
		bytes: String,
		/// The decoder's message.
		reason: String,
	},
	/// A supplied extension's value does not encode as its metadata type.
	#[error("{extension} {slot} value does not encode as metadata type {type_id}: {reason}")]
	ExtensionEncode {
		/// The extension name.
		extension: String,
		/// `explicit` or `implicit`.
		slot: &'static str,
		/// The metadata type it was encoded against.
		type_id: u32,
		/// The encoder's message.
		reason: String,
	},
	/// A supplied extension failed to produce its explicit value.
	#[error("extension {extension} failed: {source}")]
	Extension {
		/// The extension name.
		extension: String,
		/// What the extension reported.
		#[source]
		source: ExtensionError,
	},
	/// The birth block and period do not form a mortal era.
	#[error(transparent)]
	Mortality(#[from] MortalityError),
	/// frame-decode could not encode the assembled parts.
	#[error("encoding the extrinsic failed: {0}")]
	Encode(String),
}
