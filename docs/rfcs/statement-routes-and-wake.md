---
title: "Statement Routes and Product Wake"
owner: "@replghost"
status: draft
---

# RFC — Statement Routes and Product Wake

## Summary

A `Routes` service where a product durably registers a statement-store topic
filter. The host already follows the statement store, so it matches that filter
while the product is not running, starts the product, and hands over what
matched. One primitive covers every inbound case: a message for a closed
conversation, a payment for a purse, a call for a contact.

## Motivation

A product only hears about statements it is subscribed to, and a subscription
lives exactly as long as the product does. Anything addressed to a product that
is closed is missed, and only rediscovered at next launch.

[Worker Lifecycle](worker-lifecycle.md) makes that structural. A worker runs
only while something references it, the host may stop it whenever nothing does,
and it has no way to do background work of its own. Every reference holder in
that RFC is something already happening on the device: a room on screen, an
artifact on screen, a flow in flight. None of them is *a remote party addressed
this product*.

Without that holder, each feature needing inbound delivery invents its own
mechanism and answers the same questions — who runs, with what, for how long,
and how often a stranger may trigger it — differently.

## Approach

### A route

A **route** is a durable registration: a topic filter and the product that
registered it. The filter is the one the live subscription already uses, with
the same bounds, so a product describes inbound traffic the way it already
describes a subscription.

A route survives product restarts, host restarts, and reboots, until the product
removes it or the host revokes it. Registering the same filter twice changes
nothing.

Topics are opaque 32-byte values. The host matches bytes and never interprets
them, so a route tells it nothing about who a peer is or what a statement says:
it delivers signed statements and the product decrypts. A route is scoped to the
product that registered it, and a product can only register topics it can
derive.

### Delivery

On a match the host takes a worker reference — starting the product if it is not
running — delivers the matched statements to the surface if one is open and
otherwise to the worker, and releases the reference once the product
acknowledges or the deadline passes. A per-route cursor advances as it goes, so
a product that was unreachable receives what it missed at next start, in order.

Delivery is at-least-once, so a product must tolerate a repeat. A route the
product cannot service — a crash loop, deadlines repeatedly missed — is
suspended and reported, not retried forever.

### A route is a capability

A route lets a remote party start a product's code and, through notifications,
interrupt the user. So it is not merely a subscription:

- The user can see and revoke routes, per product.
- The host rate-limits wakes and coalesces a burst into one delivery carrying
  several statements.
- Wake budgets are host policy; a product sees only that a delivery was delayed
  or combined.
- A suspended or revoked route stops delivering without disturbing the product's
  live subscriptions.

The host cannot judge whether a sender deserves the user's attention — topics
are opaque and senders are a product concept. The product decides that, on
material it already authenticates. The host's job is to bound how often it can
be asked.

### What it unlocks

- **Messaging.** A message for a closed conversation arrives; the product
  decrypts, stores, and notifies.
- **[Realtime media sessions](realtime-media-sessions.md).** An invitation is
  one statement on a route, so calls need no inbound path of their own.
- **Payments.** Waiting for an incoming payment stops requiring an open
  product.

## Trade-offs

- A route is standing state the host matches forever, so a per-product cap is
  unavoidable.
- At-least-once delivery forces idempotent handling on products that would
  prefer exactly-once.
- Rate limiting and coalescing make wake timing a host policy, so a product
  cannot promise its user a ring latency.
- This is not background execution: a route buys a wake with work attached, not
  arbitrary scheduled time. Timers remain out of scope.
- A registered route tells the host that a product cares about a set of topics.
  The topics are opaque, but the set is a stable per-product fingerprint.

## Open questions

- May a route prefer the surface — raise the product rather than run its worker
  — for something like a call, or does the product always raise it itself?
- Does the cursor belong to the host or the product? Host-held is simpler and
  survives reinstall; product-held keeps the host from retaining a pointer per
  route.
- How is a route revoked once its topics stop being reachable, for example a
  contact removed, given the host cannot tell what a topic means?
- Does a route need an expiry, so an abandoned product stops costing matching
  work without the user having to find it?
