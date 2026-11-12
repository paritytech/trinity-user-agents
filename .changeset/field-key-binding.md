---
"@parity/truapi-host": minor
---

PROTOTYPE, for the web host's draft field bridge, not a TrUAPI product API. On the web worker runtime,
`signFieldKeyBinding(productId, index, binding)` signs an 80-byte field key binding (a receiving key's 16-byte id, its
X25519 public key and the People chain's genesis hash) as `productId`'s account at `Raw(index)`, and answers the account
and the signature. `verifyFieldKeyBinding(productId, account, binding, signature)` checks one. Both sign and verify
under the sr25519 signing context `truapi-web-host/field-key-binding/v1`, which no product signing call uses, so no
product request yields a signature that verifies as a binding. Products have no call that reaches either. A valid
binding shows that a holder of the wallet's keys bound the key to that account; it does not show that the host is
honest. Signing hosts on the `testing` bundle only; the iOS and Android host runtimes do not expose them.
