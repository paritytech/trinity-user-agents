//! Product manifest resolution over dotNS.
//!
//! Resolves a product id to the JSON its base name publishes at the `manifest`
//! text record, and its worker subname at the `executable` record, following
//! [RFC — Product Manifest Format][manifest]: derive the node under the
//! network's own TLD, find the resolver through the registry, read the record.
//! Parsing that JSON is [`crate::host_internal::product_manifest`]'s and
//! [`crate::host_logic::worker_manifest`]'s job.
//!
//! [manifest]: ../../../../docs/rfcs/product-manifest.md

use core::{future::Future, time::Duration};

use crate::platform::{
	CoreStorageKey, PermissionAuthorizationStatus, Platform, ProductId, StackStorage,
};
use parity_scale_codec::{Decode, Encode};
use tracing::{info, instrument, warn};
use truapi::v01;

use crate::{
	chain::EvmContractError,
	chain_runtime::ChainRuntime,
	dotns_views::{network_tld, protocol_component, protocol_registry, read_view, tld_node},
	host_internal::{
		permissions::authorization_status,
		product_manifest::{Granted, ProductBrand, RootManifest},
		sso_messages::RingVrfError,
	},
	host_logic::{
		dotns_gateway::{
			ContentResolver, DotnsCallError, DotnsError, DotnsRegistry, DotnsTransport,
			namehash_under,
		},
		worker_manifest::{WorkerManifest, check_app_manifest},
	},
	runtime::{
		dotns_lookup::{DotnsLookup, DotnsLookupKind},
		services::RuntimeServices,
	},
	unix_time::current_unix_secs,
};

/// Text record a base name publishes its root manifest at.
const MANIFEST_RECORD_KEY: &str = "manifest";
/// Text record an executable subname publishes its manifest at.
const EXECUTABLE_RECORD_KEY: &str = "executable";
/// Subname label a product's Worker executable is published under.
const WORKER_SUBNAME: &str = "worker";
/// Subname label a product's App executable is published under.
const APP_SUBNAME: &str = "app";

/// Reads `product_id`'s root manifest JSON.
///
/// `Ok(None)` means the product does not exist as far as dotNS is concerned:
/// either the node has no resolver, or its resolver holds no manifest record.
/// The two are one answer because a caller cannot act on the difference.
///
/// The TLD the identifier carries is discarded and the node re-derived under
/// the TLD the network reports. A product id is minted on one network but
/// [`DOTNS_TLDS`][tlds] spans them all, so `dim2.dot` reaching a `.paseo`
/// deployment has to resolve there rather than hash a name no registry holds.
///
/// [tlds]: crate::platform::DOTNS_TLDS
#[instrument(skip_all, fields(runtime.method = "product_manifest.fetch"))]
async fn fetch_root_manifest(
	chain: &ChainRuntime,
	asset_hub_chain_genesis_hash: [u8; 32],
	product_id: &ProductId,
) -> Result<Option<String>, DotnsError> {
	fetch_text_record(
		chain,
		asset_hub_chain_genesis_hash,
		DotnsLookupKind::Manifest(product_id),
		product_id,
		None,
		MANIFEST_RECORD_KEY,
	)
	.await
}

/// Reads the JSON `product_id`'s `worker.<product_id>.<tld>` subname publishes
/// at its `executable` record. `Ok(None)` means the product publishes no
/// Worker, as [`fetch_root_manifest`] reads an absent root manifest.
#[instrument(skip_all, fields(runtime.method = "product_manifest.fetch_worker"))]
async fn fetch_worker_manifest(
	chain: &ChainRuntime,
	asset_hub_chain_genesis_hash: [u8; 32],
	product_id: &ProductId,
) -> Result<Option<String>, DotnsError> {
	fetch_text_record(
		chain,
		asset_hub_chain_genesis_hash,
		DotnsLookupKind::WorkerManifest(product_id),
		product_id,
		Some(WORKER_SUBNAME),
		EXECUTABLE_RECORD_KEY,
	)
	.await
}

/// Reads the JSON `product_id`'s `app.<product_id>.<tld>` subname publishes
/// at its `executable` record, as [`fetch_worker_manifest`] reads the Worker's.
#[instrument(skip_all, fields(runtime.method = "product_manifest.fetch_app"))]
async fn fetch_app_manifest(
	chain: &ChainRuntime,
	asset_hub_chain_genesis_hash: [u8; 32],
	product_id: &ProductId,
) -> Result<Option<String>, DotnsError> {
	fetch_text_record(
		chain,
		asset_hub_chain_genesis_hash,
		DotnsLookupKind::AppManifest(product_id),
		product_id,
		Some(APP_SUBNAME),
		EXECUTABLE_RECORD_KEY,
	)
	.await
}

/// The node of `product_id`'s base name under `tld`, or of its `subname`.
fn record_node(tld: &str, product_id: &ProductId, subname: Option<&str>) -> [u8; 32] {
	let base = namehash_under(&tld_node(tld), product_id.bare_label());
	subname.map_or(base, |label| namehash_under(&base, label))
}

/// Reads text record `key` on `product_id`'s base name, or on its `subname`
/// when one is given. `kind` names the lookup in its follow id.
async fn fetch_text_record(
	chain: &ChainRuntime,
	asset_hub_chain_genesis_hash: [u8; 32],
	kind: DotnsLookupKind<'_>,
	product_id: &ProductId,
	subname: Option<&str>,
	key: &str,
) -> Result<Option<String>, DotnsError> {
	let mut lookup =
		DotnsLookup::pinned_to_best_block(chain, asset_hub_chain_genesis_hash, kind).await?;

	read_text_record(&mut lookup, product_id, subname, key).await
}

async fn read_text_record<T: DotnsTransport + ?Sized>(
	transport: &mut T,
	product_id: &ProductId,
	subname: Option<&str>,
	key: &str,
) -> Result<Option<String>, DotnsError> {
	let Some(protocol_registry) = protocol_registry(transport).await? else {
		return Ok(None);
	};

	let tld = network_tld(transport, &protocol_registry).await?;
	let node = record_node(&tld, product_id, subname);

	let registry = protocol_component(transport, &protocol_registry, "registry").await?;
	let resolver =
		read_view(transport, &registry, DotnsRegistry::resolverCall { node: node.into() })
			.await
			.map_err(DotnsError::Resolver)?;
	if resolver.is_zero() {
		return Ok(None);
	}

	let manifest = match read_view(
		transport,
		&resolver.into_array(),
		ContentResolver::textCall { node: node.into(), key: key.to_string() },
	)
	.await
	{
		Ok(manifest) => manifest,
		// The dotNS-issued default resolver does not implement `text`, which is
		// the same outcome as an unpublished manifest.
		Err(DotnsCallError::View(EvmContractError::Reverted { .. })) => return Ok(None),
		Err(error) => return Err(DotnsError::TextRecord(error)),
	};
	if manifest.is_empty() {
		return Ok(None);
	}
	Ok(Some(manifest))
}

#[cfg(test)]
mod tests;

/// How long a cached root manifest is honoured.
///
/// This is a revocation bound, not a performance knob: dotNS attaches no signal
/// to a record edit, so a grant a publisher withdraws stays in force until the
/// manifest is read again.
pub const MANIFEST_TTL_SECS: u64 = 24 * 60 * 60;

/// A cached root manifest lookup and when it was made.
///
/// The document is stored verbatim rather than reduced to the grants this core
/// reads today, so a later consumer needs no cache migration.
#[derive(Clone, Encode, Decode)]
pub struct CachedManifest {
	/// The [`Stack::generation`](crate::platform::Stack::generation) the
	/// lookup was made in. The core skips an entry from another generation,
	/// because the chain it was read from no longer exists.
	pub generation: String,
	/// Seconds since the Unix epoch at which the lookup was made.
	pub fetched_at_secs: u64,
	/// The manifest JSON exactly as published, or `None` where the chain
	/// answered that the product publishes none.
	///
	/// A miss is cached because refusing is the common outcome: without it every
	/// refused call reopens a chainHead follow and re-reads the contracts, and
	/// the round trip tells the caller which targets have a manifest and which
	/// do not — the distinction one uniform refusal exists to hide.
	pub json: Option<String>,
}

/// Encode a root manifest the way the core caches it, for a host that seeds a
/// grant instead of resolving one.
///
/// Write the bytes under [`CoreStorageKey::product_manifest`] for the product the
/// manifest belongs to, in the stack the host runs. `generation` is that
/// stack's [`Stack::generation`](crate::platform::Stack::generation); the core
/// ignores an entry stamped with any other. The core reads that entry before it consults the Asset
/// Hub, so a seeded manifest answers a grant on a host with no dotNS access at
/// all — which is what makes a cross-product flow reachable locally, before
/// either product is deployed.
///
/// `json` is `None` for a product that publishes no manifest, the outcome the
/// core caches for the same lifetime as a document. `fetched_at_secs` is when
/// the lookup counts as having happened: the current time for a live entry, or
/// something older than the cache lifetime to exercise a grant expiring.
///
/// A development and testing seam. Nothing enforces that a seeded manifest
/// matches what the product actually publishes, so a host offering this owes
/// the developer a way to tell the two apart.
///
/// A Worker manifest is cached the same way, under
/// [`worker_manifest_cache_key`].
pub fn encode_cached_root_manifest(
	json: Option<&str>,
	fetched_at_secs: u64,
	generation: &str,
) -> Vec<u8> {
	CachedManifest {
		generation: generation.to_string(),
		fetched_at_secs,
		json: json.map(str::to_string),
	}
	.encode()
}

/// Why a grant lookup did not admit the caller.
///
/// The wire answers one refusal for every reason, deliberately. This is the
/// operator's copy: without it "the publisher granted nothing" is reported for
/// a user's explicit denial, an unreadable keychain and a manifest that failed
/// to parse alike, and whoever debugs it goes to fix a manifest that is already
/// correct.
#[derive(Clone, Copy)]
enum RefusedBecause {
	/// The published manifest does not name this caller for this scope.
	NotGranted,
	/// The user has already refused this pair.
	UserDenied,
	/// The stored decision could not be read, so the lookup failed closed.
	DecisionUnreadable,
	/// A manifest was fetched but did not parse.
	ManifestMalformed,
	/// No manifest could be resolved at all.
	ManifestUnavailable,
	/// The proof context names a product on another network from the owner.
	ContextOffNetwork,
}

impl RefusedBecause {
	fn as_str(&self) -> &'static str {
		match self {
			Self::NotGranted => "not granted",
			Self::UserDenied => "user denied",
			Self::DecisionUnreadable => "stored decision unreadable",
			Self::ManifestMalformed => "manifest malformed",
			Self::ManifestUnavailable => "manifest unavailable",
			Self::ContextOffNetwork => "context on another network",
		}
	}
}

/// Where a grant lookup got the manifest it judged by.
///
/// A cached copy is up to [`MANIFEST_TTL_SECS`] old, and a cached miss refuses
/// for that long, so two devices can judge one grant differently by where each
/// got its manifest.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum ManifestSource {
	/// Read from the cache on this device, written `age_secs` ago.
	#[display("cache, {age_secs} s old")]
	Cached { age_secs: u64 },
	/// Read from dotNS by this lookup.
	#[display("dotNS")]
	Fetched,
	/// The host names no Asset Hub to resolve against.
	#[display("no Asset Hub")]
	NoAssetHub,
	/// The dotNS lookup failed. Nothing was cached.
	#[display("dotNS lookup failed: {reason}")]
	LookupFailed { reason: String },
	/// The lookup ran past [`MANIFEST_RESOLUTION_CEILING`].
	#[display("dotNS lookup timed out")]
	TimedOut,
}

/// Why a grant lookup refused, with what it judged by.
struct Refusal {
	because: RefusedBecause,
	/// Where the manifest came from, or `None` for a refusal that read none.
	source: Option<ManifestSource>,
	/// What the manifest lists under the `trustedProducts` key for the caller,
	/// or `None` when it has no such key or no manifest was read.
	scopes: Option<Vec<Granted>>,
}

impl Refusal {
	/// Record the refusal where a native host forwards it off the device.
	///
	/// A warning because the wire answers one refusal for every reason, so this
	/// line is the only record of which it was. The detail is cut at
	/// [`crate::logging::HOST_SINK_DETAIL_MAX_CHARS`], so the two unbounded
	/// fields go last: the source, which carries the reason a lookup failed,
	/// then the scopes the publisher lists. A failed lookup reads no scopes, so
	/// the cut lands in whichever of the two the line carries.
	fn warn(&self, caller: &ProductId, owner: &ProductId, context: Option<&ProductId>) {
		warn!(
			because = %self.because.as_str(),
			entry = %caller.bare_label(),
			caller = %caller,
			owner = %owner,
			context = context.map(tracing::field::display),
			source = self.source.as_ref().map(tracing::field::display),
			scopes = ?self.scopes,
			"ring-VRF access refused"
		);
	}

	/// Record a key-grant refusal the caller recovers from by asking the user,
	/// on the device only, since it is a step towards a prompt rather than the
	/// answer the product gets.
	fn inform(&self, caller: &ProductId, owner: &ProductId) {
		info!(
			because = %self.because.as_str(),
			entry = %caller.bare_label(),
			caller = %caller,
			owner = %owner,
			source = self.source.as_ref().map(tracing::field::display),
			scopes = ?self.scopes,
			"ring-VRF key access not granted by manifest, asking for user consent"
		);
	}
}

/// Scopes `target`'s published manifest grants `caller_id`.
///
/// A grant that cannot be established answers `false` whatever the reason — the
/// product does not resolve, it published no manifest, the fetch failed, or the
/// manifest names this caller with a narrower scope. Callers turn that into one
/// refusal, so the outcome never reveals which of those it was. Failing closed
/// also means an unreachable chain withdraws grants rather than assuming them.
pub async fn grants_scope(
	services: &RuntimeServices,
	platform: &dyn Platform,
	caller_id: &ProductId,
	target: &ProductId,
	scope: Granted,
) -> bool {
	scope_grant(services, platform, caller_id, target, scope, MANIFEST_RESOLUTION_CEILING)
		.await
		.is_ok()
}

/// Published access subject to this host's stored account-access refusals,
/// with the manifest lookup bounded by `ceiling`.
async fn scope_grant(
	services: &RuntimeServices,
	platform: &dyn Platform,
	caller_id: &ProductId,
	target: &ProductId,
	scope: Granted,
	ceiling: Duration,
) -> Result<(), Refusal> {
	let (json, source) = root_manifest_within(ceiling, services, platform, target).await;
	let source = Some(source);
	let Some(json) = json else {
		return Err(Refusal { because: RefusedBecause::ManifestUnavailable, source, scopes: None });
	};
	let Ok(manifest) = RootManifest::parse(&json) else {
		return Err(Refusal { because: RefusedBecause::ManifestMalformed, source, scopes: None });
	};
	let scopes = manifest.trusted_products.get(caller_id.bare_label()).cloned();
	if !manifest.grants(caller_id, scope) {
		return Err(Refusal { because: RefusedBecause::NotGranted, source, scopes });
	}
	if scope == Granted::Context && !caller_id.has_trusted_remote_permissions() {
		let because =
			match stored_account_decision(&services.stack().storage(platform), caller_id, target)
				.await
			{
				StoredDecision::Denied => RefusedBecause::UserDenied,
				StoredDecision::Unreadable => RefusedBecause::DecisionUnreadable,
				StoredDecision::Absent => return Ok(()),
			};
		return Err(Refusal { because, source, scopes });
	}
	Ok(())
}

/// What the stored account-access decision says, keeping "unreadable" apart
/// from "absent" so the caller can report which it was.
enum StoredDecision {
	/// The user refused this pair.
	Denied,
	/// No decision recorded.
	Absent,
	/// The store could not be read, so the lookup fails closed.
	Unreadable,
}

/// Whether the user has already refused `caller_id` access to `target`'s account.
///
/// Reads the stored decision without raising a prompt: `NotDetermined` is not a
/// refusal, and the prompt that would settle it belongs to the call the user
/// actually made, not to a grant lookup.
async fn stored_account_decision(
	storage: &StackStorage<'_, dyn Platform + '_>,
	caller_id: &ProductId,
	target: &ProductId,
) -> StoredDecision {
	// Checked under both the product-scoped key this release writes and the
	// full-id key earlier releases wrote.
	//
	// Reading only the new shape would silently discard a decision a user has
	// already made: on the ungranted path they are asked again, but on the
	// granted path the lookup reads `NotDetermined`, admits the publisher's
	// grant and never prompts. A stored "no" would become a "yes" on upgrade,
	// which is the one case this override exists for.
	//
	// The product-scoped key uses the bare label on BOTH sides. The grant it
	// overrides is resolved by `ProductId::bare_label` for the target as well as
	// the caller, so filing the refusal against the full target would let
	// `app.peopl.dot` carry a grant the user refused for `peopl.dot`.
	let candidates = [
		CoreStorageKey::product_account_access_authorization(caller_id, target),
		CoreStorageKey::account_access_authorization(caller_id, target),
	];
	for key in candidates {
		match authorization_status(storage, key).await {
			Ok(PermissionAuthorizationStatus::Denied) => return StoredDecision::Denied,
			Ok(
				PermissionAuthorizationStatus::NotDetermined |
				PermissionAuthorizationStatus::Authorized,
			) => {},
			// Fails closed: a storage fault must not let a publisher's manifest
			// turn the user's "no" into a "yes".
			Err(_) => return StoredDecision::Unreadable,
		}
	}
	StoredDecision::Absent
}

/// Longest one grant lookup may spend resolving a manifest.
///
/// A cold-cache resolution is several sequential Asset Hub operations, each
/// bounded only by its own operation timeout, so the walk's worst case is the
/// sum of them. This caps the walk as a whole.
const MANIFEST_RESOLUTION_CEILING: Duration = Duration::from_secs(30);

/// Run `future`, giving up after `ceiling`.
///
/// `None` on expiry, which every caller here treats as "no manifest": the same
/// answer an unreachable chain gives, and the same uniform refusal.
async fn with_ceiling<T>(ceiling: Duration, future: impl Future<Output = T>) -> Option<T> {
	use futures::FutureExt;
	let future = future.fuse();
	let timeout = futures_timer::Delay::new(ceiling).fuse();
	futures::pin_mut!(future, timeout);
	futures::select! {
		value = future => Some(value),
		() = timeout => {
			warn!(ceiling_secs = ceiling.as_secs(), "manifest resolution gave up");
			None
		}
	}
}

/// [`root_manifest`], or no manifest from a lookup that ran past `ceiling`.
async fn root_manifest_within(
	ceiling: Duration,
	services: &RuntimeServices,
	platform: &dyn Platform,
	target: &ProductId,
) -> (Option<String>, ManifestSource) {
	with_ceiling(ceiling, root_manifest(services, platform, target))
		.await
		.unwrap_or((None, ManifestSource::TimedOut))
}

/// Which proof contexts a caller granted another product's ring-VRF key may use.
///
/// The owner's grant covers the key, not whose pseudonym the caller mints with
/// it, so a context must be the caller's own, the owner's, or one whose product
/// grants the caller `context` (or `all`) in its manifest. Checked after the key
/// grant, never instead of it; the owner's own calls are unaffected.
///
/// With `peopl.paseo` granting `dim2next` its key:
/// - `dim2next.paseo` may prove in `dim2.paseo`'s context when `dim2.paseo` grants `dim2next`
///   `context`.
/// - It may not prove in `stash.paseo`'s, which grants nothing, nor in a namesake's on another
///   network.
///
/// Guards the alias read as well as the proof: both come from one VRF evaluation.
pub async fn require_context_access(
	services: &RuntimeServices,
	access: &AuthorizedAccess,
	context: &v01::ProductProofContext,
) -> Result<(), RingVrfError> {
	// The same ownership test the gate makes, on the same values. Comparing bare
	// labels here instead would answer "is this the owner?" differently from
	// `ring_vrf_key_access_granted`: a caller `peopl.paseo` against a handle `peopl.dot`
	// would be told to bring a grant by one and waved through as the owner by
	// the other, leaving its context unconstrained.
	if access.caller == access.owner {
		return Ok(());
	}
	let context_id = &context.product_id;
	// A product is all its executables, so `app.dim2.dot` is `dim2.dot`'s context
	// and `app.peopl.dot` is the owner's. The network is not part of that: a
	// grant published on `.dot` must not reach the pseudonym a namesake presents
	// on `.paseo`.
	if context_id.same_product_on_one_network(&access.caller) ||
		context_id.same_product_on_one_network(&access.owner)
	{
		return Ok(());
	}
	// The manifest is resolved by bare label on the host's network, so
	// `dim2.dot` and `dim2.paseo` read one document. The context is held to the
	// owner's network, so a grant read for one TLD does not admit the pseudonym
	// named under another. This is the owner's TLD as the handle spells it, not
	// the host's: an owner id from another network reads this host's manifests,
	// as `ring_vrf_key_access_granted` already does for the key grant.
	if !context_id.on_one_network(&access.owner) {
		Refusal { because: RefusedBecause::ContextOffNetwork, source: None, scopes: None }.warn(
			&access.caller,
			&access.owner,
			Some(context_id),
		);
		return Err(RingVrfError::NotAllowlisted);
	}
	let decision = scope_grant(
		services,
		services.platform.as_ref(),
		&access.caller,
		context_id,
		Granted::Context,
		MANIFEST_RESOLUTION_CEILING,
	)
	.await;
	// Recorded either way, for the reason `ring_vrf_key_access_granted` records
	// its own decision: the wire answers one refusal for every reason, and a
	// granted use raises no prompt and stores nothing.
	match decision {
		Ok(()) => {
			info!(
				caller = %access.caller,
				owner = %access.owner,
				context = %context_id,
				"ring-VRF proof context granted by the context product's manifest"
			);
			Ok(())
		},
		Err(refusal) => {
			refusal.warn(&access.caller, &access.owner, Some(context_id));
			Err(RingVrfError::NotAllowlisted)
		},
	}
}

/// The identities the gate decided about, returned together because every
/// consumer needs both.
pub struct AuthorizedAccess {
	/// The caller the gate authorized.
	pub caller: ProductId,
	/// The owner of the key.
	pub owner: ProductId,
}

/// Own-key or published access, honoring stored account-access refusals.
/// Returns canonical identities for key derivation and context checks.
///
/// A published refusal is recorded as a warning for the host.
pub async fn ring_vrf_key_access_granted(
	services: &RuntimeServices,
	platform: &dyn Platform,
	caller: ProductId,
	handle: &v01::ProductAccountId,
) -> Result<AuthorizedAccess, RingVrfError> {
	key_access(
		services,
		platform,
		caller,
		handle,
		MANIFEST_RESOLUTION_CEILING,
		|refusal, caller, owner| refusal.warn(caller, owner, None),
	)
	.await
}

/// [`ring_vrf_key_access_granted`] with the grant lookup bounded by `budget`.
///
/// A lookup still running past the budget is refused and recorded as timed
/// out. Cancelling the call is for the caller to handle, by dropping this
/// future.
pub async fn ring_vrf_key_access_granted_within(
	services: &RuntimeServices,
	platform: &dyn Platform,
	caller: ProductId,
	handle: &v01::ProductAccountId,
	budget: Option<Duration>,
) -> Result<AuthorizedAccess, RingVrfError> {
	key_access(
		services,
		platform,
		caller,
		handle,
		budget
			.map_or(MANIFEST_RESOLUTION_CEILING, |budget| budget.min(MANIFEST_RESOLUTION_CEILING)),
		|refusal, caller, owner| refusal.warn(caller, owner, None),
	)
	.await
}

/// The same decision as [`ring_vrf_key_access_granted`], for a caller that
/// answers a refusal by asking the user instead, so the refusal stays on the
/// device.
pub async fn ring_vrf_key_access_granted_or_consent(
	services: &RuntimeServices,
	platform: &dyn Platform,
	caller: ProductId,
	handle: &v01::ProductAccountId,
) -> Result<AuthorizedAccess, RingVrfError> {
	key_access(services, platform, caller, handle, MANIFEST_RESOLUTION_CEILING, Refusal::inform)
		.await
}

/// Own-key or published access, handing a published refusal to `record_refusal`.
///
/// The grant lookup is bounded by `ceiling`.
async fn key_access(
	services: &RuntimeServices,
	platform: &dyn Platform,
	caller: ProductId,
	handle: &v01::ProductAccountId,
	ceiling: Duration,
	record_refusal: fn(&Refusal, &ProductId, &ProductId),
) -> Result<AuthorizedAccess, RingVrfError> {
	let owner = handle.dot_ns_identifier.clone();
	if caller == owner {
		return Ok(AuthorizedAccess { caller, owner });
	}
	match scope_grant(services, platform, &caller, &owner, Granted::Context, ceiling).await {
		Ok(()) => {
			info!(
				caller = %caller,
				owner = %owner,
				"ring-VRF key access granted by the owner's manifest"
			);
			Ok(AuthorizedAccess { caller, owner })
		},
		Err(refusal) => {
			record_refusal(&refusal, &caller, &owner);
			Err(RingVrfError::NotAllowlisted)
		},
	}
}

/// The root manifest JSON of `target` and where it came from: the cache when
/// the entry is younger than [`MANIFEST_TTL_SECS`], dotNS otherwise.
///
/// A freshly read manifest is cached even though the caller may not be granted
/// anything by it: the document describes the product, not the asker. So is the
/// chain's answer that there is no manifest, which is authoritative for the same
/// TTL.
///
/// Lookups of one product that miss the cache together share one dotNS read,
/// through [`DotnsReads::root_manifests`](crate::runtime::services::DotnsReads::root_manifests), so
/// a burst of them holds one Asset Hub follow rather than one each. The read runs to the end
/// whatever its callers do, so one that gives up on its ceiling does not cut it short for
/// the rest.
async fn root_manifest(
	services: &RuntimeServices,
	platform: &dyn Platform,
	target: &ProductId,
) -> (Option<String>, ManifestSource) {
	let key = CoreStorageKey::product_manifest(target);
	let read = cached_dotns_json(services, platform, key, || async {
		let genesis_hash =
			services.asset_hub_chain_genesis_hash().ok_or(ManifestSource::NoAssetHub)?;
		let (chain, target) = (services.chain.clone(), target.clone());
		services
			.dotns_reads
			.root_manifests
			.run(target.bare_label().to_string(), move || async move {
				fetch_root_manifest(&chain, genesis_hash, &target).await.map_err(|error| {
					let reason = error.reason();
					warn!(%target, %reason, "root manifest lookup failed");
					ManifestSource::LookupFailed { reason }
				})
			})
			.await
			.unwrap_or_else(|| {
				Err(ManifestSource::LookupFailed { reason: READ_ENDED_UNANSWERED.to_string() })
			})
	})
	.await;
	match read {
		Ok((json, DotnsRead::Cached { age_secs })) => (json, ManifestSource::Cached { age_secs }),
		Ok((json, DotnsRead::Fetched)) => (json, ManifestSource::Fetched),
		Err(source) => (None, source),
	}
}

/// Why a shared dotNS read gave no answer: its task ended first.
const READ_ENDED_UNANSWERED: &str = "the dotNS read ended without answering";

/// `product_id`'s Worker manifest, cached and shared as [`root_manifest`] is.
/// `Ok(None)` when it publishes none or what it publishes cannot be used, which
/// the RFC treats alike; `Err` when the chain could not be read, which says
/// nothing about the product.
pub async fn worker_manifest(
	services: &RuntimeServices,
	platform: &dyn Platform,
	product_id: &ProductId,
) -> Result<Option<WorkerManifest>, String> {
	let key = worker_manifest_cache_key(product_id);
	let (json, _) = cached_dotns_json(services, platform, key, || async {
		let genesis_hash = asset_hub(services)?;
		let (chain, product) = (services.chain.clone(), product_id.clone());
		services
			.dotns_reads
			.worker_manifests
			.run(product_id.bare_label().to_string(), move || async move {
				fetch_worker_manifest(&chain, genesis_hash, &product)
					.await
					.map_err(|error| error.reason())
			})
			.await
			.unwrap_or_else(|| Err(READ_ENDED_UNANSWERED.to_string()))
	})
	.await
	.inspect_err(|reason| warn!(%product_id, %reason, "worker manifest lookup failed"))?;
	let Some(json) = json else {
		return Ok(None);
	};
	Ok(WorkerManifest::parse(&json)
		.inspect_err(|reason| warn!(%product_id, %reason, "worker manifest is unusable"))
		.ok())
}

/// Cache key for a product's Worker manifest, keyed by bare label as
/// [`CoreStorageKey::product_manifest`] is.
pub fn worker_manifest_cache_key(product_id: &ProductId) -> CoreStorageKey {
	CoreStorageKey::WorkerManifest { product_label: product_id.bare_label().to_string() }
}

/// Cache key for a product's App manifest, keyed as
/// [`worker_manifest_cache_key`] is.
pub fn app_manifest_cache_key(product_id: &ProductId) -> CoreStorageKey {
	CoreStorageKey::AppManifest { product_label: product_id.bare_label().to_string() }
}

/// The name and icon `product_id`'s root manifest gives it, read through the
/// root manifest cache. `Ok(None)` when it publishes no root manifest or
/// one that gives no brand; `Err` when the chain could not be read.
pub async fn product_brand(
	services: &RuntimeServices,
	platform: &dyn Platform,
	product_id: &ProductId,
) -> Result<Option<ProductBrand>, String> {
	let (json, source) = root_manifest(services, platform, product_id).await;
	if !matches!(source, ManifestSource::Cached { .. } | ManifestSource::Fetched) {
		return Err(source.to_string());
	}
	Ok(json.and_then(|json| {
		ProductBrand::parse(&json)
			.inspect_err(|reason| warn!(%product_id, %reason, "root manifest gives no brand"))
			.ok()
	}))
}

/// Whether `product_id` publishes an App, cached as [`worker_manifest`]
/// caches the Worker's. A published App manifest that does not parse is no
/// App. `None` when that cannot be read within `ceiling`.
pub async fn publishes_app(
	services: &RuntimeServices,
	platform: &dyn Platform,
	product_id: &ProductId,
	ceiling: Duration,
) -> Option<bool> {
	let read =
		cached_dotns_json(services, platform, app_manifest_cache_key(product_id), || async {
			fetch_app_manifest(&services.chain, asset_hub(services)?, product_id)
				.await
				.map_err(|error| error.reason())
		});
	let (json, _) = with_ceiling(ceiling, read)
		.await?
		.inspect_err(|reason| warn!(%product_id, %reason, "app manifest lookup failed"))
		.ok()?;
	Some(json.is_some_and(|json| {
		check_app_manifest(&json)
			.inspect_err(|reason| warn!(%product_id, %reason, "app manifest is unusable"))
			.is_ok()
	}))
}

/// Where [`cached_dotns_json`] read its answer.
#[derive(Clone)]
pub enum DotnsRead {
	/// From the cache on this device, written `age_secs` ago.
	Cached { age_secs: u64 },
	/// From dotNS, just now.
	Fetched,
}

/// The JSON `fetch` reads from dotNS, from its entry under `key` when that is
/// younger than [`MANIFEST_TTL_SECS`] and was read in the stack's current
/// generation, and where it was read. Whatever `fetch`
/// answers, including that there is nothing published, is cached for that
/// lifetime.
///
/// A failed lookup is not cached. It says nothing about what is published,
/// only that the chain could not be read, and holding that for a day would
/// turn one blip into a day of withdrawn grants or missing providers.
async fn cached_dotns_json<F, E>(
	services: &RuntimeServices,
	platform: &dyn Platform,
	key: CoreStorageKey,
	fetch: impl FnOnce() -> F,
) -> Result<(Option<String>, DotnsRead), E>
where
	F: Future<Output = Result<Option<String>, E>>,
{
	let storage = services.stack().storage(platform);
	let generation = services.stack().generation();
	let now = current_unix_secs();
	// `fetched_at_secs <= now` is part of the freshness test, not an assumption.
	// Without it a `saturating_sub` on a future stamp yields 0, which is below
	// any TTL, so an entry written while the device clock ran ahead would be
	// honoured forever. The TTL is documented as the revocation bound, so such
	// an entry is one a publisher could never withdraw. Treating it as stale
	// costs one lookup and cannot be worse than that.
	if let Ok(Some(bytes)) = storage.read(key.clone()).await &&
		let Ok(cached) = CachedManifest::decode(&mut bytes.as_slice()) &&
		cached.generation == generation &&
		cached.fetched_at_secs <= now &&
		now - cached.fetched_at_secs < MANIFEST_TTL_SECS
	{
		return Ok((cached.json, DotnsRead::Cached { age_secs: now - cached.fetched_at_secs }));
	}

	let json = fetch().await?;
	let entry = CachedManifest {
		generation: generation.to_string(),
		fetched_at_secs: now,
		json: json.clone(),
	};
	let _ = storage.write(key, entry.encode()).await;
	Ok((json, DotnsRead::Fetched))
}

/// The Asset Hub dotNS is read from, or why there is none.
fn asset_hub(services: &RuntimeServices) -> Result<[u8; 32], String> {
	services
		.asset_hub_chain_genesis_hash()
		.ok_or_else(|| "the host has no Asset Hub to read dotNS from".to_string())
}
