---
title: "Shared Chain Follow"
owner: "@TarikGul"
status: draft
---

# RFC — Shared Chain Follow

## Summary

dotNS lookups in the core share one `chainHead_v1_follow` per chain and borrow a pinned block from it, instead of
reading over legacy JSON-RPC. That follow sits beside the Subxt follow that transactions and block reads use, so the
core holds at most two follows per chain. Products are unchanged.

## Motivation

dotNS lookups (manifests, identities, public profiles) read Asset Hub over legacy JSON-RPC at the best block, so they
open no follow. On a full node this is fast, and it is what iOS and Android use. On the light client, which the browser
host uses and the CLI uses when asked, each lookup reads at a fresh best block hash, and smoldot downloads the runtime
code, about 2.8 MB on Asset Hub, for each new hash before legacy `state_call` can run. Asset Hub on Paseo Next makes a
block about every 2 s, so almost every lookup lands on a new hash. On a warm native light client a lookup took about
3.5 s on legacy reads against about 2 to 2.5 s on chainHead, and a burst of 20 about 4 s against 3.5 s. A chainHead
follow keeps its runtime across blocks, so lookups on it skip that download. In the browser host this is the identity
lookup for each session and the first open of a product each day.

A follow per lookup is not the answer either. Where the core owns the connection, it allows 4 follows on a full node
and 2 on smoldot, and a burst of lookups fills that and fails the rest, which phones have seen in the field.

The core needs dotNS lookups on the light client to skip that download, without spending a follow per lookup.

### Requirements

1. **Budget.** dotNS lookups hold one follow per chain, whatever the burst.
2. **Consistency.** Every read in a lookup sees the same block.
3. **Single.** The core's dotNS reads take one code path on every host.
4. **Additive.** Products see nothing new.
5. **Fast.** A dotNS lookup on the light client does not wait for the runtime of each new block.

## Approach

The core's own Asset Hub connection gets a follow hub that owns one follow. It uses the follow, storage, call and
unpin support `ChainRuntime` already has, and adds leases, an unpin rule, an operation limit, and routing each
operation's result from the follow back to its read. The hub opens the follow when the first lookup asks for it and
drains its events on a background task.

A lookup takes a lease from the hub. The lease pins the best block when its first read runs and never moves, so every
read in the lookup sees one state. It implements the existing `DotnsTransport` and `RuntimeApiCaller`, so the manifest,
identity and profile code take a lease where they open a `DotnsLookup` today, and `DotnsLookup` goes away. Each lookup
has one deadline that counts its queue time, and a lease past it fails only its lookup, with no rerun. Dropping the
lease releases its block. Leases are short, so pins stay inside smoldot's cap on finalized pins.

The hub unpins a block once it is finalized past or pruned and no lease holds it.

Each storage operation reads one key. Storage reads and runtime calls share one limit per follow, sized below the
node's operation budget, and a slot is held until the operation ends or the follow stops. Lookups for names a product
chooses, such as a manifest behind a cross product grant, share a smaller cap inside that limit, so the host's own
reads always find a slot. One product's burst can slow another product's lookups but never the host's.

When the follow stops, the hub fails the lease's reads and runs the lookup once more on a new follow, inside the same
deadline and counted as one attempt by a caller that already retries. The hub opens that follow only when a lease asks
for it. An operation that fails while the follow stays up fails only its lookup.

The hub lives as long as its connection and keeps its follow while idle, since nothing else in the core may use the
slot and a follow reopens in about a round trip. When a host closes the core's socket, the next lookup opens a new
follow.

On the CLI light client two follows is smoldot's whole cap per connection, so nothing else in the core may open a
follow. The change adds no trait and no dependency.

## Trade-offs

- **Shared fate.** Any stop ends every lease in flight at once. A full node also stops long lived follows when its pin
  pool fills, and one rerun recovers a stop on a running follow. A full node far behind on finality refuses every new
  follow, so lookups fail there until finality catches up, as transactions already do.
- **Phones give up parallel reads.** A full node runs at most 16 operations per follow, so a large burst on iOS and
  Android queues where legacy reads ran side by side. The core does not know which backend a host uses, and telling it
  would need a new signal through every host bridge and a second dotNS path, so phones take the light client path too.
- **Two follows, not one.** dotNS on the Subxt follow would save one, but Subxt puts no limit on its own operations,
  gives up on `LimitReached` after ten instant retries, and never ends a read the node reports as failed or
  inaccessible, so a lookup burst would fail transaction reads.
- **Dropped: a separate core connection for the hub.** It gets its own follow cap, so other core code could still
  follow, but every host pays another socket per chain, including phones where full nodes already allow 4 follows.
- **Dropped: keep legacy reads.** Fine on full nodes, slower on the light client.
- **Dropped: hold one best hash for a short window.** smoldot keeps the runtime of a hash it has seen, so lookups inside
  the window skip the download, but each new window pays it again and reads lag by up to the window, while a follow pays
  it once.
- **Dropped: one follow shared with products.** Products have their own connections and budgets, and pooling them lets
  one product's leak stop the follow for everyone.
- **Later: reuse the tracked runtime in smoldot's legacy `state_call`.** A small smoldot change would make legacy reads
  as fast as chainHead with no follow. Every host runs the workspace's smoldot, so it would reach all of them, but it
  means carrying a patched smoldot until an upstream release has it.
- **Not covered.** Product connections that use up their follow budget again by re-following after a reconnect.
