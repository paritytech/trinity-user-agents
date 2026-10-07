//! Funding providers: what each one declares it serves, and which of them a
//! session can be handed to.
//!
//! The host supplies the providers it offers. Each one's capabilities come
//! from the `includes.funding` configuration of its Worker manifest, read
//! from dotNS when the core has a fresh answer and from the snapshot the host
//! shipped otherwise, so the list renders before any chain read.

use truapi::latest::FundingDirection;

use crate::host_logic::worker_manifest::{
    FundingRoute, RouteDirection, WorkerManifest,
};

/// A provider the host offers, as the host supplies it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct FundingProviderEntry {
    /// The provider's product id.
    pub product_id: String,
    /// The Worker manifest JSON the host shipped for it, used until the core
    /// has read the live one. `None` waits for that read.
    pub worker_manifest: Option<String>,
}

/// A provider a session can be handed to, with only the routes that serve the
/// session's direction.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct FundingCandidate {
    /// The provider's product id.
    pub provider_id: String,
    /// Routes serving the direction; never empty.
    pub routes: Vec<FundingRoute>,
    /// Onramp adapter id for calls that need the provider's key.
    pub backend: Option<String>,
}

impl FundingCandidate {
    /// `provider_id` as a candidate for `direction`, when its manifest serves
    /// Funding in that direction.
    pub fn for_direction(
        provider_id: &str,
        manifest: &WorkerManifest,
        direction: FundingDirection,
    ) -> Option<Self> {
        let funding = manifest.funding.as_ref()?;
        let direction = match direction {
            FundingDirection::In => RouteDirection::In,
            FundingDirection::Out => RouteDirection::Out,
        };
        let routes: Vec<FundingRoute> = funding
            .routes
            .iter()
            .filter(|route| route.directions.contains(&direction))
            .cloned()
            .collect();
        (!routes.is_empty()).then(|| Self {
            provider_id: provider_id.to_string(),
            routes,
            backend: funding.backend.clone(),
        })
    }
}
