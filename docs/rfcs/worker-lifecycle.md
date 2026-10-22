---
title: "Worker Lifecycle"
owner: "@johnthecat"
status: draft
---

# RFC — Worker Lifecycle

## Summary

A product has one worker. The host runs it while a reference to it is held and may stop it when none is. Modality work
takes a reference for as long as it is on screen or in flight.

## Motivation

The [Product Manifest Format](product-manifest.md) defines the worker as the product's single background process and
does not say when it runs. A worker that runs only with the product's app view cannot serve a chat room while the view
is closed, and every modality that calls a worker needs one rule for when it runs.

### Requirements

1. **Single.** A product has one worker process, however many modalities call it.
2. **Demand-driven.** The worker runs only while the host has work that only the worker can do.
3. **Stoppable.** The host may stop an unreferenced worker at any time and the product keeps working.
4. **Additive.** Nothing is added to the TrUAPI protocol.

## Approach

The host keeps one reference count per product worker. The first reference starts the worker. When the count returns to
zero the host may stop it. Products do not ask for a worker to run at a given time: the host starts and stops the worker
executable, and the worker learns it is needed by receiving a modality call. The one declaration a product makes is the
`keepAlive` field of its Worker executable manifest (see [Foreground](#foreground)), which the host reads, not the
worker.

The design has two parts:

- **References**: what holds the worker.
- **Stopping**: when the host stops a worker.

### References

A reference is held for exactly as long as its work is on screen or in flight. Several references of one product hold
one worker: a chat room the worker serves, a pocket artifact it contributed, and a funding flow that selected the
product are three references on the same worker. A modality names its holders in its own RFC. App and Widget executables
hold no reference; their lifetime is their screen.

Reference holders, by modality:

| Holder  | Held while                                                                                               |
| ------- | -------------------------------------------------------------------------------------------------------- |
| Chat    | A room the product serves is on screen, or a message addressed to the product is in flight.              |
| Pocket  | An artifact the product contributed is on screen.                                                        |
| Funding | A funding flow with the product as selected provider is in flight, from selection until it settles.      |
| Input   | An input round is in flight, or a `Custom` candidate is on screen ([Input Modality](input-modality.md)). |

[Chat Extension v2 additions](chat-extension-v2-additions.md) adds two Chat holders:

- **Wake.** The host holds the worker for 30 s from one of three sources: a message, user action or room event sent to
  the product while no room screen holds the worker; a chat product that starts and owns no room yet, once; and a chat
  push for a room of the product that the app receives, at most 12 per product per rolling hour. A product has at most
  one wake hold, and a new wake moves its deadline. The product extends a wake only through
  `Worker::begin_operation`.
- **Call.** A room with a `Ringing` or `Active` call holds the worker until the product clears the call or deletes
  the room. A `Ringing` call holds it for at most 60 s, the time a host rings before it clears the banner.

### Foreground

A product whose worker must keep a connection open to receive what is addressed to it (a chat that follows its
statement-store topics for contact requests and messages) declares `keepAlive: "foreground"` in its Worker executable
manifest ([Product Manifest Format](product-manifest.md)). The host holds such a worker while the app is in the
foreground:

| Holder     | Held while                                                                 |
| ---------- | -------------------------------------------------------------------------- |
| Foreground | The product declares `keepAlive: "foreground"` and the app is in the foreground. |

- **One holder in the core.** The core's reference ledger owns the hold. The host reports two facts and decides
  nothing: whether the app is in the foreground (`set_app_foreground`), and what each product declares
  (`set_worker_keep_alive`). A product has at most one foreground hold. The ledger starts in the background, so a
  host that reports nothing holds nothing.
- **The foreground of a phone.** On iOS the app is in the foreground from `willEnterForeground` to
  `didEnterBackground`; a resign of active alone (a system alert, Face ID, Control Center) does not end it, because iOS
  does not suspend the app then. In the background the hold goes, and the worker stops unless another holder holds it.
- **Faults.** A worker that ends on a fault stays stopped while the hold stays. The next return to the foreground
  starts it again, so a worker that faults at each start does not loop.
- **No background work.** The hold does not keep a worker in the background. When a chat push for a room of the
  product reaches the app in the background, the host takes a notification wake for that room.

### Stopping

- The host may stop a worker whenever nothing references it, including while its output is on screen, so state that must
  survive goes through host storage.
- The host starts a worker when a reference forms, not when the user acts on the product, so a start is not user intent.
- The host runs a worker only while it is referenced, so a worker has no way to do background work of its own. Scheduled
  wakeups are out of scope. The foreground hold is a reference like the others: it holds the worker while the user
  has the app open, never in the background.

## Trade-offs

- A worker may start and stop several times in one pocket scroll. The host may keep an unreferenced worker warm; the
  product may not rely on it.
- Considered and dropped: an always-on worker, a start per call without counting, one worker per modality.
- A foreground worker costs memory and connections while the app is open, also when the user never opens the product.
  The manifest field makes the cost visible per product, and a host may refuse it (for example on low memory) as it may
  stop any worker.
- Considered for the foreground: a start whenever a statement arrives for the product. It needs the core to keep the
  product's topics while the worker sleeps and a relay to push to a suspended app. The notification wake starts the
  worker only for a chat push the app already receives, so it adds no product call.
