use parity_scale_codec::{Decode, Encode};

/// Generic error payload carrying a human-readable reason string. Used by many
/// methods as a catch-all error type.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct GenericError {
	/// Human-readable failure reason.
	pub reason: String,
}

/// X25519 public key for key agreement (RFC 0022).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct X25519PublicKey(pub [u8; 32]);

/// Topic a statement-store statement is published under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct StatementTopic(pub [u8; 32]);
