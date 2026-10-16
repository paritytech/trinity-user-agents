//! Core-owned funding sessions and their status machine.
//!
//! A session is one funding intent in flight. The core owns it, not the host
//! overlay that opened it, so dismissing the overlay after starting is not
//! cancelling: the session keeps running, persists across app death through
//! [`CoreStorageKey::FundingSessions`], and is re-attached by
//! `Funding::status_subscribe`. A session still open 24 hours after it was
//! opened expires on the core's own clock, whether or not anyone watches it.

use parity_scale_codec::{Decode, Encode};
use tracing::error;
use truapi::latest::{
	FundingDirection, FundingFailure, GenericError, HostFundingStatusSubscribeItem,
};

use crate::platform::{CoreStorage, CoreStorageKey};

/// How long a session may stay open before it expires.
const SESSION_WINDOW_MS: u64 = 24 * 60 * 60 * 1_000;
/// How many ended sessions the host has not recorded yet the core keeps,
/// newest first, so a host that never records them cannot grow the store
/// without bound.
const UNACKNOWLEDGED_LIMIT: usize = 200;
/// How many ended sessions the host has recorded the core keeps per owner,
/// newest first, so a product that watches one again after the host recorded
/// it still learns how it ended.
const RECORDED_PER_OWNER: usize = 64;
/// Format of the stored sessions blob, which leads it as a `u16`. The stored
/// enums pin their SCALE indexes, so variants added later decode alongside
/// the ones a blob was written with. A blob that leads with another format is
/// set aside rather than read.
const SESSIONS_FORMAT: u16 = 1;
/// How many unreadable blobs [`CoreStorageKey::FundingSessionsUnreadable`]
/// keeps, oldest dropped first, so repeated bad writes cannot grow it without
/// bound. The core never reads them back: they are kept for diagnosis only.
const UNREADABLE_LIMIT: usize = 8;

/// What the core knows about one session, independent of any host surface.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingSession {
	/// Identifier handed back to the caller and used to re-attach.
	pub intent: String,
	/// Product that opened the session and the only one that may watch it.
	/// `None` when the host opened it, as the Balance card does.
	pub owner_product_id: Option<String>,
	/// Which way value crosses the boundary.
	pub direction: FundingDirection,
	/// Amount sought, in the user's payment balance units. `None` until the
	/// user chooses one.
	pub amount: Option<u128>,
	/// Current stage.
	pub stage: FundingStage,
	/// When the session was opened, in Unix milliseconds.
	pub opened_at_ms: u64,
	/// When the session expires if still open, in Unix milliseconds.
	pub deadline_ms: u64,
	/// Whether the host has recorded the session's outcome in its own
	/// history. An ended session is handed to the host until it has.
	pub acknowledged: bool,
}

/// Stage of a session, as the core persists it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingStage {
	/// In flight.
	#[codec(index = 0)]
	Open,
	/// Ended without success.
	#[codec(index = 1)]
	Failed {
		/// Why it ended.
		reason: FundingFailure,
		/// When it ended, in Unix milliseconds.
		settled_at_ms: u64,
	},
	/// Inbound success.
	#[codec(index = 2)]
	Delivered {
		/// Amount credited, which may differ from the amount requested.
		credited: u128,
		/// When it ended, in Unix milliseconds.
		settled_at_ms: u64,
	},
	/// Outbound success.
	#[codec(index = 3)]
	Released {
		/// Amount debited.
		debited: u128,
		/// When it ended, in Unix milliseconds.
		settled_at_ms: u64,
	},
}

impl FundingSession {
	/// Open a session that expires one window from `now_ms`.
	pub fn new(
		intent: String,
		owner_product_id: Option<String>,
		direction: FundingDirection,
		amount: Option<u128>,
		now_ms: u64,
	) -> Self {
		Self {
			intent,
			owner_product_id,
			direction,
			amount,
			stage: FundingStage::Open,
			opened_at_ms: now_ms,
			deadline_ms: now_ms.saturating_add(SESSION_WINDOW_MS),
			acknowledged: false,
		}
	}

	/// Whether the host still has to hear of the session when funding
	/// resumes: it is in flight, or it ended and the host has not recorded
	/// the outcome.
	pub fn needs_handoff(&self) -> bool {
		!self.is_terminal() || !self.acknowledged
	}

	/// End an open session as cancelled. Returns whether it changed.
	pub fn cancel(&mut self, now_ms: u64) -> bool {
		self.fail(FundingFailure::Cancelled, now_ms)
	}

	/// Whether the session has ended.
	pub fn is_terminal(&self) -> bool {
		self.settled_at_ms().is_some()
	}

	/// When the session ended, if it has.
	pub fn settled_at_ms(&self) -> Option<u64> {
		match self.stage {
			FundingStage::Open => None,
			FundingStage::Failed { settled_at_ms, .. } |
			FundingStage::Delivered { settled_at_ms, .. } |
			FundingStage::Released { settled_at_ms, .. } => Some(settled_at_ms),
		}
	}

	/// Project the current stage onto the wire item subscribers receive.
	pub fn wire_item(&self) -> HostFundingStatusSubscribeItem {
		match (&self.stage, self.direction) {
			(FundingStage::Open, _) => {
				HostFundingStatusSubscribeItem::InProgress { expires_at: Some(self.deadline_ms) }
			},
			(FundingStage::Failed { reason, .. }, _) => {
				HostFundingStatusSubscribeItem::Failed { reason: reason.clone(), moved: 0 }
			},
			(FundingStage::Delivered { credited, .. }, _) => {
				HostFundingStatusSubscribeItem::Delivered { credited: *credited }
			},
			(FundingStage::Released { debited, .. }, _) => {
				HostFundingStatusSubscribeItem::Released { debited: *debited }
			},
		}
	}

	/// End the session with `reason`, unless it already ended. Returns whether
	/// it changed.
	pub fn fail(&mut self, reason: FundingFailure, now_ms: u64) -> bool {
		if self.is_terminal() {
			return false;
		}
		self.stage = FundingStage::Failed { reason, settled_at_ms: now_ms };
		true
	}

	/// Expire the session if it is still open at its deadline. Returns whether
	/// it expired.
	pub fn expire_if_due(&mut self, now_ms: u64) -> bool {
		now_ms >= self.deadline_ms && self.fail(FundingFailure::Expired, now_ms)
	}
}

/// Why a session operation failed.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum FundingSessionError {
	/// The host's storage callback failed.
	#[display("funding session storage failed: {reason}")]
	Storage {
		/// Failure detail.
		reason: String,
	},
}

impl From<FundingSessionError> for GenericError {
	fn from(error: FundingSessionError) -> Self {
		Self { reason: error.to_string() }
	}
}

/// The sessions worth keeping: every open one, the ended ones the host has
/// not recorded yet, newest first up to a fixed bound, and the newest ended
/// ones it has recorded, up to a fixed bound per owner. The host owns the
/// history; a recorded session is kept only so its owner can still read how it
/// ended.
pub fn retained(sessions: impl IntoIterator<Item = FundingSession>) -> Vec<FundingSession> {
	let (mut open, ended): (Vec<_>, Vec<_>) =
		sessions.into_iter().partition(|session| !session.is_terminal());
	let (mut unrecorded, mut recorded): (Vec<_>, Vec<_>) =
		ended.into_iter().partition(|session| !session.acknowledged);
	unrecorded.sort_by_key(|session| core::cmp::Reverse(session.settled_at_ms()));
	unrecorded.truncate(UNACKNOWLEDGED_LIMIT);
	// Grouped by owner, newest first within each, so each owner's newest run is
	// what survives the per-owner bound.
	recorded.sort_by(|a, b| {
		a.owner_product_id
			.cmp(&b.owner_product_id)
			.then_with(|| b.settled_at_ms().cmp(&a.settled_at_ms()))
	});
	open.append(&mut unrecorded);
	let mut owner_run = 0;
	for session in recorded {
		let same_owner = open
			.last()
			.is_some_and(|kept| kept.owner_product_id == session.owner_product_id);
		owner_run = if same_owner { owner_run + 1 } else { 1 };
		if owner_run <= RECORDED_PER_OWNER {
			open.push(session);
		}
	}
	open
}

/// Read every persisted session.
///
/// A blob that does not decode is appended to the list kept under
/// [`CoreStorageKey::FundingSessionsUnreadable`] and reads as no sessions, so
/// one bad write cannot disable funding for good, the next store cannot
/// overwrite sessions it never read, and a later bad blob does not replace an
/// earlier one. If it cannot be set aside, loading fails and the blob stays
/// where it is.
pub async fn load_sessions(
	storage: &(impl CoreStorage + ?Sized),
) -> Result<Vec<FundingSession>, FundingSessionError> {
	let storage_error = |err: GenericError| FundingSessionError::Storage { reason: err.reason };
	let Some(blob) = storage
		.read_core_storage(CoreStorageKey::FundingSessions)
		.await
		.map_err(storage_error)?
	else {
		return Ok(Vec::new());
	};
	if let Some(sessions) = decode_sessions(&blob) {
		return Ok(sessions);
	}
	error!(bytes = blob.len(), "funding sessions do not decode; keeping them aside");
	let mut aside = match storage
		.read_core_storage(CoreStorageKey::FundingSessionsUnreadable)
		.await
		.map_err(storage_error)?
	{
		Some(kept) => Vec::<Vec<u8>>::decode(&mut &kept[..]).unwrap_or_else(|_| vec![kept]),
		None => Vec::new(),
	};
	aside.push(blob);
	if aside.len() > UNREADABLE_LIMIT {
		aside.drain(..aside.len() - UNREADABLE_LIMIT);
	}
	storage
		.write_core_storage(CoreStorageKey::FundingSessionsUnreadable, aside.encode())
		.await
		.map_err(storage_error)?;
	Ok(Vec::new())
}

/// Decode a stored sessions blob of any version this core reads, or `None`
/// when it is not one.
fn decode_sessions(blob: &[u8]) -> Option<Vec<FundingSession>> {
	let input = &mut &blob[..];
	let sessions = match u16::decode(input).ok()? {
		SESSIONS_FORMAT => Vec::<FundingSession>::decode(input).ok()?,
		_ => return None,
	};
	input.is_empty().then_some(sessions)
}

/// Replace the persisted set with `sessions`, clearing the slot when empty.
pub async fn store_sessions(
	storage: &(impl CoreStorage + ?Sized),
	sessions: &[&FundingSession],
) -> Result<(), FundingSessionError> {
	let written = if sessions.is_empty() {
		storage.clear_core_storage(CoreStorageKey::FundingSessions).await
	} else {
		storage
			.write_core_storage(
				CoreStorageKey::FundingSessions,
				(SESSIONS_FORMAT, sessions).encode(),
			)
			.await
	};
	written.map_err(|err| FundingSessionError::Storage { reason: err.reason })
}

#[cfg(test)]
mod tests {
	use super::*;

	use futures::executor::block_on;

	use crate::test_support::stub_platform;

	const NOW: u64 = 1_700_000_000_000;

	fn session(direction: FundingDirection) -> FundingSession {
		FundingSession::new(
			"fs_1".to_string(),
			Some("wallet.dot".to_string()),
			direction,
			Some(100),
			NOW,
		)
	}

	fn expired(intent: &str, settled_at_ms: u64) -> FundingSession {
		FundingSession {
			intent: intent.to_string(),
			stage: FundingStage::Failed { reason: FundingFailure::Expired, settled_at_ms },
			..session(FundingDirection::In)
		}
	}

	// Recorded sessions are kept only so their owner can read how they ended,
	// so each owner keeps its newest ones and no owner can crowd out another.
	#[test]
	fn recorded_sessions_keep_the_newest_per_owner() {
		let recorded = |intent: String, owner: &str, settled_at_ms: u64| FundingSession {
			acknowledged: true,
			owner_product_id: Some(owner.to_string()),
			..expired(&intent, settled_at_ms)
		};
		let mut sessions: Vec<FundingSession> = (0..RECORDED_PER_OWNER as u64 + 2)
			.map(|n| recorded(format!("a_{n}"), "a.dot", NOW + n))
			.collect();
		sessions.push(recorded("b_0".to_string(), "b.dot", NOW));
		sessions.push(session(FundingDirection::In));

		let kept = retained(sessions);
		let count = |owner: &str| {
			kept.iter()
				.filter(|session| session.owner_product_id.as_deref() == Some(owner))
				.count()
		};

		assert_eq!(
			(
				count("a.dot"),
				count("b.dot"),
				kept.iter().any(|session| session.intent == "a_0" || session.intent == "a_1"),
				kept.iter().any(|session| !session.is_terminal()),
			),
			(RECORDED_PER_OWNER, 1, false, true)
		);
	}

	// A product only waits for the outcome; the host draws the steps, so an
	// open session reads the same whichever way value moves.
	#[test]
	fn an_open_session_reads_in_progress_until_its_deadline() {
		let in_progress = HostFundingStatusSubscribeItem::InProgress {
			expires_at: Some(NOW + SESSION_WINDOW_MS),
		};
		assert_eq!(
			(session(FundingDirection::In).wire_item(), session(FundingDirection::Out).wire_item(),),
			(in_progress.clone(), in_progress)
		);
	}

	// Nothing else ends a session that hears nothing, so expiry is what
	// guarantees every subscriber eventually gets a terminal item.
	#[test]
	fn a_session_expires_at_its_deadline_and_not_before() {
		let mut session = session(FundingDirection::In);

		assert!(!session.expire_if_due(NOW + SESSION_WINDOW_MS - 1));
		assert!(session.expire_if_due(NOW + SESSION_WINDOW_MS));
		assert_eq!(
			session.wire_item(),
			HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Expired, moved: 0 }
		);
	}

	#[test]
	fn an_ended_session_keeps_its_first_outcome() {
		let mut session = expired("fs_1", NOW);

		assert!(!session.fail(FundingFailure::Cancelled, NOW + 1));
		assert_eq!(session, expired("fs_1", NOW));
	}

	// The host owns the history: open sessions come first, then the ones it
	// has not recorded, newest first and not without bound, then the recorded
	// ones kept only so their owner can read how they ended.
	#[test]
	fn the_core_keeps_open_then_unrecorded_then_recorded_sessions() {
		let ended = |index: usize, acknowledged| FundingSession {
			acknowledged,
			..expired(&format!("fs_s{index}"), NOW + index as u64)
		};
		let open = session(FundingDirection::Out);
		let recorded = (0..3).map(|index| ended(index, true));
		let unrecorded = (100..101 + UNACKNOWLEDGED_LIMIT).map(|index| ended(index, false));

		let kept: Vec<String> = retained(recorded.chain(unrecorded).chain([open]))
			.into_iter()
			.map(|session| session.intent)
			.collect();

		let newest_unrecorded =
			(101..101 + UNACKNOWLEDGED_LIMIT).rev().map(|index| format!("fs_s{index}"));
		let recorded_newest_first = (0..3).rev().map(|index| format!("fs_s{index}"));
		assert_eq!(
			kept,
			std::iter::once("fs_1".to_string())
				.chain(newest_unrecorded)
				.chain(recorded_newest_first)
				.collect::<Vec<_>>()
		);
	}

	// Cancel ends an open session once; an ended one keeps its outcome.
	#[test]
	fn a_session_is_cancelled_once() {
		let mut open = session(FundingDirection::In);
		let mut ended = expired("fs_2", NOW);

		assert_eq!(
			(open.cancel(NOW + 1), open.wire_item(), ended.cancel(NOW + 1)),
			(
				true,
				HostFundingStatusSubscribeItem::Failed {
					reason: FundingFailure::Cancelled,
					moved: 0,
				},
				false
			)
		);
	}

	#[test]
	fn sessions_round_trip_through_core_storage() {
		let storage = stub_platform();
		let sessions = vec![session(FundingDirection::In), expired("fs_2", NOW)];

		block_on(store_sessions(storage.as_ref(), &sessions.iter().collect::<Vec<_>>()))
			.expect("stored");

		assert_eq!(block_on(load_sessions(storage.as_ref())).expect("loaded"), sessions);
	}

	#[test]
	fn an_undecodable_blob_is_kept_aside_and_reads_as_no_sessions() {
		let storage = stub_platform();
		let mut blob = (SESSIONS_FORMAT, vec![session(FundingDirection::In)]).encode();
		blob.push(0xff);
		block_on(storage.write_core_storage(CoreStorageKey::FundingSessions, blob.clone()))
			.expect("written");

		assert_eq!(block_on(load_sessions(storage.as_ref())), Ok(Vec::new()));
		assert_eq!(
			block_on(storage.read_core_storage(CoreStorageKey::FundingSessionsUnreadable)),
			Ok(Some(vec![blob].encode()))
		);
	}

	#[test]
	fn a_second_unreadable_blob_is_kept_alongside_the_first() {
		let storage = stub_platform();
		let first = vec![0xff, 0x01];
		let second = vec![0xff, 0x02];
		for blob in [&first, &second] {
			block_on(storage.write_core_storage(CoreStorageKey::FundingSessions, blob.clone()))
				.expect("written");
			assert_eq!(block_on(load_sessions(storage.as_ref())), Ok(Vec::new()));
		}

		assert_eq!(
			block_on(storage.read_core_storage(CoreStorageKey::FundingSessionsUnreadable)),
			Ok(Some(vec![first, second].encode()))
		);
	}

	#[test]
	fn a_blob_of_an_unknown_version_is_kept_aside() {
		let storage = stub_platform();
		let blob = (SESSIONS_FORMAT + 1, vec![session(FundingDirection::In)]).encode();
		block_on(storage.write_core_storage(CoreStorageKey::FundingSessions, blob.clone()))
			.expect("written");

		assert_eq!(block_on(load_sessions(storage.as_ref())), Ok(Vec::new()));
		assert_eq!(
			block_on(storage.read_core_storage(CoreStorageKey::FundingSessionsUnreadable)),
			Ok(Some(vec![blob].encode()))
		);
	}

	#[test]
	fn storing_nothing_clears_the_slot() {
		let storage = stub_platform();
		block_on(store_sessions(storage.as_ref(), &[&session(FundingDirection::In)]))
			.expect("stored");

		block_on(store_sessions(storage.as_ref(), &[])).expect("cleared");

		assert_eq!(block_on(storage.read_core_storage(CoreStorageKey::FundingSessions)), Ok(None));
	}
}
