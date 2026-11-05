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

use crate::{
	host_logic::worker_manifest::{FundingRoute, WorkerManifest},
	platform::ProductId,
};

/// A provider the host offers, as the host supplies it.
#[derive(Clone)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingProviderEntry {
	/// The provider's product id.
	pub product_id: ProductId,
	/// The Worker manifest JSON the host shipped for it, used until the core
	/// has read the live one. `None` waits for that read: until dotNS answers,
	/// the provider is neither asked for quotes nor offered for a session.
	pub worker_manifest: Option<String>,
}

/// How long what a quote answer showed about a provider is trusted over its
/// manifest.
pub const LEARNED_SUPPORT_TTL_MS: u64 = 12 * 60 * 60 * 1_000;

/// What a provider's answer to a quote ask showed about what it serves now,
/// which may differ from what its manifest declares.
#[derive(Clone, Encode, Decode)]
pub struct LearnedSupport {
	/// The provider.
	pub provider_id: ProductId,
	/// The ask's direction.
	pub direction: FundingDirection,
	/// The ask's rail.
	pub rail: FundingRail,
	/// The ask's asset symbol.
	pub asset: String,
	/// The ask's country, when it named one.
	pub country: Option<String>,
	/// Whether the provider serves it: it quoted, or refused only the amount.
	pub supported: bool,
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
		self.provider_id == other.provider_id &&
			self.direction == other.direction &&
			self.rail == other.rail &&
			self.asset == other.asset &&
			self.country == other.country
	}
}

/// A rail, asset and country a provider recently refused, so the host can
/// leave it out for a user there.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingUnsupported {
	/// The rail.
	pub rail: FundingRail,
	/// The asset symbol.
	pub asset: String,
	/// The country, or `None` when the provider refused it anywhere.
	pub country: Option<String>,
}

/// A provider a session can be handed to, with only the routes that serve the
/// session's direction.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingCandidate {
	/// The provider's product id.
	pub provider_id: ProductId,
	/// Routes serving the direction, as its manifest declares them and its
	/// recent quote answers confirmed. Empty when it declares none and no
	/// answer has confirmed one yet, so only its quotes say what it serves.
	pub routes: Vec<FundingRoute>,
	/// What its recent quote answers refused, which its routes may still
	/// declare.
	pub unsupported: Vec<FundingUnsupported>,
}

impl FundingCandidate {
	/// `provider_id` as a candidate for `direction`: the routes its manifest
	/// declares for it, with what its answers learned in the last
	/// [`LEARNED_SUPPORT_TTL_MS`] folded in. Only a manifest that serves
	/// Funding makes a candidate: one withdrawn or no longer serving it is not
	/// brought back by what earlier answers showed. A provider whose answers
	/// showed it serves the direction is a candidate even if its routes do not
	/// say so, and one that serves Funding without declaring routes is a
	/// candidate for either direction.
	pub fn for_direction(
		provider_id: &ProductId,
		manifest: Option<&WorkerManifest>,
		learned: &[LearnedSupport],
		direction: FundingDirection,
		now_ms: u64,
	) -> Option<Self> {
		let funding = manifest?.funding.as_ref()?;
		let declared = funding.routes.as_ref();
		let mut routes: Vec<FundingRoute> = declared
			.map(|routes| {
				routes
					.iter()
					.filter(|route| route.directions.contains(&direction))
					.cloned()
					.collect()
			})
			.unwrap_or_default();
		let mut unsupported = Vec::new();
		let fresh = learned.iter().filter(|support| {
			&support.provider_id == provider_id &&
				support.direction == direction &&
				support.is_fresh(now_ms)
		});
		for support in fresh {
			if support.supported {
				confirm(&mut routes, support);
			} else {
				unsupported.push(FundingUnsupported {
					rail: support.rail,
					asset: support.asset.clone(),
					country: support.country.clone(),
				});
			}
		}
		(declared.is_none() || !routes.is_empty()).then(|| Self {
			provider_id: provider_id.clone(),
			routes,
			unsupported,
		})
	}
}

/// Fold a confirmed rail, asset and country into `routes`. A country only
/// widens a route that lists its countries: one that lists none serves any,
/// and a route learned from an answer lists none, as the answer does not say
/// where else it is served.
fn confirm(routes: &mut Vec<FundingRoute>, support: &LearnedSupport) {
	let Some(route) = routes.iter_mut().find(|route| route.rail == support.rail) else {
		routes.push(FundingRoute {
			rail: support.rail,
			directions: vec![support.direction],
			assets: vec![support.asset.clone()],
			networks: None,
			countries: None,
		});
		return;
	};
	if !route.assets.contains(&support.asset) {
		route.assets.push(support.asset.clone());
	}
	if let (Some(countries), Some(country)) = (route.countries.as_mut(), &support.country) &&
		!countries.contains(country)
	{
		countries.push(country.clone());
	}
}

#[cfg(test)]
mod tests;
