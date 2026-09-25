//! Unified [`Profile`] trait.

use crate::versioned::profile::{
    HostProfilePresentError, HostProfilePresentRequest, HostProfilePresentResponse,
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
}
