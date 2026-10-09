# truapi runtime

_Runtime core for TrUAPI: dispatcher, protocol frames, SCALE-coded wire envelope._

## What the runtime is for

The `runtime` feature of `truapi` turns trait implementations of the
protocol API into a working host. It owns:

- the [`ProtocolMessage`] wire envelope and SCALE codec
- the [`Dispatcher`] that routes incoming frames to per-method handlers
- the subscription lifecycle (start/receive/stop/interrupt)
- the [`Transport`] trait that platform-specific IPC backends implement
- the auto-generated dispatcher/wire-table tables shipped under
  [`crate::generated`]
- the host embedding surface: one long-lived role handle
  (`PairingHostRuntime` or `SigningHostRuntime`) per host application, exposing
  shared [`RuntimeServices`] plus one [`ProductRuntime`] per product connection

## Architecture

Each product connection owns its dispatcher adapters and `ProductConnection`. The host runtime shares `HostAccounts<H>`, its selected account holder, the session lifecycle and `RuntimeServices` across connections. The dispatcher erases the concrete account-holder type; product execution and administration handles expose no account-holder type parameter.

```text
Per product connection
┌─────────────────────────────────────────────────────────────┐
│ ProductRuntime: frames → Dispatcher                         │
│ ProductRuntimeHost<H>: validation and protocol adapters     │
│ ProductConnection: permissions, platform adapters, demand   │
└────────────────────┬──────────────────────┬─────────────────┘
                     │ account operations   │ login / identity
Shared per host      ▼                      ▼
┌──────────────────────────────┐  ┌────────────────────────────┐
│ HostAccounts<H>              │  │ HostSession                │
│ acquire, retain, use grants  │  │ SigningHost                │
│ HostGrantStore + registry    │  │ or SsoRequestService       │
└──────────────────┬───────────┘  └────────────────────────────┘
                   │ AccountHolder
         ┌─────────┴─────────────────────┐
         ▼                               ▼
 WalletAccountHolder              SsoAccountHolderClient
 wallet secrets and consent       canonical calls ↔ SSO messages
         ▲                               │
         │                               ▼
 SsoAccountHolderService           SsoRequestService
 incoming peer consent            selected channel and transport

Shared RuntimeServices: platform, chain access and RPC clients
Host Platform: storage, prompts, chain transport and navigation
```

`HostAccounts<H>` uses the account session selected before product permission and review. It retains and uses delegated keys or wallet-issued authorization; `AccountHolder` owns wallet execution and grant issuance. Wallet lifecycle and secret helpers remain internal. The native wallet and shared host use the same ring registry. `host_logic` provides pure crypto, codecs and derivation rather than another execution layer.

`runtime.rs` owns the product runtime and shared helpers. The trait adapters
are grouped by surface under `runtime/capabilities/`; cross-capability fixtures
and tests live in `runtime/tests.rs` and `runtime/tests/`.

### Permission flow

Permission grants are scoped by product id and typed request, so a grant for
one product never authorizes another product or another permission class.

Blessed products in `truapi::platform::REMOTE_PERMISSION_TRUSTED_LABELS` bypass
recorded permissions. Only device permissions require consent.
Account access, username disclosure, signing with their own product accounts and
AutoSigning proceed without approval. Legacy-account signing still asks the user.

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

A valid remote permission request from a blessed product is authorized before
reading or writing permission storage. Empty or invalid domain bundles are denied.
Other products resolve saved decisions and prompt when undecided.

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

A device permission also carries the host application's OS gate. Both the
request path and the `CoreAdmin` status read resolve it through the host's
`PermissionStatusHost`, so an OS refusal reads as `Denied` whatever is stored,
and the stored product decision is never overwritten by it. Remote,
identity-disclosure and account-access decisions have no OS gate.

The embedder builds a role handle, `PairingHostRuntime::new(...)` or
`SigningHostRuntime::new(...)`, then calls `product_runtime(product, sink)` for
each product connection. Role-specific operations live only on the matching handle:
`cancel_pairing`, `notify_session_store_changed`, `activate_stored_session`,
`activate_external_session`, and `reset_session_state` on the pairing handle,
`activate_local_session` on the signing handle. Both handles expose
`clear_product_state` to revoke one product's capability material without
touching the session or other products, and `notify_contacts_changed` to drop
the contact handles the core cached once a contact is removed or blocked. Calling the wrong operation is
a compile error, not a runtime `Unavailable`.

`SigningHostConfig.network_suffix` is the network's bare dotNS TLD (`dot`,
`paseo`, or `testnet`). The shell supplies it alongside the chain genesis hashes
from the same network configuration used by wallet onboarding. It must match
the People chain's `NetworkSuffix.NetworkSuffix`: reserved identities derive
under `uid.<suffix>` and `peopl.<suffix>`, while the chain uses that suffix for
proof contexts. Configuration keeps local activation and key derivation
available offline. The core validates supported suffixes but does not
automatically check that the configured suffix matches the chain.

### Core database

Native signing hosts (iOS, Android, the CLI) give the core a directory for its
own SQLite database (`store` module, bundled SQLite through `rusqlite` and
`async-sqlite`). The runtime opens `core.sqlite3` there at startup, so a
missing or unwritable directory stops it from starting. Keep the directory out
of device backups: it holds durable-transaction state that must not be restored
onto another device. The required `HostRuntimeConfig.database_directory` sets
it on iOS and Android, `SigningHostRuntime::set_core_db` on any other embedder,
and `core_database_status()` reports the SQLite version, schema version and
path. Web hosts do not compile the store.

### The two roles

- Native execution uses `HostAccounts<WalletAccountHolder>`; paired execution uses `HostAccounts<SsoAccountHolderClient>`. Both share the same grant acquisition, retention and delegated signing policy.
- `WalletAccountHolder` owns entropy, wallet consent, derivation, issuance and renewal. Incoming `SsoAccountHolderService` calls it directly and shares stored account-access decisions but cannot inherit native AutoSigning or populate native grants.
- `AccountInvocation` carries the selected wallet activation and trusted caller origin. Local callers may carry a wallet-issued authorization; remote signing always requires wallet approval. Local and remote account access share stored decisions, including refusals.
- Product calls keep the selected `AuthoritySession` across review. The account holder validates that activation before wallet execution. `HostAccounts` checks grant revisions when retaining or using delegated keys; product reset clears grants without changing the wallet activation. It does not cancel an independent wallet approval already in progress.
- `AccountHolder::allocate_grants` returns a lazy, ordered stream. The host retains each success before continuing; recoverable item failures and whole-operation failures remain distinct.
- `HostGrantStore` owns retained keys and public subtrees. AutoSigning has one form, the product key, which `HostAccounts` signs with on either host. Paired keys use existing storage encodings; native grants remain in memory, and a native AutoSigning key lasts one wallet activation. Both hosts read the retained Bulletin key’s on-chain authorization before submission, checking expiry, transaction count and payload capacity. Insufficient authorization requests an approved Increase and waits for propagation; failure preserves the key. Native Statement Store submissions silently renew only the product’s retained sponsorship signer. Paired renewals use the existing SSO allocation request and its phone approval.
- `HostSession` covers login, disconnect and identity lookup through the existing `SigningHost` or `SsoRequestService`. Paired session replacement waits for pending storage cleanup, and explicit cancellation withdraws submitted SSO requests.
- `ProductConnection` holds the existing per-product adapters, permissions, action channels and open-operation references independently of the holder type. The dispatcher erases that type for native control handles. Worker scheduling and ownership remain in their existing components.

- **Paired runtime**: `HostAccounts<SsoAccountHolderClient>` uses retained keys locally and sends holder operations over encrypted SSO when needed. Transport uses the People-chain statement store in `sso_request_service/channel.rs`. The
- `ProductConsent` holds one runtime's account-access decisions and review rules. The wallet asks it before wallet work and product calls ask it for account access, both on the runtime-wide prompt channel. Allow once answers the same requester and target until the runtime restarts, the wallet locks or changes, or the requesting product is reset, and one account-access prompt runs at a time.
  v2 wire protocol uses raw X25519 keys, HKDF-SHA256, and
  ChaCha20-Poly1305. `SsoRequestService` owns pairing/login state, persisted auth-session reload and remote signing-host liveness monitoring.
- **`SigningHost`** (wallet-local): signs on device from local BIP-39 entropy,
  no pairing flow. `signing_host/local_activation.rs` establishes a session
  from host-held secret material. Its public identity is the RFC-0022
  `uid.<tld>` index-0 product account of the configured network. RFC-0024 ring-VRF keys are explicit,
  product-owned registry entries; aliases, proofs, direct signatures, and
  internal personhood flows use the requested or user-selected registered key
  without a compiled-in fallback. It resolves RFC-0004 `RingLocation` values
  against the chain's `Members` pallet and pins membership, ring pages,
  exponent, and revision reads to one finalized block before creating a proof.
  Extrinsic-payload signing and v4 transaction construction work from
  pre-encoded payload fields, so no chain metadata is needed;
  statement-store and Bulletin allowance allocation are native-only (wasm
  builds report them as unavailable) and do need metadata, which they take from
  the `RuntimeServices`-owned per-chain cache rather than re-reading it per
  call.

`host_logic` stays pure: the orchestrators above call into it for codecs,
session/SSO crypto, key derivation, and permission policy, while all I/O
(statement-store RPC, storage, prompts, chain RPC) stays in the layers above.

`host_logic::worker::WorkerLedger` holds the reference count per product
worker from the Worker Lifecycle RFC, one per host in `RuntimeServices`. Both
bindings expose it as `acquire_worker` and `release_worker`. Every `Start` or
`Stop` the counts produce is reported on one channel, the host's
`worker_demand_changed` callback, in the order the ledger produced it and one
at a time, whether the host asked for the transition or the core took the
reference itself for an open render. The host runs the executable and the core
keeps the count; on the web `@parity/truapi-host` pushes the wanted level to
the page.

### Inter-host SSO

`SsoAccountHolderService` serves one peer through the shared wallet and the activation that authenticated its channel. It owns that peer's withdrawals without host grants or per-message wallet selection. Directly dispatched stale requests produce the macro-generated NotConnected response; activation loss during a request discards its result. The transport rejects posts on an expired activation, so even a disconnected response requires a live channel. Native transports first verify their own statement and encryption public keys through `open_sso_session`, then retain independent peer services from that binding. Neither a child service nor an old binding can attach itself to a replacement activation, including the same wallet reactivated. Product reset does not invalidate the wallet binding. Already-dispatched transport writes cannot be recalled.

`SsoRequestService::call(request)` sends typed requests to
[`SsoAccountHolderService`](src/runtime/sso_account_holder_service.rs). Handlers forward remote account invocations and encode wallet receipts in the existing SSO messages. Signing consent belongs to the account implementation, resource consent and issuance to `WalletAccountHolder`. [`SsoResponderService`](src/runtime/sso_responder_service.rs) owns the signing host's side of the transport: pairing answers, the serve loop, duplicate detection and binding native transports, each peer answered by its own `SsoAccountHolderService`. Consent is bound to the request's signing session: account changes, disconnects, and reactivation invalidate pending approval before allocation or key return. Allocation failure details stay in local transcripts.
Allocation requests use the canonical `truapi::latest::AllocatableResource` type.
Signing uses canonical request and result types. Product-scoped VRF requests use
`ProductRequest<P>` to attach the caller to a canonical payload. Both product and
SSO signing encode `with_signed_transaction` with the one-byte `OptionBool` codec.

A pairing host that stops waiting because its caller withdrew the request sends
a `Cancel` naming it, when that request is still the newest on the session's
request channel. The responder reads statements while it serves a request, so a
`Cancel` fires the running request's token or stops a queued one from starting;
a withdrawn request posts no response. See the
[SSO request cancellation RFC](../../../docs/rfcs/sso-request-cancellation.md).

When a device finishes pairing, the signing host reports it to the embedder's
[`DevicePairingObserver`](src/runtime/sso_responder_service.rs), installed
once through `SigningHostRuntime::set_device_pairing_observer`, and on a native
host to `HostCallbacks::device_paired`. It carries the `PairedSsoPeer` that
pairing produced, which is also what `resume_pairing` and
`disconnect_paired_host` take.

A native host reaches the responder through
`NativeTrUApiHostRuntime`: `notify_pairing_allowance_allocation` and
`notify_pairing_failed` for the two notices a peer gets before the answer,
`establish_pairing` for the answer itself, `resume_pairing` to serve the
session, and `disconnect_paired_host` to end it. Answering and serving are
separate calls rather than one `respond_to_pairing`, because the host persists
the peer between them and it is the host's stored record that `resume_pairing`
is called with afterwards.

Two steps around those calls are the host's. The answer is signed by this
host's own SSO statement identity, so the `WalletSso` target has to be
allocated before `establish_pairing` runs, and the peer's device statement
account has to be tracked alongside it for the peer to author into the session:
`parse_pairing_deeplink` reads that account out of the deeplink, and a pairing
that then fails untracks it again unless the device was already paired. The
core prompts for nothing on the way, so that prompt is the host's, and the same
call carries the `PairingProposalMetadata` it names the peer by, trimmed and
stripped of the control characters and bidirectional overrides that would
otherwise rewrite the prompt's own text around it. Nothing signs that metadata,
so it says what the peer calls itself and not who it is. In process the same
decoder is `PairingProposal::from_deeplink`. And
`disconnect_paired_host` submits the notice and nothing more, so ending a
pairing also means cancelling that peer's `resume_pairing` task and untracking
its renewal account. `truapi-host-cli` runs both sequences.

The report fires once the handshake answer is on the Statement Store, which is
the earliest point the peer could read it. It is not proof that the peer did:
a pairing host races cancellation against the answer arriving and gives up
after its own deadline, either of which leaves a reported device that never
connects. The report is at least once per pairing, so a device that pairs
again is reported again with the same value. Resuming a stored pairing reports
nothing, so the host owns the record of which devices it has already seen; the
core keeps no list to replay. The core has no chat of its own, so announcing a
new device to the user's existing contacts belongs to the embedder.

The `host_logic::sso::messages::v1::RemoteMessage` enum owns the SCALE wire
contract. Its response variants wrap named result payloads in `Response<P>`,
which carries `responding_to` once. Macros generate request/response pairing
and dispatch; see the
[macro guide](../truapi-macros/README.md) for handler signatures and reply handling.
A new operation needs payload definitions, wire variants, a handler, and a typed
client call.

Rust consumers must update renamed SSO types and helpers even when SCALE encoding
is unchanged. Use `RemoteMessage::request(message_id, request)` to construct
requests. Decoded `SsoSessionStatement::RemoteMessages` preserves message order;
match variants directly or use the request's `SsoRequest::response_from_message`.

## Host platform interface

The `platform` module holds the capability traits a TrUAPI host implements.
Each host (web/WASM, desktop, iOS/UniFFI, Android/UniFFI) implements these
traits to provide the native capabilities the shared Rust runtime cannot reach
directly. The dispatcher calls this surface while the Rust
runtime owns product account management, SSO signing, statement-store protocol
flows, permission state, and auth state transitions.

### Type Imports

Most host-facing wire types are imported from `truapi::latest` by this module and
are exposed through the trait signatures below. `ProductContext` and
`ProductExecutionKind` are defined here instead, and codegen emits their host
codecs from these definitions. Both are SCALE-encodable so they can cross the
wasm callback boundary, where every parameter is encoded with
`parity-scale-codec`; `ProductContext` decodes through its validating
constructor, so a context off the wire carries a normalized product id.

### Product Identity

`normalize_product_identifier` is the single chokepoint that turns a host- or
wire-supplied product id into the canonical form derivation, product storage and
permission scopes are keyed by; `is_product_identifier` is its boolean form.

`DOTNS_TLDS` (`dot`, `paseo`, `test`) backs it: the TLDs dotNS deployments
register product names under, one entry per network a host can be pointed at. A
name ending in one of them is also what navigation resolves back into the host's
own product surface, so it bypasses the outbound domain grant.

`REMOTE_PERMISSION_TRUSTED_LABELS` lists the blessed product labels across
all networks in `DOTNS_TLDS`. These products bypass recorded permissions and
prompt only for device permissions. The runtime grants
account access, username disclosure, signing with their own product accounts and
AutoSigning without approval. Legacy-account signing still asks the user.

### Host Callback Traits

- `ProductStorage`: product-scoped key-value storage.
- `CoreStorage`: typed core-owned storage slots such as auth session, pairing
  identity, and permission authorization state.
- `Navigation`: open URLs in the system browser.
- `Notifications`: deliver and cancel push notifications.
- `Permissions`: prompt for device and remote authorizations.
- `Features`: report host feature support.
- `ChainProvider` / `JsonRpcConnection`: open JSON-RPC connections to chains.
  Defined in `truapi_provider::platform` so the provider implements them
  without linking the runtime, and re-exported here.
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
- `ContactsPlatform`: resolve the handles a transaction names to contacts, and
  render the picker that selects one. `contacts` is the only required method; `pick_contact`
  defaults to `Unsupported`, so a host serving no picker says so rather than
  looking like a user who declined. The host owns the UI, so the list never
  reaches the product — only a handle for the selection does. The core caches
  resolved handles; a host calls `notify_contacts_changed` on its runtime when
  a contact is removed or blocked.
- `GamePlatform`: hold the game product's next-game reminder and drop it.
  The core serves Game only to `dim2`, on every network, and answers
  `Unsupported` to any other product without calling the host. A host
  keeps one reminder per product: a schedule replaces the reminder the same
  product already holds. The core asks for no per-product consent: the host
  asks the OS for what the reminder needs, rings an alarm where the OS allows
  one and delivers a notification otherwise, may add a calendar event, and
  keeps the reminder across app kill and reboot. A schedule the host cannot
  hold fails as a host failure carrying its reason.

`Platform` is a blanket-implemented supertrait that combines the capability
traits above except `ChatPlatform`, `ContactsPlatform`, `PermissionStatusHost`,
`PocketPlatform` and `GamePlatform`, which `OptionalPlatform` lists instead: a
host supplies each only when it can serve it. Codegen reads `OptionalPlatform`
to emit each listed capability as an optional group on the host-callback
surface.

Omitting `ChatPlatform` makes the core answer Chat calls `Unsupported`, and
omitting `ContactsPlatform`, `PocketPlatform` or `GamePlatform` does the same
for Contacts, Pocket or Game calls.
Omitting `PermissionStatusHost` leaves device grants resolving from stored
state alone, which is what a host with no OS permission model does anyway.
Serving it gates both halves of the surface: a device permission request and a
status read through `CoreAdmin` resolve the same two gates, so a settings
screen never reports a capability as usable when the OS refuses it.

### Core-Owned Admin API

`CoreAdmin` is not part of the host-provided `Platform` callback surface. It is
the core-owned control API exposed to host UI for logout, pairing cancellation,
session-store refresh, and permission administration.

It also serves the session's X25519 chat identity private key. Public session
material a host needs to address the identity or the paired device travels on
`SessionUiInfo` instead; only the secret requires this deliberate call.

## Wire envelope

Every frame on the wire is encoded as:

```text
[requestId: SCALE str][trait: u8][method: u8][message_type: u8][payload bytes...]
```

The `(trait, method)` discriminant pair identifies the method via the
auto-generated [`crate::generated::wire_table::WIRE_TABLE`], and the
`message_type` byte names which leg of that method's exchange the frame
carries (`Request`/`Response`/`Cancel`, or a subscription's
`Start`/`Receive`/`Interrupt`/`Stop`). The trait
byte comes from the trait-level `#[wire_trait(id = N)]` annotation; the method
byte addresses a method within that trait, so method ids restart at 0 in every
trait. Each method's ids are exposed as a named const (`PREIMAGE_SUBMIT`, ...);
both `WIRE_TABLE` and the generated dispatcher reference those consts. Trait
ids and per-trait method ordering are part of the wire protocol; only ever
append within a trait.

The payload bytes are the SCALE-encoded inner value, inlined without a
length prefix. The pair is carried as `Payload::trait_id` and
`Payload::method_id` with the leg in `Payload::message_type`, and the
dispatcher routes on the pair via pair-keyed tables.

### Cancelling a request

A `Cancel` frame carries no payload and names an in-flight call by its
`requestId`. The dispatcher holds every in-flight request's
`CancellationToken` in a registry keyed by that id, reserved before the
handler is awaited, and a `Cancel` fires the token the id names. Handlers
reach it through `CallContext::cancel()`.

Cancelling never answers: the call it names still settles with exactly one
`Response`, and for a withdrawn call the dispatcher substitutes
`Err(CallError::Cancelled)` whatever the handler made of the token.
`encode_cancelled_response` builds those bytes without naming either of the
method's payload types, so one encoder serves every method.

Only a `Cancel` frame triggers that substitution. A token a runtime fired
itself, such as an attached timeout, still reports through the method's own
error type, which keeps the `Cancelled` variant off the wire for a peer that
never asked for it.

A `Cancel` naming nothing in flight is remembered rather than dropped. Each
transport spawns a task per frame, so a cancel can reach dispatch before the
request it names; the id goes into a short capped queue, and the request that
follows answers `Cancelled` without running its handler. A cancel for a call
that already settled lands in the same queue and is inert there, since ids are
unique for the life of a connection.
