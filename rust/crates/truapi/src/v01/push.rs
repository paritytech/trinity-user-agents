use parity_scale_codec::{Decode, Encode};

/// Request for the push address of a room of the calling product.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushAddressRequest {
	/// A room the calling product created.
	pub room_id: String,
}

/// The push address of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushAddressResponse {
	/// Opaque bytes that name this device and this room for a push. The
	/// product gives them to one contact through its own encrypted channel.
	/// The address of a room stays the same until the device gets a new push
	/// token.
	pub address: Vec<u8>,
}

/// Error of a push address request.
///
/// A host with no push relay answers `Unsupported` at the framework level
/// rather than through this enum.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPushAddressError {
	/// The device has no push token now, for example because the user did not
	/// allow notifications. Ask again later.
	NotAvailable {
		/// Human-readable reason.
		reason: String,
	},
	/// The room id breaks a limit.
	InvalidRequest {
		/// Which limit.
		reason: String,
	},
	/// Catch-all.
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}

/// Request to show a notification on the device of a push address.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushNotifyRequest {
	/// A push address that a contact gave.
	pub address: Vec<u8>,
	/// The title of the notification, at most 64 Unicode scalar values.
	pub title: String,
	/// The text of the notification, at most 256 Unicode scalar values.
	pub body: String,
}

/// The push relay took the notification.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushNotifyResponse {}

/// Error of a notification request.
///
/// A host with no push relay answers `Unsupported` at the framework level
/// rather than through this enum.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPushNotifyError {
	/// The address is not a push address of this version.
	InvalidAddress,
	/// The title or the text breaks a limit.
	InvalidRequest {
		/// Which limit.
		reason: String,
	},
	/// The product sent too many notifications in a short time. Nothing was
	/// sent.
	RateLimited,
	/// The push service did not take the notification, for example because
	/// the device token is no longer valid.
	Rejected {
		/// Human-readable reason.
		reason: String,
	},
	/// Catch-all.
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}
