---
title: "Current product context"
owner: "@pgherveou"
---

# RFC: Current product context

## Summary

Add `system.getProductContext()`, returning the full canonical product identifier bound to the current host runtime.

```ts
const context = await truapi.system.getProductContext();

context.value.productId;
// "truapi-playground.paseo"
```

## Motivation

Account and signing APIs require a full `dotNsIdentifier`, but products cannot query the exact identifier the host uses for authorization and account derivation. They must hardcode a suffix, inspect their URL, or guess the active network.

Tracking issue: [#503](https://github.com/paritytech/trinity-user-agents/issues/503).

## Detailed Design

```ts
interface HostGetProductContextResponse {
  productId: string;
}
```

`productId` is the host runtime's existing canonical identifier. The method is available before account pairing and requires no permission.

The host returns the full identifier, including `.dot`, `.paseo`, `.testnet`, or a localhost form. Products do not construct or normalize it. This keeps account requests, authorization, and account derivation on the same identifier.

## Alternatives

Returning only the network suffix would duplicate host-owned construction and normalization in every product. Inspecting the product URL would couple the API to one execution environment and would not provide an authoritative value for tests.
