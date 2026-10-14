/// Result of answering an SSO message for a caller-owned session.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum SsoRequestOutcome {
	/// Encoded response to post on the session.
	Response {
		/// SCALE-encoded `RemoteMessage` response.
		message: Vec<u8>,
	},
	/// The peer ended the session.
	Disconnected,
	/// The message did not require a response.
	Ignored,
}
