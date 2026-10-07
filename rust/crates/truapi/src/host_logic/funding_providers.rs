//! Funding providers: what each one declares it serves, and which of them a
//! session can be handed to.
//!
//! The host supplies the providers it offers. Each one's capabilities come
//! from the `includes.funding` configuration of its Worker manifest, read
//! from dotNS when the core has a fresh answer and from the snapshot the host
//! shipped otherwise, so the list renders before any chain read.

use truapi::latest::FundingDirection;

use crate::host_internal::worker_manifest::WorkerManifest;

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

/// A provider's funding configuration, with every value the core does not
/// recognise already left out.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct FundingConfig {
    /// What the provider moves and how; never empty.
    pub routes: Vec<FundingRoute>,
    /// Where the host gets a live quote.
    pub quote: FundingQuoteSource,
    /// Onramp adapter id for calls that need the provider's key.
    pub backend: Option<String>,
}

/// One payment mode a provider serves.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct FundingRoute {
    /// The payment mode.
    pub mode: FundingMode,
    /// Directions served; never empty.
    pub directions: Vec<FundingDirection>,
    /// Symbols the user pays with or receives; never empty.
    pub assets: Vec<String>,
    /// ISO 3166-1 alpha-2 codes the route serves, when declared. The quote
    /// still decides.
    pub countries: Option<Vec<String>>,
    /// Whether the user needs an account with the provider.
    pub requires_account: bool,
}

/// How the user pays or is paid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum FundingMode {
    /// A card payment.
    Card,
    /// A bank transfer.
    Bank,
    /// A crypto transfer.
    Crypto,
}

/// Where a provider's quotes come from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum FundingQuoteSource {
    /// The provider's worker answers.
    Worker,
    /// The host calls this https URL.
    Url {
        /// The URL.
        url: String,
    },
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
    /// Where the host gets a live quote.
    pub quote: FundingQuoteSource,
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
        let routes: Vec<FundingRoute> = funding
            .routes
            .iter()
            .filter(|route| route.directions.contains(&direction))
            .cloned()
            .collect();
        (!routes.is_empty()).then(|| Self {
            provider_id: provider_id.to_string(),
            routes,
            quote: funding.quote.clone(),
            backend: funding.backend.clone(),
        })
    }
}
