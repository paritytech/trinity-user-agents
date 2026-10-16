---
title: "The retentions of a product via Preimage.retentions and Preimage.stopRetention"
owner: "Leonardo Custodio"
status: draft
---

# RFC: The retentions of a product via `Preimage.retentions` and `Preimage.stopRetention`

|                 |                                                                                                                                                         |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Start Date**  | 2026-10-15                                                                                                                                              |
| **Description** | Add `retentions` (what the cache payer of the product keeps, and what each provider holds now) and `stopRetention` (stop a retention that renews each day). |
| **Authors**     | Leonardo Custodio                                                                                                                                       |

## Summary

`Preimage.retain` keeps content on a cache provider. Afterwards a product knows only what it wrote down. A person who
keeps things from two devices, or who lost the local storage of the product, cannot see them, and a product cannot tell
whether a provider still holds the content. The ledger of the cache knows every charge of a payer, and each provider
knows what it holds. This RFC adds two methods to trait 11:

- `Preimage.retentions` (wire id 7): the retentions of the cache payer of the product, from the ledger of a provider,
  with the state of each one on its provider.
- `Preimage.stopRetention` (wire id 8): stop a retention that the provider renews each day. The provider keeps the
  content until the end of the paid day.

## Wire

```rust
#[wire(id = 7)]
async fn retentions(&self, _cx: &CallContext, _request: RemotePreimageRetentionsRequest)
	-> Result<RemotePreimageRetentionsResponse, CallError<RemotePreimageReadError>>;

#[wire(id = 8)]
async fn stop_retention(&self, _cx: &CallContext, _request: RemotePreimageStopRetentionRequest)
	-> Result<RemotePreimageStopRetentionResponse, CallError<RemotePreimageRetainError>>;

pub struct RemotePreimageRetentionsRequest {
	/// The cache provider to ask for the ledger. `None` lets the host pick one.
	pub provider: Option<[u8; 32]>,
}

pub struct PreimageRetention {
	pub cid: String,
	pub provider: [u8; 32],
	pub name: Option<String>,
	pub region: Option<String>,
	pub operator: Option<String>,
	pub size: u64,
	pub pieces: u32,
	pub from: u64,
	/// The end that the payer chose, or for a retention that renews, the end of the paid days.
	pub until: u64,
	/// The end of the paid time.
	pub paid_until: u64,
	/// All that the retentions of this content on this provider cost.
	pub charged: u64,
	/// The price of one day of a retention that the provider charges one day at a time. 0 when paid in advance.
	pub per_day: u64,
	/// The provider renews the retention each day until the payer stops it.
	pub renews: bool,
	/// Kept, Fetching { pieces, total }, Lost (paid, and the provider does not hold it), Ended, or Unknown (the
	/// provider did not answer).
	pub state: RetentionState,
}

pub struct RemotePreimageRetentionsResponse {
	pub payer: [u8; 32],
	/// The retentions, the latest end first.
	pub retentions: Vec<PreimageRetention>,
}

pub struct RemotePreimageStopRetentionRequest {
	pub cid: String,
	pub provider: [u8; 32],
}

pub struct RemotePreimageStopRetentionResponse {
	/// The provider keeps the content until this time, the end of the paid day.
	pub until: u64,
}
```

## Retentions that renew each day

A v0.2 `retain` item with `renew: true` becomes a lease (`preimage-retain.md`): the payer signs one authorization, and
the provider charges one day at a time, at most `max_per_day`, from the free allowance of that day first. A lease has an
end, or renews until the payer stops it. When the ledger refuses a day, the provider keeps the content one more day of
grace, then drops it. A daily allowance can pay a lease, which it cannot do for a retention paid in advance for weeks.

`stopRetention` signs a stop request with the cache payer of the product (`cache-stop/1`: payer, provider, CID, issue
time) and sends it to the provider, which forwards it to its ledger. The ledger charges no more days of the leases of
that payer, content and provider; the paid days stay. A stop of a retention paid in advance changes nothing.

## How the host reads them

The host signs a leases request with the cache payer of the product (`cache-leases/1`: payer and issue time, valid for 5
minutes), so only the payer can list its retentions. A provider forwards it to its ledger (`POST /leases`). For each
retention the host then asks its provider which pieces it holds (`GET /pin-state/<cid>`) and gives the state.

## Permission

`retentions` needs no new permission: it reads what this product paid for. `stopRetention` needs `PreimageRetain`, as
`retain` does, and no confirmation: a stop never costs anything.

## Platform

`PreimageReadHost::retentions(request)` and `PreimageReadHost::stop_retention(request)`. A host without cache providers
answers `Unsupported`.

## Open questions

- Whether the list should include retentions of other payers of the same person (other products), for a wallet-wide
  view. Today it does not: the payer is per product, by design.
- Whether a host should move a lost retention that renews to another provider by itself, or only offer it.
