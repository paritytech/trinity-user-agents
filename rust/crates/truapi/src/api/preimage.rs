//! Unified [`Preimage`] trait.

use crate::{
	CallContext, CallError, Subscription,
	versioned::preimage::{
		RemotePreimageCacheAccountRequest, RemotePreimageCacheAccountResponse,
		RemotePreimageLookupSubscribeError, RemotePreimageLookupSubscribeItem,
		RemotePreimageLookupSubscribeRequest, RemotePreimageReadError, RemotePreimageReadRequest,
		RemotePreimageReadResponse, RemotePreimageReadTorrentRequest,
		RemotePreimageReadTorrentResponse, RemotePreimageRetainError, RemotePreimageRetainRequest,
		RemotePreimageRetainResponse, RemotePreimageSubmitError, RemotePreimageSubmitRequest,
		RemotePreimageSubmitResponse,
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
	) -> Result<RemotePreimageCacheAccountResponse, CallError<RemotePreimageReadError>> {
		Err(CallError::unavailable())
	}

	/// Keep a preimage on a cache provider for a period, paid in advance from
	/// the cache payer of the product. The host asks the person for the
	/// permission `PreimageRetain` once, and shows the size, the period and the
	/// price before it pays. The provider fetches the preimage (for a chunked
	/// file, the manifest and its chunks), checks it against its key and keeps it
	/// until `until`. Retention is best effort: Bulletin stays the durable copy.
	/// A host without cache providers answers `Unsupported`.
	///
	/// ```ts
	/// const kept = await truapi.preimage.retain({
	///   key: "0x…",
	///   periodSecs: 7n * 24n * 3600n,
	///   size: undefined,
	///   provider: undefined,
	/// });
	/// if (kept.isOk()) {
	///   console.log("kept until", kept.value.until, "for", kept.value.charged);
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
}
