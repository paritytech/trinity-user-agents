//! Root manifest resolution over dotNS.
//!
//! Resolves a product id to the JSON its base name publishes at the `manifest`
//! text record, following [RFC — Product Manifest Format][manifest]: derive the
//! node under the network's own TLD, find the resolver through the registry,
//! read the record. Parsing that JSON is
//! [`crate::host_internal::product_manifest`]'s job.
//!
//! [manifest]: ../../../../docs/rfcs/product-manifest.md

use core::future::Future;
use core::time::Duration;

use crate::platform::{
    CoreStorageKey, PermissionAuthorizationStatus, Platform, normalize_product_identifier,
};
use parity_scale_codec::{Decode, Encode};
use tracing::{info, instrument, warn};
use truapi::v01;

use crate::chain_runtime::ChainRuntime;
use crate::dotns_views::{call_bytes32_string, network_tld, protocol_component, tld_node};
use crate::host_internal::permissions::account_access_status;
use crate::host_internal::product_manifest::{Granted, RootManifest, bare_product_label};
use crate::host_internal::sso_messages::RingVrfError;
use crate::host_logic::dotns_gateway::{
    DotnsTransport, DotnsViewError, call_bytes32, call_no_args, decode_address, decode_string,
    discover_pop_controller, namehash_under,
};
use crate::runtime::dotns_lookup::DotnsLookup;
use crate::runtime::services::RuntimeServices;
use crate::unix_time::current_unix_secs;

/// Text record a base name publishes its root manifest at.
const MANIFEST_RECORD_KEY: &str = "manifest";

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
pub async fn fetch_root_manifest(
    chain: &ChainRuntime,
    asset_hub_chain_genesis_hash: [u8; 32],
    product_id: &str,
) -> Result<Option<String>, String> {
    let mut lookup = DotnsLookup::pinned_to_best_block(
        chain,
        asset_hub_chain_genesis_hash,
        &format!("manifest:{product_id}"),
    )
    .await?;

    let Some(protocol_registry) = protocol_registry(&mut lookup).await? else {
        return Ok(None);
    };

    let tld = network_tld(&mut lookup, &protocol_registry).await?;
    let node = namehash_under(&tld_node(&tld), bare_product_label(product_id));

    let registry = protocol_component(&mut lookup, &protocol_registry, "registry").await?;
    let resolver_output = lookup
        .view(&registry, call_bytes32("resolver(bytes32)", &node))
        .await
        .map_err(|err| format!("DotnsRegistry.resolver(): {err}"))?;
    let resolver = decode_address(&resolver_output)
        .map_err(|err| format!("DotnsRegistry.resolver(): {err}"))?;
    if resolver == [0u8; 20] {
        return Ok(None);
    }

    let manifest_output = match lookup
        .view(
            &resolver,
            call_bytes32_string("text(bytes32,string)", &node, MANIFEST_RECORD_KEY),
        )
        .await
    {
        Ok(output) => output,
        // The dotNS-issued default resolver does not implement `text`, which is
        // the same outcome as an unpublished manifest.
        Err(DotnsViewError::Reverted(_)) => return Ok(None),
        Err(err @ DotnsViewError::Failed(_)) => {
            return Err(format!("ContentResolver.text(): {err}"));
        }
    };
    let manifest =
        decode_string(&manifest_output).map_err(|err| format!("ContentResolver.text(): {err}"))?;
    if manifest.is_empty() {
        return Ok(None);
    }
    Ok(Some(manifest))
}

/// The deployment's `DotnsProtocolRegistry`, read from the controller.
/// `Ok(None)` when the gateway is not deployed.
///
/// [`discover_pop_controller`] resolves the controller, because
/// `DotnsGateway.DispatcherAddress` holds either the controller or a
/// `RootGatewayDispatcher` that fronts it, and both are in service. Calling
/// `protocolRegistry()` on the stored address directly reverts on a chain that
/// still keeps its dispatcher, which would refuse every grant on that chain
/// while username resolution kept working.
async fn protocol_registry<T: DotnsTransport + ?Sized>(
    transport: &mut T,
) -> Result<Option<[u8; 20]>, String> {
    let Some(controller) = discover_pop_controller(transport).await? else {
        return Ok(None);
    };
    let output = transport
        .view(&controller, call_no_args("protocolRegistry()"))
        .await
        .map_err(|err| format!("DotnsPopController.protocolRegistry(): {err}"))?;
    decode_address(&output)
        .map(Some)
        .map_err(|err| format!("DotnsPopController.protocolRegistry(): {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The node a product id resolves to on a network serving `tld`.
    fn node_on(tld: &str, product_id: &str) -> [u8; 32] {
        namehash_under(&tld_node(tld), bare_product_label(product_id))
    }

    #[test]
    fn a_node_matches_the_chain_derivation() {
        // Pinned against paseo-v2, where `ProtocolRegistry.tldNode()` reads
        // 0x096b43… and the dotNS SDK derives `browse.paseo` as 0x185056….
        assert_eq!(
            hex::encode(tld_node(".paseo")),
            "096b436ee9a398429fe33ad4b359bad4398dd74b412ec1dd043c93dfbf581874"
        );
        assert_eq!(
            hex::encode(node_on(".paseo", "browse")),
            "1850561ffded63ac23dac8fd5e793fca1f349729ed6ade91c45f44a9f7b6b781"
        );
    }

    #[test]
    fn the_tld_an_identifier_carries_does_not_change_the_node_it_resolves_to() {
        // A product id minted on `.dot` has to resolve against a `.paseo`
        // deployment; the suffix it was written with names no node of its own.
        let expected = node_on(".paseo", "dim2");
        assert_eq!(node_on(".paseo", "dim2.dot"), expected);
        assert_eq!(node_on(".paseo", "dim2.paseo"), expected);
    }

    #[test]
    fn one_identifier_resolves_differently_on_two_networks() {
        // The other half of the same property: the network's TLD is what
        // separates deployments, so the same id must not collide across them.
        assert_ne!(node_on(".paseo", "dim2.dot"), node_on(".dot", "dim2.dot"));
    }

    #[test]
    fn a_text_call_encodes_the_key_as_a_dynamic_argument() {
        let call = call_bytes32_string("text(bytes32,string)", &[0x11; 32], "manifest");
        // selector, node, offset, length, one padded word for an 8-byte key.
        assert_eq!(call.len(), 4 + 32 * 4);
        assert_eq!(&call[4..36], &[0x11; 32]);
        assert_eq!(call[67], 64, "key offset follows the node");
        assert_eq!(call[99], 8, "key length precedes its bytes");
        assert_eq!(&call[100..108], b"manifest");
    }
}

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
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CachedManifest {
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
/// Write the bytes under [`CoreStorageKey::ProductManifest`] for the product the
/// manifest belongs to. The core reads that entry before it consults the Asset
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
pub fn encode_cached_root_manifest(json: Option<&str>, fetched_at_secs: u64) -> Vec<u8> {
    CachedManifest {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusedBecause {
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
}

impl RefusedBecause {
    fn as_str(self) -> &'static str {
        match self {
            Self::NotGranted => "not granted",
            Self::UserDenied => "user denied",
            Self::DecisionUnreadable => "stored decision unreadable",
            Self::ManifestMalformed => "manifest malformed",
            Self::ManifestUnavailable => "manifest unavailable",
        }
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
    caller_id: &str,
    target: &str,
    scope: Granted,
) -> bool {
    scope_grant(services, platform, caller_id, target, scope)
        .await
        .is_ok()
}

/// `grants_scope`, keeping the reason for an operator.
pub async fn scope_grant(
    services: &RuntimeServices,
    platform: &dyn Platform,
    caller_id: &str,
    target: &str,
    scope: Granted,
) -> Result<(), RefusedBecause> {
    // Bounded here, not by the caller.
    //
    // The product-facing door runs this inside `remote_authority_call` and so
    // carries the deadline its caller asked for. The wire door does not: the
    // authority is the remote end, nothing races `cx.timeout()` there, and the
    // peer sets no deadline of its own. The responder also dispatches serially,
    // so one message naming enough uncached products would hold every other
    // product's signing on this device behind it.
    //
    // A ceiling on the resolution itself covers both doors. Where a caller's
    // deadline is shorter it still wins, because that race is applied outside.
    let Some(json) = with_ceiling(
        MANIFEST_RESOLUTION_CEILING,
        root_manifest(services, platform, target),
    )
    .await
    .flatten() else {
        return Err(RefusedBecause::ManifestUnavailable);
    };
    let Ok(manifest) = RootManifest::parse(&json) else {
        return Err(RefusedBecause::ManifestMalformed);
    };
    if !manifest.grants(bare_product_label(caller_id), scope) {
        return Err(RefusedBecause::NotGranted);
    }
    // A publisher's grant waives the publisher's own prompt. For ordinary
    // products, a refusal the user already gave still overrides it,
    // read-only: raising the prompt here would turn a grant into a way to ask
    // again.
    //
    // Read after the manifest rather than before it. The read is the same either
    // way, but taking it first let a denied pair refuse without the chain lookup
    // every other refusal pays for, and that difference in cost enumerates the
    // user's stored denials to anyone who can ask. On the wire path the caller
    // id is supplied by the peer, so that is anyone it chooses to name.
    //
    // Scope-specific by design, and in this shared helper rather than in
    // `ring_vrf_key_access_granted`: the stored decision is `AccountAccess`, so
    // it answers about reaching another product's account and says nothing about
    // its storage, and keeping it here means the frontend and the authority
    // inherit one implementation. A later scope that also implies account access
    // has to name itself here; it does not inherit this.
    if scope == Granted::Context
        && !crate::platform::normalizes_to_trusted_remote_permissions(caller_id)
    {
        match stored_account_decision(platform, caller_id, target).await {
            StoredDecision::Denied => return Err(RefusedBecause::UserDenied),
            StoredDecision::Unreadable => return Err(RefusedBecause::DecisionUnreadable),
            StoredDecision::Absent => {}
        }
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
    platform: &dyn Platform,
    caller_id: &str,
    target: &str,
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
    // overrides is resolved by `bare_product_label` for the target as well as
    // the caller, so filing the refusal against the full target would let
    // `app.peopl.dot` carry a grant the user refused for `peopl.dot`.
    let candidates = [
        (
            bare_product_label(caller_id).to_string(),
            bare_product_label(target).to_string(),
        ),
        (caller_id.to_string(), target.to_string()),
    ];
    for (caller, target) in candidates {
        match account_access_status(platform, &caller, &target).await {
            Ok(PermissionAuthorizationStatus::Denied) => return StoredDecision::Denied,
            Ok(
                PermissionAuthorizationStatus::NotDetermined
                | PermissionAuthorizationStatus::Authorized,
            ) => {}
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

/// A granted caller may act in its own proof context, the granting product's,
/// or that of a product whose manifest grants the caller `context`, and in no
/// one else's.
///
/// The contextual alias is a function of (owner key, context), so an
/// unconstrained context lets a grantee produce the alias the owner presents to
/// a product that is not a party to the grant. The owner's grant covers the
/// key; it says nothing about which pseudonym the grantee may mint with it. So
/// the product a context names consents for itself, through its own manifest,
/// the same way the owner consents for the key.
///
/// The owner's own context is admitted without that consent because refusing
/// it left the scope unusable: `PeopleLite.set_alias_account` verifies against
/// `Score.score_context`, which names the personhood product for every prover
/// on the chain, so a grantee held to its own context alone can produce no
/// proof such a chain accepts.
///
/// That admits more than it sounds. On a chain whose contexts are
/// personhood-owned, the owner's context is the pseudonym the owner presents to
/// every product in the score system. What stays refused is a context naming a
/// product that granted the caller nothing, a grant read from a namesake on
/// another network, and the `raw:` development context, which names no product
/// and whose bytes the caller chooses outright.
///
/// The context product's grant is the `context` scope (or `all`), read through
/// [`scope_grant`], so a refusal the user stored for that pair still overrides
/// it. It is checked after the key grant, never instead of it: a product
/// granting the caller its context does not open the owner's key.
///
/// The owner's own calls are unaffected: minting your own aliases in any
/// context is what the parameter is for.
///
/// Both identities come from the gate, already normalized. Re-deriving the
/// caller from the request instead would compare a peer's spelling against a
/// normalized owner and refuse an owner naming itself in another case.
///
/// This binds every call that returns the alias, not only the ones that return
/// a proof: the alias and the proof come out of one VRF evaluation, so guarding
/// the proof alone leaves the same bytes reachable through the read.
pub async fn require_context_access(
    services: &RuntimeServices,
    platform: &dyn Platform,
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
    // Normalized like every other identity the gate decided about. This is the
    // one that arrives straight off the request payload, so leaving it raw would
    // refuse a grantee for spelling its own context `DIM2.paseo`, the hazard
    // the gate exists to remove, one layer down. A context that names no
    // product, `raw:` among them, has no manifest to consent with, so it fails
    // closed.
    let Ok(context_id) = normalize_product_identifier(&context.product_id) else {
        return Err(RingVrfError::NotAllowlisted);
    };
    // A product is all its executables, so `app.dim2.dot` is `dim2.dot`'s context
    // and `app.peopl.dot` is the owner's. The network is not part of that: a
    // grant published on `.dot` must not reach the pseudonym a namesake presents
    // on `.paseo`.
    if same_product_on_one_network(&context_id, &access.caller)
        || same_product_on_one_network(&context_id, &access.owner)
    {
        return Ok(());
    }
    // The manifest is resolved by bare label on the host's network, so
    // `dim2.dot` and `dim2.paseo` read one document. Holding the context to the
    // owner's network keeps a namesake's grant from admitting a pseudonym on the
    // network the namesake does not publish on.
    if !on_one_network(&context_id, &access.owner) {
        return Err(RingVrfError::NotAllowlisted);
    }
    let decision = scope_grant(
        services,
        platform,
        &access.caller,
        &context_id,
        Granted::Context,
    )
    .await;
    // Recorded either way, for the reason `ring_vrf_key_access_granted` records
    // its own decision: the wire answers one refusal for every reason, and a
    // granted use raises no prompt and stores nothing.
    info!(
        caller = %access.caller,
        owner = %access.owner,
        context = %context_id,
        because = decision.err().map_or("granted", RefusedBecause::as_str),
        "ring-VRF proof context checked against the context product's manifest"
    );
    decision.map_err(|_| RingVrfError::NotAllowlisted)
}

/// Whether two normalized product identifiers name one product on one network.
///
/// `bare_product_label` alone answers "same product" across networks, which is
/// right where a grant is read and wrong where a proof context is: the alias is
/// what a product shows on the chain it is on, so a `.paseo` namesake's context
/// is not the `.dot` grantor's.
fn same_product_on_one_network(left: &str, right: &str) -> bool {
    bare_product_label(left) == bare_product_label(right) && on_one_network(left, right)
}

/// Whether two normalized product identifiers carry the same TLD.
fn on_one_network(left: &str, right: &str) -> bool {
    fn tld(product_id: &str) -> Option<&str> {
        product_id.rsplit_once('.').map(|(_, tld)| tld)
    }
    tld(left) == tld(right)
}

/// The identities the gate decided about, both normalized.
///
/// Returned together because every consumer needs both and deriving either one
/// again from the request re-introduces the raw-vs-normalized skew the gate
/// exists to remove: `sso_responder` hands `calling_product_id` through
/// untouched, so a caller that re-derives it compares a peer's spelling against
/// a normalized owner.
pub struct AuthorizedAccess {
    /// The caller the gate authorized, normalized.
    pub caller: String,
    /// The owner of the key, normalized.
    pub owner: String,
}

/// Whether `calling_product_id` may act on `handle`'s ring-VRF key, adjudicated
/// by the component that holds the key.
///
/// The caller owns the key, or the owner's published manifest grants the caller
/// `context`, subject to stored refusals for ordinary products, resolved against
/// the chain here rather than accepted from the request. On a paired host the request
/// arrives over the wire, and a verdict relayed by the caller would take the
/// manifest out of this decision entirely: the peer would reach every handle on
/// the device by setting one field, instead of only the handles a publisher
/// really granted.
///
/// Returns the **normalized** owner it decided about. Callers must derive from
/// that value rather than from the handle they were given: otherwise access is
/// authorized about `peopl.dot` while the key is derived from whatever spelling
/// arrived, and only a registry lookup miss separates the two.
///
/// The owner check runs first and costs nothing, so a product proving with its
/// own key never touches the network. Everything after it is a cross-product
/// access, and every reason it is refused answers the same way.
pub async fn ring_vrf_key_access_granted(
    services: &RuntimeServices,
    platform: &dyn Platform,
    calling_product_id: &str,
    handle: &v01::ProductAccountId,
) -> Result<AuthorizedAccess, RingVrfError> {
    // A caller id that does not normalize names no product, so it holds no key
    // and no manifest can grant it. It takes the same refusal as a product that
    // granted nothing rather than an error carrying the string back: on the wire
    // path this field is peer-supplied, and one refusal for every reason is the
    // whole design of this seam.
    let Ok(caller) = normalize_product_identifier(calling_product_id) else {
        return Err(RingVrfError::NotAllowlisted);
    };
    // The handle is normalized here, not only at the frontend. The frontend
    // does it before delegating, but `sso_responder` hands a wire request
    // straight to the authority unnormalized, so without this the two doors
    // disagree: an owner naming its own key `PEOPL.DOT` over the wire is
    // refused where the same request from a local product runtime succeeds.
    //
    // A handle that does not normalize names no product, so it owns no key and
    // no manifest can grant it: it takes the same refusal as a product that
    // granted nothing, rather than a distinguishable error.
    let Ok(owner) = normalize_product_identifier(&handle.dot_ns_identifier) else {
        return Err(RingVrfError::NotAllowlisted);
    };
    if caller == owner {
        return Ok(AuthorizedAccess { caller, owner });
    }
    let decision = scope_grant(services, platform, &caller, &owner, Granted::Context).await;
    if decision.is_ok() {
        // Recorded, because nothing else records it. A granted cross-product
        // access raises no prompt and writes no stored decision, so without this
        // line the only audible half of the decision is the refusal below: a
        // publisher's grant would let one product act with another's keys and
        // leave no trace on the device that it happened. This does not make the
        // access revocable, which needs a surface for the user to record a
        // decision about a pair they were never asked about, but it is what any
        // such surface would have to be built on.
        info!(
            caller = %caller,
            owner = %owner,
            "ring-VRF key access granted by the owner's manifest"
        );
        return Ok(AuthorizedAccess { caller, owner });
    }
    // The wire answer is one refusal for every reason, so the reason lives here
    // or nowhere. Which door the request came through is not repeated: the
    // enclosing span already says it (`account.*` for a local product runtime,
    // `sso_responder.*` for a paired peer).
    //
    // That span is also what says how far to trust `caller`. Under `account.*`
    // it is the product id the host bound to the connection. Under
    // `sso_responder.*` it is `calling_product_id` as decoded from the peer's
    // message: what the authenticated paired host said, not something this host
    // verified. The refusal is sound either way, because the grant is resolved
    // from the owner's manifest and never from this field, but an operator
    // reading the line should not take it as proof of who asked.
    // `info!`, not `debug!`: this is the only per-event record that a
    // cross-product key access was refused, and the wire deliberately answers
    // one error for every reason. `logging.rs` installs `LevelFilter::OFF` and
    // the CLI defaults to `info`, so at `debug` this reaches nobody on any
    // shipped host and the refusal is invisible everywhere.
    info!(
        caller = %caller,
        owner = %owner,
        because = decision.err().map_or("granted", RefusedBecause::as_str),
        "ring-VRF key access refused"
    );
    Err(RingVrfError::NotAllowlisted)
}

/// Cache key for a product's root manifest.
///
/// Keyed by the bare label, which is what the document is actually resolved by:
/// `fetch_root_manifest` walks `bare_product_label(product_id)` under the host's
/// TLD, so `peopl.dot`, `app.peopl.dot` and `worker.peopl.dot` are one dotNS
/// node holding one document. Keying by the full id instead gave each spelling
/// its own entry, so a caller naming subnames drove a fresh chain resolution and
/// a fresh durable write per spelling for a document already held, with nothing
/// bounding how many. The host resolves only against its own network, so the
/// label alone identifies the node.
///
/// Anything seeding this cache must key it the same way or the entry is written
/// where nothing reads it.
pub fn manifest_cache_key(product_id: &str) -> CoreStorageKey {
    CoreStorageKey::ProductManifest {
        product_id: bare_product_label(product_id).to_string(),
    }
}

/// `target`'s root manifest JSON, from cache when it is younger than
/// [`MANIFEST_TTL_SECS`] and from dotNS otherwise.
///
/// A freshly read manifest is cached even though the caller may not be granted
/// anything by it: the document describes the product, not the asker. So is the
/// chain's answer that there is no manifest, which is authoritative for the same
/// TTL.
///
/// A failed lookup is not cached. It says nothing about the product, only that
/// the chain could not be read, and holding that for a day would turn one blip
/// into a day of withdrawn grants.
///
/// The cache dedupes misses only once one has finished, and nothing upstream
/// caps in-flight dispatches, so a product can hold one follow per concurrent
/// miss for up to `OPERATION_TIMEOUT` each. A single-flight keyed by target or
/// a dispatch cap belongs with the request pipeline, not here.
async fn root_manifest(
    services: &RuntimeServices,
    platform: &dyn Platform,
    target: &str,
) -> Option<String> {
    let key = manifest_cache_key(target);
    let now = current_unix_secs();
    // `fetched_at_secs <= now` is part of the freshness test, not an assumption.
    // Without it a `saturating_sub` on a future stamp yields 0, which is below
    // any TTL, so an entry written while the device clock ran ahead would be
    // honoured forever. The TTL is documented as the revocation bound, so such
    // an entry is one a publisher could never withdraw. Treating it as stale
    // costs one lookup and cannot be worse than that.
    if let Ok(Some(bytes)) = platform.read_core_storage(key.clone()).await
        && let Ok(cached) = CachedManifest::decode(&mut bytes.as_slice())
        && cached.fetched_at_secs <= now
        && now - cached.fetched_at_secs < MANIFEST_TTL_SECS
    {
        return cached.json;
    }

    let genesis_hash = services.asset_hub_chain_genesis_hash()?;
    let json = match fetch_root_manifest(&services.chain, genesis_hash, target).await {
        Ok(json) => json,
        Err(reason) => {
            warn!(%target, %reason, "root manifest lookup failed");
            return None;
        }
    };
    let _ = platform
        .write_core_storage(
            key,
            CachedManifest {
                fetched_at_secs: now,
                json: json.clone(),
            }
            .encode(),
        )
        .await;
    json
}
