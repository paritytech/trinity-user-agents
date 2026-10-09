---
title: "Preimage reads with a route and a read report via Preimage.read"
owner: "Leonardo Custodio"
status: draft
---

# RFC: Preimage reads with a route and a read report via `Preimage.read`

|                 |                                                                                                                                                                                                                                                   |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Start Date**  | 2026-10-09                                                                                                                                                                                                                                        |
| **Description** | Add a `read` method to the `Preimage` trait. It reads a preimage once, through a route that the product chooses (host default, Bulletin only, cache providers only, or one provider), and returns the value with a report of how the host got it. |
| **Authors**     | Leonardo Custodio                                                                                                                                                                                                                                 |

## Summary

`Preimage.lookupSubscribe` returns the bytes of a preimage and nothing else. The host decides where it reads: from its
own caches, from cache providers, or from Bulletin. That is right for most products. A product that shows or measures
the cache needs more: a demo or a diagnostics view must choose the route and must learn which source answered, how long
it took, and which tries failed.

This RFC adds `Preimage.read`, a request with a response:

- The request names the key, a route, and whether the host may answer from its own caches.
- The response carries the value, or no value, and a read report.
- An old host answers `Unsupported`, so a product can fall back to `lookupSubscribe`.

## Motivation

The cache demo product (`cache/docs/spa-demo-plan.md`) shows what a cache provider brings. It uploads a file, then reads
it in two ways, directly from Bulletin and through cache providers. For each read it shows the time taken, the provider
that served it, the route, and whether the provider already had the content or fetched it from Bulletin first. None of
that is possible today:

- `lookupSubscribe` answers with `{ value }` only. The product cannot tell a provider from Bulletin, or a host cache
  from the network.
- The product cannot ask for Bulletin only, or for one provider. A comparison needs both.
- After a submit, the core answers lookups from its read-after-write cache (`PREIMAGE_CACHE_MAX_BYTES`, 16 MiB). A
  product that reads its own upload never measures the network.
- `lookupSubscribe` is a subscription that waits for a value. A host that reads Bulletin polls (the CLI every 6 s, dotli
  after 1 s and then every 10 s), so the time to the first value says more about the poll interval than about the
  source.

The information exists in the host. The CLI host's cache path (`truapi-host-cli/src/cache_lookup.rs`) knows the
provider, its rank, whether it is a home node, the latency, and the origin header of the provider (`local`, `peer:<id>`
or `source`). Today it only logs them.

## Approach

### Method

`Preimage.read`, wire id 3 of trait 11. Id 2 is proposed for `Preimage.retain` (`preimage-retain.md`). A new method, not
a V2 of `lookupSubscribe`, for these reasons:

- The generated client always sends the newest version of a method. A V2 lookup would therefore break every product on
  older hosts: they cannot decode V2 and answer `MalformedFrame`.
- A new method on an old host gets `UnsupportedMessage`, which the TypeScript client turns into `Unsupported`. A product
  can detect that and fall back.
- A read is one try through one route. It is not a wait for a value that may appear later, so a request with a response
  fits better than a subscription.

```rust
/// Read a preimage once, through `route`, and report how the host got it.
#[wire(id = 3)]
async fn read(
    &self,
    _cx: &CallContext,
    _request: RemotePreimageReadRequest,
) -> Result<RemotePreimageReadResponse, CallError<RemotePreimageReadError>> {
    Err(CallError::unavailable())
}
```

The `v01` types:

```rust
/// Read a preimage once.
pub struct RemotePreimageReadRequest {
    /// The preimage key: blake2b-256 of the value.
    pub key: Vec<u8>,
    /// Where the host reads.
    pub route: PreimageReadRoute,
    /// `false`: the host can answer from its own caches (the read-after-write cache, a platform cache), and the
    /// report then says so. `true`: the host reads through the route, to measure the network.
    pub skip_host_caches: bool,
}

/// Where the host reads.
pub enum PreimageReadRoute {
    /// The host's normal order, as for `lookupSubscribe`.
    Auto,
    /// Bulletin only, never a cache provider.
    Bulletin,
    /// Cache providers only, in the host's order, with no Bulletin fallback.
    Cache,
    /// This cache provider only, by its endpoint id.
    CacheProvider { id: [u8; 32] },
}

/// The value, when a source had it, and how the host got it.
pub struct RemotePreimageReadResponse {
    /// The value. The host checked it against the key. `None`: no source on the route had it.
    pub value: Option<Vec<u8>>,
    pub report: PreimageReadReport,
}

pub struct PreimageReadReport {
    /// The source of `value`, `None` when no source had it.
    pub served_by: Option<PreimageReadSource>,
    /// Every source that the host asked, in order, the one that served included.
    pub attempts: Vec<PreimageReadAttempt>,
    /// The time from the request to the response inside the host, in milliseconds.
    pub host_ms: u32,
}

pub enum PreimageReadSource {
    /// A cache of the host itself: the core's read-after-write cache or a platform cache.
    HostCache,
    /// A cache provider.
    CacheProvider {
        /// The endpoint id of the provider.
        id: [u8; 32],
        /// The provider's display name and region, from the provider set, when it has them.
        name: Option<String>,
        region: Option<String>,
        /// Where the provider got the content.
        origin: CacheOrigin,
        /// The provider's position in the host's order for this read, from 0.
        rank: u32,
        /// Whether the provider is a home node of the key.
        home: bool,
        /// The time that the provider reports for its own work, in milliseconds.
        provider_ms: Option<u32>,
        /// The provider's trace of the steps it took (homes asked, peers tried, Bulletin fetch), as JSON. The host
        /// passes it on unread, so the trace format can change without a protocol change.
        trace: Option<String>,
    },
    /// Bulletin, through the host's own Bulletin client.
    Bulletin { via: BulletinReadVia },
}

/// Where a cache provider got the content.
pub enum CacheOrigin {
    /// From its own store: it had the content already.
    Local,
    /// From another provider, by its endpoint id.
    Peer { id: [u8; 32] },
    /// From Bulletin, during this read.
    Source,
    /// The provider did not say.
    Unknown,
}

pub enum BulletinReadVia {
    /// `bitswap_v1_get` over a Bulletin node's JSON-RPC.
    Rpc,
    /// Bitswap through a light client or an IPFS node.
    Bitswap,
    /// An IPFS HTTP gateway.
    Gateway,
}

pub struct PreimageReadAttempt {
    pub source: PreimageReadAttemptSource,
    pub outcome: PreimageReadOutcome,
    /// The time of this attempt, in milliseconds.
    pub ms: u32,
}

pub enum PreimageReadAttemptSource {
    HostCache,
    CacheProvider { id: [u8; 32] },
    Bulletin { via: BulletinReadVia },
}

pub enum PreimageReadOutcome {
    /// This source sent the value.
    Served,
    /// This source does not have the value.
    Miss,
    /// The bytes did not hash to the key. The host dropped them and paid nothing.
    BadBytes,
    /// The source refused, for example a provider that the payer cannot pay.
    Refused { reason: String },
    /// The source failed: unreachable, timed out, or an error.
    Failed { reason: String },
}

pub enum PreimageReadError {
    /// `CacheProvider { id }` names no provider of the host's provider set.
    UnknownProvider,
    /// The route needs cache providers, and the host has none.
    NoCacheProviders,
    /// Catch-all.
    Unknown { reason: String },
}
```

A product, in TypeScript:

```ts
const result = await truapi.preimage.read({
  key,
  route: { tag: "Cache" },
  skipHostCaches: true,
});
if (result.isErr()) {
  // `result.error` is a CallErrorValue: "Unsupported" on a host without the capability (read with
  // lookupSubscribe instead), or a domain error such as UnknownProvider.
} else {
  const { value, report } = result.value;
  // value: `0x…` or undefined. report.servedBy: { tag: "CacheProvider", value: { id, name, region,
  // origin: { tag: "Source" }, rank: 0, home: true, providerMs, trace } }, and report.attempts, report.hostMs.
}
```

The generated client takes the request object directly: `key` is `0x` hex, every `id` is 32 bytes of `0x` hex, and the
enums are `{ tag, value }` objects as elsewhere in `@parity/truapi`.

### Semantics

- A read is one pass through the route, with no polling. A miss answers `value: None` with the report of every attempt.
  A product that waits for a fresh upload to land keeps using `lookupSubscribe`.
- The host checks the value against the key before it answers, as for `lookupSubscribe`. A source that sends bad bytes
  is an attempt with `BadBytes`, and the host tries the next source on the route.
- `skip_host_caches: false` lets the core answer from its read-after-write cache, with `served_by: HostCache`. Platform
  caches report the same. With `true`, the core skips its cache and asks the platform to skip its own.
- `Cache` and `CacheProvider` never fall back to Bulletin. `Auto` does, as `lookupSubscribe` does.
- The host pays a provider that served, as it does for `lookupSubscribe`. A report never contains payment secrets: no
  signatures, and no transfer ids.

### Permission and confirmation

None, as for `lookupSubscribe`. A read costs the user's cache allowance only when a provider serves. Two limits keep a
product from spending it fast. First, providers refuse a payer that owes receipts (8 unpaid reads in the prototype).
Second, a host may rate-limit `read` with `skip_host_caches: true`.

### Host side

- A new optional platform capability, `PreimageReadHost`, listed on `OptionalPlatform` like `GamePlatform`. A host that
  does not install it changes nothing, and products get `Unsupported`:

  ```rust
  #[async_trait]
  pub trait PreimageReadHost: Send + Sync {
      async fn read_preimage(
          &self,
          key: Vec<u8>,
          route: PreimageReadRoute,
          skip_host_caches: bool,
      ) -> Result<RemotePreimageReadResponse, PreimageReadError>;
  }
  ```

  A Rust host installs it with `set_preimage_read_host` on its pairing or signing host runtime. A web host passes an
  optional callback group, `preimageRead?: PreimageReadHost`, with
  `readPreimage(key: Uint8Array, route: PreimageReadRoute, skipHostCaches: boolean): Promise<RemotePreimageReadResponse>`.
  An error that the callback throws reaches the product as `PreimageReadError.Unknown { reason }`. A capability in its
  own trait, rather than a new method on `PreimageHost`, means existing hosts (iOS, Android, web) compile and run
  unchanged.

- The core handles `read`. No session and no permission are needed, as for a lookup:
  1. Answer from the read-after-write cache, unless `skip_host_caches` is set.
  2. Otherwise call `read_preimage`, or answer `Unsupported` without an installed `PreimageReadHost`.
  3. Check the value against the key. On a mismatch, drop the value and mark the attempt that served `BadBytes`.
  4. Set `host_ms` to the time the core measured.
- The CLI host fills the report from `CacheNodes`: provider, rank, home flag, the time of each attempt, the
  `x-cache-origin` header (`local`, `source`, or `peer:` and a full endpoint id; anything else is `Unknown`),
  `x-cache-elapsed-ms` as `provider_ms`, and `x-cache-trace` passed on as `trace`. A provider line of
  `TRUAPI_CACHE_PROVIDERS` can carry `name=<text>` and `region=<text>`. Bulletin reads report `Bulletin { via: Rpc }`,
  from its `bitswap_v1_get` client. Without a payer, a route through cache providers answers `NoCacheProviders`, and
  `Auto` reads Bulletin.
- A web host (dotli) fills it from its preimage adapter. That adapter reads through cache providers when the user turns
  them on, and otherwise through bitswap or a gateway.

```text
product                       host core                         platform (CLI / dotli)          cache provider
   | Preimage.read(key, Cache,   |                                   |                               |
   |   skipHostCaches)           |                                   |                               |
   |---------------------------->| skip read-after-write cache       |                               |
   |                             |-- read_preimage(key, Cache) ----->| order providers (QoS, homes)  |
   |                             |                                   |-- POST /acquire (signed) ---->|
   |                             |                                   |<-- bytes, x-cache-origin, ----|
   |                             |                                   |    x-cache-trace              |
   |                             |                                   | check bytes, pay receipt      |
   |                             |<-- value + report ----------------|                               |
   |                             | check value against key, host_ms  |                               |
   |<-- { value, report } -------|                                   |                               |
```

## Trade-offs

- A new method and not an option of `lookupSubscribe`: products on old hosts keep working, and the report does not add
  weight to every lookup.
- The report shows the host's sources to the product. Products already learn, from timing, whether a read was fast. The
  report adds provider ids, names and regions, which are public in the provider set. It holds no payer data.
- The provider trace is opaque JSON. The protocol stays stable while the cache prototype changes its trace. The cost is
  that products must parse it themselves.
- `skip_host_caches` makes reads more expensive for the user. It exists to measure, and a host may rate-limit it.

## Open questions

- Should `read` also exist as a subscription that reports each poll, for products that wait for an upload to land?
- Should the report carry the payment for the read (the amount, and whether the provider took the receipt)? That needs
  the host to wait for the settlement.
- Should a product be able to list the host's cache providers and their measured quality, the data of the CLI's `/cache`
  command?
- How should a host rate-limit `read` with `skip_host_caches: true`?
