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

Two ownership bands. A **per-product-connection** band (byte frames →
dispatcher → role-neutral product runtime) is minted once per host↔product
connection by the role handle and lives for that product's whole session; a
**shared per-host** band owns role-neutral infrastructure (`RuntimeServices`)
and the role object (`PairingHost` or `SigningHost`), which is itself the
`ProductAuthority` and the `HostSession`. Pure `host_logic` is a no-I/O library
both bands call, not a stage in the frame path; the host's `Platform` impl is
the syscall floor.

```text
   ┌───────────────────────────────────────────────────────┐
   │ product      sandboxed iframe · native WebView        │
   └───────────────────────────────────────────────────────┘
                              │  ▲
          SCALE frames        │  │  MessageChannel · loopback
          both directions     ▼  │  WS
   ┌───────────────────────────────────────────────────────┐
   │ binding layer :  host shell / transport adapter       │
   │ thin byte bridge  ·  no protocol logic                │
   └───────────────────────────────────────────────────────┘

 ══ per host→product connection ( one per connected product ) ══
   ┌───────────────────────────────────────────────────────┐
   │ ProductRuntime           frame endpoint               │
   │ decode each SCALE frame → dispatch one typed call     │
   └───────────────────────────────────────────────────────┘
                              │  typed method call
                              ▼
   ┌───────────────────────────────────────────────────────┐
   │ ProductRuntimeHost       role-neutral                 │
   │ validate · permission-gate · confirm                  │
   └───────────────────────────────────────────────────────┘
                              │  wallet-authority tail :
                              │  sign · alias · entropy · alloc
                              │  via  ProductAccounts, the role
                              │  bound to this product
                              ▼

 ══ shared per host app ( one per host, all connections ) ══════
     the PairingHostRuntime | SigningHostRuntime handle owns both:
   ┌─────────────────────────────┐   ┌────────────────────────┐
   │ role  =  ProductAuthority   │   │ RuntimeServices        │
   │          + HostSession      │   │ platform · chain · RPC │
   │ PairingHost | SigningHost   │   │                        │
   └─────────────────────────────┘   └────────────────────────┘
              │
              │  PairingHost only : encrypted SSO channel
              ▼
       ┌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┐
       ╎ remote signing host   ( external wallet ) ╎
       └╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┘

   both bands call host_logic for pure work, never traverse it :
   ┌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┐
   ╎ host_logic       pure library ( no I/O )              ╎
   ╎ crypto · codecs · derivation · policy                 ╎
   └╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┘

 ══ host-owned floor · where every I/O above bottoms out ═══════
   ┌───────────────────────────────────────────────────────┐
   │ Platform impl  ( TS / Swift / Kotlin )                │
   │ storage · prompts · chain RPC · navigation            │
   └───────────────────────────────────────────────────────┘
```

`ProductRuntimeHost` handles everything role-neutral (id normalization,
permission gating, confirmation, soft product-key derivation), then delegates
the wallet-authority tail (`sign_*`, `create_transaction`, `account_alias`,
`create_proof`, `allocate_resources`, `derive_entropy`) through a
`ProductAccounts` handle bound to its product, with an `AuthoritySession`
snapshot the role revalidates before touching key material. Its
per-connection state (adapters, permissions, action channels, open operations
and its own chain connection) lives in a `ProductConnection`, and login, logout
and connection state go through the role's `HostSession`.

`runtime.rs` owns the product runtime and shared helpers. The trait adapters
are grouped by surface under `runtime/capabilities/`; cross-capability fixtures
and tests live in `runtime/tests.rs` and `runtime/tests/`.

### Permission flow

Permission grants are scoped by product id and typed request, so a grant for
one product never authorizes another product or another permission class.

Trusted products in `truapi::platform::REMOTE_PERMISSION_TRUSTED_LABELS` bypass
recorded permissions and permission prompts. Device permissions still need the OS
grant.
Account access, username disclosure, signing with their own product accounts and
AutoSigning proceed without approval. Legacy-account signing still asks the user,
except for raw bytes signed inside the `<Bytes>` watermark, which cannot pass for a
transaction's signing payload.

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
        +-- trusted product --> allow without storage or prompt,
        |                       unless the OS refuses a device capability
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

A valid remote permission request from a trusted product is authorized before
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

A local session's usernames come from the dotNS contracts on Asset Hub, read
after activation. A name the host passes is shown until a record answers.

`SigningHostConfig.network_suffix` is the network's bare dotNS TLD (`dot`,
`paseo`, or `testnet`). The shell supplies it alongside the chain genesis hashes
from the same network configuration used by wallet onboarding. It must match
the People chain's `NetworkSuffix.NetworkSuffix`: reserved identities derive
under `uid.<suffix>` and `peopl.<suffix>`, while the chain uses that suffix for
proof contexts. Configuration keeps local activation and key derivation
available offline. The core validates supported suffixes but does not
automatically check that the configured suffix matches the chain.

#### JAM peer transport

Native product runtimes (iOS, Android, CLI) serve `JamPeerTransport` (trait
111) themselves over JAMNP-S QUIC, with one endpoint per product connection.
Every `dial` first requires `RemotePermission::JamPeers { genesis }` through the
flow above. The connection asks once per genesis: concurrent dials wait for the
same prompt, a refusal stays `NotGranted` for the connection, and the answer is
persisted even when every dial waiting on it has given up.

- A dial answers within 10 seconds, prompt included. One still waiting then
  answers `Unreachable`, a cancelled one `Cancelled`, and what it would have
  opened is dropped without holding one of the 8 connection slots.
- The ALPN is `jam_peer_transport::alpn(genesis)`, and the peer certificate
  must carry the Ed25519 key the dial names. The P-256 key is for WebTransport
  hosts and is ignored.
- The first granted dial creates the endpoint, so a refused product binds no
  socket. Disposing the connection closes every peer connection, and later
  calls are `Denied`.

The browser core keeps the trait's `NotGranted` defaults, because its
JavaScript session answers trait 111 before frames reach the core.
`cargo test -p truapi --features mock --test live_jam_test_instance -- --include-ignored`
dials JAM-TEST-INSTANCE through a product runtime.

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

### Durable transactions

The `durable` module runs over the core database. A domain registers presigned
mortal extrinsics (`DurableRequest::presigned`, then `execute` or
`execute_with`, whose hook writes the domain's own rows in the same
transaction); the engine broadcasts them once committed and follows each with
a submission watch. Whatever no watch owns, including everything a previous
process left live, is decided by recovery passes: the completion ladder over
the shared chain capabilities and the domain's `CompletionOracle`. Every
verdict is a compare-and-set against the status and attempt it was derived
from, and rows are never deleted.

A domain can also schedule transactions it cannot sign yet
(`DurableRequest::scheduled`), each with a `SubmissionPlan`: the
`SubmissionPolicy` registered in the `DurableRegistry` that builds it, and that
policy's `PolicyParams` for it. They wait as `PENDING_SUBMISSION` rows until the
submission executor builds them: one task per domain, policy and group asks the
policy to `prepare` its waiting rows, starts an attempt for each `Ready` answer
and fails each `GiveUp`, backing off after rounds that decide nothing. A
presigned submission may carry a plan as well. When a verdict proves a
transaction with a policy failed, the verdict writer asks the policy's
`can_retry`, and a yes sends the row back to be built instead of failing it.
It is built again after a cooldown that grows with the attempts the row
records, so the wait survives a restart. The executor starts with the engine in `set_core_db` and again in every
recovery run.

The engine reports whether work is pending, including rows waiting to be
built, to `HostCallbacks::durable_work_changed`; the host then runs a
background task awaiting `run_durable_recovery()`, which returns once nothing
is live.

### The two roles

Both implement the role-neutral **`ProductAuthority`** and **`HostSession`**
traits; each owns its role-specific lifecycle, so no method exists on a role
that can't mean it:

- **`PairingHost`** (seedless): the user's keys live in an external wallet, so
  signing/aliases/entropy relay over an encrypted SSO channel (statement store
  on the People chain). The v2 wire protocol uses raw X25519 keys, HKDF-SHA256,
  and ChaCha20-Poly1305. `PairingHost` keeps the paired session's account
  material; [`SsoRequestService`](src/runtime/sso_request_service.rs) owns
  pairing and login state, persisted auth-session reload, remote signing-host
  liveness monitoring and the request channel, and reports session changes to
  `PairingHost` through `PairedSessionOwner`.
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

`SsoRequestService::call(request)` sends typed requests to
[`SigningHostSsoService`](src/runtime/signing_host/sso_service.rs). Handlers own
consent and business logic; [`SsoResponderService`](src/runtime/sso_responder_service.rs)
owns the transport loop: it answers pairing, serves the session and drops
duplicate requests. Resource consent is bound to the request's signing session:
account changes, disconnects, and reactivation invalidate pending approval before
allocation or key return. Allocation failure details stay in local transcripts.
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
`parity-scale-codec`; `ProductContext` holds a `ProductId`, which decodes by
parsing, so a context off the wire carries a normalized product id.

### Product Identity

Parsing a `ProductId` (its `FromStr` implementation) is the single chokepoint
that turns a host- or wire-supplied product id into the canonical form
derivation, product storage and permission scopes are keyed by. Nothing else
builds a `ProductId`, so holding one means the id is normalized. Code that reads
an id uses its methods, such as `bare_label` and `has_trusted_remote_permissions`,
and `CoreStorageKey::product_manifest` keys its manifest cache entry.

`DOTNS_TLDS` (`dot`, `paseo`, `testnet`) backs it: the TLDs dotNS deployments
register product names under, one entry per network a host can be pointed at. A
name ending in one of them is also what navigation resolves back into the host's
own product surface, so it bypasses the outbound domain grant.

`REMOTE_PERMISSION_TRUSTED_LABELS` lists the trusted product labels across
all networks in `DOTNS_TLDS`. These products bypass recorded permissions and
permission prompts. Device permissions still need the OS grant. The runtime grants
account access, username disclosure, signing with their own product accounts and
AutoSigning without approval. Legacy-account signing still asks the user, except
for raw bytes signed inside the `<Bytes>` watermark.

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
- `UserConfirmation`: confirm signing, transaction, resource and alias
  actions before the core asks the paired wallet. `preimage.submit` raises
  no per-upload confirmation: the `PreimageSubmit` permission authorizes it.
  `AllowAlways` covers every upload; `AllowOnce` covers a single upload.
  Allocating or renewing the Bulletin allowance may still ask the paired
  wallet.
- `ThemeHost`: stream the host theme into the runtime.
- `PreimageHost`: submit and look up preimages through the host-selected backend.
- `ChatPlatform`: create product-scoped native chat rooms, register product
  chat bots, post messages into rooms, and stream the product's room list.
- `PermissionStatusHost`: report the OS status of a device capability without
  prompting, so a stored grant can be revalidated before it is acted on.
- `PocketPlatform`: stream the product's Pocket card collection and remove a
  card from it. The host owns the collection and decides which cards are
  privileged.
- `ContactsPlatform`: resolve opaque handles to contacts, render single or multiple
  selection pickers, and place host-owned contact names over product surfaces.
  `contacts` is the only required method; `pick_contact` and `pick_contacts`
  default to `Unsupported`, never a fake selection. The multi-picker receives a
  host-private `ContactSelection` record of resolved accounts. Confirmed empty
  selection is distinct from dismissal, and unresolved initial handles fail closed.
  Session and directory generations are checked across host calls.
  `place_contact_labels` is independent of Profile grants; products receive neither
  names nor availability. Placements are serialized and cleared at connection
  teardown and session change. Hosts call `notify_contacts_changed` when a contact
  is removed or blocked, invalidating cached handles. Their label layers clear stale
  names and refresh the live placement from the current directory.
- `ProfilePlatform`: show a product-referenced profile in host-owned UI, show
  a contact's shared profile naming the contact who sent it, and draw the
  avatars of contacts who shared one over a chat product. The host resolves,
  decrypts and renders each reference; nothing returns to the product but
  acceptance. Naming the contact is optional and presents the reference alone
  by default; drawing avatars is optional and draws nothing by default.

`Platform` is a blanket-implemented supertrait that combines the capability
traits above except `ChatPlatform`, `ContactsPlatform`, `PermissionStatusHost`,
`PocketPlatform`, `GamePlatform` and `ScannerPlatform`, which `OptionalPlatform`
lists instead: a host supplies each only when it can serve it. Four traits are in
neither. `ExpandedCardHost` travels per connection rather than with the
platform. `BalancePlatform`, `PaymentPlatform` and `TopUpPlatform` are installed
on their own, with `set_balance_platform`, `set_payment_platform` and
`set_top_up_platform` or their native `set_*_callbacks`, so only a Rust or
native host can serve them: the host-callback bridge cannot yet carry their
typed errors or their two-argument status streams, so a browser host, and the
CLI today, answer them `Unsupported`. Codegen reads `OptionalPlatform` to emit each listed capability
as an optional group on the host-callback surface.

Omitting `ChatPlatform` makes the core answer Chat calls `Unsupported`, and
omitting `ContactsPlatform`, `PocketPlatform`, `GamePlatform` or
`ScannerPlatform` does the same for Contacts, Pocket, Game or Scanner calls. A
connection without an `ExpandedCardHost` answers a Widget's `ExpandedCard` calls
`Unsupported`.
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
