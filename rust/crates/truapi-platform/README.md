# truapi-platform

Platform capability traits for TrUAPI host implementations.

Each host (web/WASM, desktop, iOS/UniFFI, Android/UniFFI) implements these
traits to provide the native capabilities the shared Rust runtime cannot reach
directly. The dispatcher in `truapi-server` calls this surface while the Rust
runtime owns product account management, SSO signing, statement-store protocol
flows, permission state, and auth state transitions.

## Type Imports

Most host-facing wire types are imported from `truapi::latest` by this crate and
are exposed through the trait signatures below. `ProductContext` and
`ProductExecutionKind` are defined here instead, and codegen emits their host
codecs from these definitions. Both are SCALE-encodable so they can cross the
wasm callback boundary, where every parameter is encoded with
`parity-scale-codec`; `ProductContext` decodes through its validating
constructor, so a context off the wire carries a normalized product id.

## Product Identity

`normalize_product_identifier` is the single chokepoint that turns a host- or
wire-supplied product id into the canonical form derivation, product storage and
permission scopes are keyed by; `is_product_identifier` is its boolean form.

`DOTNS_TLDS` (`dot`, `paseo`, `test`) backs it: the TLDs dotNS deployments
register product names under, one entry per network a host can be pointed at. A
name ending in one of them is also what navigation resolves back into the host's
own product surface, so it bypasses the outbound domain grant.

`REMOTE_PERMISSION_TRUSTED_LABELS` lists bare product labels — no TLD, so one
entry covers every network in `DOTNS_TLDS` — whose products hold every
`RemotePermission` without a user prompt, tested with
`has_trusted_remote_permissions`. It covers remote permissions only: device
permissions, identity disclosure and cross-product account access always prompt.
A stored decision outranks the list, so a `Denied` written through `CoreAdmin`
revokes the grant.

## Host Callback Traits

- `ProductStorage`: product-scoped key-value storage.
- `CoreStorage`: typed core-owned storage slots such as auth session, pairing
  identity, and permission authorization state.
- `Navigation`: open URLs in the system browser.
- `Notifications`: deliver and cancel push notifications.
- `Permissions`: prompt for device and remote authorizations.
- `Features`: report host feature support.
- `ChainProvider` / `JsonRpcConnection`: open JSON-RPC connections to chains.
- `AuthPresenter`: render core-owned auth state transitions.
- `UserConfirmation`: confirm signing, transaction, resource, alias, and
  preimage actions before the core asks the paired wallet.
- `ThemeHost`: stream the host theme into the runtime.
- `PreimageHost`: submit and look up preimages through the host-selected backend.
- `ChatPlatform`: create product-scoped native chat rooms, register product
  chat bots, post messages into rooms, and stream the product's room list.
- `PermissionStatusHost`: report the OS status of a device capability without
  prompting, so a stored grant can be revalidated before it is acted on.
- `PocketPlatform`: stream the product's Pocket card collection and remove a
  card from it. The host owns the collection and decides which cards are
  privileged.
- `ProfilePlatform`: show a product-referenced profile in host-owned UI. The
  host resolves, decrypts and renders the reference; nothing returns to the
  product but acceptance.

`Platform` is a blanket-implemented supertrait that combines the capability
traits above except `ChatPlatform`, `PermissionStatusHost`, `PocketPlatform`
and `ProfilePlatform`, which `OptionalPlatform` lists instead: a host supplies each
only when it can serve it. Codegen reads `OptionalPlatform` to emit each listed
capability as an optional group on the host-callback surface.

Omitting `ChatPlatform` makes the core answer Chat calls `Unsupported`, and
omitting `PocketPlatform` or `ProfilePlatform` does the same for Pocket or
Profile calls.
Omitting `PermissionStatusHost` leaves device grants resolving from stored
state alone, which is what a host with no OS permission model does anyway.
Serving it gates both halves of the surface: a device permission request and a
status read through `CoreAdmin` resolve the same two gates, so a settings
screen never reports a capability as usable when the OS refuses it.

## Core-Owned Admin API

`CoreAdmin` is not part of the host-provided `Platform` callback surface. It is
the core-owned control API exposed to host UI for logout, pairing cancellation,
session-store refresh, and permission administration.

It also serves the session's X25519 chat identity private key. Public session
material a host needs to address the identity or the paired device travels on
`SessionUiInfo` instead; only the secret requires this deliberate call.
