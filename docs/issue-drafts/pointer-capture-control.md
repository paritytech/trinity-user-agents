# Add host-mediated pointer capture to TrUAPI

## Status

Parked prototype note. Do not start an RFC or implementation from this draft
yet. Current hosts capture non-Tri2D surfaces after a primary click, which is
sufficient for the existing game prototypes. Revisit only after prototypes
demonstrate that products need runtime capture/release control beyond the
host's click-to-capture and Escape-to-release policy.

## Summary

Pointer lock is host functionality. A product should request and release it through TrUAPI rather than declaring a separate `relative-pointer` feature in its manifest. The host owns the application surface, enforces user activation, calls the platform pointer-lock API, and reports lifecycle changes. High-frequency movement remains on the bounded device-input data path.

This separates three concerns:

- The App manifest declares ordinary `pointer` input support.
- TrUAPI controls whether pointer lock is free, armed, or captured.
- Request denial is a call error, and an older host reports the protocol's
  standard unsupported-method error.
- The runtime input ABI delivers absolute positions while free and relative
  deltas while captured.

## Motivation

Graphics profile is not a pointer policy. Some framebuffer and WebGPU products are first-person games that need unbounded relative motion; others are cursor-driven tools that must leave the system pointer free. Defaulting all non-Tri2D surfaces to pointer lock surprises cursor-driven products, while a static manifest feature cannot express when a mixed menu/game product wants capture.

Pointer lock also cannot be delegated to sandboxed product code. Browser and native hosts own the trusted surface and must preserve an unconditional escape path.

## Proposed control plane

Names are placeholders for the RFC:

```ts
input.pointerCapture.request(): Promise<Result<\"armed\" | \"captured\", PointerCaptureError>>

input.pointerCapture.release(): Promise<Result<void, PointerCaptureError>>

input.pointerCapture.subscribe(): Subscription<
  \"free\" | \"armed\" | \"captured\"
>
```

The subscription follows the existing `theme.subscribe()` and
`locale.subscribe()` convention: it emits the current state immediately and
then every transition. A separate `status()` request would duplicate the first
subscription item.

A browser generally cannot honor `request()` immediately because pointer lock
requires a transient user activation. In that case `request()` returns
`armed`; the host acquires lock on the next eligible primary click on the
requesting product's active surface.

`release()` is idempotent and always permitted. Escape always releases capture
and is consumed by the host for that transition rather than forwarded to the
product as a key press.

`denied` is a request error rather than a durable capture state: after denial
the surface remains `free`. A host that predates these methods already has a
standard answer—the TrUAPI protocol-error frame for an unknown discriminant—so
`unsupported` does not belong in the domain state enum.

## Fit with the existing TrUAPI architecture

The API should be a small `Input` method group, backed by an optional
`PointerCaptureHost` platform callback. The TrUAPI server is product-scoped, so
the callback already runs in the identity and lifecycle context of the calling
product. Web, desktop, and other surface-owning hosts implement the callback;
CLI or non-pointer hosts omit it and receive the existing `Unsupported`
behavior.

This should not be added to `HostDevicePermissionRequest`. RFC 0002 defines
device permission decisions as indefinitely persisted grants or denials, often
paired with an operating-system permission. Pointer lock is transient surface
state, requires a current user gesture, and must always be releasable. A future
host policy may gate the request, but the first API does not need a persisted
permission.

The RFC must allocate new append-only wire discriminants for request, release,
and the subscription lifecycle. Existing discriminants cannot be reused.

## Input data plane

Pointer movement does not travel through TrUAPI. The existing bounded runtime input channel remains the data plane:

- Free pointer: event type `5`, absolute canvas `x`/`y`.
- Captured pointer: event type `6`, signed relative `dx`/`dy`.
- Buttons: event types `3` and `4` in either state.

Hosts should discard the first cursor-warp sample after lock acquisition, reject non-finite or implausible discontinuities, and coalesce high-rate deltas to the runtime's bounded update cadence.

The browser operation is Pointer Lock (`requestPointerLock()`), not DOM pointer capture (`setPointerCapture()`). DOM pointer capture only preserves event routing during a drag and does not provide an unbounded relative pointer.

## Host invariants

- Pointer lock defaults to off for every graphics profile.
- Only the product bound to the active verified surface may request capture.
- Capture requires a real user gesture on that surface.
- A product cannot suppress Escape release.
- Losing focus, replacing the product, navigating, or destroying the surface releases capture.
- Relative events are emitted only while the surface actually owns pointer lock.
- Request denial and every state transition are observable by the product so it
  can stop mouse-look or present fallback controls.
- Hosts that predate the API return TrUAPI's standard unsupported-method error;
  products decide whether keyboard, touch, or controller input is an acceptable
  fallback.

## Manifest relationship

No separate `relative-pointer` manifest feature is required. TrUAPI already
provides generic unsupported-method handling, the runtime request expresses
current intent, and the host remains the authority.

A future RFC may revisit install-time metadata if hosts need to warn that a product cannot function without pointer lock. Such metadata would describe compatibility or user experience; it would not replace runtime request/release control.

## Future user policy

A host may eventually expose a per-product policy such as `Ask`, `Allow`, or `Block`, plus a global `Never allow pointer lock` setting. This is intentionally deferred.

If added, the setting should authorize or deny product requests; it should not force an arbitrary cursor-driven product into relative mode. The policy should be scoped to a verified product identity, show a visible capture indicator, and preserve Escape as an unconditional release path.

## Revisit criteria

Keep the existing host behavior while prototypes mature. Promote this note to
an RFC only when at least one real product demonstrates a concrete need such
as:

- a cursor-driven framebuffer or WebGPU surface that must never capture;
- a mixed menu/game product that must release and reacquire capture without
  relying on Escape and a new click;
- a cross-host inconsistency that cannot be fixed as host policy;
- a product that needs to observe capture denial or loss to provide a usable
  fallback.

If one of those cases appears, the RFC should specify request, release, the
immediate state subscription, wire ids, user-gesture semantics, and errors.
Only after that should the shared Rust core, host callbacks, Epoca, Dotli, and
products be changed.

## Unresolved questions

- Whether `request()` should be a one-shot request or remain armed across Escape release until the product cancels it.
- Whether capture permission persists per product, per publisher, per artifact version, or only for the current session.
- Whether native hosts need a distinct result for operating-system policy denial.
- Whether products need an optional install-time declaration that pointer lock is essential rather than merely supported.
- Whether a host should expose a standard visual indicator or leave presentation to each host implementation.
