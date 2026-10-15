---
title: "Product auth tokens kept by the host"
owner: "@filvecchiato"
status: draft
---

# RFC: Product auth tokens kept by the host

## Summary

A product's worker signs in to its own service, such as a funding provider's onramp backend, following that service's
own steps, and gets back a token (usually a JWT, sometimes with a refresh token). The host keeps that token for the
product in the platform's secure storage, so the worker, or the same product's App, can read it back later, including
after a restart. The host does not authenticate anything and never refreshes a token: it stores what it is given.

## Motivation

A provider's service decides how its users sign in, and the steps differ from one service to the next. A worker can
follow them, but it has nowhere safe to keep the result: product local storage is not secure storage, and an App and
its Worker would each hold their own copy. Building every service's sign-in into the host instead would make the host
an authentication client for third parties.

## Approach

### Protocol

Trait `AuthTokens`, wire id 26. Any execution of the product may call it.

| Id | Method   | Request                         | Response                       | Errors                                                                                               |
| -- | -------- | ------------------------------- | ------------------------------ | ---------------------------------------------------------------------------------------------------- |
| 0  | `store`  | `{ service, token: AuthToken }` | empty                          | `InvalidService`, `EmptyToken`, `TooLarge { max }`, `TooManyServices { max }`, `Unknown { reason }` |
| 1  | `get`    | `{ service }`                   | `{ token: Option<AuthToken> }` | `InvalidService`, `Unknown { reason }`                                                               |
| 2  | `remove` | `{ service }`                   | empty                          | `InvalidService`, `Unknown { reason }`                                                               |

`get` and `remove` share one error type, `HostAuthTokensLookupError`.

```rust
struct AuthToken {
    token: String,
    refresh_token: Option<String>,
    expires_at: Option<u64>, // Unix milliseconds
}
```

`store` replaces any token already kept for the service. `remove` of a token that is not kept succeeds. A product
without a session gets `Denied`; a host that cannot keep tokens answers `Unsupported`.

### Rules

- **Scoping.** Tokens belong to the signed-in account and the product. The core hands the host an opaque key,
  `hex(blake2_256(("truapi/auth-token", root_public_key, product_id, service).encode()))`, where `root_public_key` is
  the session's root account key. One product never reaches another's token whatever service name it uses, and one
  account never reaches another's on the same device. The key is 64 lowercase hex characters, so a host uses it as a
  storage key as is, with nothing to join or escape. The host also gets the product id with each call, to count and
  clear one product's tokens.
- **Limits.** `service` is 1 to 64 characters of `a-z`, `0-9`, `.`, `_` and `-`. `token` is non-empty, and `token`
  plus `refresh_token` take at most 8192 bytes. A product keeps tokens for at most 16 services; storing a 17th
  answers `TooManyServices`, while replacing one it already keeps does not. The core refuses a request that breaks a
  limit before the host stores anything.
- **Storage.** The host keeps tokens in Keychain on iOS (this device only, readable after the first unlock) and
  Keystore-backed storage on Android, across app restarts. The core never caches them.
- **Clearing.** The core drops every token when the session ends: on logout, disconnect and session reset. It drops a
  product's tokens when that product's state is cleared. The host does not have to track sign-out itself, though it
  also drops them when it wipes the user's data. Tokens belong to the signed-in account, which is also why a product
  without a session is refused.
- **No host-side auth.** The host neither signs in to a product's service nor refreshes a token. `expires_at` is
  informational; the worker decides when to refresh and stores the new token itself.
- **Not for device tokens.** The device-uniqueness backend's tokens stay inside the host and never pass through this
  trait.

### Host surface

`AuthTokenPlatform` (`store_auth_token`, `auth_token`, `remove_auth_token` by key, `auth_token_count` and
`clear_auth_tokens` by product), optional and installed once with `set_auth_token_platform`. Native hosts implement `NativeAuthTokenCallbacks` and install it with
`set_auth_token_callbacks`. The iOS app keeps them in the Keychain and the Android app in
its Keystore-encrypted preferences. The browser host does not serve it and answers `Unsupported`. The CLI test host
keeps tokens in memory for the life of the process.

## Trade-offs

- **A host store instead of product local storage.** `localStorage` is not secure storage and an App and its Worker
  would each hold their own copy. Keeping the token with the host gives one copy in the platform keystore that any of
  the product's executions can read.
- **The worker refreshes, not the host.** Every service refreshes differently. Teaching the host each provider's flow
  would make it an authentication client for third parties; keeping it a store keeps the host out of those flows.
- **No web support.** A browser has no storage a host page can protect from the product's own origin, so the web host
  answers `Unsupported` rather than offering weaker guarantees under the same name.
