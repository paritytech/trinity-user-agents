use parity_scale_codec::{Decode, Encode};

/// A credential a product obtained from its own service and keeps with the
/// host. Its `Debug` form never shows either secret.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct AuthToken {
	/// The token the service issued, typically a JWT. Never empty.
	pub token: String,
	/// The token the product exchanges with its service for a new one, if the
	/// service issued one.
	pub refresh_token: Option<String>,
	/// When `token` expires, in milliseconds since the Unix epoch, if the
	/// service said. The host does not act on it.
	pub expires_at: Option<u64>,
}

impl core::fmt::Debug for AuthToken {
	fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		formatter
			.debug_struct("AuthToken")
			.field("token", &"<redacted>")
			.field("refresh_token", &self.refresh_token.as_ref().map(|_| "<redacted>"))
			.field("expires_at", &self.expires_at)
			.finish()
	}
}

/// Request to keep a token for one of the product's services.
///
/// `service` names the service within the product: 1 to 64 characters of
/// `a-z`, `0-9`, `.`, `_` and `-`. `token` and `refresh_token` together take
/// at most 8192 bytes of UTF-8. A token already kept for `service` is
/// replaced. A product keeps tokens for at most 16 services.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAuthTokensStoreRequest {
	/// The product's name for the service the token belongs to.
	pub service: String,
	/// The token to keep.
	pub token: AuthToken,
}

/// Why a token was not kept.
///
/// A product without a session is answered `Denied`, and a host that cannot
/// keep tokens `Unsupported`, at the framework level rather than through this
/// enum.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostAuthTokensStoreError {
	/// `service` is not 1 to 64 characters of `a-z`, `0-9`, `.`, `_` and `-`.
	InvalidService,
	/// `token` is empty.
	EmptyToken,
	/// `token` and `refresh_token` together exceed `max` bytes.
	TooLarge {
		/// The limit, in bytes of UTF-8.
		max: u32,
	},
	/// The product already keeps tokens for `max` services, and `service` is
	/// not one of them.
	TooManyServices {
		/// The most services one product may keep a token for.
		max: u32,
	},
	/// Catch-all.
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}

/// Request to read the token kept for one of the product's services.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAuthTokensGetRequest {
	/// The product's name for the service.
	pub service: String,
}

/// The token kept for the service.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAuthTokensGetResponse {
	/// The token, or `None` when none is kept, including after sign-out.
	pub token: Option<AuthToken>,
}

/// Request to drop the token kept for one of the product's services.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAuthTokensRemoveRequest {
	/// The product's name for the service.
	pub service: String,
}

/// Why a token could not be read or dropped.
///
/// A product without a session is answered `Denied`, and a host that cannot
/// keep tokens `Unsupported`, at the framework level rather than through this
/// enum.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostAuthTokensLookupError {
	/// `service` is not 1 to 64 characters of `a-z`, `0-9`, `.`, `_` and `-`.
	InvalidService,
	/// Catch-all.
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}
