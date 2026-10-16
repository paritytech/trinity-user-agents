use parity_scale_codec::{Decode, Encode};

/// Request to open the host's share sheet with a text.
///
/// The host draws the sheet and lets the user choose where the text goes. The
/// product learns only whether the user shared it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct HostShareRequest {
	/// The text to share, for example a message with a link. At most 8 KiB of
	/// UTF-8, with no control characters other than line breaks and tabs.
	pub text: String,
}

/// How a share ended.
///
/// A dismissal is an outcome rather than an error, because it is worth
/// offering again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ShareOutcome {
	/// The user sent the text somewhere.
	Shared,
	/// The user closed the sheet without sharing.
	Dismissed,
}

/// Outcome of a share.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostShareResponse {
	/// How the share ended.
	pub outcome: ShareOutcome,
}

/// Error returned by the share sheet.
///
/// A host with no share sheet answers `Unsupported` at the framework level
/// rather than through this enum.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostShareError {
	/// Another share sheet is open.
	Busy,
	/// The calling execution is not on screen, and is not a Worker handling a
	/// tap from the user, so no sheet was opened.
	NotVisible,
	/// The request breaks a limit, so no sheet was shown.
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
