---
title: "Preimage retention in cache providers via Preimage.retain"
owner: "Leonardo Custodio"
status: draft
---

# RFC: Preimage retention in cache providers via `Preimage.retain`

|                 |                                                                                                                                                                  |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Start Date**  | 2026-10-08                                                                                                                                                       |
| **Description** | Add a `retain` method to the `Preimage` trait so a product can ask its host to keep a preimage in a cache provider for a period, paid from the product's allowance. |
| **Authors**     | Leonardo Custodio                                                                                                                                                |

## Summary

Products store preimages on Bulletin with `Preimage.submit` and read them with `Preimage.lookupSubscribe`. Bulletin keeps
them durably but serves them slowly. Cache providers, a Metanode service, keep verified copies near users and serve them
in milliseconds. This RFC lets a product ask its host to keep a preimage in a cache provider for a period it chooses.
The host confirms with the user, picks the provider, and pays for the retention from the product's allowance. Reads
need no change: `lookupSubscribe` already goes through the host, and a host with cache providers asks them before
Bulletin.

## Motivation

A product that publishes content for many readers (a profile picture, a shared document, a release) wants every read to
be fast, including the first ones. Today it cannot say so:

- `Preimage.submit` stores the value durably on Bulletin, and says nothing about where it is served from.
- A host that reads through cache providers (the CLI host on the `lc/cache-prototype` branch) makes repeat reads fast,
  but until someone reads the content through a provider, every read waits for Bulletin.
- A cache provider sells retention for a period, paid in advance by whoever asks for it. Only the user, through the
  host, can authorize that spending. A product never connects to a provider itself, so it needs a host call.

## Approach

### Method

`Preimage.retain`, wire id 2 of trait 11:

```rust
#[wire(id = 2)]
async fn retain(
    &self,
    _cx: &CallContext,
    _request: RemotePreimageRetainRequest,
) -> Result<RemotePreimageRetainResponse, CallError<RemotePreimageRetainError>> {
    Err(CallError::Unsupported)
}
```

The `v01` types:

```rust
/// Request to keep a preimage on a cache provider for a period, paid in advance.
pub struct RemotePreimageRetainRequest {
    /// The preimage key: blake2b-256 of the value. For a chunked file, the key of its manifest.
    pub key: Vec<u8>,
    /// How long to keep the preimage, in seconds from now.
    pub period_secs: u64,
    /// The size that the retention pays for. For a chunked file, the size of the manifest and all its chunks, so the
    /// provider keeps the chunks too. `None`: the host reads the value to learn its size.
    pub size: Option<u64>,
    /// The cache provider to keep it. `None` lets the host pick one, a home node of the key first.
    pub provider: Option<[u8; 32]>,
}

/// The retention that the host bought.
pub struct RemotePreimageRetainResponse {
    /// Unix seconds until which the provider keeps the preimage.
    pub until: u64,
    /// The provider that keeps it, with its name and region from the provider set.
    pub provider: [u8; 32],
    pub name: Option<String>,
    pub region: Option<String>,
    /// The size that the retention paid for, and the chunks of a chunked file that the provider keeps with it.
    pub size: u64,
    pub chunks: u32,
    /// What the retention cost, in the units of the ledger. A retention that a previous one covers costs 0.
    pub charged: u64,
}

/// Why the host could not keep a preimage.
pub enum PreimageRetainError {
    /// No source has the preimage yet. The product can ask again after its submit lands.
    NotFound,
    /// The period is longer than the providers accept.
    PeriodTooLong { max_secs: u64 },
    /// The payer cannot pay for the period.
    NotAvailable,
    /// The person declined the retention: the permission or the confirmation.
    Declined,
    /// Catch-all.
    Unknown { reason: String },
}
```

A product, in TypeScript:

```ts
const submitted = await truapi.preimage.submit(value);
const kept = await truapi.preimage.retain({
  key: submitted.value,
  periodSecs: 7n * 24n * 3600n,
  size: undefined,
  provider: undefined,
});
```

The size of the request matters for a chunked file. TrUAPI stores a file above 2 MiB as 1 MiB chunks and a manifest,
each a preimage, and a product reads the chunks one by one. A provider that keeps only the manifest serves the chunks
from Bulletin. With the size of the manifest and all its chunks, the provider keeps the chunks too, in one payment
(`cache` `docs/spa-redesign-plan.md`, D4).

### Semantics

- Retention is best effort. A successful answer means that a provider accepted the content, checked it against its key
  and was paid. The provider can still lose it; reads then fall back to Bulletin, which stays the durable copy.
- A retention never ends after the content's Bulletin retention (14 days on Paseo), so a provider never holds the only
  copy. A longer period answers `PeriodTooLong`.
- Retaining again extends the retention, and the host pays only for the time after the current end. A retry of the
  same call is not paid twice.
- Retention is not tied to submit. A product can retain any preimage, for example content that its users read often.

### Permission and confirmation

- A new remote permission, `PreimageRetain`, decided once for each product, as `PreimageSubmit` is.
- A confirmation for each call, with the size, the period and the price, shown by the platform: the platform knows the
  provider and its prices. truapi 0.25 has no `UserConfirmationReview` for preimages, so the core asks only for the
  permission. A platform that does not show a confirmation (the CLI host) relies on the permission.

### Host side

- A new method of the platform capability for cache providers, `PreimageReadHost::retain_preimage(request)`, next to
  `read_preimage`, `read_torrent` and `cache_account`. A host without cache providers has no such capability and
  answers `Unsupported`.
- The core checks the key and the permission. The platform does the provider part.
- The CLI host (`truapi-host-cli/src/cache_lookup.rs`) picks a provider in the same order as reads (the key's home nodes
  first, then measured latency), or the provider of the request. It signs a retention authorization with its cache
  payer (`//allowance//cache//{product}` of the signed-in account, or a test seed) and sends it to `POST /pin`. A
  refused payer is `NotAvailable`, a key that no source has is `NotFound`, and a window longer than the provider keeps
  content is `PeriodTooLong`.
- dotli shows the size, the period and the price before it signs, and pays with its cache payer.

The messages, with the cache prototype's provider API:

```text
product                 host core / CLI platform                      cache provider            coordinator (ledger)
   |  Preimage.retain(key, period)  |                                       |                          |
   |------------------------------->| permission, confirmation              |                          |
   |                                | sign authorization:                   |                          |
   |                                |   {transfer, payer, provider, cid,    |                          |
   |                                |    Retention, size, from = now,       |                          |
   |                                |    until = now + period}              |                          |
   |                                |---- POST /pin {bulletin:<cid>, ------>|                          |
   |                                |     authorization}                    | fetch: local, peers,     |
   |                                |                                       | Bulletin; check the CID  |
   |                                |                                       |-- settle (signed) ------>|
   |                                |                                       |<-- charged --------------|
   |                                |<--- {until, charged} -----------------|                          |
   |<-- Ok({ until }) --------------|                                       |                          |
```

The authorization is the receipt that the prototype defines in `cache/src/payment.rs`: an sr25519 signature by the payer
over the transfer id, the payer and provider keys, the CID, the service, the size of the content and the window `from`
and `until`. The ledger charges by the signed size, so a provider cannot charge for a larger size, and the provider
refuses the authorization when the content has another size. The host knows the size, because it shows the price in
the confirmation. The retention ends at `until`, a fixed time, so a replay of the authorization cannot extend it. The
provider charges only for the time after now and after the current end, and never more than `until - from`. Only the
provider that the authorization names can ask the ledger for the charge: it signs the request with its endpoint key.
The ledger charges each transfer id once, and a transfer id signed again for another window is a conflict, not a free
retry.

## Trade-offs

- A separate method, not an option of `submit`: a product can retain content it did not upload, and extend a retention
  later, without a new upload.
- The host picks the provider. Products do not know providers and should not; the host already ranks them for reads.
- One provider per call in a first version. More providers give more availability at a higher price. A product that
  needs more can be given a `replicas` field later.
- Payment from the product's allowance account keeps the retention of one product unlinkable to another (RFC 0010),
  at the cost of an allowance for each product that uses it.
- Hosts without cache providers answer `Unsupported` until they get them, so products must treat `retain` as optional.

## Open questions

- How many providers should the host retain on by default, and may a product ask for more?
- How is the cache allowance funded (Universal Service Allowance, dotUSD), and how does the confirmation show the price?
- Should the host renew a retention by itself before it ends, like the statement-store allowance renewal?
- Should a product be told when a provider loses retained content early?
