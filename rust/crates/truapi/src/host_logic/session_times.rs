//! When a pairing host's paired session began and when its keys last changed.
//!
//! The times live in their own core storage slot, [`CoreStorageKey::SessionTimes`],
//! and not in the session blob: a core that cannot decode the blob deletes
//! it, and the blob is shared by every tab of a browser host, so a new field
//! there would sign the user out of any tab still running an older core.
//!
//! These times only keep an honest pairing host off a session its signing
//! host has already dropped. They do not stop whoever copied the session, who
//! can drop or rewrite the slot; the signing host enforces the same limits.

use parity_scale_codec::{Decode, DecodeAll, Encode};

use crate::host_logic::session::SsoSessionInfo;

/// Seconds after pairing at which a session ends.
pub const SESSION_LIFETIME_SECS: u64 = 30 * 24 * 60 * 60;

/// Seconds after which a session's keys are due to change.
pub const KEY_ROTATION_INTERVAL_SECS: u64 = 24 * 60 * 60;

/// Times of the paired session whose statement-store key is `session_key`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SessionTimes {
	/// The session's own statement-store public key.
	session_key: [u8; 32],
	/// Unix seconds when the session was paired, or first seen by a core that
	/// keeps these times.
	paired_at: u64,
	/// Unix seconds when the session's keys last changed, `None` when no core
	/// recorded it.
	last_rotated_at: Option<u64>,
}

/// The stored form of [`SessionTimes`], so a later layout is a new version
/// that older cores leave alone instead of overwriting.
#[derive(Encode, Decode)]
enum VersionedSessionTimes {
	#[codec(index = 0)]
	V1(SessionTimes),
}

/// What the session times slot holds for one session.
#[derive(Debug, PartialEq, Eq)]
pub enum StoredSessionTimes {
	/// The session's own times.
	Current(SessionTimes),
	/// Nothing, or another session's times, which the session's own replace.
	Replaceable,
	/// A value this core cannot read, which it must leave as it is.
	Unreadable,
}

impl SessionTimes {
	/// Times of a session paired at `now`, whose keys are new.
	pub fn paired(session: &SsoSessionInfo, now: u64) -> Self {
		Self { session_key: session.ss_public_key, paired_at: now, last_rotated_at: Some(now) }
	}

	/// Times of a session first seen at `now` without any, so paired before
	/// this core kept them.
	pub fn first_seen(session: &SsoSessionInfo, now: u64) -> Self {
		Self { session_key: session.ss_public_key, paired_at: now, last_rotated_at: None }
	}

	/// What `stored`, the slot's value, holds for `session`.
	pub fn from_stored(stored: Option<&[u8]>, session: &SsoSessionInfo) -> StoredSessionTimes {
		let Some(mut blob) = stored else {
			return StoredSessionTimes::Replaceable;
		};
		match VersionedSessionTimes::decode_all(&mut blob) {
			Ok(VersionedSessionTimes::V1(times)) if times.session_key == session.ss_public_key => {
				StoredSessionTimes::Current(times)
			},
			Ok(VersionedSessionTimes::V1(_)) => StoredSessionTimes::Replaceable,
			Err(_) => StoredSessionTimes::Unreadable,
		}
	}

	/// The slot's value for these times.
	pub fn to_stored(&self) -> Vec<u8> {
		VersionedSessionTimes::V1(self.clone()).encode()
	}

	/// Whether the session has outlived [`SESSION_LIFETIME_SECS`] at `now`.
	pub fn is_expired(&self, now: u64) -> bool {
		now.saturating_sub(self.paired_at) >= SESSION_LIFETIME_SECS
	}

	/// Whether the session's keys are due to change at `now`.
	pub fn is_rotation_due(&self, now: u64) -> bool {
		self.last_rotated_at
			.is_none_or(|rotated_at| now.saturating_sub(rotated_at) > KEY_ROTATION_INTERVAL_SECS)
	}
}

#[cfg(test)]
mod tests;
