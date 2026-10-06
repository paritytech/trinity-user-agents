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
`ProductAuthority`. Pure `host_logic` is a no-I/O library both bands call, not a
stage in the frame path; the host's `Platform` impl is the syscall floor.

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
                              │  via  Arc<dyn ProductAuthority>
                              ▼

 ══ shared per host app ( one per host, all connections ) ══════
     the PairingHostRuntime | SigningHostRuntime handle owns both:
   ┌─────────────────────────────┐   ┌────────────────────────┐
   │ role  =  ProductAuthority   │   │ RuntimeServices        │
   │ PairingHost | SigningHost   │   │ platform · chain · RPC │
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
`create_proof`, `allocate_resources`, `derive_entropy`) through an
`Arc<dyn ProductAuthority>` handle with an `AuthoritySession` snapshot the
role revalidates before touching key material.

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

Both implement the role-neutral **`ProductAuthority`** trait; each owns its
role-specific lifecycle, so no method exists on a role that can't mean it:

- **`PairingHost`** (seedless): the user's keys live in an external wallet, so
  signing/aliases/entropy relay over an encrypted SSO channel (statement store
  on the People chain; the channel lives in `pairing_host/sso_channel.rs`). The
  v2 wire protocol uses raw X25519 keys, HKDF-SHA256, and
  ChaCha20-Poly1305. It owns pairing/login state, persisted auth-session reload,
  and remote signing-host liveness monitoring.
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

`PairingHost::call(request)` sends typed requests to
[`SigningHostSsoService`](src/runtime/signing_host/sso_service.rs). Handlers own
consent and business logic; `sso_responder.rs` owns the transport loop and shared
allowance helpers. Resource consent is bound to the request's signing session:
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
[`DevicePairingObserver`](src/runtime/signing_host/sso_responder.rs), installed
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
- `FundingPlatform`: show the native funding overlay for a session, with its
  direction and amount and the product that opened it, and report whether the
  user started or dismissed it. Installed with `set_funding_platform`; the
  Balance card opens sessions with `open_funding`. The core owns the sessions,
  persists them, expires them, and answers Funding calls `Unsupported` while no
  overlay is installed.
  Once a provider is chosen, a signing host calls `assign_funding_deposit` to
  give an inbound session its deposit account under `fund.<network suffix>`;
  the core then polls that account at finalized Asset Hub blocks and moves the
  session to `Converting` once the expected balance is there. `quote_funding_deposit` gives the
  deposit that credits the session's amount, and assignment refuses less.
  Assignment needs
  `enable_funding_conversion` with the network's CASH asset id, and fixes the
  route then: a teleport for CASH, a PSM mint for a stablecoin the PSM serves.
  The core converts with one Asset Hub transaction signed by the deposit
  account, paying fees in the deposited asset, after dry-running it on Asset
  Hub and the message it forwards on People. Once the CASH lands on People,
  the core credits it through `TopUpPlatform` with the deposit account's key
  as a `PrivateKey` source, and the session ends `Delivered`. Crediting
  follows getcash: each of up to three top-ups is sized from the account's
  CASH on People when it starts and has its own id. A short claim leaves
  the rest to the next one, and after the last the session is delivered
  with what was claimed. A conversion the PSM will not serve as quoted (fee
  too high, amount out of range) is held at once; one it cannot serve now
  (minting stopped, debt ceiling) is held on the third refusal, as is any
  other refusal, a conversion included on chain that failed among them. A
  top-up is kept once sized and before it is registered, so a restart
  registers the same amount under the same id. A held conversion, a credit
  that claimed nothing, or one that timed out fails with its funds still on
  the account, stays out of the history bound, and `retry_funding` picks it
  up: the conversion again by its route, or the credit from its last
  attempt. A product sees the retried session by subscribing again.
  For a withdrawal, `assign_funding_withdrawal` gives an outbound session
  that names its amount a `wd:eph:<destination>:<n>` account, the next one
  with no CASH on People, and asks the host's `PaymentPlatform` to have the
  user pay the amount into it, as the funding product, under getcash's
  payment ids (the account, then `blake2(account ‖ attempt)`). The watch
  reads the account's CASH and the host's latest word on the current
  attempt: CASH there moves the session to `Paid`, a payment under way no
  longer expires, a failed or refused one ends the session for
  `retry_funding`, which asks again under the next id while the account is
  still read, and one never taken expires after 30 minutes. A cancel reads
  both first and is refused once the payment is taken. The watch starts
  before the request, which resolves only once the user has decided.
  A `Paid` session's CASH then moves to Asset Hub as getcash moves it, one
  transaction per pass decided from the account's balances: a pool swap on
  People, paid in CASH, buys the PAS the fees need, then one XCM, paid in
  that PAS, carries all the CASH and PAS to Asset Hub, sells the CASH there
  for PAS and deposits it on the withdrawal account. The XCM is sized by dry
  runs on People (fees measured with all the PAS, then an exact allowance
  that traps nothing) and on Asset Hub (what lands); CASH teleports when
  Asset Hub trusts People for it and is reserve-withdrawn otherwise. It has
  landed once the account holds no CASH and Asset Hub shows at least the
  dry run's landing less 5%, and the session is `Withdrawn`; three rejected
  transactions, or 15 minutes of refusals, hold it for `retry_funding`.
  `set_withdrawal_payout` names where the PAS goes from there: a provider's
  deposit channel with its expiry, checked against the provider's own record
  by whoever hands it over, or the user's own account. The withdrawal
  account then pays everything it holds there with `Balances::transfer_all`,
  which closes it, as getcash's sweep pays a channel; a channel within
  about 23 minutes of closing (the read, the transaction's 64-block era and
  ten minutes for the provider) is not paid, and the session waits for a new
  payout and a retry. Once the account is empty, down to the existential
  deposit, the session is `Released`. Following the provider's swap to its
  destination is the provider's; a channel should name the withdrawal
  account as its refund address, as getcash's do, and a refund that lands
  there is paid out again once a new payout is set.
  `cancel_funding` cancels a session while nothing has arrived, as getcash
  does: it is refused once a deposit was seen or while anything is on the
  deposit account, which is read first, and nothing is cancelled if that
  read cannot be confirmed within 8 seconds. A cancelled session's account
  is still read for 72 hours, so a payment that arrives after all converts.
  Native hosts reach all of this through `NativeTrUApiHostRuntime`:
  `set_funding_callbacks` (the overlay), `set_top_up_callbacks` with
  `notify_top_up_status` (the top-up engine), `enable_funding_conversion`,
  `open_funding`, `quote_funding_deposit`, `assign_funding_deposit`,
  `retry_funding`, `funding_session` and `funding_sessions`.
  For progress and history, each session carries the time it first reached
  each in-flight `FundingStep` (awaiting deposit, deposit seen, converting,
  landed, claiming), between its `opened_at_ms` and the end its stage
  records, from which a host draws each rail's markers, and the CASH that
  `landed` on People, so a partial credit shows what stays on the account.
  `funding_deposit_address` gives the deposit account as an Asset Hub
  address (SS58 prefix 0) for the deposit screen. `funding_sessions` lists
  sessions in flight first, a held one among them, then ended ones, each
  newest first. An ended session is handed to the host through
  `funding_session_changed` each time funding resumes until the host calls
  `acknowledge_funding_session`, so its history writes every outcome once;
  the core keeps the 50 newest recorded sessions and every unrecorded one
  within the 200 newest ended. Amounts cross the FFI as decimal strings.
  Each watched deposit account is read for every deposit asset the host
  names when it enables conversion, and the native token. A deposit counts
  as delivered once it reaches the deposit quoted for its asset, or what the
  provider was asked for without a quote. Below that, a short or wrong-asset
  deposit shows as a mismatch, and `accept_funding_deposit` converts what
  arrived instead. An ended session's account is read every five minutes
  for 72 hours: a full deposit that arrives after expiry converts on its
  own, and a mismatch can still be accepted. A product stream that already
  ended on `Failed` does not hear of a reopened session; the product sees
  the new stage by subscribing again. `funding_account_secret` exports an
  account's raw seed, as getcash does, for a user to take funds back with a
  wallet.
- `PaymentPlatform`: pay from the user's balance to an account once the user
  approves, and stream each payment's status by its caller-chosen id.
  Installed with `set_payment_platform`; native hosts use
  `set_payment_callbacks` with `notify_payment_status`. The core requires a
  session and refuses the funding product. Without it, `request` and
  `statusSubscribe` answer `Unsupported`.
- `TopUpPlatform`: claim a top-up source's funds into the user's balance and
  stream each top-up's status. Installed with `set_top_up_platform`. The core
  requires a session and checks the source keys; the host owns claiming,
  retries, partial claims, persistence and scoping ids to the product. Without
  it, `topUp` and `topUpStatusSubscribe` answer `Unsupported`.
- `ContactsPlatform`: resolve the handles a transaction names to contacts, and
  render the picker that selects one. `contacts` is the only required method; `pick_contact`
  defaults to `Unsupported`, so a host serving no picker says so rather than
  looking like a user who declined. The host owns the UI, so the list never
  reaches the product — only a handle for the selection does. The core caches
  resolved handles; a host calls `notify_contacts_changed` on its runtime when
  a contact is removed or blocked.

`Platform` is a blanket-implemented supertrait that combines the capability
traits above except `ChatPlatform`, `ContactsPlatform`, `PermissionStatusHost`
and `PocketPlatform`, which `OptionalPlatform` lists instead: a host supplies
each only when it can serve it. Codegen reads `OptionalPlatform` to emit each listed
capability as an optional group on the host-callback surface.

Omitting `ChatPlatform` makes the core answer Chat calls `Unsupported`, and
omitting `ContactsPlatform` or `PocketPlatform` does the same for Contacts or
Pocket calls.
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
