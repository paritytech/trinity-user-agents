---
title: "The cache account of a product via Preimage.cacheAccount, and the price of a read"
owner: "Leonardo Custodio"
status: draft
---

# RFC: The cache account of a product via `Preimage.cacheAccount`, and the price of a read

|                 |                                                                                                                                                       |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Start Date**  | 2026-10-14                                                                                                                                            |
| **Description** | Add a `cacheAccount` method to the `Preimage` trait, and the price of each cache read to the read report. A product can show what the cache costs. |
| **Authors**     | Leonardo Custodio                                                                                                                                     |

## Summary

A host pays cache providers for each read and for retention, with a payer key for each product
(`//allowance//cache//{product}`). The ledger of the cache gives each account two tiers: a free allowance that refills
each day (a stand-in for the Universal Service Allowance of a person) and paid credits. A product cannot see any of
this today. This RFC adds:

- `price: Option<u64>` to `PreimageReadSource::CacheProvider`: the price of the read that the provider gave
  (`x-cache-price`), which the host pays with its receipt.
- `Preimage.cacheAccount` (wire id 5 of trait 11): the account of the payer of the product, as the ledger of a cache
  provider gives it (`GET /account/<payer>` of the provider), and the prices of that ledger.

An old host answers `Unsupported`, and a product shows no wallet.

## Wire

```rust
#[wire(id = 5)]
async fn cache_account(
	&self,
	_cx: &CallContext,
	_request: RemotePreimageCacheAccountRequest,
) -> Result<RemotePreimageCacheAccountResponse, CallError<RemotePreimageReadError>>;

pub struct RemotePreimageCacheAccountRequest {
	/// The cache provider to ask. `None` lets the host pick one from its order for reads.
	pub provider: Option<[u8; 32]>,
}

pub struct RemotePreimageCacheAccountResponse {
	pub payer: [u8; 32],
	pub provider: [u8; 32],
	pub allowance: u64,
	pub allowance_retention: u64,
	pub paid: u64,
	pub allowance_per_day: u64,
	pub retention_allowance_per_day: u64,
	pub read_price: u64,
	pub retention_price: u64,
}
```

The error is `PreimageReadError` of `Preimage.read`: `NoCacheProviders` when the host has no provider to ask,
`UnknownProvider` for a provider outside its set, and `Unknown` when it has no payer or the provider does not answer.

## Platform

`PreimageReadHost` gets a third method, `cache_account(request)`. A web host gives `preimageRead.cacheAccount`; the
CLI host asks `/account/<payer>` of the first provider of its order, or of the provider of the request.

## Trust

The answer is what a cache provider says its ledger says. A product shows it, and does not decide anything with it. A
provider cannot spend from the account: every charge needs a receipt that the host signs after it checked the bytes,
or a retention authorization that the person confirmed. When the ledger is the shared allowance service of Metanode, a
host asks it directly instead of a provider.

## Privacy

The payer key is for one product, so the account does not link one product of a person to another. A provider learns
that a payer asks for its account, which it learns from each read already.

## Open questions

- The units: "credits" until the allowance service names them.
- Whether a product may ask for the account of another provider, or only the host's choice.
