//! A pairing host's key change that the signing host has not answered yet.
//!
//! The record lives in its own core storage slot,
//! [`CoreStorageKey::PendingSessionRotation`], next to the session. Whichever
//! run or tab finds it sends the same request again, and the signing host
//! answers a request it already served with the same answer, so the change
//! completes on the keys it was started with however often it is interrupted.
//!
//! [`CoreStorageKey::PendingSessionRotation`]: crate::platform::CoreStorageKey::PendingSessionRotation

use parity_scale_codec::{Decode, DecodeAll, Encode};

use crate::{
	host_internal::{extrinsic::sr25519_secret_from_bytes, sso_messages::RotateSessionRequest},
	host_logic::{
		product_account::SR25519_SIGNING_CONTEXT,
		session::SsoSessionInfo,
		sso::pairing::{
			PairingDeviceIdentity, generate_pairing_device_identity, rotated_sso_session_info,
		},
	},
};

/// The new keys of one session's key change and the request that asks for it.
#[derive(derive_more::Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PendingSessionRotation {
	/// Statement-store public key of the session being replaced.
	replaces: [u8; 32],
	/// Message id the request is sent under, every time it is sent.
	message_id: String,
	/// The pairing host's keys once the change is answered.
	#[debug("\"<redacted>\"")]
	keys: PairingDeviceIdentity,
	/// The new statement key's signature carried by the request.
	signature: [u8; 64],
}

/// The stored form of [`PendingSessionRotation`], so a later layout is a new version
/// that older cores leave alone instead of overwriting.
#[derive(Encode, Decode)]
enum VersionedPendingRotation {
	#[codec(index = 0)]
	V1(PendingSessionRotation),
}

/// What the pending rotation slot holds for one session.
#[derive(Debug, PartialEq, Eq)]
pub enum StoredPendingSessionRotation {
	/// A key change of this session, still unanswered.
	Pending(Box<PendingSessionRotation>),
	/// Nothing, or another session's change, which a new one replaces.
	Replaceable,
	/// A value this core cannot read, possibly a newer core's, which it must
	/// leave as it is.
	Unreadable,
}

impl PendingSessionRotation {
	/// A change of `session` to freshly generated keys, sent as `message_id`.
	pub fn start(session: &SsoSessionInfo, message_id: String) -> Result<Self, String> {
		let keys = generate_pairing_device_identity().map_err(|error| error.to_string())?;
		let payload = RotateSessionRequest::signed_payload(
			session.ss_public_key,
			keys.statement_store_public_key,
			keys.encryption_public_key,
		);
		let secret = sr25519_secret_from_bytes(&keys.statement_store_secret)?;
		let signature = secret
			.sign_simple(SR25519_SIGNING_CONTEXT, &payload, &secret.to_public())
			.to_bytes();
		Ok(Self { replaces: session.ss_public_key, message_id, keys, signature })
	}

	/// What `stored`, the slot's value, holds for `session`.
	pub fn from_stored(
		stored: Option<&[u8]>,
		session: &SsoSessionInfo,
	) -> StoredPendingSessionRotation {
		let Some(mut blob) = stored else {
			return StoredPendingSessionRotation::Replaceable;
		};
		match VersionedPendingRotation::decode_all(&mut blob) {
			Ok(VersionedPendingRotation::V1(pending))
				if pending.replaces == session.ss_public_key =>
			{
				StoredPendingSessionRotation::Pending(Box::new(pending))
			},
			Ok(VersionedPendingRotation::V1(_)) => StoredPendingSessionRotation::Replaceable,
			Err(_) => StoredPendingSessionRotation::Unreadable,
		}
	}

	/// The slot's value for this change.
	pub fn to_stored(&self) -> Vec<u8> {
		VersionedPendingRotation::V1(self.clone()).encode()
	}

	/// Message id the request is sent under.
	pub fn message_id(&self) -> &str {
		&self.message_id
	}

	/// The request asking the signing host for this change.
	pub fn request(&self) -> RotateSessionRequest {
		RotateSessionRequest {
			statement_account_id: self.keys.statement_store_public_key,
			encryption_public_key: self.keys.encryption_public_key,
			signature: self.signature,
		}
	}

	/// The session `current` continues as once the change is answered.
	pub fn rotated(&self, current: &SsoSessionInfo) -> Result<SsoSessionInfo, String> {
		rotated_sso_session_info(current, self.keys.clone())
	}
}

#[cfg(test)]
mod tests;
