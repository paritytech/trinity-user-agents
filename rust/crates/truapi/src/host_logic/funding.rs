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

use crate::platform::{CoreStorage, CoreStorageKey, ProductId};

/// How long a session may stay open before it expires.
const SESSION_WINDOW_MS: u64 = 24 * 60 * 60 * 1_000;
/// How many ended sessions the host has not recorded yet the core keeps in
/// full, newest first, so a host that never records them cannot grow the
/// sessions blob without bound. Older ones are compacted into the session
/// pack.
const UNACKNOWLEDGED_LIMIT: usize = 200;
/// How many ended sessions the host has recorded the core keeps in full per
/// owner, newest first. Older ones are compacted into the session pack.
pub(crate) const RECORDED_PER_OWNER: usize = 64;
/// Format of the stored sessions blob, which leads it as a `u16`. The stored
/// enums pin their SCALE indexes, so variants added later decode alongside
/// the ones a blob was written with. A blob that leads with another format is
/// set aside rather than read.
const SESSIONS_FORMAT: u16 = 1;
/// Format of the stored session pack, which leads it as a `u16`, read like
/// the sessions blob.
const PACK_FORMAT: u16 = 1;
/// How many unreadable blobs [`CoreStorageKey::FundingSessionsUnreadable`]
/// keeps, oldest dropped first, so repeated bad writes cannot grow it without
/// bound. The core never reads them back: they are kept for diagnosis only.
const UNREADABLE_LIMIT: usize = 8;

/// An ended session the host recorded, compacted once newer recorded sessions
/// of its owner pushed it out of the full record. It keeps what history needs,
/// so its owner can still read how it ended and the host can still list it;
/// the pack is never trimmed.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingSessionSummary {
	/// The session's identifier.
	pub intent: String,
	/// Product that opened the session, `None` when the host did.
	pub owner_product_id: Option<ProductId>,
	/// Which way value crossed the boundary.
	pub direction: FundingDirection,
	/// Amount sought, in the user's payment balance units.
	pub amount: Option<u128>,
	/// How the session ended, with what moved and when.
	pub stage: FundingStage,
	/// When the session was opened, in Unix milliseconds.
	pub opened_at_ms: u64,
}

impl From<FundingSession> for FundingSessionSummary {
	fn from(session: FundingSession) -> Self {
		Self {
			intent: session.intent,
			owner_product_id: session.owner_product_id,
			direction: session.direction,
			amount: session.amount,
			stage: session.stage,
			opened_at_ms: session.opened_at_ms,
		}
	}
}

impl FundingSession {
	/// The session as history keeps it, leaving the session as it is.
	pub fn summary(&self) -> FundingSessionSummary {
		FundingSessionSummary {
			intent: self.intent.clone(),
			owner_product_id: self.owner_product_id.clone(),
			direction: self.direction,
			amount: self.amount,
			stage: self.stage.clone(),
			opened_at_ms: self.opened_at_ms,
		}
	}
}

impl FundingSessionSummary {
	/// The terminal item a subscriber receives.
	pub fn wire_item(&self) -> HostFundingStatusSubscribeItem {
		self.stage.wire_item(None)
	}

	/// When the session ended.
	pub fn settled_at_ms(&self) -> Option<u64> {
		self.stage.settled_at_ms()
	}
}

/// What the core knows about one session, independent of any host surface.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingSession {
	/// Identifier handed back to the caller and used to re-attach.
	pub intent: String,
	/// Product that opened the session and the only one that may watch it.
	/// `None` when the host opened it, as the Balance card does.
	pub owner_product_id: Option<ProductId>,
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

impl FundingStage {
	/// When the session ended, if it has.
	fn settled_at_ms(&self) -> Option<u64> {
		match self {
			Self::Open => None,
			Self::Failed { settled_at_ms, .. } |
			Self::Delivered { settled_at_ms, .. } |
			Self::Released { settled_at_ms, .. } => Some(*settled_at_ms),
		}
	}

	/// The wire item subscribers receive for this stage; an open stage
	/// reports `expires_at`.
	fn wire_item(&self, expires_at: Option<u64>) -> HostFundingStatusSubscribeItem {
		match self {
			Self::Open => HostFundingStatusSubscribeItem::InProgress { expires_at },
			Self::Failed { reason, .. } => {
				HostFundingStatusSubscribeItem::Failed { reason: reason.clone() }
			},
			Self::Delivered { credited, .. } => {
				HostFundingStatusSubscribeItem::Delivered { credited: *credited }
			},
			Self::Released { debited, .. } => {
				HostFundingStatusSubscribeItem::Released { debited: *debited }
			},
		}
	}
}

impl FundingSession {
	/// Open a session that expires one window from `now_ms`.
	pub fn new(
		intent: String,
		owner_product_id: Option<ProductId>,
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
		self.stage.settled_at_ms()
	}

	/// Project the current stage onto the wire item subscribers receive.
	pub fn wire_item(&self) -> HostFundingStatusSubscribeItem {
		self.stage.wire_item(Some(self.deadline_ms))
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
	/// The stored funding records are in a format newer than this app reads,
	/// as after a downgrade. They are left as they are, and the core writes
	/// nothing over them, so funding stays unavailable until the app is
	/// updated.
	#[display("funding records were stored by a newer app (format {format})")]
	NewerFormat {
		/// The stored format.
		format: u16,
	},
}

impl From<FundingSessionError> for GenericError {
	fn from(error: FundingSessionError) -> Self {
		Self { reason: error.to_string() }
	}
}

/// Split `sessions` into those kept in full and the ended ones to compact.
/// Kept in full: every open one, the ended ones the host has not recorded yet,
/// newest first up to a fixed bound, and the newest ended ones it has recorded,
/// up to a fixed bound per owner. The ended ones past either bound go to the
/// pack, so no outcome is lost.
pub fn retained(
	sessions: impl IntoIterator<Item = FundingSession>,
) -> (Vec<FundingSession>, Vec<FundingSessionSummary>) {
	let (mut open, ended): (Vec<_>, Vec<_>) =
		sessions.into_iter().partition(|session| !session.is_terminal());
	let (mut unrecorded, mut recorded): (Vec<_>, Vec<_>) =
		ended.into_iter().partition(|session| !session.acknowledged);
	unrecorded.sort_by_key(|session| core::cmp::Reverse(session.settled_at_ms()));
	let unrecorded_overflow = unrecorded.split_off(UNACKNOWLEDGED_LIMIT.min(unrecorded.len()));
	// Grouped by owner, newest first within each, so each owner's newest run is
	// what survives the per-owner bound.
	recorded.sort_by(|a, b| {
		a.owner_product_id
			.cmp(&b.owner_product_id)
			.then_with(|| b.settled_at_ms().cmp(&a.settled_at_ms()))
	});
	open.append(&mut unrecorded);
	let mut owner: Option<Option<ProductId>> = None;
	let mut owner_run = 0;
	let (newest, older): (Vec<_>, Vec<_>) = recorded.into_iter().partition(|session| {
		if owner.as_ref() == Some(&session.owner_product_id) {
			owner_run += 1;
		} else {
			owner = Some(session.owner_product_id.clone());
			owner_run = 1;
		}
		owner_run <= RECORDED_PER_OWNER
	});
	open.extend(newest);
	let compacted = older.into_iter().chain(unrecorded_overflow);
	(open, compacted.map(FundingSessionSummary::from).collect())
}

/// Read every persisted session.
///
/// A blob that does not decode is set aside under
/// [`CoreStorageKey::FundingSessionsUnreadable`] and reads as no sessions; one
/// in a newer format fails with [`FundingSessionError::NewerFormat`] and stays
/// where it is.
pub async fn load_sessions(
	storage: &(impl CoreStorage + ?Sized),
) -> Result<Vec<FundingSession>, FundingSessionError> {
	load_versioned(storage, CoreStorageKey::FundingSessions, SESSIONS_FORMAT).await
}

/// Read the session pack, in no particular order.
///
/// A blob that does not decode is set aside under
/// [`CoreStorageKey::FundingSessionsUnreadable`] and reads as an empty pack;
/// one in a newer format fails with [`FundingSessionError::NewerFormat`] and
/// stays where it is.
pub async fn load_pack(
	storage: &(impl CoreStorage + ?Sized),
) -> Result<Vec<FundingSessionSummary>, FundingSessionError> {
	load_versioned(storage, CoreStorageKey::FundingSessionPack, PACK_FORMAT).await
}

/// Read the list stored under `key`, which leads with `format`.
///
/// A blob that does not decode is appended to the list kept under
/// [`CoreStorageKey::FundingSessionsUnreadable`] and reads as an empty list,
/// so one bad write cannot disable funding for good, the next store cannot
/// overwrite records it never read, and a later bad blob does not replace an
/// earlier one. If it cannot be set aside, loading fails and the blob stays
/// where it is. A blob in a newer format is not this core's to move: loading
/// fails with [`FundingSessionError::NewerFormat`] and leaves it, so the
/// registry, which writes only after a load, never writes over it.
async fn load_versioned<T: Decode>(
	storage: &(impl CoreStorage + ?Sized),
	key: CoreStorageKey,
	format: u16,
) -> Result<Vec<T>, FundingSessionError> {
	let storage_error = |err: GenericError| FundingSessionError::Storage { reason: err.reason };
	let Some(blob) = storage.read_core_storage(key.clone()).await.map_err(storage_error)? else {
		return Ok(Vec::new());
	};
	match decode_versioned(&blob, format) {
		Decoded::Records(records) => return Ok(records),
		Decoded::Newer(stored) => return Err(FundingSessionError::NewerFormat { format: stored }),
		Decoded::Unreadable => {},
	}
	error!(?key, bytes = blob.len(), "funding records do not decode; keeping them aside");
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

/// What a stored list turned out to be.
enum Decoded<T> {
	/// A list in the format this core writes.
	Records(Vec<T>),
	/// A list in a newer format, left for the app that wrote it.
	Newer(u16),
	/// Anything else: an older format or a corrupt blob.
	Unreadable,
}

/// Decode a stored list that leads with `format`.
fn decode_versioned<T: Decode>(blob: &[u8], format: u16) -> Decoded<T> {
	let input = &mut &blob[..];
	let Ok(stored) = u16::decode(input) else {
		return Decoded::Unreadable;
	};
	if stored > format {
		return Decoded::Newer(stored);
	}
	match Vec::<T>::decode(input) {
		Ok(records) if stored == format && input.is_empty() => Decoded::Records(records),
		_ => Decoded::Unreadable,
	}
}

/// Replace the persisted set with `sessions`, clearing the slot when empty.
pub async fn store_sessions(
	storage: &(impl CoreStorage + ?Sized),
	sessions: &[&FundingSession],
) -> Result<(), FundingSessionError> {
	store_versioned(storage, CoreStorageKey::FundingSessions, SESSIONS_FORMAT, sessions).await
}

/// Encode `pack` as [`store_pack`] writes it.
pub fn pack_blob<'a>(pack: impl IntoIterator<Item = &'a FundingSessionSummary>) -> Vec<u8> {
	(PACK_FORMAT, pack.into_iter().collect::<Vec<_>>()).encode()
}

/// Replace the persisted pack with `blob`, built by [`pack_blob`].
pub async fn store_pack(
	storage: &(impl CoreStorage + ?Sized),
	blob: Vec<u8>,
) -> Result<(), FundingSessionError> {
	storage
		.write_core_storage(CoreStorageKey::FundingSessionPack, blob)
		.await
		.map_err(|err| FundingSessionError::Storage { reason: err.reason })
}

/// Write `records` under `key`, led by `format`, clearing the slot when empty.
async fn store_versioned<T: Encode>(
	storage: &(impl CoreStorage + ?Sized),
	key: CoreStorageKey,
	format: u16,
	records: &[&T],
) -> Result<(), FundingSessionError> {
	let written = if records.is_empty() {
		storage.clear_core_storage(key).await
	} else {
		storage.write_core_storage(key, (format, records).encode()).await
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
			Some(ProductId::for_test("wallet.dot")),
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

	// Each owner keeps its newest recorded sessions in full, so no owner can
	// crowd out another, and its older ones are compacted rather than dropped.
	#[test]
	fn recorded_sessions_keep_the_newest_per_owner_and_compact_the_rest() {
		let recorded = |intent: String, owner: &str, settled_at_ms: u64| FundingSession {
			acknowledged: true,
			owner_product_id: Some(ProductId::for_test(owner)),
			..expired(&intent, settled_at_ms)
		};
		let mut sessions: Vec<FundingSession> = (0..RECORDED_PER_OWNER as u64 + 2)
			.map(|n| recorded(format!("a_{n}"), "a.dot", NOW + n))
			.collect();
		sessions.push(recorded("b_0".to_string(), "b.dot", NOW));
		sessions.push(session(FundingDirection::In));

		let (kept, compacted) = retained(sessions);
		let count = |owner: &str| {
			kept.iter()
				.filter(|session| session.owner_product_id.as_ref().is_some_and(|id| *id == owner))
				.count()
		};

		assert_eq!(
			(
				count("a.dot"),
				count("b.dot"),
				kept.iter().any(|session| session.intent == "a_0" || session.intent == "a_1"),
				kept.iter().any(|session| !session.is_terminal()),
				compacted,
			),
			(
				RECORDED_PER_OWNER,
				1,
				false,
				true,
				vec![
					FundingSessionSummary::from(recorded("a_1".to_string(), "a.dot", NOW + 1)),
					FundingSessionSummary::from(recorded("a_0".to_string(), "a.dot", NOW)),
				]
			)
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
			HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Expired }
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
	// ones kept only so their owner can read how they ended. An unrecorded
	// session past the bound is compacted, not lost.
	#[test]
	fn the_core_keeps_open_then_unrecorded_then_recorded_sessions() {
		let ended = |index: usize, acknowledged| FundingSession {
			acknowledged,
			..expired(&format!("fs_s{index}"), NOW + index as u64)
		};
		let open = session(FundingDirection::Out);
		let recorded = (0..3).map(|index| ended(index, true));
		let unrecorded = (100..101 + UNACKNOWLEDGED_LIMIT).map(|index| ended(index, false));

		let (kept, compacted) = retained(recorded.chain(unrecorded).chain([open]));
		let kept: Vec<String> = kept.into_iter().map(|session| session.intent).collect();

		let newest_unrecorded =
			(101..101 + UNACKNOWLEDGED_LIMIT).rev().map(|index| format!("fs_s{index}"));
		let recorded_newest_first = (0..3).rev().map(|index| format!("fs_s{index}"));
		assert_eq!(
			(kept, compacted),
			(
				std::iter::once("fs_1".to_string())
					.chain(newest_unrecorded)
					.chain(recorded_newest_first)
					.collect::<Vec<_>>(),
				vec![FundingSessionSummary::from(ended(100, false))],
			)
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
				HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Cancelled },
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
		let corrupt = |tail: u8| {
			let mut blob = SESSIONS_FORMAT.encode();
			blob.extend([0xff, tail]);
			blob
		};
		let first = corrupt(1);
		let second = corrupt(2);
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

	// An app downgraded below the one that stored its sessions cannot read
	// them, and must not move or overwrite them: the newer app reads them
	// again once it is back.
	#[test]
	fn a_blob_of_a_newer_format_is_left_where_it_is() {
		let storage = stub_platform();
		let blob = (SESSIONS_FORMAT + 1, vec![session(FundingDirection::In)]).encode();
		block_on(storage.write_core_storage(CoreStorageKey::FundingSessions, blob.clone()))
			.expect("written");

		assert_eq!(
			(
				block_on(load_sessions(storage.as_ref())),
				block_on(storage.read_core_storage(CoreStorageKey::FundingSessions)),
				block_on(storage.read_core_storage(CoreStorageKey::FundingSessionsUnreadable)),
			),
			(
				Err(FundingSessionError::NewerFormat { format: SESSIONS_FORMAT + 1 }),
				Ok(Some(blob)),
				Ok(None),
			)
		);
	}

	// A blob of an older format is not this core's either, but it is not
	// coming back, so it is set aside like a corrupt one.
	#[test]
	fn a_blob_of_an_older_format_is_kept_aside() {
		let storage = stub_platform();
		let blob = (0u16, vec![session(FundingDirection::In)]).encode();
		block_on(storage.write_core_storage(CoreStorageKey::FundingSessions, blob.clone()))
			.expect("written");

		assert_eq!(block_on(load_sessions(storage.as_ref())), Ok(Vec::new()));
		assert_eq!(
			block_on(storage.read_core_storage(CoreStorageKey::FundingSessionsUnreadable)),
			Ok(Some(vec![blob].encode()))
		);
	}

	// A compacted session answers with the same terminal item it had in full.
	#[test]
	fn a_summary_reads_as_the_session_ended() {
		let delivered = FundingSession {
			stage: FundingStage::Delivered { credited: 90, settled_at_ms: NOW + 1 },
			..session(FundingDirection::In)
		};
		let summary = FundingSessionSummary::from(delivered.clone());

		assert_eq!(
			(summary.wire_item(), summary.settled_at_ms()),
			(delivered.wire_item(), Some(NOW + 1))
		);
	}

	#[test]
	fn the_pack_round_trips_through_core_storage() {
		let storage = stub_platform();
		let pack = vec![FundingSessionSummary::from(expired("fs_2", NOW))];

		block_on(store_pack(storage.as_ref(), pack_blob(&pack))).expect("stored");

		assert_eq!(block_on(load_pack(storage.as_ref())).expect("loaded"), pack);
	}

	#[test]
	fn an_undecodable_pack_is_kept_aside_and_reads_as_empty() {
		let storage = stub_platform();
		let mut blob = PACK_FORMAT.encode();
		blob.extend([0xff, 0x01]);
		block_on(storage.write_core_storage(CoreStorageKey::FundingSessionPack, blob.clone()))
			.expect("written");

		assert_eq!(block_on(load_pack(storage.as_ref())), Ok(Vec::new()));
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
