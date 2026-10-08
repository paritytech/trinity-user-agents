//! Unified [`Contacts`] trait.

use crate::versioned::contacts::{
    HostContactsPickError, HostContactsPickRequest, HostContactsPickResponse,
};
use crate::{CallContext, CallError};
use crate::{wasm_env, wire, wire_trait};

/// User-mediated access to the user's contacts.
///
/// A product never reads the contact list. It opens the host's picker; the host
/// renders an overlay from the chat lists its chat extensions hold, and
/// returns only the person the user selected. Names, accounts, and every other
/// contact the user did not pick stay host-side.
///
/// That is also why there is no permission to request: the user choosing a
/// contact in host UI is the consent, and a product that is never handed the
/// list has nothing to be granted.
#[wasm_env]
#[wire_trait(id = 20)]
#[crate::async_trait]
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
}
