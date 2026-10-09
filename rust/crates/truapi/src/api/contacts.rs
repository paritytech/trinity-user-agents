//! Unified [`Contacts`] trait.

use crate::versioned::contacts::{
    HostContactsPickError, HostContactsPickManyError, HostContactsPickManyRequest,
    HostContactsPickManyResponse, HostContactsPickRequest, HostContactsPickResponse,
    HostContactsPlaceLabelsError, HostContactsPlaceLabelsRequest, HostContactsPlaceLabelsResponse,
};
use crate::{CallContext, CallError};
use crate::{wire, wire_trait};

/// User-mediated access to the user's contacts.
///
/// A product never reads the contact list. It opens the host's picker; the host
/// renders an overlay from the chat lists its chat extensions hold, and
/// returns only handles for the people the user selected. Names, accounts, and
/// every other contact the user did not pick stay host-side.
///
/// That is also why there is no permission to request: the user choosing a
/// contact in host UI is the consent, and a product that is never handed the
/// list has nothing to be granted.
#[wire_trait(id = 20)]
#[crate::async_trait]
pub trait Contacts: Send + Sync {
    /// Ask the host to let the user pick one contact.
    ///
    /// Resolves with the chosen contact's handle, or with why nothing was
    /// chosen. A host that serves no picker answers `Unsupported`.
    ///
    /// The handle is not an address and cannot be turned into one by a product.
    /// To pay the person, put the handle where the recipient goes in the call
    /// and list it in `contacts` on the transaction payload: the host replaces
    /// it with their account before anything is signed or shown. Profile also
    /// accepts handles as disclosure recipients and as contacts to present or
    /// draw avatars for, without returning accounts or profile contents.
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

    /// Edit a complete selection in the host's multi-select contact picker.
    ///
    /// `selected` preselects existing handles. Confirming none returns `Picked`
    /// with an empty `handles` list; dismissing never changes the selection.
    /// Unresolvable initial handles reject the entire request.
    ///
    /// ```ts
    /// const result = await truapi.contacts.pickMany({ selected: [] });
    /// assert(result.isOk(), "contacts.pickMany failed:", result);
    /// if (result.value.outcome.tag === "Picked") {
    ///   console.log("confirmed handles:", result.value.outcome.value.handles);
    /// }
    /// ```
    #[wire(id = 1)]
    async fn pick_many(
        &self,
        _cx: &CallContext,
        _request: HostContactsPickManyRequest,
    ) -> Result<HostContactsPickManyResponse, CallError<HostContactsPickManyError>> {
        Err(CallError::unavailable())
    }

    /// Draw contact names in host-owned rectangles over the product surface.
    ///
    /// Labels do not require a shared Profile photo or disclosure. The response
    /// reveals no name, identity or per-slot availability. Each call replaces
    /// the previous placement; empty `slots` clears it.
    ///
    /// ```ts
    /// // Empty placement clears this product's host-owned labels.
    /// const result = await truapi.contacts.placeLabels({
    ///   surfaceWidth: 640,
    ///   surfaceHeight: 480,
    ///   slots: [],
    /// });
    /// assert(result.isOk(), "contacts.placeLabels failed:", result);
    /// ```
    #[wire(id = 2)]
    async fn place_labels(
        &self,
        _cx: &CallContext,
        _request: HostContactsPlaceLabelsRequest,
    ) -> Result<HostContactsPlaceLabelsResponse, CallError<HostContactsPlaceLabelsError>> {
        Err(CallError::unavailable())
    }
}
