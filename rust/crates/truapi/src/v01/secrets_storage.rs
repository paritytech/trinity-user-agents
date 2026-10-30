use parity_scale_codec::{Decode, Encode};

/// Request to keep a secret under one of the product's keys.
///
/// `key` is 1 to 64 characters of `a-z`, `0-9`, `.`, `_` and `-`. `value` is
/// 1 to 8192 bytes. A value already kept under `key` is replaced. A product
/// keeps at most 16 keys per account.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSecretsStorageWriteRequest {
	/// The product's name for the secret.
	pub key: String,
	/// The secret to keep.
	pub value: Vec<u8>,
}

impl core::fmt::Debug for HostSecretsStorageWriteRequest {
	fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		formatter
			.debug_struct("HostSecretsStorageWriteRequest")
			.field("key", &self.key)
			.field("value", &"<redacted>")
			.finish()
	}
}

/// Why a secret was not kept.
///
/// A product without a session is answered `Denied`, and a host that cannot
/// keep secrets `Unsupported`, at the framework level rather than through
/// this enum.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostSecretsStorageWriteError {
	/// `key` is not 1 to 64 characters of `a-z`, `0-9`, `.`, `_` and `-`.
	InvalidKey,
	/// `value` is empty.
	EmptyValue,
	/// `value` is longer than `max` bytes.
	TooLarge {
		/// The limit, in bytes.
		max: u32,
	},
	/// Catch-all.
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}

/// Request to read the secret kept under one of the product's keys.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSecretsStorageReadRequest {
	/// The product's name for the secret.
	pub key: String,
}

/// The secret kept under the key.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSecretsStorageReadResponse {
	/// The secret, or `None` when none is kept, including after sign-out.
	pub value: Option<Vec<u8>>,
}

impl core::fmt::Debug for HostSecretsStorageReadResponse {
	fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		formatter
			.debug_struct("HostSecretsStorageReadResponse")
			.field("value", &self.value.as_ref().map(|_| "<redacted>"))
			.finish()
	}
}

/// Request to drop the secret kept under one of the product's keys.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSecretsStorageClearRequest {
	/// The product's name for the secret.
	pub key: String,
}

/// Why a secret could not be read or dropped.
///
/// A product without a session is answered `Denied`, and a host that cannot
/// keep secrets `Unsupported`, at the framework level rather than through
/// this enum.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostSecretsStorageLookupError {
	/// `key` is not 1 to 64 characters of `a-z`, `0-9`, `.`, `_` and `-`.
	InvalidKey,
	/// Catch-all.
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}
