---
title: "Statement Routes and Product Wake"
owner: "@replghost"
status: draft
---

# RFC 0030 — Statement Routes and Product Wake

## Summary

A `Routes` host API where a product durably registers a statement-store topic
filter, and the host — which already follows the statement store — matches it
while the product is not running, starts the product's worker, and hands over
the matched statements. One primitive replaces every per-feature inbound path: a
message arriving for a closed chat, a cheque arriving for a purse, a call
invitation arriving for a contact.

## Motivation

Today a product only learns about a statement it subscribed to, and
`statement_store.subscribe` is a live subscription: it exists while the product
runs and ends when it stops. Nothing survives the product, so a product that is
not open misses everything addressed to it and rediscovers it on next launch.

[Worker Lifecycle](worker-lifecycle.md) makes that structural, not incidental.
The worker is demand-driven, the host may stop it whenever nothing references
it, it has "no way to do background work of its own", scheduled wakeups are out
of scope, and an always-on worker was considered and dropped. Its reference
table holds a worker for a chat room on screen, a pocket artifact on screen, a
funding flow in flight, and an input round in flight — every holder is
*something already happening on this device*. There is no holder for *a remote
party addressed this product*.

So each feature that needs inbound delivery is pushed toward its own bespoke
mechanism, and each would have to answer the same questions — who runs, with
what, for how long, how often a stranger may trigger it — separately and
differently. `paritytech/getcash-community` shows what a product does in the
meantime: it holds a host-specific keep-alive operation while it has work,
exports `onEvent("background.wake")`, and notes that without the operation API
"the worker lives only as long as a surface is open". That is a workaround for a
missing contract, and it is not in TrUAPI.

## Approach

### A route

A **route** is a durable registration: a topic filter, the product that
registered it, and a delivery policy. It is the same `TopicFilter` the live
subscription uses (`MatchAll` ≤ 4 topics, `MatchAny` ≤ 128, per RFC 0008), so a
product describes inbound traffic exactly as it already describes a
subscription, and the host enforces the same bounds.

A route is registered once and survives product restarts, host restarts, and
reboots until the product removes it or the host revokes it. Registration is
idempotent on the filter.

Topics are opaque 32-byte values. The host matches bytes and never interprets
them, so a route tells the host nothing about who the peer is or what the
statement says: it delivers signed statements, and the product decrypts. A
product may only register topics it can derive — the host scopes a route to the
registering product and hands its matches to nobody else.

### Delivery

On a match the host:

1. forms a Worker Lifecycle reference for that product, which starts the worker
   if it is not running;
2. delivers the matched statements to the product, to the surface if one is
   open, otherwise to the worker;
3. holds the reference until the product acknowledges the delivery or the
   delivery deadline passes, then releases it as any other holder;
4. advances a per-route cursor, so a product that was unreachable receives the
   matches it missed on the next start, in order, once.

Delivery is at-least-once with a durable cursor; a product must tolerate a
repeat. A route the product cannot service — a crash loop, a deadline missed
repeatedly — is suspended rather than retried forever, and reported as such.

### Limits, and why a route is a permission

A route lets a remote party start a product's code and, through the notification
path, interrupt the user. That is a capability, not a subscription, so:

- The user sees routes, per product, and can revoke them.
- The host rate-limits wakes per route and per originating peer where it can
  distinguish one, and coalesces bursts into a single wake carrying several
  statements.
- Wake budgets are host policy, visible to the product only as a delivery being
  coalesced or delayed.
- A suspended or revoked route stops delivering without ending the product's
  live subscriptions.

The host cannot decide whether a particular sender deserves the user's
attention: topics are opaque and senders are a product concept. The product
decides, on the material it already authenticates, and the host's job is to
bound how often it can be asked.

### What this gives the features that need it

- **Chat.** Messages addressed to a closed conversation arrive; the worker
  decrypts, stores, and notifies.
- **RFC 0029 media sessions.** An invitation is one statement on a route. The
  worker rings and raises the surface; the surface answers. No media-specific
  inbound path exists, and none is needed.
- **RFC 0017 coin payments.** `listen_for_payment` stops requiring an open
  product for a cheque to be noticed.

## Trade-offs

- A durable route is standing state per product, and the host pays for matching
  it forever. A cap per product is unavoidable.
- At-least-once delivery with a cursor forces idempotent handling on products
  that would prefer exactly-once.
- Coalescing and rate limiting make wake timing a host policy, so a product
  cannot promise its user a ring latency.
- This is deliberately not a general background-execution API: a route buys a
  wake with work to do, not arbitrary scheduled time. Products wanting timers
  are still out of scope.
- Route registration reveals to the host that a product cares about a set of
  topics. It is opaque, but it is a stable per-product fingerprint the host could
  correlate over time.

## Open questions

- Should a route be able to prefer the surface — raise the app rather than run
  the worker — for cases like a call, or does the product always raise the
  surface itself after a worker wake?
- Does the cursor belong to the host or to the product? Host-held is simpler and
  survives reinstall; product-held avoids the host retaining a per-route pointer.
- How is a route revoked when its topics stop being reachable — for example a
  Chat contact removed — given the host cannot tell what a topic means?
- Does a route need an expiry, so an abandoned product stops costing matching
  work without the user having to find it?
