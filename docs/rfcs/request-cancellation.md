---
title: "Request cancellation"
owner: "@decrypto21"
status: draft
---

# RFC: Request cancellation

## Summary

A caller can withdraw a request it has already sent. A `Cancel` leg on the method's own wire address fires the call's
cancellation token on the far side, and the call still settles with exactly one response.

## Motivation

One-shot calls are not short. `signing.createTransaction` and `resourceAllocation.request` wait for a person to act on
a paired phone; the `chain` methods wait on a remote node. Nothing carries the caller's decision to stop waiting.

`CallContext` already holds a `CancellationToken`, and the runtime already honours it: `remote_authority_call` races the
handler against the token and gives it a bounded unwind before dropping it. But every generated request handler builds
its own context with `CallContext::with_request_id`, a fresh token no frame can reach. Only a timeout or a host-internal
decision fires it.

So the product abandons calls the host keeps running. The TypeScript client arms a deadline on every request that drops
the pending entry and rejects the promise without sending anything, and a product that navigates away or unmounts a
component leaks the same way with no deadline at all. In both cases the signing prompt stays on the phone, the chain
query keeps its connection, and the paired authority keeps a message in flight, because nothing told the host
otherwise.

A timeout is the wrong instrument for this. A signing call has to allow minutes, which is far too slow to serve as an
abort, and it reports elapsed time for what was a person's decision to stop.

Subscriptions have none of this problem. `Stop` is exactly this message, and it already works.

## Approach

`Cancel` is a message type on the request/response family, beside `Request` and `Response`, carried on the method's own
`(trait, method)` address and correlated by `requestId`. It has no payload and travels in the same direction as that
method's `Request`. [RFC 0028][0028] put the leg in the envelope for this case: a new leg costs a message-type value and
a dispatch arm, and no method id moves. Every request method is cancellable, with no per-method opt-in.

What a cancel does and does not promise:

- Exactly one `Response` still settles a `requestId`.
- A withdrawn call answers `CallError::Cancelled`, whatever its handler made of the token.
- A `Cancel` naming no call in flight is never answered, and is remembered rather than dropped, because it may have
  overtaken the request it names.
- Cancelling is cooperative in what it stops, not in what it answers. A handler that never observes its token runs to
  completion and loses a result nobody is waiting for.

### Host side

The dispatcher gains a registry of in-flight requests keyed by `requestId`, holding each call's token, in the same
shape as the subscription registry that already serves `Stop`. It reserves the entry before awaiting the handler, so a
cancel arriving during setup is not lost, and handlers take their token from the registry rather than minting one. The
runtime's existing `in_flight` map is keyed by a monotonic dispatch id and holds `AbortHandle`s, which is the right
shape for dropping every call at teardown and the wrong one for naming a single call from the wire. It stays as it is.

Frame order does not survive the trip. Transports spawn a task per inbound frame, so the `Request` and `Cancel` a
client wrote to the socket in order can reach dispatch the other way round, and the cheaper `Cancel` often wins. A
withdrawal that found nothing would be lost, and the product would wait out its full deadline while the host kept
working, with the phone still prompting. So a `Cancel` that finds no call records the id instead, in a short capped
queue, and the `Request` that follows answers `Cancelled` without running its handler at all. Not running it is the
point: a `createTransaction` withdrawn before it started must never reach a person.

For a withdrawn call the dispatcher substitutes the response, and it can build one without naming either of a method's
payload types: `Err(CallError::Cancelled)` is the `Result`'s `Err` tag followed by the variant's index, and `Cancelled`
carries nothing, so the same two bytes are a valid response leg for every request method. The client already uses that
trick to decline a host-initiated subscription with a fixed `HostFailure` frame.

### Why this rides no codec bump

`CallError` gains `Cancelled` as its last variant, so every existing discriminant keeps its index. Two things then keep
the change additive.

The wire schema hash moves, because `CallError`'s shape is in the fingerprint precisely so an error discriminant cannot
change unannounced. But that fingerprint is read only by the debugger, deciding whether its decode table matches the
host it is tapping. It gates nothing between a product and a host.

The variant is reachable only by a peer that asked for it, and the dispatcher holds that line: it substitutes the
`Cancelled` response only for a call a `Cancel` frame withdrew, never for one whose token a runtime fired itself. A
host-internal timeout still becomes an `AuthorityError::Cancelled` mapped into the method's own domain error, so a
product that never sends a `Cancel` never has to decode the new variant.

That is worth the care, because `WIRE_CODEC_VERSION` is what a bump would cost. The handshake compares it for exact
equality in both directions, so moving it is a flag day rather than a rollout: a product on the new number cannot talk
to a host on the old one at all, for any method, and native hosts ship on app-release cycles. Keeping the generated
client's number in step with `truapi::WIRE_CODEC_VERSION` is #848's work, not this change's.

### Peers that predate the leg

A `Cancel` arriving at one is dropped with a log and no reply. The `(255, 255)` protocol error answers an unknown
`(trait, method)` pair, and a `Cancel` addresses a pair the old peer implements, so it reaches the request arm's
message-type guard and dies there. Nothing else about that pairing breaks: the call proceeds and settles normally, and
only the withdrawal is lost. But the caller cannot tell a cancel the host honoured from one it never understood, and it
has nothing to fall back on but its own deadline. That is the gap [#478][478] closes, and this RFC assumes a product
checks method-level support before offering an abort.

### Client side

A product cancels through an `AbortSignal`, taken as the last argument of every generated request method. Aborting
sends `Cancel` and lets the promise settle on the response, so the product reads `Cancelled` instead of a local
rejection the host never heard about. A signal already aborted when the call is made sends nothing at all. The client's
own deadline sends `Cancel` before it rejects, which is the leak it closes: today that deadline drops the pending entry
and rejects without telling anyone.

Tearing down an execution is unchanged. It aborts the dispatch futures wholesale, which drops the handlers outright and
is strictly stronger than firing their tokens. The registry entries those futures held are not released on that path,
since the release runs after the handler returns and an aborted future never reaches it. They cost a slot each until the
registry itself is dropped with the connection, and a monotonic id is never presented again, so nothing is left
reachable.

### What a withdrawn call stops

A handler stops at the next point where going on would ask a person, or would act or send a request on the product's
behalf. Work already handed to another party is not recalled, and a call whose remote effect starts with one request is
left to finish. Per site:

- **Prompts for this call's action stop.** The local confirmation behind every `signing` method,
  `resourceAllocation.request`, `preimage.submit`, a cold `account.get`, and VRF signing races the token. A withdrawn
  call stops waiting, and an answer the person gives afterwards authorizes nothing. The host's modal is not dismissed,
  because `UserConfirmation` has no way to withdraw a prompt it has shown.
- **Permission prompts finish, and the action behind them does not happen.** Identity disclosure, account access, and
  the device and remote permission gates ask about the product, not about this call, and the answer is stored for
  later calls. The decision is recorded. The call then does nothing further: a statement is not submitted, a
  notification is not shown, a navigation is not handed off, and a paired-host lookup is not sent. The cost is that the
  product waits for the person to answer before it learns the call is over. The exception is an account-access prompt
  the local signing host shows inside an authority call (`account.getAccountAlias`, `account.listRingVrfKeys`): it
  ends with that call's unwind grace, so its answer is not recorded.
- **Paired-host requests stop at this host.** A request whose call was withdrawn before it was published is never
  published, including a withdrawal that lands while the request is still subscribing. One already published ends the
  local wait by the path a timeout already takes, and the paired host is not told, because the SSO protocol has no
  cancel message. See the open question.
- **A local signing host starts no further allocation.** When this host holds the keys, `resourceAllocation.request`
  allocates each resource in turn, and a withdrawal stops it before the next one. An allocation already under way
  runs until the unwind grace ends.
- **Other `chain` one-shots finish.** Each is a single JSON-RPC round trip. Several of them start a node-side operation
  and answer the id that stops it, and dropping one mid-flight would leave an operation that nobody can name.
- **A broadcast is not sent once withdrawn, and is stopped if it already was.** A withdrawn `broadcastTransaction`
  answers `Cancelled`, so the product never learns the operation id it would stop the broadcast with. The host stops it
  instead, within the unwind grace, when the node returned an id; without one there is nothing to stop it with. Only a
  withdrawal stops it: a call the host cancels for any other reason still answers with the id. A transaction a peer has
  already received may still be included; stopping ends only this host's rebroadcast. A `Cancel` that arrives after the
  handler has returned but before the dispatcher settles the call is not seen by the handler, so that broadcast keeps
  running.
- **Bulletin submission stops.** `preimage.submit` builds, broadcasts and watches its transaction as separate steps.
  A withdrawal before the broadcast prevents it; after the broadcast it only stops the watch.
- **Login finishes.** `account.requestLogin` does not observe the token, so a withdrawn login keeps the pairing flow
  open until the person completes or dismisses it.

Each handler returns its method's own error. For a call a `Cancel` frame withdrew, the dispatcher replaces it with
`CallError::Cancelled`. A stop caused by a host-internal timeout reaches the product as the domain error it always did.

## Trade-offs

- Cooperative, not preemptive. A handler that never observes the token runs to completion: the caller is told the call
  was cancelled, but the work behind it was not stopped, and the caller waits as long as it would have.
- Aborting is not instant. The product learns the outcome when the response arrives.
- Permission prompts are not raced, so a call withdrawn during one settles only once the person answers.
- One dispatch arm and one registry per side, and codegen emits the arm for every request method.
- A single protocol-level cancel address carrying the target `requestId` in its payload, as `(255, 255)` carries
  protocol errors, needs no per-method codegen. It was dropped because a frame that names no method is opaque to a
  debug tap and to any per-method policy, and it splits teardown across two mechanisms when `Stop` already works
  per-method.

## Open questions

1. Does the SSO protocol need a cancel message? It does for resource allocation and not for signing, and a message
   alone would not reach the prompt it is meant to stop.

   What the paired host does once the person approves decides whether the missing cancel matters. A signing request, a
   `createTransaction` included, only returns bytes. The phone broadcasts nothing, and a response nobody is waiting for
   is skipped by the next call's reply matcher, so a withdrawn signature costs a stale prompt. A resource allocation
   spends. After approval the responder registers a statement-store or bulletin allowance on chain, or claims one of the
   day's PGAS slots, depending on what was asked for, and records a renewal target, so a withdrawal that stops at this
   host still lets the phone spend on the product's behalf.

   A `Cancel` variant would not be enough to stop that. The responder serves one request at a time. It awaits each
   request, prompt included, before it reads the next statement, so a cancel queued behind the prompt it targets is read
   only after that prompt has been answered. `Disconnected` already has the same limit. And `confirm_user_action` has
   no way to withdraw a prompt it has shown. Honouring a cancel on the paired host needs all three of these:

   - a request variant naming the withdrawn message id;
   - a responder that reads control messages while a request is running;
   - a platform prompt that can be dismissed, with a check between allocation steps.

   The last one lands in every native SSO stack, not only in the Rust responder. That is a change of its own, not a
   follow-on to this one.

[0028]: 0028-wire-message-type-byte.md
[478]: https://github.com/paritytech/trinity-user-agents/issues/478
