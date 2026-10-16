---
title: "The cache providers of a host via Preimage.cacheProviders"
owner: "Leonardo Custodio"
status: draft
---

# RFC: The cache providers of a host via `Preimage.cacheProviders`

|                 |                                                                                                                                         |
| --------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| **Start Date**  | 2026-10-15                                                                                                                              |
| **Description** | Add a `cacheProviders` method to the `Preimage` trait, so a product can show the cache providers of the host and a person can pick one. |
| **Authors**     | Leonardo Custodio                                                                                                                       |

## Summary

A host reads through cache providers and keeps content on them (`Preimage.retain`). It picks a provider itself: a home
provider of the content first, then the one with the best measured latency. A person who keeps a site for readers in
one region wants to pick the provider in that region. This RFC adds `Preimage.cacheProviders` (wire id 6 of trait 11):
the providers of the host, in its order for a target, with a name, a region, an operator and what the host measured.
`retain` already takes an optional `provider`; with this list a product can fill it.

## Wire

```rust
#[wire(id = 6)]
async fn cache_providers(
	&self,
	_cx: &CallContext,
	_request: RemotePreimageCacheProvidersRequest,
) -> Result<RemotePreimageCacheProvidersResponse, CallError<RemotePreimageReadError>>;

pub struct RemotePreimageCacheProvidersRequest {
	/// The content that the order is for. The home providers of the target come first. `None` gives the order for
	/// reads in general.
	pub target: Option<PreimageTarget>,
}

pub enum PreimageTarget {
	/// A preimage key: blake2b-256 of the value.
	Key(Vec<u8>),
	/// A CID that Bulletin has, as text.
	Cid(String),
	/// A DotNS name. The host resolves it to the CID of its contenthash.
	DotNs(String),
}

pub struct CacheProviderInfo {
	pub id: [u8; 32],
	pub name: Option<String>,
	pub region: Option<String>,
	pub operator: Option<String>,
	/// `Registry` (the provider registry or the provider set of the host) or `User` (added by the person).
	pub source: CacheProviderSource,
	/// The provider is a home provider of the target.
	pub home: bool,
	pub latency_ms: Option<u32>,
	pub failed_recently: bool,
}

pub struct RemotePreimageCacheProvidersResponse {
	/// The providers, first the one that the host asks first.
	pub providers: Vec<CacheProviderInfo>,
}
```

`PreimageTarget` is also the target of a v0.2 `retain` item (RFC `preimage-retain.md`).

## Semantics

- The order is the order of the host for that target: home providers first (rendezvous hashing of the content id over
  the provider set), then by measured latency, and a provider that failed a short time ago last.
- The list is advice. A product passes one `id` to `retain`; the host refuses an id outside its set with
  `UnknownProvider`.
- No permission: the list is public data of the host (the provider registry is on chain), and it carries nothing about
  the person.

## Platform

`PreimageReadHost::cache_providers(request)`. dotli answers from its provider set and quality store, the CLI host from
its provider set or the registry (`TRUAPI_CACHE_REGISTRY`). A host without cache providers answers `Unsupported`.

## Open questions

- Whether a person can add a provider that is not in the registry (`source: User`), and how a host shows that it is
  not vetted.
- Whether the list should carry the price of each provider, once operators set their own prices.
