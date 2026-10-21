//! Unified [`Usernames`] trait.

use crate::{
	CallContext, CallError,
	versioned::usernames::{
		HostUsernamesSearchError, HostUsernamesSearchRequest, HostUsernamesSearchResponse,
	},
	wire, wire_trait,
};

/// The usernames of the network, through the directory of the host.
///
/// A product finds people by the start of their username, as the host's own
/// contact search does.
#[wire_trait(id = 27)]
#[crate::async_trait]
pub trait Usernames: Send + Sync {
	/// Find the usernames that start with `prefix`.
	///
	/// ```ts
	/// const result = await truapi.usernames.search({ prefix: "ali" });
	/// assert(result.isOk(), "usernames.search failed:", result);
	/// console.log(result.value.usernames);
	/// ```
	#[wire(id = 0)]
	async fn search(
		&self,
		_cx: &CallContext,
		_request: HostUsernamesSearchRequest,
	) -> Result<HostUsernamesSearchResponse, CallError<HostUsernamesSearchError>> {
		Err(CallError::unavailable())
	}
}
