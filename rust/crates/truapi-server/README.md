# truapi-server

_Runtime core for TrUAPI: dispatcher, protocol frames, SCALE-coded wire envelope._

## What this crate is for

`truapi-server` is the runtime that turns trait implementations of the `truapi` API into a working host. It owns:

- the [`ProtocolMessage`] wire envelope and SCALE codec
- the [`Dispatcher`] that routes incoming frames to per-method handlers
- the subscription lifecycle (start/receive/stop/interrupt)
- the [`Transport`] trait that platform-specific IPC backends implement
- the auto-generated dispatcher/wire-table tables shipped under [`crate::generated`]
- the host embedding surface: one long-lived role handle (`PairingHostRuntime` or `SigningHostRuntime`) per host
  application, exposing shared [`RuntimeServices`] plus one [`ProductRuntime`] per product connection

## Architecture

Two ownership bands. A **per-product-connection** band (byte frames → dispatcher → role-neutral product runtime) is
minted once per host↔product connection by the role handle and lives for that product's whole session; a **shared
per-host** band owns role-neutral infrastructure (`RuntimeServices`) and the role object (`PairingHost` or
`SigningHost`), which is itself the `ProductAuthority`. Pure `host_logic` is a no-I/O library both bands call, not a
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

`ProductRuntimeHost` handles everything role-neutral (id normalization, permission gating, confirmation, soft
product-key derivation), then delegates the wallet-authority tail (`sign_*`, `create_transaction`, `account_alias`,
`create_proof`, `allocate_resources`, `derive_entropy`) through an `Arc<dyn ProductAuthority>` handle with an
`AuthoritySession` snapshot the role revalidates before touching key material.

`runtime.rs` owns the product runtime and shared helpers. The trait adapters are grouped by surface under
`runtime/capabilities/`; cross-capability fixtures and tests live in `runtime/tests.rs` and `runtime/tests/`.

### Permission flow

Permission grants are scoped by product id and typed request, so a grant for one product never authorizes another
product or another permission class.

Remote permissions carry one exception. A product whose label is listed in
`truapi_platform::REMOTE_PERMISSION_TRUSTED_LABELS` holds every `RemotePermission` without a prompt: while nothing is
stored the lookup reports `Authorized` and writes nothing. A stored `Denied` still wins, so the admin surface revokes
it. Device permissions, identity disclosure and account access always prompt.

Statement Store allocation uses the durable `StatementStoreAllowance { derivation_index }` decision: `None` is the
legacy allowance account; `Some(index)` is that exact product-account selector. Both approval and denial survive runtime
restart through the same `CoreAdmin` permission APIs. The product connection's storage is authoritative, including any
host-provided artifact namespace. A storage failure prevents provisioning. Chat, identity disclosure, other resources,
and ordinary signing retain their separate authorization contracts.

Implicit Statement Store provisioning ensures the current-period allowance without adding slots and reuses an authorized
durable decision without another grant prompt. An explicit `ResourceAllocation.request` instead requests additional
quota (`Increase`) and always requires per-operation confirmation. Its initial approval can establish a missing durable
grant in that same review; cancelling an increase never revokes a previously authorized grant. Denied grants, storage
failures, session changes, and changed administrative decisions block allocation.

Revocation blocks subsequent provisioning but does not withdraw already issued on-chain quota. Product grants no longer
create background renewal promises: the signing host's global storage cannot resolve an artifact-scoped decision. Old
product-derived renewal entries are pruned; wallet and paired-device renewal remain enabled. Next-period provisioning
happens on demand through the product's scoped runtime. A remote signing host retains its independent per-operation
confirmation; the product grant does not silently authorize another host.

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
        +-- NotDetermined / missing ---+   (remote + trusted label: allow,
                                       |    see below)
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

A remote permission resolves in this order:

1. `Remote { domains: [] }` is `Denied`. An empty bundle grants nothing, so failing closed outranks the whitelist.
2. A stored decision wins — the exact slot for a non-domain variant, or the most specific matching
   `remote_domain_candidates` entry for a domain.
3. Nothing stored and the product's label is trusted: `Authorized`, with no prompt and no write.
4. Nothing stored and the label is untrusted: `NotDetermined`, so the lookup prompts and persists the answer.

Because a trusted product's grant is never written, revoking its domain access means writing `Denied` for the `*`
pattern; denying a single host leaves every other host granted.

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

A device permission also carries the host application's OS gate. Both the request path and the `CoreAdmin` status read
resolve it through the host's `PermissionStatusHost`, so an OS refusal reads as `Denied` whatever is stored, and the
stored product decision is never overwritten by it. Remote, identity-disclosure and account-access decisions have no OS
gate.

The embedder builds a role handle, `PairingHostRuntime::new(...)` or `SigningHostRuntime::new(...)`, then calls
`product_runtime(product, sink)` for each product connection. Role-specific operations live only on the matching handle:
`cancel_pairing`, `notify_session_store_changed`, `activate_stored_session`, `activate_external_session`, and
`reset_session_state` on the pairing handle, `activate_local_session` on the signing handle. Both handles expose
`clear_product_state` to revoke one product's capability material without touching the session or other products.
Calling the wrong operation is a compile error, not a runtime `Unavailable`.

`SigningHostConfig.network_suffix` is the network's bare dotNS TLD (`dot`, `paseo`, or `testnet`). The shell supplies it
alongside the chain genesis hashes from the same network configuration used by wallet onboarding. It must match the
People chain's `NetworkSuffix.NetworkSuffix`: reserved identities derive under `uid.<suffix>` and `peopl.<suffix>`,
while the chain uses that suffix for proof contexts. Configuration keeps local activation and key derivation available
offline. The core validates supported suffixes but does not automatically check that the configured suffix matches the
chain.

`SigningHostConfig.coinage_instance_id` is trusted host/network configuration for instance-scoped Coinage runtimes. The
constructor leaves it `None` for legacy ABI compatibility; an embedder sets `Some(instance_id)` before creating the
signing runtime. Instance-scoped Coinage operations fail closed when it is missing. This is an asset instance, not a
purse derivation identifier, and is not exposed as guest-selected configuration. The encrypted wallet snapshot binds
this configured selection across restarts; changing it for the same root and genesis is rejected before inventory or
payment work. The trusted review identifies the selected asset alongside the chain, recipient, amount, and debit
ceiling. The asset instance does not change the current iOS MAIN_PURSE/page-0 derivations
(`//coinage//4294967295//0/<index>` for coins, `//coinage-ring-vrf//4294967295//0//<index>` for vouchers) or create a
separate allocator. Snapshot version 3 refuses legacy `//pps` snapshots without clearing them. Native iOS and Host
allocator state is not shared; same-wallet use requires reconciliation and one owner, not concurrent allocators.

The signing runtime persists initialized Chat products in the wallet/network-owned `CoreStorageKey::NativeChatProducts`
slot (index 16). Unlock restores their existing devices and background subscriptions only when the current
`ChatAuthority` and `StatementSubmit` grants remain authorized; it never prompts or generates a replacement for missing
device state. `clear_product_state` forgets this product from reception without deleting wallet custody or received
history. This is in-process restoration, not an OS background scheduler.

Native `native_describe_core_storage_key` and WASM `describeCoreStorageKey` let embedders route permission slots to the
same verified-artifact namespace used by their product execution. Root callbacks used by restored receivers must resolve
that current namespace; copying grants into a broader wallet namespace would defeat artifact revocation. WASM role
handles accept optional execution-local raw platform callbacks as the third `productRuntime` argument while retaining
one shared authority and wallet allocator.

A host retaining another main-purse allocator must reject `MainPurseCoinage` storage access explicitly rather than
return an absent value and open a second purse. This refuses incoming claims as well as outgoing spending. An
identity-only wallet loader is not another allocator; a single Host runtime may own the purse while that loader retains
responsibility for secure unlock.

### The two roles

Both implement the role-neutral **`ProductAuthority`** trait; each owns its role-specific lifecycle, so no method exists
on a role that can't mean it:

- **`PairingHost`** (seedless): the user's keys live in an external wallet, so signing/aliases/entropy relay over an
  encrypted SSO channel (statement store on the People chain; the channel lives in `pairing_host/sso_channel.rs`). The
  v2 wire protocol uses raw X25519 keys, HKDF-SHA256, and ChaCha20-Poly1305. It owns pairing/login state, persisted
  auth-session reload, and remote signing-host liveness monitoring.
- **`SigningHost`** (wallet-local): signs on device from local BIP-39 entropy, no pairing flow.
  `signing_host/local_activation.rs` establishes a session from host-held secret material. Its public identity is the
  RFC-0022 `uid.<tld>` index-0 product account of the configured network. RFC-0024 ring-VRF keys are explicit,
  product-owned registry entries; aliases, proofs, direct signatures, and internal personhood flows use the requested or
  user-selected registered key without a compiled-in fallback. It resolves RFC-0004 `RingLocation` values against the
  chain's `Members` pallet and pins membership, ring pages, exponent, and revision reads to one finalized block before
  creating a proof. Extrinsic-payload signing and v4 transaction construction work from pre-encoded payload fields, so
  no chain metadata is needed; statement-store and Bulletin allowance allocation are native-only (wasm builds report
  them as unavailable) and do need metadata, which they take from the `RuntimeServices`-owned per-chain cache rather
  than re-reading it per call.

`host_logic` stays pure: the orchestrators above call into it for codecs, session/SSO crypto, key derivation, and
permission policy, while all I/O (statement-store RPC, storage, prompts, chain RPC) stays in the layers above.

`host_logic::worker::WorkerLedger` holds the reference count per product worker from the Worker Lifecycle RFC, one per
host in `RuntimeServices`. Both bindings expose it as `acquire_worker` and `release_worker`. Every `Start` or `Stop` the
counts produce is reported on one channel, the host's `worker_demand_changed` callback, in the order the ledger produced
it and one at a time, whether the host asked for the transition or the core took the reference itself for an open
render. The host runs the executable and the core keeps the count; on the web `@parity/truapi-host` pushes the wanted
level to the page.

### Inter-host SSO

`PairingHost::call(request)` sends typed requests to [`SigningHostSsoService`](src/runtime/signing_host/sso_service.rs).
Handlers own consent and business logic; `sso_responder.rs` owns the transport loop and shared allowance helpers.
Resource consent is bound to the request's signing session: account changes, disconnects, and reactivation invalidate
pending approval before allocation or key return. Allocation failure details stay in local transcripts. Allocation
requests use the canonical `truapi::latest::AllocatableResource` type. Signing uses canonical request and result types.
Product-scoped VRF requests use `ProductRequest<P>` to attach the caller to a canonical payload. Both product and SSO
signing encode `with_signed_transaction` with the one-byte `OptionBool` codec.

The `host_logic::sso::messages::v1::RemoteMessage` enum owns the SCALE wire contract. Its response variants wrap named
result payloads in `Response<P>`, which carries `responding_to` once. Macros generate request/response pairing and
dispatch; see the [macro guide](../truapi-macros/README.md) for handler signatures and reply handling. A new operation
needs payload definitions, wire variants, a handler, and a typed client call.

Rust consumers must update renamed SSO types and helpers even when SCALE encoding is unchanged. Use
`RemoteMessage::request(message_id, request)` to construct requests. Decoded `SsoSessionStatement::RemoteMessages`
preserves message order; match variants directly or use the request's `SsoRequest::response_from_message`.

## Wire envelope

Every frame on the wire is encoded as:

```text
[requestId: SCALE str][trait: u8][method: u8][message_type: u8][payload bytes...]
```

The `(trait, method)` discriminant pair identifies the method via the auto-generated
[`crate::generated::wire_table::WIRE_TABLE`], and the `message_type` byte names which leg of that method's exchange the
frame carries (`Request`/`Response`, or a subscription's `Start`/`Receive`/`Interrupt`/`Stop`). The trait byte comes
from the trait-level `#[wire_trait(id = N)]` annotation; the method byte addresses a method within that trait, so method
ids restart at 0 in every trait. Each method's ids are exposed as a named const (`PREIMAGE_SUBMIT`, ...); both
`WIRE_TABLE` and the generated dispatcher reference those consts. Trait ids and per-trait method ordering are part of
the wire protocol; only ever append within a trait.

The payload bytes are the SCALE-encoded inner value, inlined without a length prefix. The pair is carried as
`Payload::trait_id` and `Payload::method_id` with the leg in `Payload::message_type`, and the dispatcher routes on the
pair via pair-keyed tables.
