# truapi runtime

_Runtime core for TrUAPI: dispatcher, protocol frames, SCALE-coded wire envelope._

## What the runtime is for

The `runtime` feature of `truapi` turns trait implementations of the protocol API into a working host. It owns:

- the [`ProtocolMessage`] wire envelope and SCALE codec
- the [`Dispatcher`] that routes incoming frames to per-method handlers
- the subscription lifecycle (start/receive/stop/interrupt)
- the [`Transport`] trait that platform-specific IPC backends implement
- the auto-generated dispatcher/wire-table tables shipped under [`crate::generated`]
- the host embedding surface: one long-lived role handle (`PairingHostRuntime` or `SigningHostRuntime`) per host application, exposing shared [`RuntimeServices`] plus one [`ProductRuntime`] per product connection

## Architecture

Each product connection owns a `ProductRuntime<H>` and a `ProductRuntimeHost<H>`. Each runtime shares its own `RuntimeServices` and concrete `HostAccounts<H>` across its products. Native composition uses `WalletAccountHolder`; paired composition uses `SsoAccountHolderClient`. The runtime facades own composition and lifecycle, and the dispatcher provides the execution boundary.

```text
per product connection
    product frames -> ProductRuntime -> ProductRuntimeHost<H>
                                             |
shared within one host                       v
    RuntimeServices                    HostAccounts<H>
    platform, providers,               delegated keys, host grants,
    transport and repositories         product account execution
                                             |
                               +-------------+-------------+
                               |                           |
                       WalletAccountHolder       SsoAccountHolderClient
                       wallet approval, roots,             |
                       allocation and signing      encrypted SSO transport
                               ^                           |
                               |                 SsoAccountHolderService
                               +---------------------------+
                                 direct wallet authorization
```

Incoming SSO calls the wallet holder directly. It cannot read or populate the native host's grants or inherit local approval. `HostAccounts` resolves delegated execution before consent and keeps exported signing keys separate from activation-bound wallet authorization. Purpose-specific entropy supports local derivation without exposing wallet roots or allowance derivation.

`host_core.rs` owns the runtime facades and `ProductRuntime<H>`; `runtime.rs` owns `ProductRuntimeHost<H>` and shared helpers. Trait adapters are grouped by surface under `runtime/capabilities/`; cross-capability fixtures and tests live in `runtime/tests.rs` and `runtime/tests/`. Pure `host_logic` supplies codecs, derivation and policy without I/O.

### Permission flow

Permission grants are scoped by product id and typed request, so a grant for one product never authorizes another product or another permission class.

Blessed products in `truapi::platform::REMOTE_PERMISSION_TRUSTED_LABELS` bypass recorded host-side remote, account-access and identity-disclosure decisions. Device permissions retain their product and OS gates. These host rules do not bypass wallet authorization or manifest checks: incoming SSO retains independent wallet approval, and resource allocation, including AutoSigning, requires wallet approval. Legacy-account signing also asks the user.

```text
Product app
(product_id = "my-product")
        |
        | host API call
        | e.g. getUserId(), chain submit, camera, remote fetch
        v
Generated product client / host callback bridge
        |
        v
ProductRuntime
        |
        | attaches current ProductContext.product_id
        v
PermissionsService(storage, platform, product_id)
        |
        +-- blessed product, non-device --> allow without storage or prompt
        |
        | builds storage key:
        |
        | CoreStorageKey::PermissionAuthorization {
        |   product_id: "my-product",
        |   request: PermissionAuthorizationRequest::...
        | }
        v
CoreStorage lookup
        |
        +-- Authorized ---------------> allow protected host/backend call
        |
        +-- Denied -------------------> return PermissionDenied / deny call
        |
        +-- NotDetermined / missing ---+
                                       |
                                       v
                              Platform prompt callback
                                       |
                   +-------------------+-------------------+
                   |                   |                   |
                   v                   v                   v
          device_permission()   remote_permission()   confirm_user_action()
          camera/mic/etc        chain/preimage/etc    identity disclosure
                   |                   |                   |
                   +-------------------+-------------------+
                                       |
                                       v
                         user chooses Allow / Deny
                                       |
                                       v
                      write Authorized / Denied to CoreStorage
                      under the same product-scoped key
                                       |
                         +-------------+-------------+
                         |                           |
                         v                           v
                    Authorized                    Denied
                    allow call                    deny call
```

#### Auto-granted remote permissions

A valid remote permission request from a blessed product is authorized before reading or writing permission storage. Empty or invalid domain bundles are denied. Other products resolve saved decisions and prompt when undecided.

Permission administration uses the same key without prompting:

```text
Product UI
  |
  | permission_authorization_status(request)
  | set_permission_authorization_status(request, status)
  v
HostAdmin / ProductRuntime
  |
  v
PermissionsService
  |
  v
CoreStorageKey::PermissionAuthorization { product_id, request }
```

A device permission also carries the host application's OS gate. Both the request path and the `CoreAdmin` status read resolve it through the host's `PermissionStatusHost`, so an OS refusal reads as `Denied` whatever is stored, and the stored product decision is never overwritten by it. Remote, identity-disclosure and account-access decisions have no OS gate.

The embedder constructs `PairingHostRuntime` or `SigningHostRuntime`, then calls `product_runtime(product, sink)` for each product connection. Pairing lifecycle includes cancellation, protected session restoration and reset. Native signing initialization supplies a wallet-only secret provider and selects a stable wallet identifier. `prepare_wallet` returns an opaque activation whose public owner selects the repositories; `activate_wallet` publishes it only after those repositories are installed. Both facades expose product-state cleanup and contact-cache invalidation. Native Worker executions are created only by the Rust supervisor.

`SigningHostConfig.network_suffix` is the network's bare dotNS TLD (`dot`, `paseo`, or `testnet`). The shell supplies it alongside the chain genesis hashes from the same network configuration used by wallet onboarding. It must match the People chain's `NetworkSuffix.NetworkSuffix`: reserved identities derive under `uid.<suffix>` and `peopl.<suffix>`, while the chain uses that suffix for proof contexts. Configuration keeps local activation and key derivation available offline. The core validates supported suffixes but does not automatically check that the configured suffix matches the chain.

### Core database

Native SDK activation opens `<database_directory>/<owner hex>/core.sqlite3` through `RuntimeStore` and the existing `Db` foundation. A facade binds to its first owner; switching owners constructs another facade. Activation and product lifetime tokens prevent old executions from writing after lock, removal or reset. Keep the directory out of device backups because durable transaction state must not be restored onto another device. Other embedders, including the CLI, can provide `Db` through `SigningHostRuntime::set_core_db`. Web hosts do not compile SQLite.

Rust owns product/core records, paired devices, allowance records, detailed statement slots, the product catalog, notification reconciliation and worker records. Product/core payloads use versioned authenticated encryption bound to owner, table and row. The installation encryption key and host signing material use `SecretCoreStorage`; wallet roots use the separate wallet-only provider. See the [storage contract](../../../docs/design/host-storage.md) and [implementation findings](../../../docs/design/host-account-holder-findings.md).

### The two roles

- `PairingHostRuntime` composes `HostAccounts<SsoAccountHolderClient>` and owns pairing/login lifecycle, protected auth-session restoration and remote signing-host liveness. Domain operations use the existing encrypted SSO channel in `pairing_host/sso_channel.rs`. Purpose-specific derivation stays local.
- `SigningHostRuntime` composes `HostAccounts<WalletAccountHolder>` and exposes the same wallet holder to `SsoAccountHolderService`. The holder owns wallet authorization, root entropy, derivation, allocation and renewal. SSO advertises the RFC-0022 `uid.<tld>` index-0 product account; database and native grant ownership use the wallet root public key. RFC-0024 ring-VRF entries remain product-owned, and ring reads are pinned to one finalized block. Native allowance allocation uses the shared per-chain metadata cache.

Native workers use `native/workers.rs`: durable Chat/Pocket reasons, stored operations and live references feed one decision per product. The supervisor chooses bundle updates, delays replacement while operations remain, and retries engine failures. Native adapters fetch immutable bundles and start or stop engines with the execution Rust supplies. Web hosts retain the `WorkerLedger` reference-count callback contract. The [worker design](../../../docs/design/core-owned-workers.md) describes the durable policy.

### Inter-host SSO

`SsoAccountHolderClient` sends domain operations to [`SsoAccountHolderService`](src/runtime/signing_host/sso_service.rs). Macro-generated handlers invoke `WalletAccountHolder` with authenticated caller provenance and independent wallet approval. `sso_responder.rs` owns transport, correlation and replay ordering. Account changes, disconnect and reactivation invalidate pending approval before allocation or key return. Allocation failure details stay in local transcripts. Requests and results use canonical `truapi::latest` payloads, and both product and SSO signing retain the one-byte `OptionBool` encoding for `with_signed_transaction`.

A pairing host that stops waiting because its caller withdrew the request sends a `Cancel` naming it, when that request is still the newest on the session's request channel. The responder reads statements while it serves a request, so a `Cancel` fires the running request's token or stops a queued one from starting; a withdrawn request posts no response. See the [SSO request cancellation RFC](../../../docs/rfcs/sso-request-cancellation.md).

When a device finishes pairing, the signing host reports it to the embedder's [`DevicePairingObserver`](src/runtime/signing_host/sso_responder.rs), installed once through `SigningHostRuntime::set_device_pairing_observer`, and on a native host to `HostCallbacks::device_paired`. It carries the `PairedSsoPeer` that pairing produced, which is also what `resume_pairing` and `disconnect_paired_host` take.

Native `establish_pairing` provisions wallet and peer statement allowances, checks the results, announces progress and answers the handshake. It captures one wallet activation throughout. Rust records a cleanup intent before tracking a new peer, preserves completed pairings and recovers interrupted attempts before publishing a reopened wallet. Native UI presents the pairing proposal; its sanitized metadata describes the peer but is not an authenticated identity.

The roster lives in Rust-owned paired-host records. Native apps resume those sessions through `resume_pairing`, cancel their listener tasks during removal and use Rust APIs for record removal and renewal cleanup. Lock terminates listeners for the old activation. The pairing observer fires after the handshake answer reaches the Statement Store, which does not prove the peer received it. The core has no chat of its own, so announcing a device to contacts belongs to the embedder.

The `host_internal::sso_messages::v1::RemoteMessage` enum owns the SCALE wire contract. Its response variants wrap named result payloads in `Response<P>`, which carries `responding_to` once. Macros generate request/response pairing and dispatch; see the [macro guide](../truapi-macros/README.md) for handler signatures and reply handling. A new operation needs payload definitions, wire variants, a handler, and a typed client call.

Rust consumers must update renamed SSO types and helpers even when SCALE encoding is unchanged. Use `RemoteMessage::request(message_id, request)` to construct requests. Decoded `SsoSessionStatement::RemoteMessages` preserves message order; match variants directly or use the request's `SsoRequest::response_from_message`.

## Host platform interface

The `platform` module holds the capability traits a TrUAPI host implements. Each host (web/WASM, desktop, iOS/UniFFI, Android/UniFFI) implements these traits to provide the native capabilities the shared Rust runtime cannot reach directly. The dispatcher calls this surface while the Rust runtime owns product account management, SSO signing, statement-store protocol flows, permission state, and auth state transitions.

### Type Imports

Most host-facing wire types are imported from `truapi::latest` by this module and are exposed through the trait signatures below. `ProductContext` and `ProductExecutionKind` are defined here instead, and codegen emits their host codecs from these definitions. Both are SCALE-encodable so they can cross the wasm callback boundary, where every parameter is encoded with `parity-scale-codec`; `ProductContext` decodes through its validating constructor, so a context off the wire carries a normalized product id.

### Product Identity

`normalize_product_identifier` is the single chokepoint that turns a host- or wire-supplied product id into the canonical form derivation, product storage and permission scopes are keyed by; `is_product_identifier` is its boolean form.

`DOTNS_TLDS` (`dot`, `paseo`, `testnet`) backs it: the TLDs dotNS deployments register product names under, one entry per network a host can be pointed at. A name ending in one of them is also what navigation resolves back into the host's own product surface, so it bypasses the outbound domain grant.

`REMOTE_PERMISSION_TRUSTED_LABELS` lists the blessed product labels across all networks in `DOTNS_TLDS`. Its host permission exemptions follow the permission flow above; they do not confer wallet authorization on an incoming SSO request.

### Host Callback Traits

- `ProductStorage`: product-scoped key-value storage.
- `CoreStorage`: typed nonsecret records such as permission decisions and public account state.
- `SecretCoreStorage`: asynchronous typed protected storage for host grants, sessions, device identity and the installation encryption key. Only `None` means absence; failed protection or persistence is an error.
- `Navigation`: open URLs in the system browser.
- `Notifications`: deliver and cancel push notifications.
- `Permissions`: prompt for device and remote authorizations.
- `Features`: report host feature support.
- `ChainProvider` / `JsonRpcConnection`: open JSON-RPC connections to chains. Defined in `truapi_provider::platform` so the provider implements them without linking the runtime, and re-exported here.
- `AuthPresenter`: render core-owned auth state transitions.
- `UserConfirmation`: present product-host consent and independent wallet consent for signing, transactions, resources, aliases and preimages.
- `ThemeHost`: stream the host theme into the runtime.
- `LocaleHost`: emit the current host locale and subsequent changes.
- `ProductOperations`: begin and end product operations with optional labels that keep a worker active.
- `PreimageHost`: retrieve preimages and stream availability through the host-selected backend; Rust owns Bulletin submission.
- `ChatPlatform`: create product-scoped native chat rooms, register product chat bots, post messages into rooms, and stream the product's room list.
- `PermissionStatusHost`: report the OS status of a device capability without prompting, so a stored grant can be revalidated before it is acted on.
- `PocketPlatform`: stream the product's Pocket card collection and remove a card from it. The host owns the collection and decides which cards are privileged.
- `ContactsPlatform`: resolve the handles a transaction names to contacts, and render the picker that selects one. `contacts` is the only required method; `pick_contact` defaults to `Unsupported`, so a host serving no picker says so rather than looking like a user who declined. The host owns the UI, so the product receives only a handle for the selection. The core caches resolved handles; a host calls `notify_contacts_changed` on its runtime when a contact is removed or blocked.

`Platform` is a blanket-implemented supertrait that combines the capability traits above except `ChatPlatform`, `ContactsPlatform`, `PermissionStatusHost` and `PocketPlatform`, which `OptionalPlatform` lists instead: a host supplies each only when it can serve it. Codegen reads `OptionalPlatform` to emit each listed capability as an optional group on the host-callback surface.

Omitting `ChatPlatform` makes the core answer Chat calls `Unsupported`, and omitting `ContactsPlatform` or `PocketPlatform` does the same for Contacts or Pocket calls. Omitting `PermissionStatusHost` leaves device grants resolving from stored state alone, which is what a host with no OS permission model does anyway. Serving it gates both halves of the surface: a device permission request and a status read through `CoreAdmin` resolve the same two gates, so a settings screen never reports a capability as usable when the OS refuses it.

### Core-Owned Admin API

`CoreAdmin` is not part of the host-provided `Platform` callback surface. It exposes logout, permission administration, public subtree resolution and explicit secret reads to host UI. `PairingHostAdmin` separately exposes pairing cancellation and session-store change notification.

Its secret reads return the session's X25519 chat identity key, a paired host's statement signing key, and the installation's device encryption key. Public session material travels on `SessionUiInfo`; secrets require explicit calls and do not appear in auth-state broadcasts.

## Wire envelope

Every frame on the wire is encoded as:

```text
[requestId: SCALE str][trait: u8][method: u8][message_type: u8][payload bytes...]
```

The `(trait, method)` discriminant pair identifies the method via the auto-generated [`crate::generated::wire_table::WIRE_TABLE`], and the `message_type` byte names which leg of that method's exchange the frame carries (`Request`/`Response`/`Cancel`, or a subscription's `Start`/`Receive`/`Interrupt`/`Stop`). The trait byte comes from the trait-level `#[wire_trait(id = N)]` annotation; the method byte addresses a method within that trait, so method ids restart at 0 in every trait. Each method's ids are exposed as a named const (`PREIMAGE_SUBMIT`, ...); both `WIRE_TABLE` and the generated dispatcher reference those consts. Trait ids and per-trait method ordering are part of the wire protocol; only ever append within a trait.

The payload bytes are the SCALE-encoded inner value, inlined without a length prefix. The pair is carried as `Payload::trait_id` and `Payload::method_id` with the leg in `Payload::message_type`, and the dispatcher routes on the pair via pair-keyed tables.

### Cancelling a request

A `Cancel` frame carries no payload and names an in-flight call by its `requestId`. The dispatcher holds every in-flight request's `CancellationToken` in a registry keyed by that id, reserved before the handler is awaited, and a `Cancel` fires the token the id names. Handlers reach it through `CallContext::cancel()`.

Cancelling never answers: the call it names still settles with exactly one `Response`, and for a withdrawn call the dispatcher substitutes `Err(CallError::Cancelled)` whatever the handler made of the token. `encode_cancelled_response` builds those bytes without naming either of the method's payload types, so one encoder serves every method.

Only a `Cancel` frame triggers that substitution. A token a runtime fired itself, such as an attached timeout, still reports through the method's own error type, which keeps the `Cancelled` variant off the wire for a peer that never asked for it.

A `Cancel` naming nothing in flight is remembered rather than dropped. Each transport spawns a task per frame, so a cancel can reach dispatch before the request it names; the id goes into a short capped queue, and the request that follows answers `Cancelled` without running its handler. A cancel for a call that already settled lands in the same queue and is inert there, since ids are unique for the life of a connection.
