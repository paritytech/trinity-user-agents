# Add host-mediated pointer capture to TrUAPI

## Status

Future proposal for a TrUAPI RFC. This document records the design direction; it does not define a stable API and does not propose a user-facing permission setting yet.

## Summary

Pointer lock is host functionality. A product should request and release it through TrUAPI rather than declaring a separate `relative-pointer` feature in its manifest. The host owns the application surface, enforces user activation, calls the platform pointer-lock API, and reports lifecycle changes. High-frequency movement remains on the bounded device-input data path.

This separates three concerns:

- The App manifest declares ordinary `pointer` input support.
- TrUAPI controls whether pointer lock is free, armed, captured, denied, or unsupported.
- The runtime input ABI delivers absolute positions while free and relative deltas while captured.

## Motivation

Graphics profile is not a pointer policy. Some framebuffer and WebGPU products are first-person games that need unbounded relative motion; others are cursor-driven tools that must leave the system pointer free. Defaulting all non-Tri2D surfaces to pointer lock surprises cursor-driven products, while a static manifest feature cannot express when a mixed menu/game product wants capture.

Pointer lock also cannot be delegated to sandboxed product code. Browser and native hosts own the trusted surface and must preserve an unconditional escape path.

## Proposed control plane

Names are placeholders for the RFC:

```ts
input.pointerCapture.request(): Promise<
  "armed" | "captured" | "denied" | "unsupported"
>

input.pointerCapture.release(): Promise<"free">

input.pointerCapture.status(): Promise<
  "free" | "armed" | "captured" | "denied" | "unsupported"
>

input.pointerCapture.onChange((state) => {
  // free | armed | captured | denied | unsupported
})
```

A browser generally cannot honor `request()` immediately because pointer lock requires a transient user activation. In that case `request()` returns `armed`; the host acquires lock on the next eligible primary click on the requesting product's active surface.

`release()` is always permitted. Escape always releases capture and is consumed by the host for that transition rather than forwarded to the product as a key press.

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
- Denial and release are observable by the product so it can stop mouse-look or present fallback controls.
- Unsupported hosts return `unsupported`; products decide whether keyboard, touch, or controller input is an acceptable fallback.

## Manifest relationship

No separate `relative-pointer` manifest feature is required if TrUAPI provides capability discovery and an explicit `unsupported` result. The runtime request itself expresses current intent, and the host remains the authority.

A future RFC may revisit install-time metadata if hosts need to warn that a product cannot function without pointer lock. Such metadata would describe compatibility or user experience; it would not replace runtime request/release control.

## Future user policy

A host may eventually expose a per-product policy such as `Ask`, `Allow`, or `Block`, plus a global `Never allow pointer lock` setting. This is intentionally deferred.

If added, the setting should authorize or deny product requests; it should not force an arbitrary cursor-driven product into relative mode. The policy should be scoped to a verified product identity, show a visible capture indicator, and preserve Escape as an unconditional release path.

## Migration direction

1. Keep the existing temporary host behavior only for legacy products while TrUAPI control is unavailable.
2. Specify request, release, status, lifecycle events, user-gesture semantics, and error behavior in a TrUAPI RFC.
3. Implement the control plane in the shared Rust core and host bindings.
4. Update Epoca and Dotli to default all App v2 surfaces to free pointer input.
5. Update mouse-look products to request capture when entering gameplay and release it when entering menus.
6. Remove the temporary profile-based default after migrated products and hosts are deployed.

## Unresolved questions

- Whether `request()` should be a one-shot request or remain armed across Escape release until the product cancels it.
- Whether capture permission persists per product, per publisher, per artifact version, or only for the current session.
- Whether native hosts need a distinct result for operating-system policy denial.
- Whether products need an optional install-time declaration that pointer lock is essential rather than merely supported.
- Whether a host should expose a standard visual indicator or leave presentation to each host implementation.
