//! The pairing host's change to new session keys once its keys are due.
//!
//! The change is kept in [`CoreStorageKey::PendingSessionRotation`] until the
//! signing host answers it, and then the new keys, their times and the session
//! move to storage together. A signing host that refuses the change leaves the
//! session on its keys, which a later check asks to change again.

use core::{pin::pin, time::Duration};

use futures::future::{Either, select};
use tracing::{debug, warn};
use truapi::{CallContext, latest::GenericError};

use super::SsoRequestService;
use crate::{
	host_internal::sso_messages::RotateSessionError,
	host_logic::{
		pending_session_rotation::{PendingSessionRotation, StoredPendingSessionRotation},
		session::{
			SessionInfo, SsoSessionInfo, decode_persisted_session, encode_persisted_session,
		},
		session_times::{SessionTimes, StoredSessionTimes},
	},
	platform::CoreStorageKey,
	runtime::{
		allowances::session_storage_id,
		sso_remote::{SsoRemoteResponseError, SsoSessionKey, sso_message_id},
	},
	time::{RealTimer, Timer},
	unix_time::current_unix_secs,
};

/// How long a new change waits before reading back whose change the slot
/// holds, so instances that started one together go on with the same one.
#[cfg(not(test))]
const CLAIM_SETTLE: Duration = Duration::from_secs(1);
#[cfg(test)]
const CLAIM_SETTLE: Duration = Duration::from_millis(1);

/// How long one request for the change waits for its answer.
#[cfg(not(test))]
const ANSWER_WAIT: Duration = Duration::from_secs(60);
#[cfg(test)]
const ANSWER_WAIT: Duration = Duration::from_millis(500);

/// How often a selected session's keys are checked between restores, so a
/// long-running instance changes them too.
#[cfg(not(test))]
pub const ROTATION_CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);
#[cfg(test)]
pub const ROTATION_CHECK_INTERVAL: Duration = Duration::from_millis(200);

/// How one request for a key change ended.
enum RotationAnswer {
	/// The signing host serves the new keys.
	Served,
	/// Another change of the session was served first.
	Replaced,
	/// The signing host keeps the session on its keys.
	Refused(String),
	/// This instance signed out while it waited.
	SignedOut,
	/// No answer arrived.
	Unanswered(String),
}

/// Which keys the stored session runs on, seen from one change of it.
enum StoredSessionKeys {
	/// The keys the change replaces.
	Replaced,
	/// The keys the change moves to: another instance completed it.
	Rotated,
	/// Other keys, or no session at all.
	Other,
}

impl SsoRequestService {
	/// Change the selected session's keys in the background, when they are due.
	pub fn rotate_keys_when_due(&self) {
		let Some(service) = self.weak_self.upgrade() else {
			return;
		};
		self.spawner.spawn(async move { service.rotate_due_keys().await });
	}

	async fn rotate_due_keys(&self) {
		let Some(_rotating) = self.rotation.try_lock() else {
			return;
		};
		let Some(session) = self.session_state.current() else {
			return;
		};
		let Some(sso) = session.sso.as_ref() else {
			return;
		};
		let refused = *self.refused_rotation.lock().expect("refused rotation mutex poisoned") ==
			Some(sso.ss_public_key);
		if refused {
			return;
		}
		let due = self
			.current_times(sso)
			.await
			.is_some_and(|times| times.is_rotation_due(current_unix_secs()));
		if !due {
			return;
		}
		let Some(pending) = self.claim_rotation(sso).await else {
			return;
		};
		self.drive_rotation(&session, &pending).await;
	}

	/// The change of `sso` the slot holds, starting one when it holds none.
	///
	/// Instances share the slot without a lock, so after writing a new change
	/// this waits and goes on with whichever change the slot then holds.
	async fn claim_rotation(&self, sso: &SsoSessionInfo) -> Option<PendingSessionRotation> {
		match self.stored_rotation(sso).await? {
			StoredPendingSessionRotation::Pending(pending) => return Some(*pending),
			StoredPendingSessionRotation::Unreadable => return None,
			StoredPendingSessionRotation::Replaceable => {},
		}
		let started = PendingSessionRotation::start(sso, sso_message_id())
			.inspect_err(|reason| warn!(%reason, "session key change could not start"))
			.ok()?;
		self.platform
			.write_core_storage(CoreStorageKey::PendingSessionRotation, started.to_stored())
			.await
			.inspect_err(|error| warn!(reason = %error.reason, "session key change write failed"))
			.ok()?;
		RealTimer.sleep(CLAIM_SETTLE).await;
		match self.stored_rotation(sso).await? {
			StoredPendingSessionRotation::Pending(pending) => Some(*pending),
			StoredPendingSessionRotation::Replaceable |
			StoredPendingSessionRotation::Unreadable => None,
		}
	}

	/// Ask for `pending` once and act on the answer, together with any
	/// disconnect of `session`'s keys the signing host announced meanwhile.
	///
	/// A signing host that served the change disconnects the keys it
	/// replaced, so a disconnect ends the session only when the change was not
	/// served.
	async fn drive_rotation(&self, session: &SessionInfo, pending: &PendingSessionRotation) {
		let Some(sso) = session.sso.as_ref() else {
			return;
		};
		let answer = self.ask_for_rotation(session, pending).await;
		let disconnected = self.take_disconnect(SsoSessionKey::from_session(sso));
		match (answer, disconnected) {
			(RotationAnswer::Served, _) => self.commit_rotation(session, pending).await,
			(RotationAnswer::SignedOut, _) => self.clear_pending_rotation(pending).await,
			(RotationAnswer::Unanswered(reason), false) => {
				warn!(%reason, "session key change unanswered; sending it again later");
			},
			(RotationAnswer::Refused(reason), false) => {
				debug!(%reason, "signing host keeps the session keys until the next restore");
				*self.refused_rotation.lock().expect("refused rotation mutex poisoned") =
					Some(sso.ss_public_key);
				self.clear_pending_rotation(pending).await;
			},
			(RotationAnswer::Replaced, _) |
			(RotationAnswer::Refused(_) | RotationAnswer::Unanswered(_), true) => {
				warn!("signing host stopped serving keys whose change it did not serve");
				self.clear_pending_rotation(pending).await;
				self.end_replaced_session(sso).await;
			},
		}
	}

	/// Send `pending`'s request on `session` and wait a bounded time for its answer.
	async fn ask_for_rotation(
		&self,
		session: &SessionInfo,
		pending: &PendingSessionRotation,
	) -> RotationAnswer {
		let cx = CallContext::default();
		let request = self.send_rotation_request(
			&cx,
			session,
			pending.message_id().to_string(),
			pending.request(),
		);
		let answer = match select(pin!(request), RealTimer.sleep(ANSWER_WAIT)).await {
			Either::Left((answer, _)) => answer,
			Either::Right(_) => return RotationAnswer::Unanswered("no answer in time".to_string()),
		};
		match answer {
			Ok(Ok(())) => RotationAnswer::Served,
			Ok(Err(RotateSessionError::Replaced)) => RotationAnswer::Replaced,
			Ok(Err(refusal)) => RotationAnswer::Refused(refusal.to_string()),
			Err(SsoRemoteResponseError::NotUnderstood) => {
				RotationAnswer::Refused(SsoRemoteResponseError::NotUnderstood.to_string())
			},
			Err(SsoRemoteResponseError::LocalDisconnected) => RotationAnswer::SignedOut,
			Err(failure) => RotationAnswer::Unanswered(failure.to_string()),
		}
	}

	/// Store `session` on the keys `pending` changed it to, unless the store
	/// no longer holds `session`.
	///
	/// Runs under the session store activation, so this instance restores
	/// nothing in between. Another instance restoring between the writes can
	/// still record new times for the new keys, which restarts their lifetime
	/// rather than ending a live session.
	async fn commit_rotation(&self, session: &SessionInfo, pending: &PendingSessionRotation) {
		let Some(sso) = session.sso.as_ref() else {
			return;
		};
		let rotated_sso = match pending.rotated(sso) {
			Ok(rotated_sso) => rotated_sso,
			Err(reason) => {
				warn!(%reason, "rotated session could not be derived");
				return;
			},
		};
		let activation = self.session_store_activation.lock().await;
		match self.stored_session_keys(sso, &rotated_sso).await {
			StoredSessionKeys::Replaced => {},
			StoredSessionKeys::Rotated => {
				self.clear_pending_rotation(pending).await;
				drop(activation);
				self.notify_session_store_changed();
				return;
			},
			StoredSessionKeys::Other => {
				self.clear_pending_rotation(pending).await;
				return;
			},
		}
		let times = self.current_times(sso).await;
		let rotated_times =
			times.as_ref().map(|times| times.rotated(&rotated_sso, current_unix_secs()));
		let mut rotated = session.clone();
		rotated.sso = Some(rotated_sso.clone());
		if !self.write_rotated_session(sso, &rotated, rotated_times.as_ref()).await {
			self.roll_back_rotation(sso, &rotated_sso, times.as_ref()).await;
			return;
		}
		self.clear_pending_rotation(pending).await;
		drop(activation);
		self.notify_session_store_changed();
	}

	/// Write the rotated session's grants, times and session, in the order
	/// another instance has to find them: the times for the new keys come
	/// before the session carries them, as [`SessionTimes`] requires. Without
	/// `times`, the old keys' times could not be read, so the slot is left as
	/// it is and the restore treats the new keys as it treats any session
	/// without times of its own. Stops before the session when the store no
	/// longer holds the old one.
	async fn write_rotated_session(
		&self,
		sso: &SsoSessionInfo,
		rotated: &SessionInfo,
		times: Option<&SessionTimes>,
	) -> bool {
		let Some(rotated_sso) = rotated.sso.as_ref() else {
			return false;
		};
		if !self.carry_allowance_keys(sso, rotated_sso).await {
			return false;
		}
		if let Some(times) = times &&
			let Err(error) = self
				.platform
				.write_core_storage(CoreStorageKey::SessionTimes, times.to_stored())
				.await
		{
			warn!(reason = %error.reason, "rotated session times write failed");
			return false;
		}
		if !matches!(self.stored_session_keys(sso, rotated_sso).await, StoredSessionKeys::Replaced)
		{
			return false;
		}
		self.platform
			.write_core_storage(CoreStorageKey::AuthSession, encode_persisted_session(rotated))
			.await
			.inspect_err(|error| warn!(reason = %error.reason, "rotated session write failed"))
			.is_ok()
	}

	/// Undo the writes of a commit that stopped short of the session: the
	/// copied grants go, and the times go back to the old keys while the store
	/// still holds them. A store a sign-out cleared stays clear.
	async fn roll_back_rotation(
		&self,
		sso: &SsoSessionInfo,
		rotated_sso: &SsoSessionInfo,
		times: Option<&SessionTimes>,
	) {
		let copied = CoreStorageKey::AllowanceKeys { session_id: session_storage_id(rotated_sso) };
		let grants = self.platform.clear_core_storage(copied).await;
		let times = match (self.stored_session_keys(sso, rotated_sso).await, times) {
			(StoredSessionKeys::Replaced, Some(times)) => {
				self.platform
					.write_core_storage(CoreStorageKey::SessionTimes, times.to_stored())
					.await
			},
			(StoredSessionKeys::Replaced, None) | (StoredSessionKeys::Rotated, _) => Ok(()),
			(StoredSessionKeys::Other, _) => self.clear_times_of(rotated_sso).await,
		};
		for error in [grants, times].into_iter().filter_map(Result::err) {
			warn!(reason = %error.reason, "session key change roll-back failed");
		}
	}

	/// Delete the times slot when it holds `sso`'s times.
	async fn clear_times_of(&self, sso: &SsoSessionInfo) -> Result<(), GenericError> {
		let stored = self.platform.read_core_storage(CoreStorageKey::SessionTimes).await?;
		match SessionTimes::from_stored(stored.as_deref(), sso) {
			StoredSessionTimes::Current(_) => {
				self.platform.clear_core_storage(CoreStorageKey::SessionTimes).await
			},
			StoredSessionTimes::Replaceable | StoredSessionTimes::Unreadable => Ok(()),
		}
	}

	/// Copy the allowance grants stored for `from` to `to`'s session id.
	async fn carry_allowance_keys(&self, from: &SsoSessionInfo, to: &SsoSessionInfo) -> bool {
		let stored = self
			.platform
			.read_core_storage(CoreStorageKey::AllowanceKeys {
				session_id: session_storage_id(from),
			})
			.await;
		let result = match stored {
			Ok(None) => return true,
			Ok(Some(keys)) => {
				self.platform
					.write_core_storage(
						CoreStorageKey::AllowanceKeys { session_id: session_storage_id(to) },
						keys,
					)
					.await
			},
			Err(error) => Err(error),
		};
		result
			.inspect_err(|error| {
				warn!(reason = %error.reason, "allowance grants could not follow the key change");
			})
			.is_ok()
	}

	/// `sso`'s own stored times; `None` when the slot holds other times or
	/// cannot be read.
	async fn current_times(&self, sso: &SsoSessionInfo) -> Option<SessionTimes> {
		let stored = self.platform.read_core_storage(CoreStorageKey::SessionTimes).await.ok()?;
		match SessionTimes::from_stored(stored.as_deref(), sso) {
			StoredSessionTimes::Current(times) => Some(times),
			StoredSessionTimes::Replaceable | StoredSessionTimes::Unreadable => None,
		}
	}

	/// Which keys the stored session runs on, compared with `sso`'s and the
	/// keys a change of it moves to.
	async fn stored_session_keys(
		&self,
		sso: &SsoSessionInfo,
		rotated_sso: &SsoSessionInfo,
	) -> StoredSessionKeys {
		match self.stored_session_key().await {
			Some(key) if key == sso.ss_public_key => StoredSessionKeys::Replaced,
			Some(key) if key == rotated_sso.ss_public_key => StoredSessionKeys::Rotated,
			_ => StoredSessionKeys::Other,
		}
	}

	/// The statement key of the stored paired session, if one can be read.
	async fn stored_session_key(&self) -> Option<[u8; 32]> {
		let blob = self.platform.read_core_storage(CoreStorageKey::AuthSession).await.ok()??;
		decode_persisted_session(&blob).ok()?.sso.map(|sso| sso.ss_public_key)
	}

	/// What the pending rotation slot holds for `sso`; `None` when it cannot be read.
	async fn stored_rotation(&self, sso: &SsoSessionInfo) -> Option<StoredPendingSessionRotation> {
		let stored = self
			.platform
			.read_core_storage(CoreStorageKey::PendingSessionRotation)
			.await
			.inspect_err(|error| warn!(reason = %error.reason, "session key change read failed"))
			.ok()?;
		Some(PendingSessionRotation::from_stored(stored.as_deref(), sso))
	}

	/// Delete `pending` from its slot unless the slot now holds another change.
	async fn clear_pending_rotation(&self, pending: &PendingSessionRotation) {
		let stored = self.platform.read_core_storage(CoreStorageKey::PendingSessionRotation).await;
		let result = match stored {
			Ok(Some(stored)) if stored == pending.to_stored() => {
				self.platform.clear_core_storage(CoreStorageKey::PendingSessionRotation).await
			},
			Ok(_) => return,
			Err(error) => Err(error),
		};
		if let Err(error) = result {
			warn!(reason = %error.reason, "session key change delete failed");
		}
	}

	/// Whether a change of the selected session on `key` is stored, so its
	/// signing host disconnecting `key` may be that change being served.
	pub async fn change_pending_for(&self, key: SsoSessionKey) -> bool {
		let Some(session) = self.session_state.current() else {
			return false;
		};
		let Some(sso) = session.sso.as_ref().filter(|sso| SsoSessionKey::from_session(sso) == key)
		else {
			return false;
		};
		matches!(self.stored_rotation(sso).await, Some(StoredPendingSessionRotation::Pending(_)))
	}

	/// Remember that the signing host disconnected `key`, and settle it once
	/// no change is being asked for: the pending change is asked for again,
	/// and without one the session ends.
	pub fn follow_disconnect_during_rotation(&self, key: SsoSessionKey) {
		*self
			.disconnected_during_rotation
			.lock()
			.expect("rotation disconnect mutex poisoned") = Some(key);
		let Some(service) = self.weak_self.upgrade() else {
			return;
		};
		self.spawner.spawn(async move {
			let _rotating = service.rotation.lock().await;
			if !service.disconnect_unsettled(key) {
				return;
			}
			let selected = service.session_state.current().and_then(|session| {
				let sso = session.sso.clone()?;
				(SsoSessionKey::from_session(&sso) == key).then_some((session, sso))
			});
			let Some((session, sso)) = selected else {
				service.take_disconnect(key);
				return;
			};
			match service.stored_rotation(&sso).await {
				Some(StoredPendingSessionRotation::Pending(pending)) => {
					service.drive_rotation(&session, &pending).await;
				},
				Some(
					StoredPendingSessionRotation::Replaceable |
					StoredPendingSessionRotation::Unreadable,
				) |
				None => {
					service.take_disconnect(key);
					service.end_replaced_session(&sso).await;
				},
			}
		});
	}

	/// Whether a disconnect of `key` is remembered and not yet acted on.
	fn disconnect_unsettled(&self, key: SsoSessionKey) -> bool {
		*self
			.disconnected_during_rotation
			.lock()
			.expect("rotation disconnect mutex poisoned") ==
			Some(key)
	}

	/// Whether the signing host disconnected `key` while a change was asked
	/// for, forgetting it.
	fn take_disconnect(&self, key: SsoSessionKey) -> bool {
		let mut disconnected = self
			.disconnected_during_rotation
			.lock()
			.expect("rotation disconnect mutex poisoned");
		let taken = *disconnected == Some(key);
		if taken {
			*disconnected = None;
		}
		taken
	}

	/// End the session on `sso`'s keys after its signing host stopped serving
	/// them, unless the store holds the session on other keys, which is
	/// restored instead.
	async fn end_replaced_session(&self, sso: &SsoSessionInfo) {
		if self.stored_session_key().await.is_some_and(|key| key != sso.ss_public_key) {
			self.notify_session_store_changed();
		} else {
			self.end_disconnected_session(SsoSessionKey::from_session(sso)).await;
		}
	}

	/// End the selected session on `key` after its signing host disconnected
	/// it, as [`Self::end_replaced_session`] does.
	pub async fn end_session_unless_stored_elsewhere(&self, key: SsoSessionKey) {
		let selected = self.session_state.current().and_then(|session| session.sso);
		match selected.filter(|sso| SsoSessionKey::from_session(sso) == key) {
			Some(sso) => self.end_replaced_session(&sso).await,
			None => self.end_disconnected_session(key).await,
		}
	}
}

#[cfg(test)]
mod tests;
