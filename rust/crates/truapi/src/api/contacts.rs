//! Unified [`Contacts`] trait.

use crate::{
	CallContext, CallError,
	versioned::contacts::{
		HostContactsPickError, HostContactsPickRequest, HostContactsPickResponse,
		HostContactsUsernamesError, HostContactsUsernamesRequest, HostContactsUsernamesResponse,
	},
	wire, wire_trait,
};

/// User-mediated access to the user's contacts.
///
/// The picker is how every product reaches a contact. It opens an overlay the
/// host draws from the chat lists its chat extensions hold, and returns only
/// the person the user selected. Accounts, and every contact the user did not
/// pick, stay host-side.
///
/// That is also why there is no permission to request: the user choosing a
/// contact in host UI is the consent. The one exception is the game product,
/// Jollity, which may also read the DotNS usernames of the contacts of the
/// user, never their accounts.
#[wire_trait(id = 20)]
#[async_trait::async_trait]
pub trait Contacts: Send + Sync {
	/// Ask the host to let the user pick one contact.
	///
	/// Resolves with the chosen contact's handle, or with why nothing was
	/// chosen. A host that serves no picker answers `Unsupported`.
	///
	/// The handle is not an address and cannot be turned into one. To pay the
	/// person it names, put the handle where the recipient goes in the call and
	/// list it in `contacts` on the transaction payload: the host replaces it
	/// with their account before anything is signed or shown. A handle sent
	/// anywhere else is 32 bytes that resolve to nobody.
	///
	/// ```ts
	/// const result = await truapi.contacts.pick({});
	/// assert(result.isOk(), "contacts.pick failed:", result);
	/// const outcome = result.value.outcome;
	/// switch (outcome.tag) {
	///   case "Picked":
	///     console.log("picked:", outcome.value.handle);
	///     break;
	///   case "Dismissed":
	///     console.log("the user closed the picker; worth offering again");
	///     break;
	///   case "NoContacts":
	///     console.log("nothing to pick from");
	///     break;
	/// }
	/// ```
	#[wire(id = 0)]
	async fn pick(
		&self,
		_cx: &CallContext,
		_request: HostContactsPickRequest,
	) -> Result<HostContactsPickResponse, CallError<HostContactsPickError>> {
		Err(CallError::unavailable())
	}

	/// Return the DotNS usernames of the chat contacts of the user.
	///
	/// Served only to the game product, `dim2` on every network, with no prompt.
	/// Any other product, and a host that lists no usernames, gets `Unsupported`.
	/// The game product gets `NotConnected` while nobody is signed in.
	///
	/// ```ts
	/// const result = await truapi.contacts.usernames({});
	/// if (result.isErr()) {
	///   assert(
	///     result.error.tag === "Unsupported" || result.error.tag === "NotConnected",
	///     "contacts.usernames failed:",
	///     result,
	///   );
	///   console.log("contact usernames are not served here:", result.error.tag);
	/// } else {
	///   console.log("contact usernames:", result.value.usernames);
	/// }
	/// ```
	#[wire(id = 1)]
	async fn usernames(
		&self,
		_cx: &CallContext,
		_request: HostContactsUsernamesRequest,
	) -> Result<HostContactsUsernamesResponse, CallError<HostContactsUsernamesError>> {
		Err(CallError::unavailable())
	}
}
