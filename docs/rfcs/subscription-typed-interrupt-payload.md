---
title: "Subscription Typed Interrupt Payload"
owner: "@johnthecat"
status: draft
---

# RFC — Subscription Typed Interrupt Payload

## Summary

Every subscription names its interrupt type, and its stream can end with a value of that type at any point.
`Subscription<Item, Interrupt>` replaces both `Subscription<Item>` and `Result<Subscription<Item>, CallError<E>>`,
because a start-time failure is an interrupt with no items before it. A product receives that value as `error`, and a
clean end as `complete`.

```ts
truapi.theme.subscribe({
  next: (theme) => apply(theme),
  error: (error) => console.warn("theme stream failed:", error.reason),
  complete: () => {},
});
```

## Motivation

A live subscription cannot fail. `Subscription<Item>` yields items only, so when the platform stream behind
`theme.subscribe` errors, the runtime drops the error and the product's theme freezes on its last value. A chain-head
follow whose connection drops ends with the same frame as one that finished cleanly.

A failure has a type only before the first item. A method whose start can fail declares
`Result<Subscription<Item>, CallError<E>>` and sends that error as its interrupt payload; the same failure one item
later has no shape to be sent in. A method with no start-time failure carries `CallError<GenericError>` on its interrupt
leg whatever its own failures are, and its default body yields no items, so a host that does not implement it is read as
a stream that finished.

Host-initiated methods need the same in the other direction. A product that streams items to the host has no way to say
that it is done, or why it stopped. Its completion is dropped, and its error becomes a fixed
`HostFailure { reason: "unavailable" }` payload that the host reports as
`GenericError { reason: "product interrupted the host-initiated subscription" }`.

## Approach

`_interrupt` carries `Result<(), Interrupt>` for the method's declared interrupt type. `Ok(())` is a normal end, which
the TS client delivers as `complete`. `Err(value)` is that value, encoded as declared and without further wrapping,
which the client delivers as `error` with `reason` set to it. `_stop` is unchanged.

`Subscription<Item, Interrupt>` is a stream whose items are `Result<Item, Interrupt>`; the first `Err` is terminal. The
dispatcher encodes it as `Err(value)` and drops the rest of the stream. A stream that ends without an `Err` encodes
`Ok(())`. `Interrupt` is `CallError<E>`, so the framework's own failures ride the interrupt leg of every method. Methods
with a domain error declare `CallError<E>`, so their interrupt bytes are unchanged. Methods without one declare
`CallError<GenericError>`, which is what their platform streams yield, so the runtime forwards the platform error
instead of dropping it. `Subscription::interrupted(value)` is a stream that ends with the given interrupt and replaces
`Subscription::empty()`, so an unimplemented method interrupts with `CallError::unavailable()` instead of completing.

Codegen emits `ObservableLike<Item, Reason>`, `SubscriptionError<Reason>` and an interrupt decoder for every
subscription. The decoder reads `Result<(), CallError<E>>`, delivers `Ok(())` as `complete`, and delivers a payload it
cannot decode as an error.

A host-initiated method declares `Subscription<Item, Interrupt>` like any other. The product registers a handler taking
the request and two callbacks, `send` for each item and `interrupt` for the end of the stream, and returns its teardown.
`interrupt()` ends the host's stream, `interrupt(value)` ends it with `Err(value)`, and the server preserves the bytes.

`WIRE_CODEC_VERSION` stays at 2. A clean end and a failure encode as the bytes the interrupt leg already carries for
each, so no frame changes.

## Trade-offs

- Every subscription implementer changes its return type, on all hosts, in one pass.
- `CallError<GenericError>` carries a string, not a discriminated enum. A method that needs a richer reason declares its
  own interrupt type.
- A product ends a host-initiated stream by calling `interrupt`, and the host's stream ends there rather than holding
  its last item until `_stop`.
