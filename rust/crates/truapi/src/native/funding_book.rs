//! What a native host reads its funding state from. The core keeps it as
//! funding changes and tells the host only what changed; the host reads the
//! rest back from the runtime.

use std::{
	collections::{BTreeSet, HashMap},
	path::{Path, PathBuf},
	sync::{Mutex, MutexGuard, PoisonError},
};

use truapi::latest::HostFundingStatusSubscribeItem;

use crate::{
	host_logic::{
		funding::{FundingProgress, FundingSession, FundingSessionSummary},
		funding_activity::{ActivityView, activity},
		funding_providers::FundingOptions,
	},
	platform::FundingQuoteList,
};

/// Lock `mutex`, recovering its value if a holder panicked: the book stays
/// valid between statements.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The file under the core database directory whose presence says a
/// provider still works on a funding session.
const RESUME_MARKER: &str = "funding-resume";

/// What changed in the funding state a host reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FundingChangeKind {
	/// A session's record, progress, or whether it is acknowledged; the
	/// activity with it.
	Session,
	/// What a session's overlay offers.
	Options,
	/// A session's quote list.
	Quotes,
	/// Whether a provider still works on any session.
	InProgress,
}

/// A session the host has heard of, with its progress now.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FundingBookEntry {
	/// The session.
	pub session: FundingSession,
	/// Its progress now.
	pub progress: FundingProgress,
}

/// The funding state the core keeps for a native host beside the sessions
/// themselves, which the registry holds.
#[derive(Default)]
pub struct FundingSessionBook {
	state: Mutex<BookState>,
}

#[derive(Default)]
struct BookState {
	/// What each open overlay offers.
	options: HashMap<String, FundingOptions>,
	/// Each open overlay's latest quote list.
	quotes: HashMap<String, FundingQuoteList>,
	/// Ended sessions the host has not acknowledged.
	unacknowledged: BTreeSet<String>,
	/// Whether a provider still works on any session, once funding resumed.
	in_progress: Option<bool>,
	/// The activity as last worked out, until a session changes.
	activity: Option<ActivityView>,
}

impl FundingSessionBook {
	/// Record that session `intent` changed to `status`, as `session` holds
	/// it now. A session the core keeps only as a summary drops its overlay
	/// state; one that ended stays owed to the host until acknowledged.
	pub fn session_changed(
		&self,
		intent: &str,
		status: &HostFundingStatusSubscribeItem,
		session: Option<&FundingSession>,
	) {
		let mut state = lock(&self.state);
		state.activity = None;
		let ended = !matches!(status, HostFundingStatusSubscribeItem::InProgress { .. });
		let owed = match session {
			Some(session) => session.is_terminal() && !session.acknowledged,
			None => {
				state.options.remove(intent);
				state.quotes.remove(intent);
				ended
			},
		};
		if owed {
			state.unacknowledged.insert(intent.to_string());
		} else {
			state.unacknowledged.remove(intent);
		}
	}

	/// Record whether a provider still works on any session.
	pub fn in_progress_changed(&self, in_progress: bool) {
		lock(&self.state).in_progress = Some(in_progress);
	}

	/// Record what session `intent`'s overlay offers.
	pub fn options_changed(&self, intent: &str, options: FundingOptions) {
		lock(&self.state).options.insert(intent.to_string(), options);
	}

	/// Record session `intent`'s latest quote list.
	pub fn quotes_changed(&self, intent: &str, quotes: FundingQuoteList) {
		lock(&self.state).quotes.insert(intent.to_string(), quotes);
	}

	/// Drop session `intent`'s overlay state once its overlay closed.
	pub fn overlay_closed(&self, intent: &str) {
		let mut state = lock(&self.state);
		state.options.remove(intent);
		state.quotes.remove(intent);
	}

	/// Record that the host acknowledged ended session `intent`.
	pub fn acknowledged(&self, intent: &str) {
		let mut state = lock(&self.state);
		state.unacknowledged.remove(intent);
		state.activity = None;
	}

	/// What session `intent`'s overlay offers.
	pub fn options(&self, intent: &str) -> Option<FundingOptions> {
		lock(&self.state).options.get(intent).cloned()
	}

	/// Session `intent`'s latest quote list.
	pub fn quotes(&self, intent: &str) -> Option<FundingQuoteList> {
		lock(&self.state).quotes.get(intent).cloned()
	}

	/// Ended sessions the host has not acknowledged.
	pub fn unacknowledged(&self) -> Vec<String> {
		lock(&self.state).unacknowledged.iter().cloned().collect()
	}

	/// Whether a provider still works on any session; `None` until funding
	/// resumed.
	pub fn in_progress(&self) -> Option<bool> {
		lock(&self.state).in_progress
	}

	/// The activity from `entries` and `history`, worked out once until a
	/// session changes.
	pub fn activity(
		&self,
		entries: impl FnOnce() -> Vec<(FundingSession, FundingProgress)>,
		history: impl FnOnce() -> Vec<FundingSessionSummary>,
	) -> ActivityView {
		if let Some(view) = lock(&self.state).activity.clone() {
			return view;
		}
		let view = activity(&entries(), &history());
		lock(&self.state).activity = Some(view.clone());
		view
	}
}

/// The resume marker's path under `database_directory`.
pub fn resume_marker(database_directory: &Path) -> PathBuf {
	database_directory.join(RESUME_MARKER)
}

/// Leave or clear the resume marker under `database_directory`, so the host
/// starts funding at its next launch only while a provider still works on a
/// session.
pub fn write_resume_marker(database_directory: &Path, in_progress: bool) {
	let marker = resume_marker(database_directory);
	let written = if in_progress {
		std::fs::write(&marker, b"")
	} else {
		match std::fs::remove_file(&marker) {
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
			other => other,
		}
	};
	if let Err(error) = written {
		tracing::error!(marker = %marker.display(), %error, "funding resume marker not written");
	}
}

#[cfg(test)]
mod tests;
