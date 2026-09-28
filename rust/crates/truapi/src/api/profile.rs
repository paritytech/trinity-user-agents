//! Unified [`Profile`] trait.

use crate::versioned::profile::{
    HostProfileDiscloseError, HostProfileDiscloseRequest, HostProfileDiscloseResponse,
    HostProfilePlaceContactAvatarsError, HostProfilePlaceContactAvatarsRequest,
    HostProfilePlaceContactAvatarsResponse, HostProfilePresentContactError,
    HostProfilePresentContactRequest, HostProfilePresentContactResponse, HostProfilePresentError,
    HostProfilePresentRequest, HostProfilePresentResponse, HostProfileRetractError,
    HostProfileRetractRequest, HostProfileRetractResponse,
};
use crate::{CallContext, CallError};
use crate::{wire, wire_trait};

/// Profiles shown in host-owned UI.
///
/// The product hands over an opaque reference; the host resolves, decrypts and
/// renders it. Profile bytes never return to the product.
#[wire_trait(id = 22)]
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
    /// are. App executions only. The first disclosure asks the user once for
    /// this product; a refusal, then or remembered, is `PermissionDenied`. A
    /// reference this core cannot screen is `InvalidReference`, and with no
    /// user signed in the call is `NotConnected`.
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
    ///   peerIdentity: "0x0000000000000000000000000000000000000000000000000000000000000000",
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

    /// Tell the host where this product draws chat contacts' avatars, so it
    /// can draw each contact's shared photo and mood ring over them on its own
    /// layer.
    ///
    /// Each call replaces the product's placement; an empty `slots` clears it.
    /// The host draws only for contacts who shared a profile with the user,
    /// and keeps the placement current as they share or withdraw one, until
    /// the product replaces it or goes away. The answer is the same whoever
    /// shared: nothing about any slot, and no profile data, returns to the
    /// product. Taps still reach the product, which opens a profile with
    /// `presentContact`.
    ///
    /// App executions only. Rects are in the units of the surface size the
    /// product gives: framebuffer pixels for a PolkaVM product, CSS pixels of
    /// its viewport for a web product. A placement with more than 64 slots, a
    /// surface side outside 1 to 16384, an avatar that is not square or is
    /// outside 1 to 1024 a side, or a repeated `slot` is `Unknown`. A host that
    /// cannot draw over the product is `Unsupported`; with no user signed in
    /// the call is `NotConnected`.
    ///
    /// ```ts
    /// const result = await truapi.profile.placeContactAvatars({
    ///   surfaceWidth: 360,
    ///   surfaceHeight: 640,
    ///   slots: [
    ///     {
    ///       slot: 0,
    ///       peerIdentity: "0x0000000000000000000000000000000000000000000000000000000000000000",
    ///       rect: { x: 16, y: 80, width: 44, height: 44 },
    ///       clip: { x: 0, y: 64, width: 360, height: 576 },
    ///     },
    ///   ],
    /// });
    /// console.log("contact avatars placed:", result);
    /// ```
    #[wire(id = 4)]
    async fn place_contact_avatars(
        &self,
        _cx: &CallContext,
        _request: HostProfilePlaceContactAvatarsRequest,
    ) -> Result<
        HostProfilePlaceContactAvatarsResponse,
        CallError<HostProfilePlaceContactAvatarsError>,
    > {
        Err(CallError::unavailable())
    }
}
