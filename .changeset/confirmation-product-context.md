---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

**Breaking SSO wire change.** Every product-originated SSO request (signing, transaction creation, resource allocation,
VRF and ring-VRF) carries the caller's `ProductContext`. A pairing host and a signing host on either side of this
version cannot talk to each other: upgrade both together.

The `UserConfirmation` host callbacks name the product that asked and how its request arrived:
`confirmUserAction(product, route, review, options)` and `confirmPermission(product, route, review, options)` take the
requesting `ProductContext` first. That argument is the only place a review names its requester: `AccountAliasReview`,
`CreateProofReview`, `SignVrfReview` and `ResourceAllocationReview` carry no `callingProductId`. The new `RequestRoute`
is `Local` for a product running on this host, or `PairedHost` with the `PairedSsoPeer` a relayed SSO request arrived
from. A native wallet passes that peer to `handleSsoRequest(peer, message)`.

Every prompt callback (`devicePermission`, `remotePermission`, `confirmUserAction`, `confirmPermission`) receives a
trailing `{ signal }` whose `AbortSignal` aborts when the core withdraws the request behind the prompt: the product
cancelled its call, its connection closed, a paired host withdrew an SSO request, or the runtime was disposed. A host
should dismiss the prompt when it fires; an answer given afterwards reaches nobody. Permission and account-access
prompts are withdrawn on cancellation like signing reviews.

The mock host records the product each confirmation names, in `confirmationProducts()`.

The `truapi-host` CLI names the requesting product and the paired host a relayed request came from in every approval,
and marks an approval withdrawn when the core withdraws the request behind it.
