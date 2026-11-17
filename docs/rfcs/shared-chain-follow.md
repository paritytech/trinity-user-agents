---
title: "Shared Chain Follow"
owner: "@TarikGul"
status: draft
---

# RFC — Shared Chain Follow

## Summary

dotNS lookups in the core share one `chainHead_v1_follow` per chain and borrow a pinned block from it, instead of
reading over legacy JSON-RPC. It sits beside the Subxt follow that transactions and other core reads use, so the core
holds two follows per chain. Products are unchanged.

## Motivation

dotNS lookups (manifests, identities, public profiles) read Asset Hub over legacy JSON-RPC at the best block, so they
open no follow. On a full node this is fast, and it is what iOS and Android use. On the light client, which the browser
host and the CLI use, each lookup reads at a fresh best block hash, and smoldot downloads the multi-MB runtime code for
each new hash before legacy `state_call` can run. In test runs a single lookup took 6 to 12 s there against about 1 s
over RPC, and a burst of 20 took 10 to 14 s. A chainHead follow keeps its runtime across blocks, so lookups on it skip
that download.

A follow per lookup is not the answer either. A connection allows 4 follows on a full node and 2 on smoldot, and a burst
of lookups fills that and fails the rest, which phones have seen in the field.

The core needs chainHead reads without spending a follow per read.

### Requirements

1. **Budget.** dotNS lookups hold one follow per chain, whatever the burst.
2. **Consistency.** Every read in a lookup sees the same block.
3. **Single.** The core's dotNS reads take one code path on every host.
4. **Additive.** Products see nothing new.

## Approach

Each chain connection in the core gets a hub that owns one follow. It uses the follow, storage, call and unpin support
`ChainRuntime` already has, along with its typed follow loss, and adds only leases, pin retention and an operation
limit. The hub opens the follow when the first lookup asks for it and drains its events on a background task tied to
the connection, so it ends when the connection closes.

A lookup takes a lease from the hub. The lease pins the best block when its first read runs and never moves, so every
read in the lookup sees one state. It implements the existing `DotnsTransport` and `RuntimeApiCaller`, so the manifest,
identity and profile code take a lease where they open a `DotnsLookup` today, and `DotnsLookup` goes away. Each lookup
has one deadline that counts its queue time, and a lease past it fails like a loss. Dropping the lease releases its
block. Leases are short, so pins stay inside both the full node's 60 s age limit and smoldot's cap on finalized pins.

The hub unpins every block no lease holds once it is no longer the best block, so pins stay few even when finality
stalls.

Each storage operation reads one key. Storage reads and runtime calls share one limit per follow, sized below the
node's operation budget, and a slot is held until the operation ends or the follow stops. Operations a product
triggers, such as a manifest lookup behind a cross product grant, share a smaller cap inside that limit, so the host's
own reads always find a slot.

When the follow stops, the lease's reads fail and the code that took the lease runs the lookup once more on a new
follow. The hub opens that follow only when a lease asks for it. An operation that fails while the follow stays up fails
only its lookup.

The hub lives as long as its connection. The phones close the core's sockets in the background, so there it lasts one
foreground session and the first lookup after a resume opens the follow.

Two follows per chain is exactly smoldot's cap, so nothing else in the core may open a follow. The change adds no trait
and no dependency.

## Trade-offs

- **Shared fate.** Any stop ends every lease in flight at once. Prompt unpinning and short leases keep the follow inside
  node limits, and one rerun recovers the stops a node forces.
- **Phones give up parallel reads.** A full node runs at most 16 operations per follow, so a large burst on iOS and
  Android queues where legacy reads ran side by side. One path on every host is worth that.
- **Two follows, not one.** dotNS on the Subxt follow would save one, but Subxt puts no limit on its own operations and
  gives up on `LimitReached` after ten instant retries, so a lookup burst would fail transaction reads.
- **Dropped: keep legacy reads.** Fine on full nodes, slow on the light client.
- **Dropped: one best hash shared across a burst.** smoldot downloads the runtime once per block hash, so this shortens a
  burst, but a single lookup still pays that download on each new block.
- **Dropped: reuse the tracked runtime in smoldot's legacy `state_call`.** It would make legacy reads fast on the light
  client, but needs a smoldot fork or an upstream release, while chainHead already does it.
- **Dropped: one follow shared with products.** Products have their own connections and budgets, and pooling them lets
  one product's leak stop the follow for everyone.
- **Not covered.** Product connections that use up their follow budget again by re-following after a reconnect.

## Open questions

- **Idle unfollow.** Should the hub unfollow after a grace period with no lease held? It saves a follow while idle, but
  the next lookup pays a new follow and, on the light client, waits for its runtime again. On smoldot nothing else in
  the core may use the freed slot.
