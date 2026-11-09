//! The funding state a host reads back beside the sessions themselves. The
//! core fills it as it tells the host's feed what changed, whichever runtime
//! serves the host.

use std::{
	collections::{BTreeSet, HashMap},
	sync::Mutex,
};

use truapi::latest::HostFundingStatusSubscribeItem;

use super::lock;
use crate::{
	host_logic::{
		funding::{FundingSession, FundingSessionSummary},
		funding_activity::{ActivityView, FundingBookEntry, activity},
		funding_providers::FundingOptions,
	},
	platform::FundingQuoteList,
};

/// The funding state the core keeps for the host beside the sessions
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

	/// The activity from `entries` and `history`, worked out once until a
	/// session changes.
	pub fn activity(
		&self,
		entries: impl FnOnce() -> Vec<FundingBookEntry>,
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

#[cfg(test)]
mod tests;
