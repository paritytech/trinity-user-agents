//! Unified [`Profile`] trait.

use crate::versioned::profile::{
    HostProfileDiscloseError, HostProfileDiscloseRequest, HostProfileDiscloseResponse,
    HostProfilePresentContactError, HostProfilePresentContactRequest,
    HostProfilePresentContactResponse, HostProfilePresentError, HostProfilePresentRequest,
    HostProfilePresentResponse, HostProfileRetractError, HostProfileRetractRequest,
    HostProfileRetractResponse,
};
use crate::{CallContext, CallError};
use crate::{wire, wire_trait};

/// Profiles shown in host-owned UI.
///
/// The product hands over an opaque reference; the host resolves, decrypts and
/// renders it. Profile bytes never return to the product.
#[wire_trait(id = 20)]
#[crate::async_trait]
pub trait Profile: Send + Sync {
    /// Show the referenced profile in host-owned UI.
    ///
    /// Resolves once the host has taken the presentation, not when the user
    /// dismisses it. Loading and fetch failures are shown to the user, not
    /// returned; a reference this host cannot parse is `InvalidReference`.
    ///
    /// ```ts
    /// const result = await truapi.profile.present({
    ///   reference: "bafkreigh2akiscaildc6ybwhxslp6rx2u4m2vpbhgvzhpsfkyzxiezxcnq#" + "00".repeat(44),
    /// });
    /// console.log("profile presentation:", result);
    /// ```
    #[wire(id = 0)]
    async fn present(
        &self,
        _cx: &CallContext,
        _request: HostProfilePresentRequest,
    ) -> Result<HostProfilePresentResponse, CallError<HostProfilePresentError>> {
        Err(CallError::unavailable())
    }
    /// Give the user's chat contacts this reference to their profile.
    ///
    /// The host stores it as the user's own and relays it to each contact,
    /// replacing whatever it sent before; the product never learns who they
    /// are. App executions only. A reference this core cannot screen is
    /// `InvalidReference`.
    ///
    /// ```ts
    /// const result = await truapi.profile.disclose({
    ///   reference: "seity-contacts:v1:" + "00".repeat(64),
    /// });
    /// console.log("profile disclosed:", result);
    /// ```
    #[wire(id = 1)]
    async fn disclose(
        &self,
        _cx: &CallContext,
        _request: HostProfileDiscloseRequest,
    ) -> Result<HostProfileDiscloseResponse, CallError<HostProfileDiscloseError>> {
        Err(CallError::unavailable())
    }

    /// Withdraw the reference this product disclosed. Contacts are told to
    /// drop what they hold. A product that did not disclose it is refused.
    ///
    /// ```ts
    /// const result = await truapi.profile.retract();
    /// console.log("profile retracted:", result);
    /// ```
    #[wire(id = 2)]
    async fn retract(
        &self,
        _cx: &CallContext,
        _request: HostProfileRetractRequest,
    ) -> Result<HostProfileRetractResponse, CallError<HostProfileRetractError>> {
        Err(CallError::unavailable())
    }

    /// Show a chat contact's profile in host-owned UI.
    ///
    /// The product names the contact; the host looks up the reference that
    /// contact shared and presents it as `present` would. The reference never
    /// reaches the product. A contact who shared nothing is `NotShared`.
    ///
    /// ```ts
    /// const result = await truapi.profile.presentContact({
    ///   peerIdentity: new Uint8Array(32),
    /// });
    /// console.log("contact profile presentation:", result);
    /// ```
    #[wire(id = 3)]
    async fn present_contact(
        &self,
        _cx: &CallContext,
        _request: HostProfilePresentContactRequest,
    ) -> Result<HostProfilePresentContactResponse, CallError<HostProfilePresentContactError>> {
        Err(CallError::unavailable())
    }
}
