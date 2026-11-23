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
2. **Demand-driven.** A host consumer or the running worker can hold a reference while it needs the execution.
3. **Stoppable.** The host may stop an unreferenced worker at any time and the product keeps working.
4. **Connection-scoped.** A guest can retain its own worker. Disconnecting releases that connection’s references.

## Approach

The host keeps one reference count per product worker. The first reference starts the worker. When the count returns to
zero the host may stop it. A running worker may also retain itself while finishing work. The host starts and stops the
worker executable, and the worker learns it is needed by receiving a modality call.

The design has two parts:

- **References**: what holds the worker.
- **Stopping**: when the host stops a worker.

### References

A reference is held for exactly as long as its work is on screen or in flight. Several references of one product hold
one worker: a chat room the worker serves, a pocket artifact it contributed, and a funding flow that selected the
product are three references on the same worker. A modality names its holders in its own RFC. App and Widget executables
hold no reference; their lifetime is their screen.

Each holder acquires its own reference and releases it once. A renderer subscription can end or restart while its
surface keeps the same reference. Looking up the execution and calling its renderer both leave the count unchanged.

Reference holders, by modality:

| Holder  | Held while                                                                                               |
| ------- | -------------------------------------------------------------------------------------------------------- |
| Chat    | A room the product serves is on screen, or a message addressed to the product is in flight.              |
| Pocket  | A worker-backed card side is live. Multiple views of the same side share its claim.                      |
| Funding | The picker needs a provider quote or an assigned funding session still needs the provider.               |
| Input   | An input round is in flight, or a `Custom` candidate is on screen ([Input Modality](input-modality.md)). |

### Stopping

- The host may stop a worker whenever nothing references it, including while its output is on screen, so state that must
  survive goes through host storage.
- The host starts a worker when a reference forms, not when the user acts on the product, so a start is not user intent.
- A guest can finish its own work after the screen closes by taking a
  [worker reference](storage-subscriptions-pending-operations.md#worker-references) before the last external owner leaves.
  Scheduled wakeups are out of scope.

## Trade-offs

- A worker may start and stop several times in one pocket scroll. The host may keep an unreferenced worker warm; the
  product may not rely on it.
- Considered and dropped: an always-on worker, a start per call without counting, one worker per modality.
