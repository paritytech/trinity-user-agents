//! The funding state a host reads back beside the sessions themselves. The
//! core fills it as it tells the host's feed what changed, whichever runtime
//! serves the host.

use std::{collections::HashMap, sync::Mutex};

use super::lock;
use crate::{host_logic::funding_providers::FundingOptions, platform::FundingQuoteList};

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
}

impl FundingSessionBook {
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

	/// What session `intent`'s overlay offers.
	pub fn options(&self, intent: &str) -> Option<FundingOptions> {
		lock(&self.state).options.get(intent).cloned()
	}

	/// Session `intent`'s latest quote list.
	pub fn quotes(&self, intent: &str) -> Option<FundingQuoteList> {
		lock(&self.state).quotes.get(intent).cloned()
	}
}

#[cfg(test)]
mod tests;
