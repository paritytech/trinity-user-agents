---
title: "Shared Chain Follow"
owner: "@TarikGul"
status: draft
---

# RFC — Shared Chain Follow

## Summary

The core keeps one `chainHead_v1_follow` per chain for its own reads. dotNS lookups borrow a pinned block from it instead
of reading over legacy JSON-RPC. Products and the Subxt follow used for transactions are unchanged.

## Motivation

dotNS lookups (manifests, identities, public profiles) read Asset Hub over legacy JSON-RPC at the best block, so they
open no follow. On a full node this is fast, and it is what iOS and Android use. On the light client, which the browser
host and the CLI use, legacy `state_call` makes smoldot fetch the runtime code for each new block hash before it can run
the call. A burst of 20 lookups takes 10 to 14 s there, close to the 10 s step budget, and a single lookup is slower than
it would be on chainHead.

A follow per lookup is not the answer either. A connection allows 4 follows on a full node and 2 on smoldot, and a burst
of lookups fills that and fails the rest. The phones hit this in the field before dotNS moved to legacy reads.

The core needs chainHead reads without spending a follow per read.

### Requirements

1. **One follow.** Core reads hold one follow per chain, whatever the burst.
2. **One block per lookup.** Every read in a lookup sees the same state.
3. **One path.** The shared follow replaces the legacy dotNS reads rather than sitting beside them.
4. **No protocol change.** Products see nothing new.

## Approach

Each chain connection in the core gets a hub that owns one follow. The hub opens the follow when the first lookup asks
for it and drains its events on a background task tied to the connection, so dropping the connection stops it.

A lookup takes a lease from the hub. The lease pins the best block when its first read runs and never moves, so every
read in the lookup sees one state. It implements the existing `DotnsTransport` and `RuntimeApiCaller`, so the manifest,
identity and profile code take a lease where they open a `DotnsLookup` today, and `DotnsLookup` goes away. Dropping the
lease releases its block, and a lease lives well under the node's 60 s limit on old pins.

The hub unpins every block that is not the best block, the finalized block, or held by a lease, as events arrive. Pins
stay at the leased blocks even when finality stalls.

Storage reads and runtime calls share one limit per follow, sized below the node's operation budget. It works like the
request limit in `HostRpcClient`: a slot is held until the operation finishes or its `chainHead_v1_stopOperation` is
answered, and a dropped lease stops its operations.

When the follow stops, reads in flight fail with a typed loss error and the lookup runs once more on a new follow. The
hub opens that follow only when a lease asks for it.

The hub lives as long as its connection. The phones close the core's sockets in the background, so there it lasts one
foreground session and the first lookup after a resume opens the follow.

The core then holds two follows per chain, the hub and Subxt. That is exactly smoldot's cap, so nothing else in the core
may open a follow. The change adds no trait and no dependency.

## Trade-offs

- **Shared fate.** Any stop ends every lease at once. Unpinning on every event, a bounded lease age and one rerun keep
  that rare and recoverable.
- **Cold start.** The first lookup on a cold light client waits for sync and one runtime compile.
- **Two follows, not one.** dotNS on the Subxt follow would save one, but Subxt has no operation limit and retries
  `LimitReached` without backoff, so a lookup burst would starve transactions.
- **Dropped: keep legacy reads.** Fine on full nodes, slow on the light client.
- **Dropped: a follow per lookup.** Fills the follow budget under a burst.
- **Dropped: a queue in front of profile reads.** Caps one caller and leaves the cause for the next reader.
- **Dropped: one follow shared with products.** Products have their own connections and budgets, and pooling them lets
  one product's leak stop the follow for everyone.
- **Not covered.** Product connections that refill their follow budget after a reconnect.

## Open questions

- **Idle unfollow.** Should the hub unfollow after a grace period with no lease held? It saves a follow while idle but
  adds a state where the next lease waits for the unfollow to be answered. On smoldot the slot is spoken for either way.
- **First follow on a phone light client.** How long sync plus runtime compile takes there sets the hub's startup bound.
  Phones use full nodes today, so this only matters if that changes.
