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

use crate::host_logic::worker_manifest::{FundingRoute, WorkerManifest};

/// A provider the host offers, as the host supplies it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingProviderEntry {
	/// The provider's product id.
	pub product_id: String,
	/// The Worker manifest JSON the host shipped for it, used until the core
	/// has read the live one. `None` waits for that read.
	pub worker_manifest: Option<String>,
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
	pub provider_id: String,
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
		provider_id: &str,
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
			support.provider_id == provider_id &&
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
			provider_id: provider_id.to_string(),
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
mod tests {
	use super::*;

	const NOW: u64 = 1_700_000_000_000;

	fn learned(supported: bool, at_ms: u64) -> LearnedSupport {
		LearnedSupport {
			provider_id: "ramp.dot".to_string(),
			direction: FundingDirection::In,
			rail: FundingRail::Bank,
			asset: "EUR".to_string(),
			country: Some("DE".to_string()),
			supported,
			learned_at_ms: at_ms,
		}
	}

	fn manifest(funding: &str) -> WorkerManifest {
		WorkerManifest::parse(&format!(
			r#"{{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"index.js","includes":{{"funding":{funding}}}}}"#
		))
		.expect("parses")
	}

	/// Declares card in only, so bank in comes from what answers showed.
	fn card_in() -> WorkerManifest {
		manifest(r#"{"routes":[{"mode":"CARD","directions":["In"],"assets":["EUR"]}]}"#)
	}

	// Routes only narrow which providers are asked, so a provider that
	// declares none is a candidate either way and its quotes decide, while
	// declared routes still leave out a direction they do not serve.
	#[test]
	fn a_provider_without_routes_is_a_candidate_for_either_direction() {
		let undeclared = manifest("{}");
		let card_in = card_in();
		let candidate = |manifest: &WorkerManifest, direction| {
			FundingCandidate::for_direction("ramp.dot", Some(manifest), &[], direction, NOW)
				.map(|candidate| candidate.routes)
		};

		assert_eq!(
			(
				candidate(&undeclared, FundingDirection::In),
				candidate(&undeclared, FundingDirection::Out),
				candidate(&card_in, FundingDirection::Out),
			),
			(Some(Vec::new()), Some(Vec::new()), None)
		);
	}

	// What an answer showed widens the manifest only while it is trusted;
	// after that the manifest applies again. A rail learned this way lists no
	// countries, as one answer does not say where else it is served.
	#[test]
	fn learned_support_applies_for_twelve_hours() {
		let card_in = card_in();
		let candidate = |at_ms| {
			FundingCandidate::for_direction(
				"ramp.dot",
				Some(&card_in),
				&[learned(true, at_ms)],
				FundingDirection::In,
				NOW,
			)
			.map(|candidate| {
				candidate.routes.into_iter().map(|route| route.rail).collect::<Vec<_>>()
			})
		};
		let bank = FundingCandidate::for_direction(
			"ramp.dot",
			Some(&card_in),
			&[learned(true, NOW)],
			FundingDirection::In,
			NOW,
		)
		.and_then(|candidate| {
			candidate.routes.into_iter().find(|route| route.rail == FundingRail::Bank)
		});

		assert_eq!(
			(
				candidate(NOW - LEARNED_SUPPORT_TTL_MS + 1),
				candidate(NOW - LEARNED_SUPPORT_TTL_MS),
				bank
			),
			(
				Some(vec![FundingRail::Card, FundingRail::Bank]),
				Some(vec![FundingRail::Card]),
				Some(FundingRoute {
					rail: FundingRail::Bank,
					directions: vec![FundingDirection::In],
					assets: vec!["EUR".to_string()],
					networks: None,
					countries: None,
				}),
			)
		);
	}

	// A provider that withdrew its manifest, or no longer serves Funding,
	// stops being offered at once, whatever its earlier answers showed.
	#[test]
	fn learned_support_needs_a_manifest_serving_funding() {
		let without_funding = WorkerManifest::parse(
			r#"{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"index.js","includes":{"chat":true}}"#,
		)
		.expect("parses");
		let candidate = |manifest: Option<&WorkerManifest>| {
			FundingCandidate::for_direction(
				"ramp.dot",
				manifest,
				&[learned(true, NOW)],
				FundingDirection::In,
				NOW,
			)
		};

		assert_eq!((candidate(None), candidate(Some(&without_funding))), (None, None));
	}
}
