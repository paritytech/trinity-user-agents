//! Unified [`Preimage`] trait.

use crate::{
	CallContext, CallError, Subscription,
	versioned::preimage::{
		RemotePreimageCacheAccountError, RemotePreimageCacheAccountRequest,
		RemotePreimageCacheAccountResponse, RemotePreimageCacheProvidersRequest,
		RemotePreimageCacheProvidersResponse, RemotePreimageLookupSubscribeError,
		RemotePreimageLookupSubscribeItem, RemotePreimageLookupSubscribeRequest,
		RemotePreimageReadError, RemotePreimageReadRequest, RemotePreimageReadResponse,
		RemotePreimageReadTorrentRequest, RemotePreimageReadTorrentResponse,
		RemotePreimageRetainError, RemotePreimageRetainRequest, RemotePreimageRetainResponse,
		RemotePreimageRetentionsRequest, RemotePreimageRetentionsResponse,
		RemotePreimageStopRetentionRequest, RemotePreimageStopRetentionResponse,
		RemotePreimageSubmitError, RemotePreimageSubmitRequest, RemotePreimageSubmitResponse,
	},
	wire, wire_trait,
};

/// Preimage lookup and submission methods.
#[wire_trait(id = 11)]
#[crate::async_trait]
pub trait Preimage: Send + Sync {
	/// Subscribe to preimage lookups for a given key.
	///
	/// ```ts
	/// import { firstValueFrom, from } from "rxjs";
	///
	/// // Submit a preimage first so the lookup resolves to a value.
	/// const value = `0x${crypto.getRandomValues(new Uint8Array(4)).toHex()}` as `0x${string}`;
	/// const submitted = await truapi.preimage.submit(value);
	/// assert(submitted.isOk(), "submit failed:", submitted);
	///
	/// const item = await firstValueFrom(
	///   from(truapi.preimage.lookupSubscribe({ request: { key: submitted.value } })),
	/// );
	/// assert(item.value === value, "preimage lookup returned the wrong value:", item);
	/// console.log("preimage lookup received:", item);
	/// ```
	#[wire(id = 0)]
	async fn lookup_subscribe(
		&self,
		_cx: &CallContext,
		_request: RemotePreimageLookupSubscribeRequest,
	) -> Subscription<
		RemotePreimageLookupSubscribeItem,
		CallError<RemotePreimageLookupSubscribeError>,
	> {
		Subscription::interrupted(CallError::unavailable())
	}

	/// Submit a preimage. Returns the preimage key (hash) on success.
	///
	/// Requires the `PreimageSubmit` permission and raises no per-upload
	/// confirmation. `AllowAlways` covers every upload; `AllowOnce` covers a
	/// single upload. Allocating or renewing the Bulletin allowance may still
	/// ask the paired wallet.
	///
	/// ```ts
	/// const value = `0x${crypto.getRandomValues(new Uint8Array(4)).toHex()}` as `0x${string}`;
	/// const result = await truapi.preimage.submit(value);
	/// assert(result.isOk(), "submit failed:", result);
	/// console.log("preimage submitted:", result.value);
	/// ```
	#[wire(id = 1)]
	async fn submit(
		&self,
		_cx: &CallContext,
		_request: RemotePreimageSubmitRequest,
	) -> Result<RemotePreimageSubmitResponse, CallError<RemotePreimageSubmitError>> {
		Err(CallError::unavailable())
	}

	/// Read a preimage once, through a route that the product chooses. Report
	/// how the host got it: the source that served it, and every source that
	/// the host asked. A miss answers no value. A host without this
	/// capability answers `Unsupported`, and the product can use
	/// `lookupSubscribe` instead. The trait keeps wire id 2 free for
	/// `Preimage.retain`.
	///
	/// ```ts
	/// const value = `0x${crypto.getRandomValues(new Uint8Array(4)).toHex()}` as `0x${string}`;
	/// const submitted = await truapi.preimage.submit(value);
	/// assert(submitted.isOk(), "submit failed:", submitted);
	///
	/// const read = await truapi.preimage.read({
	///   key: submitted.value,
	///   route: { tag: "Auto" },
	///   skipHostCaches: false,
	/// });
	/// if (read.isErr()) {
	///   console.log("this host cannot report reads:", read.error);
	/// } else {
	///   assert(read.value.value === value, "preimage read returned the wrong value:", read.value);
	///   console.log("preimage read report:", read.value.report);
	/// }
	/// ```
	#[wire(id = 3)]
	async fn read(
		&self,
		_cx: &CallContext,
		_request: RemotePreimageReadRequest,
	) -> Result<RemotePreimageReadResponse, CallError<RemotePreimageReadError>> {
		Err(CallError::unavailable())
	}

	/// Read one file of a BitTorrent torrent once, through cache providers.
	/// A cache provider gets the file from the swarm, or from its own store or
	/// another provider when it has the file already. The host checks the file
	/// against the piece hashes of the torrent, and the answer has the file,
	/// its preimage key and a report as for `read`. The cache providers then
	/// also keep the file under its preimage key, so the product can read it
	/// again with `read` and that key. While a provider still downloads the file, the answer
	/// has no value and a `progress`: the product calls again, for the file or a newer progress.
	/// A host without this capability answers `Unsupported`.
	///
	/// ```ts
	/// const read = await truapi.preimage.readTorrent({
	///   magnet: "magnet:?xt=urn:btih:2befcb908a1be306ba26451f75d51e83a5f43d36",
	///   file: undefined,
	///   route: { tag: "Cache" },
	/// });
	/// if (read.isErr()) {
	///   console.log("this host cannot read torrents:", read.error);
	/// } else {
	///   console.log("torrent read report:", read.value.report, "key:", read.value.key);
	/// }
	/// ```
	#[wire(id = 4)]
	async fn read_torrent(
		&self,
		_cx: &CallContext,
		_request: RemotePreimageReadTorrentRequest,
	) -> Result<RemotePreimageReadTorrentResponse, CallError<RemotePreimageReadError>> {
		Err(CallError::unavailable())
	}

	/// The cache account of the payer with which the host pays cache providers
	/// for this product: what is left of its free allowance today, its paid
	/// credits, and the prices of reads and of retention. The host asks a cache
	/// provider, which asks its ledger. A host without cache providers answers
	/// `Unsupported`.
	///
	/// ```ts
	/// const account = await truapi.preimage.cacheAccount({ provider: undefined });
	/// if (account.isOk()) {
	///   console.log("allowance left:", account.value.allowance, "paid:", account.value.paid);
	/// }
	/// ```
	#[wire(id = 5)]
	async fn cache_account(
		&self,
		_cx: &CallContext,
		_request: RemotePreimageCacheAccountRequest,
	) -> Result<RemotePreimageCacheAccountResponse, CallError<RemotePreimageCacheAccountError>> {
		Err(CallError::unavailable())
	}

	/// Keep one or more items on cache providers, paid from the cache payer of
	/// the product. An item is a preimage key, a CID that Bulletin has, or a
	/// DotNS name, which the host resolves to the CID of its contenthash. The
	/// host asks the person for the permission `PreimageRetain` once, and shows
	/// one confirmation with the size, the period, the provider and the price of
	/// every item before it pays. A provider fetches the content (for a UnixFS
	/// file, the root and all its pieces), checks it against its CID and keeps it
	/// until the end. A dry run answers the size, the provider and the price of
	/// each item, keeps nothing, and asks for nothing. Retention is best effort:
	/// Bulletin stays the durable copy. A host without cache providers answers
	/// `Unsupported`.
	///
	/// ```ts
	/// const quote = await truapi.preimage.retain({
	///   items: [{
	///     target: { tag: "DotNs", value: "site.paseo" },
	///     periodSecs: 7n * 24n * 3600n,
	///     size: undefined,
	///     provider: undefined,
	///     renew: false,
	///   }],
	///   dryRun: true,
	/// });
	/// if (quote.isOk()) {
	///   console.log("price", quote.value.items[0].price, "on", quote.value.items[0].region);
	/// }
	/// ```
	#[wire(id = 2)]
	async fn retain(
		&self,
		_cx: &CallContext,
		_request: RemotePreimageRetainRequest,
	) -> Result<RemotePreimageRetainResponse, CallError<RemotePreimageRetainError>> {
		Err(CallError::unavailable())
	}

	/// The cache providers of the host, in its order for `target`: the home
	/// providers of the target first, then by the time that the host measured.
	/// A product shows them so that a person can pick the provider of a
	/// retention. A host without cache providers answers `Unsupported`.
	///
	/// ```ts
	/// const providers = await truapi.preimage.cacheProviders({ target: undefined });
	/// if (providers.isOk()) {
	///   console.log(providers.value.providers.map(provider => provider.region));
	/// }
	/// ```
	#[wire(id = 6)]
	async fn cache_providers(
		&self,
		_cx: &CallContext,
		_request: RemotePreimageCacheProvidersRequest,
	) -> Result<RemotePreimageCacheProvidersResponse, CallError<RemotePreimageReadError>> {
		Err(CallError::unavailable())
	}

	/// The retentions of the cache payer of this product: what the ledger charged
	/// for each content and provider, and what each provider holds now. A host
	/// without cache providers answers `Unsupported`.
	///
	/// ```ts
	/// const kept = await truapi.preimage.retentions({ provider: undefined });
	/// if (kept.isOk()) {
	///   console.log(kept.value.retentions.length, "retentions");
	/// }
	/// ```
	#[wire(id = 7)]
	async fn retentions(
		&self,
		_cx: &CallContext,
		_request: RemotePreimageRetentionsRequest,
	) -> Result<RemotePreimageRetentionsResponse, CallError<RemotePreimageReadError>> {
		Err(CallError::unavailable())
	}

	/// Stop the renewal of a retention. The provider keeps the content until the
	/// end of the day that the payer paid for. Needs the permission
	/// `PreimageRetain`. A host without cache providers answers `Unsupported`.
	///
	/// ```ts
	/// const stopped = await truapi.preimage.stopRetention({ cid: "bafy…", provider: "0x…" });
	/// if (stopped.isOk()) {
	///   console.log("kept until", stopped.value.until);
	/// }
	/// ```
	#[wire(id = 8)]
	async fn stop_retention(
		&self,
		_cx: &CallContext,
		_request: RemotePreimageStopRetentionRequest,
	) -> Result<RemotePreimageStopRetentionResponse, CallError<RemotePreimageRetainError>> {
		Err(CallError::unavailable())
	}
}
