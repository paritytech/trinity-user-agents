//! Unified [`Usernames`] trait.

use crate::{
	CallContext, CallError,
	versioned::usernames::{
		HostUsernamesAddContactError, HostUsernamesAddContactRequest,
		HostUsernamesAddContactResponse, HostUsernamesSearchError, HostUsernamesSearchRequest,
		HostUsernamesSearchResponse,
	},
	wasm_env, wire, wire_trait,
};

/// The usernames of the network, through the directory of the host.
///
/// A product finds people by the start of their username, as the host's own
/// contact search does, and asks the host to add a person to the contacts of
/// the app. The host asks the user first and adds the person its own way, so
/// every product of the user can then pick the person (`contacts.pick`).
#[wasm_env]
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

	/// Ask the host to add the owner of `username` to the contacts of the app.
	/// The host asks the user, so a Worker calls it only after a tap.
	///
	/// ```ts
	/// const result = await truapi.usernames.addContact({ username: "alice.42" });
	/// assert(result.isOk(), "usernames.addContact failed:", result);
	/// console.log(result.value.outcome);
	/// ```
	#[wire(id = 1)]
	async fn add_contact(
		&self,
		_cx: &CallContext,
		_request: HostUsernamesAddContactRequest,
	) -> Result<HostUsernamesAddContactResponse, CallError<HostUsernamesAddContactError>> {
		Err(CallError::unavailable())
	}
}
