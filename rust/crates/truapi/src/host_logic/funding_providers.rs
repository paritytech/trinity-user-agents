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
	/// Where the provider's Worker manifest comes from.
	pub origin: FundingProviderOrigin,
}

/// Where the core reads a provider's Worker manifest.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingProviderOrigin {
	/// Published on dotNS: the core reads the live manifest, and goes by the
	/// host's snapshot until it has.
	#[default]
	Published,
	/// Shipped inside the host: its `worker_manifest` is what the core goes
	/// by, and dotNS is never read for it.
	Bundled,
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
		self.provider_id == other.provider_id &&
			self.direction == other.direction &&
			self.rail == other.rail &&
			self.asset == other.asset &&
			self.network == other.network &&
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

/// The amounts a provider takes for a rail, asset and network, as its quote
/// refusals showed them, so the host can show the limits before quoting.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
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
	/// The limits its recent quote refusals showed.
	pub limits: Vec<FundingLimit>,
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
		let mut limits = Vec::new();
		let fresh = learned.iter().filter(|support| {
			support.provider_id == provider_id &&
				support.direction == direction &&
				support.is_fresh(now_ms)
		});
		for support in fresh {
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
			limits,
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
			networks: support.network.clone().map(|network| vec![network]),
			countries: None,
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
	if let (Some(countries), Some(country)) = (route.countries.as_mut(), &support.country) &&
		!countries.contains(country)
	{
		countries.push(country.clone());
	}
}

/// What the core knows about funding from one country, for the host's
/// country picker.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingCountry {
	/// ISO 3166-1 alpha-2 code.
	pub country: String,
	/// Whether a provider serves it.
	pub support: FundingCountrySupport,
}

/// Whether a provider serves a country.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingCountrySupport {
	/// A provider serves it, by its manifest or a recent quote answer.
	Supported {
		/// The smallest amount a provider serving it showed it takes there,
		/// in the user's balance units, when one did.
		min: Option<u128>,
	},
	/// Every provider whose recent answers named it refused it, and no
	/// manifest declares it.
	Unsupported,
}

/// What `providers`' manifests and recent quote answers say about each
/// country for `direction`, `rail` and `asset`, by country code. A country
/// nothing names is left out, as the core knows nothing about it.
pub fn country_support(
	providers: &[(String, Option<WorkerManifest>)],
	learned: &[LearnedSupport],
	direction: FundingDirection,
	rail: FundingRail,
	asset: &str,
	now_ms: u64,
) -> Vec<FundingCountry> {
	let mut countries: Vec<FundingCountry> = Vec::new();
	let mut note = |country: &str, supported: bool, min: Option<u128>| {
		let index = match countries.iter().position(|known| known.country == country) {
			Some(index) => index,
			None => {
				countries.push(FundingCountry {
					country: country.to_string(),
					support: FundingCountrySupport::Unsupported,
				});
				countries.len() - 1
			},
		};
		let support = &mut countries[index].support;
		*support = match (support.clone(), supported) {
			(FundingCountrySupport::Supported { min: known }, true) => {
				FundingCountrySupport::Supported { min: lowest(known, min) }
			},
			(FundingCountrySupport::Unsupported, true) => FundingCountrySupport::Supported { min },
			(kept, false) => kept,
		};
	};
	for (provider_id, manifest) in providers {
		let routes = manifest
			.as_ref()
			.and_then(|manifest| manifest.funding.as_ref())
			.and_then(|funding| funding.routes.as_ref());
		for route in routes.into_iter().flatten().filter(|route| {
			route.rail == rail &&
				route.directions.contains(&direction) &&
				route.assets.iter().any(|known| known == asset)
		}) {
			for country in route.countries.iter().flatten() {
				note(country, true, None);
			}
		}
		for support in learned.iter().filter(|support| {
			&support.provider_id == provider_id &&
				support.direction == direction &&
				support.rail == rail &&
				support.asset == asset &&
				support.is_fresh(now_ms)
		}) {
			if let Some(country) = &support.country {
				note(country, support.supported, support.min);
			}
		}
	}
	countries.sort_by(|a, b| a.country.cmp(&b.country));
	countries
}

fn lowest(a: Option<u128>, b: Option<u128>) -> Option<u128> {
	match (a, b) {
		(Some(a), Some(b)) => Some(a.min(b)),
		(a, b) => a.or(b),
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

		let undeclared = manifest("{}");
		let candidate = FundingCandidate::for_direction(
			"ramp.dot",
			Some(&undeclared),
			&[usdt],
			FundingDirection::In,
			NOW,
		)
		.expect("candidate");

		assert_eq!(
			(candidate.routes, candidate.limits),
			(
				vec![FundingRoute {
					rail: FundingRail::Crypto,
					directions: vec![FundingDirection::In],
					assets: vec!["USDT".to_string()],
					networks: Some(vec!["polkadot".to_string()]),
					countries: None,
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

	// The country picker shows what the core knows: a country a manifest or
	// an answer serves, with the lowest minimum shown there, and one that
	// every answer naming it refused. A country nothing names is left out.
	#[test]
	fn countries_show_what_manifests_and_answers_say() {
		let declared = manifest(
			r#"{"routes":[{"mode":"CARD","directions":["In"],"assets":["EUR"],"countries":["PT","DE"]}]}"#,
		);
		let in_country = |country: &str, supported, min| LearnedSupport {
			rail: FundingRail::Card,
			country: Some(country.to_string()),
			supported,
			min,
			..learned(true, NOW)
		};
		let providers = [
			("ramp.dot".to_string(), Some(declared)),
			("other.dot".to_string(), Some(manifest("{}"))),
		];
		let learned = [
			in_country("DE", true, Some(2_000)),
			LearnedSupport {
				provider_id: "other.dot".to_string(),
				..in_country("DE", true, Some(1_500))
			},
			in_country("UY", false, None),
			LearnedSupport { rail: FundingRail::Bank, ..in_country("FR", true, None) },
		];

		assert_eq!(
			country_support(
				&providers,
				&learned,
				FundingDirection::In,
				FundingRail::Card,
				"EUR",
				NOW
			),
			vec![
				FundingCountry {
					country: "DE".to_string(),
					support: FundingCountrySupport::Supported { min: Some(1_500) },
				},
				FundingCountry {
					country: "PT".to_string(),
					support: FundingCountrySupport::Supported { min: None },
				},
				FundingCountry {
					country: "UY".to_string(),
					support: FundingCountrySupport::Unsupported
				},
			]
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
