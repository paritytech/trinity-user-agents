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

use std::sync::{Arc, Mutex};

use truapi::latest::{FundingDirection, FundingRail};

use futures::{channel::mpsc, lock::Mutex as AsyncMutex};
use parity_scale_codec::{Decode, Encode};
use tracing::warn;

use super::{funding::lock, product_manifest::worker_manifest, services::RuntimeServices};
use crate::{
	host_logic::{
		funding::FundingSessionError,
		funding_providers::{
			FundingAmountLimits, FundingCandidate, FundingCountry, FundingNetworkLimits,
			FundingOptions, FundingProviderEntry, FundingProviderOrigin, FundingSelection,
			LearnedSupport, LimitScope, amount_limits, country_support, provider_config,
		},
		funding_view,
		worker_manifest::WorkerManifest,
	},
	platform::{CoreStorageKey, ProductId},
	unix_time::current_unix_millis,
	wake::WakeSignal,
};

/// A provider the host offers, with the Worker manifest it shipped already
/// read and what dotNS says it publishes.
#[derive(Clone)]
struct OfferedProvider {
	product_id: ProductId,
	snapshot: Option<WorkerManifest>,
	origin: FundingProviderOrigin,
	/// Whether it ships an App, when the host said.
	has_app: Option<bool>,
	/// What dotNS last said the provider publishes: its Worker manifest, or
	/// `None` for nothing usable. `None` until dotNS has answered.
	live: Option<Option<WorkerManifest>>,
	/// Whether its manifest is being read.
	reading: bool,
}

impl OfferedProvider {
	/// The manifest the core goes by: the host's for a bundled provider;
	/// otherwise dotNS's answer once it has one, the host's snapshot until
	/// then.
	fn manifest(&self) -> Option<WorkerManifest> {
		match (&self.live, self.origin) {
			(Some(live), FundingProviderOrigin::Published) => live.clone(),
			_ => self.snapshot.clone(),
		}
	}
}

/// The host's funding providers and what each one serves.
#[derive(Default)]
pub struct FundingProviders {
	/// The providers, in the host's order, each once.
	entries: Mutex<Vec<OfferedProvider>>,
	/// What providers' quote answers showed about what they serve, newest
	/// per ask, persisted under [`CoreStorageKey::FundingSupport`].
	learned: Mutex<Vec<LearnedSupport>>,
	/// Held across every load and write of the records, so writes land in
	/// order and none lands before the stored records are in; the flag
	/// records whether they are.
	learned_store: AsyncMutex<bool>,
	/// Quote lists waiting for the providers to change before their next
	/// round.
	changed: WakeSignal,
	/// The providers the host's configuration last listed, so a refresh
	/// that lists the same ones changes nothing.
	configured: Mutex<Option<Vec<FundingProviderEntry>>>,
}

impl FundingProviders {
	/// Resolves once the providers the core goes by next change: the host
	/// lists others, or dotNS answers for one with what it serves.
	pub fn on_change(&self) -> mpsc::Receiver<()> {
		self.changed.subscribe()
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

	/// The quote targets asked to price an ask in `direction` on `rail`: those
	/// whose routes, as declared and as their answers confirmed, carry the
	/// rail, or that declare none, in the host's order.
	pub fn quote_targets_for(
		&self,
		direction: FundingDirection,
		rail: FundingRail,
	) -> Vec<ProductId> {
		let candidates = self.candidates(direction);
		self.quote_targets()
			.into_iter()
			.filter(|provider_id| {
				candidates.iter().any(|candidate| {
					candidate.provider_id == *provider_id &&
						funding_view::routes_rail(candidate, rail)
				})
			})
			.collect()
	}

	/// The host's providers in its order, each with the manifest the core
	/// goes by.
	fn providers(&self) -> Vec<(ProductId, Option<WorkerManifest>)> {
		manifests(&lock(&self.entries))
	}

	/// Make `offered` the providers, keeping what dotNS said of each one
	/// listed before. Returns whether the providers the core goes by changed.
	fn offer(&self, mut offered: Vec<OfferedProvider>) -> bool {
		let mut entries = lock(&self.entries);
		let before = manifests(&entries);
		for entry in &mut offered {
			if let Some(known) = entries.iter().find(|known| known.product_id == entry.product_id) {
				entry.live = known.live.clone();
				entry.reading = known.reading;
			}
		}
		*entries = offered;
		manifests(&entries) != before
	}

	/// Keep `manifest` as what dotNS says `product_id` publishes. A provider
	/// the host no longer lists is left out. Returns whether the providers
	/// the core goes by changed.
	fn answered(&self, product_id: &ProductId, manifest: Option<WorkerManifest>) -> bool {
		let mut entries = lock(&self.entries);
		let Some(entry) = entries.iter_mut().find(|entry| entry.product_id == *product_id) else {
			return false;
		};
		let before = entry.manifest();
		entry.live = Some(manifest);
		entry.manifest() != before
	}

	/// Mark every published provider whose manifest is not being read as
	/// being read, and return them.
	fn start_reads(&self) -> Vec<ProductId> {
		lock(&self.entries)
			.iter_mut()
			.filter(|entry| entry.origin == FundingProviderOrigin::Published && !entry.reading)
			.map(|entry| {
				entry.reading = true;
				entry.product_id.clone()
			})
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

	/// Whether `provider_id` ships an App, when the host said so.
	pub fn has_app(&self, provider_id: &ProductId) -> Option<bool> {
		lock(&self.entries)
			.iter()
			.find(|entry| entry.product_id == *provider_id)
			.and_then(|entry| entry.has_app)
	}

	/// Whether a session can be handed to `provider_id`: it is a quote target.
	pub fn is_quote_target(&self, provider_id: &ProductId) -> bool {
		self.quote_targets().contains(provider_id)
	}
}

/// Each of `entries` with the manifest the core goes by, in order.
fn manifests(entries: &[OfferedProvider]) -> Vec<(ProductId, Option<WorkerManifest>)> {
	entries
		.iter()
		.map(|entry| (entry.product_id.clone(), entry.manifest()))
		.collect()
}

/// A provider's manifest read in flight. Dropping it, as the read ends or
/// its task is dropped, lets the next refresh read the manifest again.
struct ManifestRead {
	services: Arc<RuntimeServices>,
	product_id: ProductId,
}

impl Drop for ManifestRead {
	fn drop(&mut self) {
		let mut entries = lock(&self.services.funding_providers().entries);
		if let Some(entry) = entries.iter_mut().find(|entry| entry.product_id == self.product_id) {
			entry.reading = false;
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
		if self.funding_providers().offer(offered) {
			self.funding_providers_changed();
		}
		self.refresh_funding_providers();
		Ok(())
	}

	/// Offer the providers the host's remote configuration lists by name,
	/// then the `bundled` ones it does not name. A name that is not a
	/// product id is skipped and logged. A call listing what the last one
	/// did changes nothing, so the host can call it on every refresh.
	pub fn set_funding_provider_config(
		self: &Arc<Self>,
		published_names: Vec<String>,
		bundled: Vec<FundingProviderEntry>,
	) -> Result<(), FundingSessionError> {
		let (entries, invalid) = provider_config(&published_names, bundled);
		for name in invalid {
			warn!(%name, "funding provider configuration names no product");
		}
		let mut configured = lock(&self.funding_providers().configured);
		if configured.as_ref() == Some(&entries) {
			return Ok(());
		}
		self.set_funding_providers(entries.clone())?;
		*configured = Some(entries);
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
		let limits_on = |selection: &FundingSelection, asset: &str, network: Option<&str>| {
			providers.amount_limits(LimitScope {
				direction: session.direction,
				rail: selection.rail,
				asset,
				network,
				country: selection.country.as_deref(),
			})
		};
		// The asset the overlay works in: the one the user picked, else the
		// one the core resolves for the rail, so a selection without an asset
		// already gets that asset's countries and limits.
		let asset = selection.as_ref().and_then(|selection| match selection.rail {
			FundingRail::Crypto => selection.asset.clone(),
			rail => Some(selection.asset.clone().unwrap_or_else(|| {
				funding_view::fiat_asset(&candidates, rail, selection.currency.as_deref())
			})),
		});
		let (countries, limits, networks) = match &selection {
			None => (Vec::new(), FundingAmountLimits::ANY, Vec::new()),
			Some(selection) => {
				let countries = asset
					.as_deref()
					.map(|asset| providers.countries(session.direction, selection.rail, asset))
					.unwrap_or_default();
				let networks = match selection.rail {
					FundingRail::Crypto => {
						declared_networks(&candidates, session.direction, asset.as_deref())
							.into_iter()
							.map(|(network, assets)| {
								let limits = assets
									.iter()
									.map(|asset| limits_on(selection, asset, Some(&network)))
									.reduce(FundingAmountLimits::widest)
									.unwrap_or(FundingAmountLimits::ANY);
								FundingNetworkLimits {
									display_name: funding_view::network_name(&network),
									network,
									limits,
								}
							})
							.collect()
					},
					_ => Vec::new(),
				};
				let limits = limits_on(
					selection,
					asset.as_deref().unwrap_or_default(),
					selection.network.as_deref(),
				);
				(countries, limits, networks)
			},
		};
		let rails = funding_view::routed_rails(&candidates);
		let default_rail = funding_view::default_rail(&rails);
		let tokens = funding_view::crypto_tokens(
			&candidates,
			selection.as_ref().and_then(|selection| selection.network.as_deref()),
		);
		if self.funding_country_due() {
			self.detect_country_for_options(intent);
		}
		feed.funding_options_changed(
			intent.to_string(),
			FundingOptions {
				candidates,
				countries,
				limits,
				networks,
				rails,
				default_rail,
				asset,
				tokens,
				detected_country: self.recent_funding_country(),
			},
		);
	}

	/// Ask the host for the user's country in the background, and send
	/// session `intent` its options again once it answered one.
	fn detect_country_for_options(&self, intent: &str) {
		let Some(services) = self.funding().services() else {
			return;
		};
		let intent = intent.to_string();
		(self.spawner)(Box::pin(async move {
			if services.detected_funding_country().await.is_some() {
				services.send_funding_options(&intent);
			}
		}));
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
		for product_id in self.funding_providers().start_reads() {
			let read = ManifestRead { services: self.clone(), product_id };
			(self.spawner)(Box::pin(async move {
				read.services.read_provider_manifest(&read.product_id).await;
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
		if let Ok(manifest) = read &&
			self.funding_providers().answered(product_id, manifest)
		{
			self.funding_providers_changed();
		}
	}

	/// The providers the core goes by changed, often because dotNS answered
	/// for one the host lists without a manifest: every open overlay is sent
	/// its options again, and every watched quote list asks again at once,
	/// so a provider read after the overlay opened still shows and quotes.
	fn funding_providers_changed(&self) {
		self.funding_providers().changed.wake();
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
			offered.push(OfferedProvider {
				product_id,
				snapshot,
				origin: entry.origin,
				has_app: entry.has_app,
				live: None,
				reading: false,
			});
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
mod tests;
