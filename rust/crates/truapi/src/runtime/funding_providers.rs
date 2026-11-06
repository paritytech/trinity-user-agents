//! The funding providers a session can be handed to, kept current against
//! dotNS.
//!
//! The host supplies the providers it offers, each with the Worker manifest it
//! shipped; candidates are answered from what dotNS last said about a provider,
//! and from that snapshot until dotNS has answered, so the list renders with no
//! chain read. Only the providers the host lists are offered: the list is the
//! host's to control. Every query re-checks them in the background through
//! the manifest cache, so a provider whose publisher changed, withdrew or
//! unpublished it is corrected within the cache lifetime.

use std::{
	collections::{HashMap, HashSet},
	sync::{Arc, Mutex},
};

use truapi::latest::{FundingDirection, FundingRail};

use futures::{channel::oneshot, lock::Mutex as AsyncMutex};
use parity_scale_codec::{Decode, Encode};

use super::{
	funding::{Signal, lock},
	product_manifest::worker_manifest,
	services::RuntimeServices,
};
use crate::{
	host_logic::{
		funding::FundingSessionError,
		funding_providers::{
			FUNDING_MIN_AMOUNT, FundingAmountLimits, FundingCandidate, FundingCountry,
			FundingNetworkLimits, FundingOptions, FundingProviderEntry, FundingProviderOrigin,
			FundingSelection, LearnedSupport, LimitScope, amount_limits, country_support,
		},
		worker_manifest::WorkerManifest,
	},
	platform::{CoreStorageKey, ProductId},
	unix_time::current_unix_millis,
};

/// A provider the host offers, with the Worker manifest it shipped already
/// read.
#[derive(Clone)]
struct OfferedProvider {
	product_id: ProductId,
	snapshot: Option<WorkerManifest>,
	origin: FundingProviderOrigin,
}

/// The host's funding providers and what each one serves.
#[derive(Default)]
pub struct FundingProviders {
	/// The providers, in the host's order.
	entries: Mutex<Vec<OfferedProvider>>,
	/// What dotNS last said each provider publishes: its Worker manifest, or
	/// `None` for nothing usable.
	live: Mutex<HashMap<ProductId, Option<WorkerManifest>>>,
	/// Providers whose manifest is being read.
	refreshing: Mutex<HashSet<ProductId>>,
	/// What providers' quote answers showed about what they serve, newest
	/// per ask, persisted under [`CoreStorageKey::FundingSupport`].
	learned: Mutex<Vec<LearnedSupport>>,
	/// Held across every load and write of the records, so writes land in
	/// order and none lands before the stored records are in; the flag
	/// records whether they are.
	learned_store: AsyncMutex<bool>,
	/// Quote lists waiting for the providers to change before their next
	/// round.
	changed: Mutex<Signal>,
}

impl FundingProviders {
	/// Resolves once the providers the core goes by next change: the host
	/// lists others, or dotNS answers for one with what it serves.
	pub fn on_change(&self) -> oneshot::Receiver<()> {
		lock(&self.changed).wait()
	}

	/// Every provider serving `direction`, in the host's order. What their
	/// recent quote answers showed is folded into what their manifests say.
	pub fn candidates(&self, direction: FundingDirection) -> Vec<FundingCandidate> {
		let learned = lock(&self.learned).clone();
		let now_ms = current_unix_millis();
		self.providers()
			.into_iter()
			.filter_map(|(provider_id, manifest)| {
				FundingCandidate::for_direction(
					&provider_id,
					manifest.as_ref(),
					&learned,
					direction,
					now_ms,
				)
			})
			.collect()
	}

	/// Every provider a quote is asked of and a session can be handed to:
	/// each whose manifest serves Funding, in the host's order, whatever routes
	/// it declares, since a provider can serve more or less than it last
	/// published. One with no manifest yet, or whose manifest was withdrawn,
	/// is neither asked nor chosen.
	pub fn quote_targets(&self) -> Vec<ProductId> {
		self.providers()
			.into_iter()
			.filter(|(_, manifest)| {
				manifest.as_ref().is_some_and(|manifest| manifest.funding.is_some())
			})
			.map(|(provider_id, _)| provider_id)
			.collect()
	}

	/// The host's providers in its order, each with the manifest the core
	/// goes by.
	fn providers(&self) -> Vec<(ProductId, Option<WorkerManifest>)> {
		let entries = lock(&self.entries).clone();
		entries
			.iter()
			.map(|entry| (entry.product_id.clone(), self.manifest(entry)))
			.collect()
	}

	/// Record what an answer showed, replacing what was known for the same
	/// ask and dropping what is no longer trusted.
	fn learn(&self, mut support: LearnedSupport) {
		let mut learned = lock(&self.learned);
		let now_ms = support.learned_at_ms;
		if let Some(known) =
			learned.iter().find(|known| known.is_fresh(now_ms) && known.same_ask(&support))
		{
			support.keep_limits_of(known);
		}
		learned.retain(|known| known.is_fresh(now_ms) && !known.same_ask(&support));
		learned.push(support);
	}

	/// The amounts a session can take in `scope`, across every provider a
	/// quote is asked of.
	fn amount_limits(&self, scope: LimitScope<'_>) -> FundingAmountLimits {
		let learned = lock(&self.learned).clone();
		amount_limits(&self.quote_targets(), &learned, scope, current_unix_millis())
	}

	/// What the providers' manifests and recent quote answers say about each
	/// country for `direction`, `rail` and `asset`.
	fn countries(
		&self,
		direction: FundingDirection,
		rail: FundingRail,
		asset: &str,
	) -> Vec<FundingCountry> {
		let learned = lock(&self.learned).clone();
		country_support(&self.providers(), &learned, direction, rail, asset, current_unix_millis())
	}

	/// Whether a session can be handed to `provider_id`: it is a quote target.
	pub fn is_quote_target(&self, provider_id: &ProductId) -> bool {
		self.quote_targets().contains(provider_id)
	}

	/// The manifest the core goes by for `entry`: the host's for a bundled
	/// provider; otherwise dotNS's answer once it has one, the host's
	/// snapshot until then.
	fn manifest(&self, entry: &OfferedProvider) -> Option<WorkerManifest> {
		if entry.origin == FundingProviderOrigin::Bundled {
			return entry.snapshot.clone();
		}
		match lock(&self.live).get(&entry.product_id) {
			Some(live) => live.clone(),
			None => entry.snapshot.clone(),
		}
	}
}

impl RuntimeServices {
	/// Replace the funding providers the host offers, and start reading their
	/// manifests. A shipped manifest that does not parse is refused, and
	/// nothing changes.
	pub fn set_funding_providers(
		self: &Arc<Self>,
		entries: Vec<FundingProviderEntry>,
	) -> Result<(), FundingSessionError> {
		let offered = offered_providers(entries)?;
		let providers = &self.funding_providers();
		let before = providers.providers();
		lock(&providers.live)
			.retain(|product_id, _| offered.iter().any(|entry| &entry.product_id == product_id));
		*lock(&providers.entries) = offered;
		if providers.providers() != before {
			self.funding_providers_changed();
		}
		self.refresh_funding_providers();
		Ok(())
	}

	/// Record what the user picked in session `intent`'s overlay, and send
	/// the host the options it leads to. A session whose overlay is not up
	/// offers nothing to pick, so its selection is ignored.
	pub fn select_funding_options(self: &Arc<Self>, intent: &str, selection: FundingSelection) {
		if self.funding().select(intent, selection) {
			self.send_funding_options(intent);
		}
	}

	/// Send the host what session `intent`'s overlay offers for the user's
	/// selection so far. Nothing is sent for a session the core does not
	/// hold.
	pub fn send_funding_options(&self, intent: &str) {
		let (Some(feed), Some(session)) = (self.funding().feed(), self.funding().get(intent))
		else {
			return;
		};
		let providers = self.funding_providers();
		let selection = self.funding().selection(intent);
		let candidates = providers.candidates(session.direction);
		let limits_on = |selection: &FundingSelection, network: Option<&str>| {
			providers.amount_limits(LimitScope {
				direction: session.direction,
				rail: selection.rail,
				asset: selection.asset.as_deref().unwrap_or_default(),
				network,
				country: selection.country.as_deref(),
			})
		};
		let (countries, limits, networks) = match &selection {
			None => (
				Vec::new(),
				FundingAmountLimits { min: Some(FUNDING_MIN_AMOUNT), max: None },
				Vec::new(),
			),
			Some(selection) => {
				let countries = selection
					.asset
					.as_deref()
					.map(|asset| providers.countries(session.direction, selection.rail, asset))
					.unwrap_or_default();
				let networks = match selection.rail {
					FundingRail::Crypto => declared_networks(
						&candidates,
						session.direction,
						selection.asset.as_deref(),
					)
					.into_iter()
					.map(|(network, assets)| {
						let limits = assets
							.iter()
							.map(|asset| {
								providers.amount_limits(LimitScope {
									direction: session.direction,
									rail: FundingRail::Crypto,
									asset,
									network: Some(&network),
									country: selection.country.as_deref(),
								})
							})
							.reduce(FundingAmountLimits::widest)
							.unwrap_or(FundingAmountLimits {
								min: Some(FUNDING_MIN_AMOUNT),
								max: None,
							});
						FundingNetworkLimits { network, limits }
					})
					.collect(),
					_ => Vec::new(),
				};
				(countries, limits_on(selection, selection.network.as_deref()), networks)
			},
		};
		feed.funding_options_changed(
			intent.to_string(),
			FundingOptions { candidates, countries, limits, networks },
		);
	}

	/// Read every host provider's manifest that is not already being read. A
	/// read that fails keeps what the core had, since it says nothing about
	/// the provider.
	pub fn refresh_funding_providers(self: &Arc<Self>) {
		let services = self.clone();
		(self.spawner)(Box::pin(async move {
			let mut loaded = services.funding_providers().learned_store.lock().await;
			services.load_learned_support(&mut loaded).await;
		}));
		let entries = lock(&self.funding_providers().entries).clone();
		for entry in entries
			.into_iter()
			.filter(|entry| entry.origin == FundingProviderOrigin::Published)
		{
			if !lock(&self.funding_providers().refreshing).insert(entry.product_id.clone()) {
				continue;
			}
			let services = self.clone();
			(self.spawner)(Box::pin(async move {
				services.read_provider_manifest(&entry.product_id).await;
			}));
		}
	}

	/// Record what a provider's answer showed about what it serves, and
	/// persist the records once the stored ones are loaded, so a write never
	/// replaces records it has not read.
	pub fn learn_funding_support(self: &Arc<Self>, support: LearnedSupport) {
		self.funding_providers().learn(support);
		let services = self.clone();
		(self.spawner)(Box::pin(async move {
			// Each write takes the records as they are once it holds the
			// lock, so the last one to land is the newest.
			let mut loaded = services.funding_providers().learned_store.lock().await;
			if !services.load_learned_support(&mut loaded).await {
				return;
			}
			let records = lock(&services.funding_providers().learned).clone();
			if let Err(error) = services
				.platform
				.write_core_storage(CoreStorageKey::FundingSupport, records.encode())
				.await
			{
				tracing::warn!(reason = %error.reason, "storing what funding providers serve failed");
			}
		}));
	}

	/// Load what earlier answers showed unless `loaded` says it is in,
	/// keeping what is still trusted and anything learned since. Returns
	/// whether the records are loaded; a read that fails is tried again on
	/// the next call.
	async fn load_learned_support(&self, loaded: &mut bool) -> bool {
		if *loaded {
			return true;
		}
		let stored = match self.platform.read_core_storage(CoreStorageKey::FundingSupport).await {
			Ok(Some(bytes)) => {
				Vec::<LearnedSupport>::decode(&mut bytes.as_slice()).unwrap_or_else(|error| {
					tracing::warn!(%error, "dropping undecodable funding provider records");
					Vec::new()
				})
			},
			Ok(None) => Vec::new(),
			Err(error) => {
				tracing::warn!(reason = %error.reason, "reading what funding providers serve failed");
				return false;
			},
		};
		let now_ms = current_unix_millis();
		let mut learned = lock(&self.funding_providers().learned);
		for support in stored.into_iter().filter(|support| support.is_fresh(now_ms)) {
			if learned.iter().all(|known| !known.same_ask(&support)) {
				learned.push(support);
			}
		}
		*loaded = true;
		true
	}

	/// Read `product_id`'s Worker manifest into what the core goes by.
	async fn read_provider_manifest(&self, product_id: &ProductId) {
		let read = worker_manifest(self, self.platform.as_ref(), product_id).await;
		let providers = &self.funding_providers();
		if let Ok(manifest) = read {
			let before = providers.providers();
			lock(&providers.live).insert(product_id.clone(), manifest);
			if providers.providers() != before {
				self.funding_providers_changed();
			}
		}
		lock(&providers.refreshing).remove(product_id);
	}

	/// The providers the core goes by changed, often because dotNS answered
	/// for one the host lists without a manifest: every open overlay is sent
	/// its options again, and every watched quote list asks again at once,
	/// so a provider read after the overlay opened still shows and quotes.
	fn funding_providers_changed(&self) {
		lock(&self.funding_providers().changed).fire();
		for intent in self.funding().presenting() {
			self.send_funding_options(&intent);
		}
	}
}

/// The host's `entries` with their shipped manifests read, in its order,
/// each product once.
fn offered_providers(
	entries: Vec<FundingProviderEntry>,
) -> Result<Vec<OfferedProvider>, FundingSessionError> {
	let mut offered: Vec<OfferedProvider> = Vec::with_capacity(entries.len());
	for entry in entries {
		let product_id = entry.product_id;
		let snapshot = entry
			.worker_manifest
			.as_deref()
			.map(WorkerManifest::parse)
			.transpose()
			.map_err(|reason| FundingSessionError::InvalidProvider {
				reason: format!("{product_id}: {reason}"),
			})?;
		if entry.origin == FundingProviderOrigin::Bundled && snapshot.is_none() {
			return Err(FundingSessionError::InvalidProvider {
				reason: format!("{product_id}: a bundled provider needs its manifest"),
			});
		}
		if offered.iter().all(|kept| kept.product_id != product_id) {
			offered.push(OfferedProvider { product_id, snapshot, origin: entry.origin });
		}
	}
	Ok(offered)
}

/// The networks `candidates`' crypto routes declare in `direction`, each
/// once in the order first declared, with the assets declared on it: only
/// `asset` when one is picked, otherwise every one.
fn declared_networks(
	candidates: &[FundingCandidate],
	direction: FundingDirection,
	asset: Option<&str>,
) -> Vec<(String, Vec<String>)> {
	let mut networks: Vec<(String, Vec<String>)> = Vec::new();
	let routes = candidates
		.iter()
		.flat_map(|candidate| &candidate.routes)
		.filter(|route| route.rail == FundingRail::Crypto && route.directions.contains(&direction));
	for route in routes {
		let assets = route
			.assets
			.iter()
			.filter(|declared| asset.is_none_or(|asset| *declared == asset));
		for network in route.networks.iter().flatten() {
			let index = match networks.iter().position(|(known, _)| known == network) {
				Some(index) => index,
				None => {
					networks.push((network.clone(), Vec::new()));
					networks.len() - 1
				},
			};
			for declared in assets.clone() {
				if !networks[index].1.contains(declared) {
					networks[index].1.push(declared.clone());
				}
			}
		}
	}
	networks.retain(|(_, assets)| !assets.is_empty());
	networks
}

#[cfg(test)]
mod tests {
	use super::*;

	use truapi::latest::FundingRail;

	use crate::host_logic::worker_manifest::FundingRoute;

	// The network picker lists each network a crypto route declares once, in
	// the order the providers declare them: for the picked asset, or for any
	// asset before one is picked.
	#[test]
	fn declared_networks_are_listed_once_in_order() {
		let route = |rail, assets: &[&str], networks: Option<&[&str]>| FundingRoute {
			rail,
			directions: vec![FundingDirection::In],
			assets: assets.iter().map(|asset| asset.to_string()).collect(),
			networks: networks.map(|n| n.iter().map(|network| network.to_string()).collect()),
			countries: None,
		};
		let candidate = |routes| FundingCandidate {
			provider_id: ProductId::for_test("ramp.dot"),
			routes,
			unsupported: Vec::new(),
			limits: Vec::new(),
		};
		let candidates = [
			candidate(vec![
				route(FundingRail::Crypto, &["USDT"], Some(&["polkadot", "ethereum"])),
				route(FundingRail::Crypto, &["USDC"], Some(&["base"])),
				route(FundingRail::Card, &["USDT"], None),
			]),
			candidate(vec![route(FundingRail::Crypto, &["USDT"], Some(&["ethereum", "tron"]))]),
		];

		let usdt = |networks: &[&str]| -> Vec<(String, Vec<String>)> {
			networks
				.iter()
				.map(|network| (network.to_string(), vec!["USDT".to_string()]))
				.collect()
		};
		assert_eq!(
			(
				declared_networks(&candidates, FundingDirection::In, Some("USDT")),
				declared_networks(&candidates, FundingDirection::In, None)
					.into_iter()
					.map(|(network, _)| network)
					.collect::<Vec<_>>(),
			),
			(
				usdt(&["polkadot", "ethereum", "tron"]),
				vec![
					"polkadot".to_string(),
					"ethereum".to_string(),
					"base".to_string(),
					"tron".to_string()
				]
			)
		);
	}

	fn manifest(routes: &str) -> String {
		format!(
			r#"{{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"index.js","includes":{{"funding":{{"routes":[{routes}]}}}}}}"#
		)
	}

	const CARD_IN: &str = r#"{"mode":"CARD","directions":["In"],"assets":["EUR"]}"#;
	const CRYPTO_OUT: &str = r#"{"mode":"CRYPTO","directions":["Out"],"assets":["USDT"]}"#;

	fn providers(entries: &[(&str, Option<String>)]) -> FundingProviders {
		let providers = FundingProviders::default();
		*lock(&providers.entries) = offered_providers(
			entries
				.iter()
				.map(|(product_id, worker_manifest)| FundingProviderEntry {
					product_id: ProductId::for_test(product_id),
					worker_manifest: worker_manifest.clone(),
					origin: FundingProviderOrigin::Published,
				})
				.collect(),
		)
		.expect("offered");
		providers
	}

	fn ids(candidates: Vec<FundingCandidate>) -> Vec<ProductId> {
		candidates.into_iter().map(|candidate| candidate.provider_id).collect()
	}

	// The list renders from what the host shipped before any chain read, in
	// the host's order, with only the routes that serve the direction.
	#[test]
	fn candidates_come_from_the_snapshot_until_dotns_answers() {
		let providers = providers(&[
			("card.dot", Some(manifest(CARD_IN))),
			("both.dot", Some(manifest(&format!("{CARD_IN},{CRYPTO_OUT}")))),
			("unshipped.dot", None),
		]);

		let outbound = providers.candidates(FundingDirection::Out);

		assert_eq!(
			(
				ids(providers.candidates(FundingDirection::In)),
				outbound.iter().map(|candidate| candidate.routes.clone()).collect::<Vec<_>>(),
			),
			(
				vec![ProductId::for_test("card.dot"), ProductId::for_test("both.dot")],
				vec![vec![FundingRoute {
					rail: FundingRail::Crypto,
					directions: vec![FundingDirection::Out],
					assets: vec!["USDT".to_string()],
					networks: None,
					countries: None,
				}]],
			)
		);
	}

	// A provider the host ships in the app is never published, so dotNS
	// answering "nothing" must not drop it the way it drops a listed one.
	#[test]
	fn a_bundled_provider_keeps_the_hosts_manifest() {
		let providers = providers(&[("listed.dot", Some(manifest(CARD_IN)))]);
		lock(&providers.entries).extend(
			offered_providers(vec![FundingProviderEntry {
				product_id: ProductId::for_test("bundled.dot"),
				worker_manifest: Some(manifest(CARD_IN)),
				origin: FundingProviderOrigin::Bundled,
			}])
			.expect("offered"),
		);
		lock(&providers.live).extend([
			(ProductId::for_test("listed.dot"), None),
			(ProductId::for_test("bundled.dot"), None),
		]);

		assert_eq!(
			ids(providers.candidates(FundingDirection::In)),
			vec![ProductId::for_test("bundled.dot")]
		);
	}

	// A snapshot the core cannot read is the host's mistake, so it is refused
	// when offered rather than leaving the provider silently missing.
	#[test]
	fn an_unreadable_snapshot_is_refused_when_offered() {
		let offered = |worker_manifest: &str| {
			offered_providers(vec![FundingProviderEntry {
				product_id: ProductId::for_test("card.dot"),
				worker_manifest: Some(worker_manifest.to_string()),
				origin: FundingProviderOrigin::Published,
			}])
			.map(|offered| offered.len())
			.map_err(|error| matches!(error, FundingSessionError::InvalidProvider { .. }))
		};

		assert_eq!((offered(&manifest(CARD_IN)), offered("{")), (Ok(1), Err(true)));
	}

	// A stale snapshot is corrected by what the provider publishes now,
	// including when it stopped serving Funding.
	#[test]
	fn dotns_answer_wins_over_the_snapshot() {
		let providers = providers(&[
			("withdrawn.dot", Some(manifest(CARD_IN))),
			("changed.dot", Some(manifest(CARD_IN))),
			("unshipped.dot", None),
		]);
		let live = |routes: &str| WorkerManifest::parse(&manifest(routes)).ok();
		lock(&providers.live).extend([
			(ProductId::for_test("withdrawn.dot"), None),
			(ProductId::for_test("changed.dot"), live(CRYPTO_OUT)),
			(ProductId::for_test("unshipped.dot"), live(CARD_IN)),
		]);

		assert_eq!(
			(
				ids(providers.candidates(FundingDirection::In)),
				ids(providers.candidates(FundingDirection::Out)),
			),
			(vec![ProductId::for_test("unshipped.dot")], vec![ProductId::for_test("changed.dot")])
		);
	}

	// A limit stands on its own clock: a later quote does not renew it, and a
	// quote at an amount below a known minimum shows that minimum is gone.
	#[test]
	fn a_limit_ages_on_its_own_and_a_lower_quote_clears_it() {
		use crate::host_logic::funding_providers::{LEARNED_SUPPORT_TTL_MS, LearnedLimit};
		let ask = |asked, min: Option<u128>, at_ms| LearnedSupport {
			provider_id: ProductId::for_test("ramp.dot"),
			direction: FundingDirection::In,
			rail: truapi::latest::FundingRail::Card,
			asset: "EUR".to_string(),
			network: None,
			country: None,
			asked,
			supported: true,
			min: min.map(|amount| LearnedLimit { amount, learned_at_ms: at_ms }),
			max: None,
			learned_at_ms: at_ms,
		};
		let min_after = |answers: &[LearnedSupport]| {
			let providers = FundingProviders::default();
			for answer in answers {
				providers.learn(answer.clone());
			}
			lock(&providers.learned)
				.iter()
				.map(|known| known.min.map(|min| min.amount))
				.collect::<Vec<_>>()
		};
		let later = LEARNED_SUPPORT_TTL_MS - 1;

		assert_eq!(
			(
				min_after(&[ask(5, Some(10), 1_000), ask(20, None, 2_000)]),
				min_after(&[ask(5, Some(10), 1_000), ask(8, None, 2_000)]),
				min_after(&[ask(5, Some(10), 1_000), ask(20, None, 1_000 + later + 2)]),
			),
			(vec![Some(10)], vec![None], vec![None])
		);
	}

	// Quotes and selection cover the same providers: each whose manifest
	// serves Funding, whatever its routes say, and none still waiting for its
	// manifest or whose manifest was withdrawn.
	#[test]
	fn quotes_go_to_the_providers_a_session_can_be_handed_to() {
		let providers = providers(&[
			("card.dot", Some(manifest(CARD_IN))),
			("unshipped.dot", None),
			("withdrawn.dot", Some(manifest(CARD_IN))),
		]);
		lock(&providers.live).insert(ProductId::for_test("withdrawn.dot"), None);

		assert_eq!(
			(
				providers.quote_targets(),
				["card.dot", "unshipped.dot", "withdrawn.dot"]
					.map(|provider_id| providers.is_quote_target(&ProductId::for_test(provider_id))),
			),
			(vec![ProductId::for_test("card.dot")], [true, false, false])
		);
	}
}
