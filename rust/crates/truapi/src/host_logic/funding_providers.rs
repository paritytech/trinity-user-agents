//! Funding providers: what each one declares it serves, and which of them a
//! session can be handed to.
//!
//! The host supplies the providers it offers. Each one's capabilities come
//! from the `includes.funding` configuration of its Worker manifest, read
//! from dotNS when the core has a fresh answer and from the snapshot the host
//! shipped otherwise, so the list renders before any chain read. What a
//! provider's quote answers show about what it serves now takes precedence
//! for a while, since a manifest can lag behind the provider.

use parity_scale_codec::{Decode, Encode};
use truapi::latest::{FundingDirection, FundingRail};

use crate::host_logic::worker_manifest::{FundingMode, FundingRoute, RouteDirection, WorkerManifest};

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
    /// The host ships the provider itself, so `worker_manifest` is what the
    /// core goes by and dotNS is never read for it.
    pub bundled: bool,
}

/// How long what a quote answer showed about a provider is trusted over its
/// manifest.
pub const LEARNED_SUPPORT_TTL_MS: u64 = 12 * 60 * 60 * 1_000;

/// What a provider's answer to a quote ask showed about what it serves now,
/// which may differ from what its manifest declares.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct LearnedSupport {
    /// The provider.
    pub provider_id: String,
    /// The ask's direction.
    pub direction: FundingDirection,
    /// The ask's rail.
    pub rail: FundingRail,
    /// The ask's asset symbol.
    pub asset: String,
    /// The ask's network, for a crypto ask that named one.
    pub network: Option<String>,
    /// The ask's country, when it named one.
    pub country: Option<String>,
    /// Whether the provider serves it: it quoted, or refused only the amount.
    pub supported: bool,
    /// The smallest amount it takes, from its latest `BelowMinimum`.
    pub min: Option<u128>,
    /// The largest amount it takes, from its latest `AboveMaximum`.
    pub max: Option<u128>,
    /// When the answer came, in Unix milliseconds.
    pub learned_at_ms: u64,
}

impl LearnedSupport {
    /// Whether this is still trusted at `now_ms`.
    pub fn is_fresh(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.learned_at_ms) < LEARNED_SUPPORT_TTL_MS
    }

    /// Whether `other` is about the same provider and ask.
    pub fn same_ask(&self, other: &Self) -> bool {
        self.provider_id == other.provider_id
            && self.direction == other.direction
            && self.rail == other.rail
            && self.asset == other.asset
            && self.network == other.network
            && self.country == other.country
    }
}

/// A rail, asset and country a provider recently refused, so the host can
/// leave it out for a user there.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct FundingUnsupported {
    /// The rail.
    pub rail: FundingRail,
    /// The asset symbol.
    pub asset: String,
    /// The country, or `None` when the provider refused it anywhere.
    pub country: Option<String>,
}

/// The amounts a provider takes for a rail, asset and network, as its quote
/// refusals showed them, so the host can show the limits before quoting.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct FundingLimit {
    /// The rail.
    pub rail: FundingRail,
    /// The asset symbol.
    pub asset: String,
    /// The network, for crypto.
    pub network: Option<String>,
    /// The smallest amount it takes, in balance units, when known.
    pub min: Option<u128>,
    /// The largest amount it takes, in balance units, when known.
    pub max: Option<u128>,
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
    /// Routes serving the direction, as its manifest declares them and its
    /// recent quote answers confirmed; never empty.
    pub routes: Vec<FundingRoute>,
    /// What its recent quote answers refused, which its routes may still
    /// declare.
    pub unsupported: Vec<FundingUnsupported>,
    /// The limits its recent quote refusals showed.
    pub limits: Vec<FundingLimit>,
    /// Onramp adapter id for calls that need the provider's key.
    pub backend: Option<String>,
}

impl FundingCandidate {
    /// `provider_id` as a candidate for `direction`: the routes its manifest
    /// declares for it, with what its answers learned in the last
    /// [`LEARNED_SUPPORT_TTL_MS`] folded in. A provider whose answers showed
    /// it serves the direction is a candidate even if its manifest does not
    /// say so.
    pub fn for_direction(
        provider_id: &str,
        manifest: Option<&WorkerManifest>,
        learned: &[LearnedSupport],
        direction: FundingDirection,
        now_ms: u64,
    ) -> Option<Self> {
        let funding = manifest.and_then(|manifest| manifest.funding.as_ref());
        let route_direction = match direction {
            FundingDirection::In => RouteDirection::In,
            FundingDirection::Out => RouteDirection::Out,
        };
        let mut routes: Vec<FundingRoute> = funding
            .map(|funding| {
                funding
                    .routes
                    .iter()
                    .filter(|route| route.directions.contains(&route_direction))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut unsupported = Vec::new();
        let mut limits = Vec::new();
        let fresh = learned.iter().filter(|support| {
            support.provider_id == provider_id
                && support.direction == direction
                && support.is_fresh(now_ms)
        });
        for support in fresh {
            let mode = mode_of(support.rail);
            if support.min.is_some() || support.max.is_some() {
                limits.push(FundingLimit {
                    rail: support.rail,
                    asset: support.asset.clone(),
                    network: support.network.clone(),
                    min: support.min,
                    max: support.max,
                });
            }
            if support.supported {
                confirm(&mut routes, mode, route_direction, support);
            } else {
                unsupported.push(FundingUnsupported {
                    rail: support.rail,
                    asset: support.asset.clone(),
                    country: support.country.clone(),
                });
            }
        }
        (!routes.is_empty()).then(|| Self {
            provider_id: provider_id.to_string(),
            routes,
            unsupported,
            limits,
            backend: funding.and_then(|funding| funding.backend.clone()),
        })
    }
}

/// Fold a confirmed rail, asset and country into `routes`.
fn confirm(
    routes: &mut Vec<FundingRoute>,
    mode: FundingMode,
    direction: RouteDirection,
    support: &LearnedSupport,
) {
    let Some(route) = routes.iter_mut().find(|route| route.mode == mode) else {
        routes.push(FundingRoute {
            mode,
            directions: vec![direction],
            assets: vec![support.asset.clone()],
            networks: support.network.clone().map(|network| vec![network]),
            countries: support.country.clone().map(|country| vec![country]),
            requires_account: false,
        });
        return;
    };
    if !route.assets.contains(&support.asset) {
        route.assets.push(support.asset.clone());
    }
    if let Some(network) = &support.network {
        let networks = route.networks.get_or_insert_with(Vec::new);
        if !networks.contains(network) {
            networks.push(network.clone());
        }
    }
    if let (Some(countries), Some(country)) = (route.countries.as_mut(), &support.country)
        && !countries.contains(country)
    {
        countries.push(country.clone());
    }
}

/// The manifest's mode for a quote's rail.
pub fn mode_of(rail: FundingRail) -> FundingMode {
    match rail {
        FundingRail::Card => FundingMode::Card,
        FundingRail::Bank => FundingMode::Bank,
        FundingRail::Crypto => FundingMode::Crypto,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_700_000_000_000;

    fn learned(supported: bool, at_ms: u64) -> LearnedSupport {
        LearnedSupport {
            provider_id: "ramp.dot".to_string(),
            direction: FundingDirection::In,
            rail: FundingRail::Bank,
            asset: "EUR".to_string(),
            network: None,
            country: Some("DE".to_string()),
            supported,
            min: None,
            max: None,
            learned_at_ms: at_ms,
        }
    }

    // What an answer showed stands in for the manifest only while it is
    // trusted; after that the manifest applies again.
    #[test]
    fn learned_support_applies_for_twelve_hours() {
        let candidate = |at_ms| {
            FundingCandidate::for_direction(
                "ramp.dot",
                None,
                &[learned(true, at_ms)],
                FundingDirection::In,
                NOW,
            )
            .map(|candidate| candidate.routes)
        };

        assert_eq!(
            (candidate(NOW - LEARNED_SUPPORT_TTL_MS + 1), candidate(NOW - LEARNED_SUPPORT_TTL_MS)),
            (
                Some(vec![FundingRoute {
                    mode: FundingMode::Bank,
                    directions: vec![RouteDirection::In],
                    assets: vec!["EUR".to_string()],
                    networks: None,
                    countries: Some(vec!["DE".to_string()]),
                    requires_account: false,
                }]),
                None,
            )
        );
    }

    // A refusal of only the amount shows what the provider takes, so the host
    // can show the limit before quoting, and a crypto answer shows the network
    // it serves, for the network picker.
    #[test]
    fn answers_show_the_limits_and_the_crypto_networks() {
        let usdt = LearnedSupport {
            rail: FundingRail::Crypto,
            asset: "USDT".to_string(),
            network: Some("polkadot".to_string()),
            country: None,
            min: Some(10_000_000),
            ..learned(true, NOW)
        };

        let candidate =
            FundingCandidate::for_direction("ramp.dot", None, &[usdt], FundingDirection::In, NOW)
                .expect("candidate");

        assert_eq!(
            (candidate.routes, candidate.limits),
            (
                vec![FundingRoute {
                    mode: FundingMode::Crypto,
                    directions: vec![RouteDirection::In],
                    assets: vec!["USDT".to_string()],
                    networks: Some(vec!["polkadot".to_string()]),
                    countries: None,
                    requires_account: false,
                }],
                vec![FundingLimit {
                    rail: FundingRail::Crypto,
                    asset: "USDT".to_string(),
                    network: Some("polkadot".to_string()),
                    min: Some(10_000_000),
                    max: None,
                }],
            )
        );
    }
}
