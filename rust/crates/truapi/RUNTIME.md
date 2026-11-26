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
  (`HostRuntime<PairingHost>` or `HostRuntime<SigningHost>`) per host application, exposing
  shared [`RuntimeServices`] plus one [`ProductRuntime`] per product connection

## Architecture

Each product connection has its own `ProductRuntime`, `ProductRuntimeHost` and `ProductConnection`, which hold that product's adapters, permissions, action streams and its own chain connection, and a `ProductAccounts` that gives it the shared `HostAccounts` bound to that product. Everything else is shared per host: `HostAccounts` with its `AccountHolder`, the `HostSession`, and `RuntimeServices`. `host_logic` is a pure library both call, and the host's `Platform` implementation performs all I/O.

```text
Per product connection
┌─────────────────────────────────────────────────────────────┐
│ ProductRuntime: frames → Dispatcher                         │
│ ProductRuntimeHost<H>: validation and protocol adapters     │
│ ProductConnection: permissions, platform adapters, demand   │
│ ProductAccounts<H>: HostAccounts bound to this product      │
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
 WalletAccountHolder              RemoteAccountHolder
 wallet secrets and issuance      canonical calls ↔ SSO messages
         ▲                               │
         │                               ▼
 SsoAccountHolderService           SsoRequestService
 confirms peer requests           selected channel and transport

Shared RuntimeServices: platform, chain access and RPC clients
Host Platform: storage, prompts, chain transport and navigation
```

`HostAccounts` binds each call to the calling product and the current wallet activation, keeps the grants it was issued, and signs with delegated keys itself. It confirms signing, transaction, VRF and allocation requests with the user before the `AccountHolder` acts on them; allowances it allocates in the background need no confirmation. The `AccountHolder` owns wallet secrets and grant issuance and records ring-VRF registrations; `HostAccounts` records the provider the user selects, and `RingVrfRegistryStore` serializes both writes.

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

The embedder builds a role handle, `HostRuntime::<PairingHost>::new(...)` or
`HostRuntime::<SigningHost>::new(...)`, then calls `product_runtime(product, sink)` for
each product connection. Role-specific operations live only on the matching handle:
`cancel_pairing`, `notify_session_store_changed`, `activate_stored_session`,
`activate_external_session`, and `reset_session_state` on the pairing handle,
`activate_local_session` on the signing handle. Both handles expose
`clear_product_state` to revoke one product's capability material and remove
the values it wrote to product storage, through `ProductStorage::clear_product`,
without touching the session or other products, and `notify_contacts_changed` to drop
the contact handles the core cached once a contact is removed or blocked. Calling the wrong operation is
a compile error, not a runtime `Unavailable`. Stop a product's executions
before clearing its state, since a value written after the clear is kept.

A local session's usernames come from the dotNS contracts on Asset Hub, read
after activation. A name the host passes is shown until a record answers.

### Stack

Both host configs carry a `Stack`, the core's only chain configuration. Its
doc in `src/stack.rs` lists what it holds.

A native or browser shell puts three things in its runtime config: the stack's
pointer, the signed descriptor its build bundles, and the build's
[`ReservedValuesPolicy`](src/stack.rs), whose doc says what the core reserves
and which policies it refuses. The core resolves the `Stack` from them with
`StackRefresh::load`.

A Rust host that resolves its chains itself, such as the CLI, uses
`Stack::new`, and a test-host browser bundle passes `runtimeConfig.testStack`.
The core does not refresh a stack built either of those ways. Each constructor
documents what it refuses.

Reserved identities derive under `peopl.<tld>`, and the People
chain scopes proof contexts with `NetworkSuffix.NetworkSuffix`, so the two have
to be equal. Taking the TLD from the stack keeps local activation and key
derivation working offline. The core reads the chain's suffix later and
reports a difference, as the next section describes.

#### Checks against the chains

When an operator restarts a stack's chains from genesis, the same endpoints
serve new chains with new genesis hashes. A build that still runs the old
descriptor would read them as the old chains. So the core asks every chain
connection for its genesis hash before anything else uses the connection. This
covers the core's own connections, each product's, and the statement
subscriptions on People.

- If the hash is the one the connection was opened for, the core hands the
  connection out.
- If the hash is different, the call fails with a host failure. For a chain of
  the stack, `StackStatus::mismatches` lists it until a connection reaches the
  chain the descriptor names. Without delaying that failure, the core also
  asks the connections it handed out earlier for that chain again, apart from
  the statement subscriptions, because a host may have redialled underneath
  them. It closes those that give a different hash or say the chain is halted,
  as dotli does while it redials. Those that give the right hash, fail another
  way, or say nothing for 30 seconds stay open, since one endpoint can front
  several nodes.
- If the chain cannot be reached, or says nothing for 30 seconds, the call fails
  as unavailable and the next connection asks again. A chain that dotli reports
  as halted is asked again for up to 47 seconds, as a halted follow is, and is
  unavailable after that. Any other error fails the call with a host failure.
  None of these is a mismatch.

While a chain is listed, the core fetches the descriptor every five minutes
instead of every hour, since the operator publishes the new genesis hashes with
the restart. Once the core holds a descriptor that changes a chain, it sets
`restart_required` and fetches hourly again.

A host learns of a mismatch through its stack callbacks, and can read it from
`stack_status`. truapi-host's `/web` and `/node` hosts run the core in a worker
(`worker-runtime.ts`), which builds the pairing runtime without the stack
callbacks and exposes no `stackStatus()` to the host, so a host on them sees a
refused chain only as failing calls. A `WasmProductRuntime` built on its own
has no status either. A browser host that passes `stackChanged` without
`fetchStackDescriptor` still gets the reports, and nothing is fetched for it.
Without the callbacks the chain is still refused, and nothing is fetched. A host
shows a listed chain as out of service and keeps its own features off it. A
`StackMismatch::NetworkSuffix` entry, described below, names no chain to keep
off: the core keeps using People, so a host keeps People in service as well and
shows the entry as a fault in the stack. The operator corrects it by setting
People's suffix to the `tld`. A corrected descriptor on another TLD does not
help a running build, and the entry stays listed. A host that fetches the
descriptor sees that copy refused in `last_refresh`. Only a new host build that
bundles that descriptor runs on its TLD.

The check has three limits:

- It runs only when the core opens a connection. If a host redials underneath
  one, as dotli does, the core notices when it opens the next connection for
  that chain, in any scope.
- A light client answers from its chain spec, so a chain that restarted is out
  of its reach and never listed. Only the hourly fetch brings it the new
  descriptor.
- A `Stack` built any other way, such as with `Stack::new` or from `testStack`,
  is not checked.

The core also checks People's `NetworkSuffix.NetworkSuffix` against the
descriptor's `tld`. It makes no request for this. A signing host already reads
People's suffix when it builds statement store and Bulletin allowance proofs,
and when it renews a statement store allowance, and the core compares that
value. Only People's suffix is compared: a PGAS claim reads Asset Hub's
`NetworkSuffix`, which the core does not check. If People's suffix differs,
`StackStatus::mismatches` lists it until a later read gives the `tld` or the
runtime ends. The core keeps the suffix in memory only, so a runtime built
again, after a relaunch or after `restart_required`, lists no suffix until its
own allowance work reads one. A suffix that is not listed may not have been
read yet.

This is only a report. The core keeps using People and does not fetch the
descriptor any sooner. It matters because the core derives its identities under
the `tld` but builds proof contexts with People's suffix. If the two differ, an
identity derived under one does not match the other.

A failed read changes nothing, and neither does a suffix that is not one to
sixteen bytes long: a suffix listed before stays listed, and the allowance work
fails with that error. A pairing host does no allowance work, so it never lists
a suffix.

#### Descriptor refresh

The core starts from the bundled descriptor or the last one it verified,
whichever has the higher `sequence`. The bundled descriptor has to be one the
core can run under the build's policy even when the stored one is newer, so a
build that refuses its own bundle fails on every device, not only on fresh
installs. It never waits on the network to start,
and waits a few seconds at most for the stored copy before it starts from the
bundled one.

The core has no HTTP client. A native host installs `NativeStackCallbacks` with
`set_stack_callbacks` to make the fetch and receive the stack's `StackStatus`,
and the core keeps the last descriptor it verified in its database. A browser
host passes the `fetchStackDescriptor` and `stackChanged` raw callbacks, and the
core keeps the descriptor in core storage under `CoreStorageKey::StackRecord`.
Without the callbacks the core stays on the descriptor it started from.

The core fetches once the callbacks are installed, unless the last fetch was
under an hour ago, then once an hour and whenever the host calls
`refresh_stack`. A sleeping device can stretch that hour, so a host calls it
when its app returns to the foreground. `StackFetchRequest` says how to make
the request.

`trinity-stack`'s `StackReader` decides whether to take a fetched descriptor:
a failed fetch, a refused descriptor or a lower `sequence` keeps the current
one. The core also refuses a descriptor it could not start from, such as one
without a People chain, and one on another TLD than the bundled copy, since
accounts derive under the TLD. A stored descriptor it would refuse is ignored
until the next one the core takes replaces it, whatever its `sequence`. After
a refused or failed fetch, `last_refresh` reports that fetch even if writing
the record fails.

On a stack whose build allows it reserved values the core pins more of the
bundled copy: the set of chain roles and each role's genesis hash, `assets`,
`contracts`, `dotns`, `coinage`, and the `sha256` of the relay's `lightSpec`,
which carries the relay's sync checkpoint. A descriptor that changes one is
refused the same way. A fresher relay checkpoint, and a fork made again from a
later block, change the relay's `lightSpec` and so need a new build. So does a
wipe: a stack whose build allows it reserved values, such as pcf-devnet in
dotli, cannot follow a wipe of its chains, because the new genesis hashes are
pinned. The core refuses the new descriptor and stays on the wiped chains until
a host build that bundles the new descriptor replaces the running one.

The pins fix what the descriptor names for those fields, so a stolen
descriptor key cannot name other genesis hashes, assets or contracts for such a
stack.
They do not cover what a host reads from the chains. The descriptor's
endpoints are not pinned, and the genesis hash the core checks is the one an
endpoint reports, so those reads are protected only by a light client that
starts from the pinned relay light spec, and that protects the relay alone: a
parachain's `paraId` and `lightSpec` are not pinned. A host that reads over RPC
has no such protection.

A newer descriptor the core takes that changes the genesis hash of the relay,
Asset Hub, People or Bulletin chain, the network, the `generation` or the
`accountDataStore` contract applies from the next start. Once the core has
stored it, `StackStatus` has `restart_required` set, and the host builds the
runtime again. Any other change needs no restart. `StackStatus` carries it
once the core has stored the descriptor or its store has failed, at most 30
seconds after the fetch, and shows the copy from before the fetch until then.
It documents the rest, including the `generation` a host keys cached chain
data by.

### Core database

Native signing hosts (iOS, Android, the CLI) give the core a directory for its
own SQLite database (`store` module, bundled SQLite through `rusqlite` and
`async-sqlite`). The runtime opens `core.sqlite3` there at startup, so a
missing or unwritable directory stops it from starting. Keep the directory out
of device backups: it holds durable-transaction state that must not be restored
onto another device. The required `HostRuntimeConfig.database_directory` sets
it on iOS and Android, and `HostRuntime::<SigningHost>::set_core_db` on any other
embedder. Such an embedder also sets the `InstallationIdProvider` whose id NFT
purse item keys are derived under with `set_installation_provider`, and the
`EncryptedStorage` that keeps NFT receive key and deposited item secrets with
`set_encrypted_storage`, then calls `start_core_services` once, which starts the durable engine and NFT purses over
the database. `core_database_status()` reports the SQLite version, schema
version and path. Web hosts do not compile the store.

The database also holds the Pocket collection (`pocket_cards` module): the
cards in the Pocket and the newest face of each side. The host lists the cards it
pins in `HostRuntimeConfig.pinned_pocket_cards`, adds the cards the user
approves with `add_pocket_card`, and follows the collection with
`observe_pocket_cards`. A card no longer pinned stays only if the user added
it. The core keeps the newest face of a card from every
`RenderContext::PocketCard` render, and of its back from every `PocketCardBack`
render, at most every two seconds and once more when the render stops, so a
card has both sides offline and at cold start. `pocket_card_face` reads a side.
A product connection's renderer actions leave out a press on a back that
connection has not drawn: a product whose SDK predates `PocketCardBack` cannot
decode one and would lose its whole action stream.

It also keeps materials (`materials` module), the text behind each material
ID a renderer tree names. The core reads a material it lacks from Bulletin
itself, with `bitswap_v1_get` over the Bulletin connection it holds for
preimage submission, not through the host's preimage lookup. Only text that
hashes to the ID and passes format version 1, as Shaderlab's reference parser
reads it, is kept. A node that answers with bytes that do not hash to the ID
ends the fetch, and the material falls back as `Integrity`. A material a saved
Pocket face draws is never
evicted, and the others share 4 MiB, oldest out first. At start-up the core
fetches what the saved faces draw and the database lacks. From a material's
text the core builds the shaders an app compiles: the GLSL ES 3.00 that
Shaderlab's own compile step writes, for OpenGL ES, and naga's Metal
translation of it, with where each uniform and texture goes.

A host that draws materials (`materials_enabled` in its startup config) asks
the core to resolve a tree's materials: for each `Materials` node, depth first,
whether it is loading, ready with its built layers, or drawn plain and why.
The resolution follows the shared fetch, checks the tree's limits and each layer's
arguments, builds the shaders for the platform (Metal on iOS, GLSL ES
elsewhere), and changes as materials arrive or fail and as the app reports a
shader it could not compile or an image it could not use. A node that falls
back is logged on the device, and when the resolution runs on a product's execution,
reported to that product through `Renderer::failure_subscribe`, once. The
capability answer to the materials question is yes only with materials on, for
the format version and features the core reads.

Secrets the database must not hold go to the host's `EncryptedStorage`: the
Keychain on iOS, encrypted preferences on Android, a plaintext file in the CLI.
It reads, writes and deletes values by key and cannot list them, so a feature
keeps its public rows in the database and stores each secret under a key
derived from its row. Native hosts install it from their callbacks; any other
embedder calls `HostRuntime::<SigningHost>::set_encrypted_storage`. Web hosts do not
compile it.

### Durable transactions

The `durable` module runs over the core database. A domain registers presigned
mortal extrinsics (`DurableRequest::presigned`, then `execute` or
`execute_with`, whose hook writes the domain's own rows in the same
transaction); the engine broadcasts them once committed and follows each with
a submission watch. Whatever no watch owns, including everything a previous
process left live, is decided by recovery passes: the completion ladder over
the shared chain capabilities and the domain's `CompletionOracle`, which a
pass asks once per domain about its undecided rows. Every
verdict is a compare-and-set against the status and attempt it was derived
from, and rows are never deleted.

Each domain is registered in the `DurableRegistry` with the genesis hash of
its chain, its `CompletionOracle` and its one `SubmissionPolicy`. A domain can
also defer transactions it cannot sign yet (`DurableRequest::deferred`),
each with a `SubmissionPlan`: the `PolicyParams` its domain's policy builds it
from. They wait as `PENDING_SUBMISSION` rows until the
submission executor builds them: one task per domain and group asks the
policy to `prepare` its waiting rows, starts an attempt for each `Ready` answer
and fails each `GiveUp`, backing off after rounds that decide nothing. A
presigned submission may carry a plan as well. When a verdict proves a
transaction with a plan failed, the verdict writer asks the domain policy's
`can_retry`, and a yes sends the row back to be built instead of failing it.
It is built again after a cooldown that grows with the attempts the row
records, so the wait survives a restart. The executor starts with the engine in `start_core_services` and again in every
recovery run.

The engine reports whether work is pending, including rows waiting to be
built, to `HostCallbacks::durable_work_changed`; the host then runs a
background task awaiting `run_durable_recovery()`, which returns once nothing
is live.

### The two roles

- **`PairingHost`** (seedless): runs `HostAccounts<RemoteAccountHolder>`, which keeps the paired session's grants, and owns logout. `SsoRequestService` owns pairing and login state, persisted session reload and remote signing-host liveness, and carries account operations to the phone over the encrypted SSO channel (People-chain statement store; v2 wire protocol with raw X25519 keys, HKDF-SHA256 and ChaCha20-Poly1305). It reports session changes to the grant store `HostAccounts` keeps its grants in through `PairedSessionOwner`, holding back grant writes until each change and the cleanup it leaves are done. The paired host confirms a signing, transaction, VRF or allocation request before forwarding it, and the phone confirms it again.
- **`SigningHost`** (wallet-local): runs `HostAccounts<WalletAccountHolder>`. `WalletAccountHolder` owns local entropy, derivation, grant issuance and renewal, and asks for account access before a product uses another product's ring-VRF keys; `SigningHost` routes activation, lock and product reset through `HostAccounts`. It signs on device from local BIP-39 entropy,
  no pairing flow. `signing_host/local_activation.rs` establishes a session
  from host-held secret material. Its public identity is the RFC-0028
  `peopl.<tld>` index-0 product account of the configured network. RFC-0024 ring-VRF keys are explicit,
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
- `WalletAccountHolder`: owns the active wallet on a signing host. Only its `with_keys` reads the root entropy, after checking that the call's session is still the installed wallet activation, so a lock or a wallet switch fails signing, issuance and pairing answers that began under the previous wallet.
- `ProductConsent`: one per runtime. It holds account-access decisions and review rules, and runs account-access prompts one at a time for local product calls and paired requests alike. A paired caller never skips a review. A signature with another product's account is confirmed once per kind and owning product for the rest of the product execution.
- `HostGrantStore`: holds the keys and public subtrees issued to `HostAccounts`, per wallet on a signing host and per pairing on a paired host. A signing host keeps them in memory for one wallet activation. A product reset revokes that product's grants and fails grant work that began before it, without ending the wallet session.

`host_logic` stays pure: the orchestrators above call into it for codecs,
session/SSO crypto, key derivation, and permission policy, while all I/O
(statement-store RPC, storage, prompts, chain RPC) stays in the layers above.

`host_logic::worker::WorkerLedger` holds the reference count per product
worker from the Worker Lifecycle RFC, one per host in `RuntimeServices`. Both
bindings expose it as `claim_worker`, one reference per claim, and the host
reports a worker that stopped without being asked, because it crashed, failed
to boot or the system killed it, with `worker_exited`: while references remain
the ledger requests `Start` again, and the core hands the restarted worker its
funding sessions once it connects. Exits less than a minute apart form a run:
the first restart is immediate, then each waits 1 s, doubling up to 5 min, and
a restart still waiting is dropped once nothing holds the worker or it was
started meanwhile. On the run's third crash the host hears
`worker_crashing(product_id, crashes)` and tells the user that product is not
working. Every `Start` or
`Stop` the counts produce is requested through the host's
`request_worker_transition` callback, in the order the ledger produced it and one
at a time, whether the host asked for the transition or the core took the
reference itself for an open render. The host runs the executable and the core
keeps the count; on the web `@parity/truapi-host` pushes the wanted level to
the page.

### Inter-host SSO

On the signing host, [`SsoResponderService`](src/runtime/sso_responder_service.rs) owns the transport: it answers pairing, runs the serve loop, drops duplicate requests, and gives each peer its own [`SsoAccountHolderService`](src/runtime/sso_account_holder_service.rs). That service is bound to the wallet activation that authenticated the peer's channel and calls `WalletAccountHolder` directly; once that activation ends it answers only not-connected. A native wallet with its own transport opens the same per-peer service from the native runtime.

On the paired host, `SsoRequestService::call(request)` sends typed requests to the peer's `SsoAccountHolderService`. That service confirms each signing, transaction, VRF and allocation request with the user before `WalletAccountHolder` acts on it, and approval is bound to the request's signing session, so account changes, disconnects and reactivation invalidate a pending one. Allocation failure details stay in local transcripts.
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
once through `HostRuntime::<SigningHost>::set_device_pairing_observer`, and on a native
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
`ExecutableKind` are defined here instead, and codegen emits their host
codecs from these definitions. Both are SCALE-encodable so they can cross the
wasm callback boundary, where every parameter is encoded with
`parity-scale-codec`; `ProductContext` holds a `ProductId`, which decodes by
parsing, so a context off the wire carries a normalized product id.

### Product Identity

Parsing a `ProductId` (its `FromStr` implementation) is the single chokepoint
that normalizes a host- or wire-supplied product id. Nothing else builds a
`ProductId`, so holding one means the id is normalized. Derivation, product
storage and permission scopes are keyed by a normalized id the stack serves. Code that reads
an id uses its methods, such as `bare_label` and `has_trusted_remote_permissions`,
and `CoreStorageKey::product_manifest` keys its manifest cache entry.

Parsing checks syntax only, so any DNS label is a TLD: a wire frame or a stored
record is decoded without a stack. The stack decides which ids it serves:
localhost ids, and names under its own TLD (`Stack::serves`).

A name can also be written under `dot` on any stack. On a stack with another
TLD it names the stack's own product: the stack's dotNS registry resolves it,
so `game.dot` on a `paseo` host is the product `game.paseo`, with that
product's keys, entropy, storage and permissions (`Stack::served_id`). A name
under any other TLD names no product of the stack. A stack whose own TLD is
`dot` serves `dot` ids as they are, and only a build that allows Polkadot's
reserved values can run one.

The core reads a name this way where a product or a person writes it: a call
that names a product (answered `MalformedFrame` when the stack does not serve
it), every product a paired host's request names, and a link, share host or
Pocket deeplink read through a `Stack` (`parse_navigate`,
`parse_product_share_host`). `runtime::served` describes how a served value
reaches the account calls. `parse_product_link` and `parse_dev_server` do not
take a stack and answer the id as written.

A host and the core hold one id for a product. `ProductContext::served_by`
opens a product only under an id the stack serves and refuses a `dot` name on
another stack, naming the id to open instead. A host that is given a name asks
for the id first: a wasm pairing or signing host runtime's `parseProductId`, or
`Stack::served_id` in Rust, which the CLI applies to `--product-id`. A native
host gets served ids from `parse_navigate` and `parse_product_share_host` and
builds any other id under the stack's TLD. Every other id a host passes is
taken as written, and a worker claim under an id the stack does not serve
holds nothing. The subtree lookup, `HostAccounts::derive_entropy`, every key the
signing wallet derives for a product other than its contact handle key, which
only a connection's own product reaches, the manifest readers and worker
references refuse an id the stack does not serve.

`HostRuntime` and the wasm runtimes administer stored state by the id as
written. `clear_product_state` clears the grants and the product storage kept
under exactly that id, served or not, so a host can drop them for an id it no
longer runs, such as a `dot` id an earlier release ran on a `paseo` stack.
Permission statuses, ring-VRF registrations, Pocket cards and chat rooms kept
under such an id stay in the store, where no product reads them.
`permission_authorization_status` and its setter read and change the statuses
of an id the stack serves and of an id it serves nothing under. They refuse a
`dot` name on a stack with another TLD, as the product a status belongs to or
as a product the request names: the stack keeps that product's statuses under
its own id, and a status stored under the name would be one no product reads. A native host can clear with `clear_product_state`, but it
reads and sets stored statuses only through a `NativeProductExecution`, which
opens only for a served product.

`REMOTE_PERMISSION_TRUSTED_LABELS` lists the trusted product labels. A label
counts only under an id the stack serves. These products bypass recorded permissions and
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
  card from it. Native hosts do not implement it: the core serves it from its
  own database (see Core database). The CLI host passes an in-memory one.
- `FundingPlatform`: show the native funding overlay for a session, with its
  direction and amount and the product that opened it, and report whether the
  user started or dismissed it. Installed with `set_funding_platform`; the
  Balance card opens sessions with `open_funding`. The core owns the sessions,
  persists them, expires them, and answers Funding calls `Unsupported` while no
  overlay is installed.
  Native hosts reach this through `NativeTrUApiHostRuntime`:
  `set_funding_callbacks` (the overlay), `open_funding`,
  `set_funding_providers`, `get_funding_quote`, `select_funding_options`, `select_funding_provider`, `cancel_funding`,
  `confirm_funding_cancel` and `acknowledge_funding_session`. Each of these
  reports something the user did or the host owns. The core tells the host
  what changed with `funding_changed(intent, kind)` (`Session`, `Options`,
  `Quotes` or `Providers`), and the host reads it back:
  `funding_session`, `funding_progress`, `funding_options`, `funding_quotes`,
  `funding_entries`, `funding_summary` and `funding_history`. A Rust host
  installs a `FundingFeed` with `set_funding_feed` before its funding
  platform and gets the same notices. Whenever whether a provider still works
  on a session changes, and once each time funding resumes, the native
  runtime writes a launch marker that `funding_resume_pending` reads at the
  next launch. Amounts
  cross the FFI as decimal strings.
  Sessions are stored in the core database, one row each; a row the core
  cannot read is skipped and left in place.
  The core owns the funding history. A session is stored only once the user
  starts it in the overlay. When funding resumes, every session in flight
  and every ended one the host has not acknowledged is announced again. An
  ended session is announced each time funding resumes until the host calls `acknowledge_funding_session` once it
  has told the user how it ended. A released withdrawal whose
  provider still owes its payout outcome, or the refund of a failed payout,
  is kept and served whether or not the host acknowledged it, and is handed
  over again when the provider reports; one refunded after an accepted
  cancel stays in full until its payout outcome is in. A cancelled top-up a late payment may
  still open stays in full for its window too. Every other ended session is
  compacted into the session pack once it ends, acknowledged or not (intent,
  owner, direction, amounts, outcome and times, the provider, rail and asset,
  the row's status, the payout, the refund and the provider's references),
  which it never trims, so a product watching a compacted session still gets
  its terminal item. The pack keeps whether the host acknowledged each
  session: the host is handed the session in full with the change that ended
  it, and the 200 newest it has not acknowledged come through again, as
  summaries, when funding resumes.
  `funding_history()` returns every ended session, held in full or
  compacted, as a summary newest first. A product opens a session with
  `request({ direction, amount? })`, which answers at once with its `intent`,
  and watches it with `statusSubscribe({ intent })`; another product's
  session reads as `NotFound`. The overlay runs on a task of its own, so no
  call waits on it; a session the user dismisses ends its watchers as
  `Failed { Cancelled }`. Sessions and the pack live in the core database,
  one versioned row per session: a row that does not decode or is of an older
  format is skipped and left in place, and one in a newer format is left
  untouched while funding answers a host failure. Funding
  needs the core database: natively the configured `database_directory`,
  otherwise `set_core_db`. Products see a session in flight as
  `InProgress`.
  The host lists its providers by their published names with
  `set_funding_provider_config`; a Rust host can instead hand
  `set_funding_providers` the Worker manifest it ships for each. Before the
  overlay shows, the core announces the session's options, which `funding_options(intent)` works out
  as it is read: `candidates` are the
  providers serving that session's direction, with their routes, from what the
  `includes.funding` configuration of each provider's Worker manifest declares:
  dotNS's answer once the core has read it (cached for a day, re-checked in the
  background on every query), the shipped snapshot until then, so the list
  renders with no chain read. A provider whose manifest no longer serves
  Funding drops out.
  `get_funding_quote(intent, amount)` prices `amount` on the rail, asset,
  network and country the last `select_funding_options` named (card and bank
  take the resolved fiat asset), returning `None` while there is nothing to
  price; asking what the running request prices keeps it. It asks every
  funding provider whose routes carry the rail, or that declares none (a
  route its answers confirmed does not narrow one that declares none): the core
  calls each provider's worker with `quote`, holding the worker while it
  waits, and the worker answers priced from its own API. Every provider's
  worker is held from the moment the overlay opens until it closes, so it
  is up before the first quote; a provider handed the session keeps its
  worker after that. A provider under a TLD the stack does not serve has no
  worker to hold, so none is started for it and a quote asked of it times
  out. A quote already asked keeps its provider's worker
  until it is answered or times out, overlay closed or not, unless the
  session is handed to another provider: then every quote still waiting
  ends at once and lets its worker go. The manifest names no backend: a provider signs in to its own API
  itself and keeps its tokens with the host.
  Each provider's row is `Pending`, then `Quoted` or `Unavailable`, and a
  provider that does not answer within 10 seconds is unavailable. A provider's
  answer is reused for the same ask for 30 seconds. What an answer shows (a
  quote or an out-of-range amount: served; a refused country: not served
  there, only for an ask that named the country) is stored under
  `FundingSupport` and folded into the candidates for 12 hours, as
  routes the manifest did not declare and as `unsupported` entries the host
  can filter by the user's country, while the provider's current manifest
  serves Funding. A crypto ask names the `network` it moves on, one a route's
  `networks` declares, and the networks quotes confirmed join the candidate's
  routes. A refusal of only the amount is kept as the candidate's `limits`
  (minimum and maximum per rail, asset, network and country), each for 12
  hours from its own refusal: a later quote does not renew it, and a quote at
  an amount it rules out clears it. The host reports the user's rail,
  asset, network and country with `select_funding_options(intent,
  selection)` whenever one changes, and the options pushed back carry
  `limits`, which combine them across every provider into the
  range the amount screen enforces: the smallest minimum and the largest
  maximum, with no bound while any provider has not shown one. For a crypto selection, the options' `networks`
  give the same range on each network a candidate's route declares, for
  the network picker: for the picked asset, or the widest across the
  network's assets before one is picked. The host's
  own product floor applies on top. A quote carries its asset's
  `asset_decimals`, so the host shows asset amounts without knowing the
  asset.
  An ask that names no country takes the one the host's
  `detected_country` answers (coarse location if already permitted, then
  SIM or network country, then locale region, without any permission
  prompt), kept only when it is an ISO 3166-1 alpha-2 code and reused for an
  hour, or asked again after a minute when it found none; the user's own
  choice is passed as the ask's country. The options' `countries` say what
  the core knows about each country for the selection's rail and asset, for
  the host's picker: `Supported` with the lowest
  minimum a provider showed there, from manifest routes and recent answers,
  or `Unsupported` when every answer naming it refused it; a country nothing
  names is left out.
  Natively `funding_changed` announces each list with `Quotes`, and
  `funding_quotes` reads it as a `FundingQuoteList`, only ever the current
  request's: the `request_id` `get_funding_quote` returned, whether it is
  `complete` (no row pending), the rows in the host's provider order,
  `best` (the least to pay inbound, the most to receive outbound, the earliest row on a tie, among providers the
  session can be handed to), which the host selects until the user picks
  another, and `refresh_at_ms`. The core asks every provider again each
  minute, each row keeping its answer until the provider answers again,
  until the session is assigned, ends or leaves the overlay, the host asks
  for another ask, or `select_funding_options` names another rail, asset,
  network or country than the ask, which also clears the list
  (`funding_quotes` answers `None`, announced with `Quotes`); from then on
  no list of that request arrives, mid round too, and the quotes still
  asked end with the workers they held. The
  options are sent again after each list, since a refusal can teach the core
  new limits. At most eight providers are asked at once, each with its own
  deadline. A session can be handed only a quote from the list its host was
  last shown, so a quote never crosses sessions. A quote that does not price
  the ask's amount, moves nothing, charges more in fees than the user sends,
  has an unusable id or has expired shows as the provider being unable to
  serve the ask.
  `start_funding_when_quoted(intent, request_id)` does that for the user
  who continues before every row answered: once every row of that request
  answered, the core selects `best` as `select_funding_provider` would, at
  once when they already have. A new ask, `select_funding_options`,
  `cancel_funding_auto_start` (the user going back from the quotes),
  `cancel_funding` or a list with
  no usable quote disarms it. `funding_available` says whether the host
  offers any provider, announced with `Providers` when
  `set_funding_provider_config` changes the list.
  `select_funding_provider` hands a session to the provider the user chose,
  on the quote they chose it by, which must be one the core offered for the
  session, not yet expired and, when the session has an amount, pricing that
  amount in balance units (what it credits inbound, what it debits outbound);
  a session opened without an amount takes the quote's. The provider
  receives it in `assign`, and the
  session keeps it with the rail and asset it priced. A session's offered
  quotes are dropped once it is assigned or ends.
  `funding_progress(intent)` gives the session's progress: the milestones it draws, each with when it was reached: in, `Started`, `Received` (the
  provider saw the payment) and `Added`; out, `Started`, `Received` (released
  to the provider) and `Sent` (paid out). A milestone the provider skipped
  takes the time of the first later one. `band` is the line under them, what
  is happening now (`AwaitingPayment`, `CheckingPayment`, `OnItsWay`,
  `Adding`, `Added` in; `Converting`, `PreparingPayout`, `Sending`, `Sent`
  out; `Retrying`, `Delayed`, `Cancel` or `Ended` otherwise), and
  `last_heard_at_ms` when the provider last reported, for "Checked with the
  provider 12s ago". `detail` is the line under the band: the provider's
  latest `Progress` fact for the current step (confirmations, parts, a
  countdown, a bank's send, an attempt, or an untranslated `Other` text of at
  most 80 characters), cleared when the step moves on and never stored; each
  `Progress` also moves `last_heard_at_ms`. It also carries
  when the session failed, the latest transaction id and reference the
  provider reported with `Details`, the latest `Deposit` instructions for the
  host to draw (a crypto address with network, asset, amount, whether it is
  exact, an optional payment URI for the QR code and expiry, or bank details
  with the reference), and what arrived when a `PaymentReceived` reports a
  short or wrong-asset payment. `Deposit` is inbound only and allowed until
  funds move, so a provider can ask for the rest of a short payment.
  It is `retrying` once the provider names a second top-up after a partial
  claim. After an outbound session is released the provider reports
  `Converting` once as it prepares the payout and then
  `Payout { PaidOut | Failed }` once, and in between `PayoutRetrying` each
  time a payout attempt fails and it tries again; the outcome shows as the session's
  `payout`, beside the `Released` outcome, and either before the release is
  out of order. After a failed payout the provider gives the user's CASH
  back with a `payment.topUp`, recorded on that session (the provider's one
  such session whose debit matches the amount, otherwise each of them), and
  names it once in `Refunding { top_up_id, amount }`; the core follows its
  claims and keeps what they returned as `refunded`. Nothing expires a
  session waiting for its refund.
  The progress carries one `status` for the host to word: in flight by rail
  (`WaitingForTransfer`, `WaitingForPayment`, `WaitingForDeposit`, then
  `ConvertingToCash` and `AddingToBalance`; out, `TransactionInitiated`, then after release
  `ConvertingTo { asset }` on crypto or `PreparingPayout`, and `RetryingPayout`
  while the provider's latest word is that it retries the payout), `Retrying`, and
  `TakingLonger` once the provider has been silent for longer than the
  chosen quote's `eta_secs` (`longer_at_ms` says when, so the host reads the
  progress again then; `None` while a top-up waits on the user's payment,
  until money moved); ended, `ToppedUp`, `Sent`, `Refunded`,
  `PayoutFailed { refunded }` or `Failed { reason }`. An inbound payment
  the provider returned ends as `Failed { Refunded }`, one the user's bank or
  card issuer refused as `Failed { Declined }`. A top-up the user cancelled
  before paying stays open for 7 days to a payment that arrives anyway: when
  its provider starts a top-up for it, with no other session in flight
  taking it, the session opens again, is handed to the host again and
  settles as delivered.
  That provider's worker runs it through the `FundingProvider` trait. The
  core calls the worker with host-initiated calls the worker answers with one
  item: `assign` hands it a session, `cancel` asks it to stop one, and
  `settleCancel` passes on the user's answer to its offer. When a worker's
  connection completes its handshake, the core assigns it every session it
  still serves, a released withdrawal awaiting its payout included, with its
  last step, and repeats any cancel or decision it
  never answered; the worker keeps its own state in its storage. The worker
  calls the core: `report` stores each update on the session (only from the
  assigned provider, only forward; `Details` may come at any point while the
  session is open), `presentFrame` asks the host to
  show one of its screens through `present_provider_frame` (an App route only
  when the provider publishes an `app` subname on dotNS, read within ten
  seconds; otherwise `NoApp`). The
  core never shows it on its own: it holds the call, tells the host through
  `provider_frame_requested`, and presents it once the host calls
  `open_provider_frame`, at once while the user looks at the session or when
  the user taps a notice. One waits per session: a newer call replaces it
  (`Dismissed`), and `closeFrame`, the payment moving or the session ending
  drops it (`Closed`). `closeFrame`
  closes it through `close_provider_frame` once the provider is done with it,
  such as a card payment the user made: the open `presentFrame` answers
  `Closed` and the host shows the session sheet in its place. The core holds the
  provider's worker while a session is assigned and open, unless the stack does
  not serve the provider, and stores an
  assigned session from then on, even while its overlay is up. It records each
  top-up (inbound) or payment request (outbound) the worker starts while
  serving a session, and a `Crediting` or `Collecting` report must name one of
  those records at its amount, once. It ends a session as `Delivered` or
  `Released` itself, from the recorded amounts of the named top-ups once their
  claims are final (`Crediting`, then `Delivered`) or of the named payment once
  it completes (`Collecting`); a partial claim counts what the host claimed.
  Statuses are read again with backoff until the host decides, and a top-up
  or payment the host still does not know an hour after it was named counts
  as nothing moved. Money moved once the provider started funds for the
  session, named or not, or saw the user's payment; from then on only the
  provider ends the session, and it cannot fail it once funds started.
  Before that, an unassigned session expires 24 hours after it opened and an
  assigned one 7 days after its provider last reported. `cancel_funding` is
  the host's one cancel, whatever the session went through, and answers
  what it did: while the overlay is up it drops `present_funding`, which
  the host answers by taking the overlay down, and drops the session, or,
  when the user already chose a provider, keeps it as started and asks the
  provider to stop; it ends an unassigned session; before money
  moved it asks the provider and ends the session when asked again; after,
  it asks the provider, who answers `Refused` or `Offer`, one offer per
  cancel, and the host answers the offer with
  `confirm_funding_cancel(intent, terms)`, passing the fee and refund it
  showed (`None` declines), which the provider hears through `settleCancel`;
  terms other than the offer's answer `NoOffer`, an inbound session whose
  credit started answers `TooLate`, and an ended or unknown one `Refused`.
  A provider asked before money moved answers `Stopped`, and the core ends
  the session as cancelled. An overlay closed after the user chose a
  provider keeps the session as started and asks the provider to stop unless
  money moved; a cancel after that asks again.
- `BalancePlatform`: stream what a payment request can spend right now, the
  current value first and then each change. Installed with
  `set_balance_platform`; native hosts use `set_balance_callbacks` with
  `notify_balance`. The core requires a session and the product's
  `BalanceAccess` remote permission, asking for it on the first subscription
  and answering `PermissionDenied` when the user refuses. The balance is the
  user's, per purse: one `notify_balance` reaches every product following the
  purse, and the core checks each product's permission again before passing a
  balance on, so a revoked grant ends that product's stream with
  `PermissionDenied`. A host `InsufficientBalance` reaches a product without
  that permission as `Rejected`. Without a balance view, `balanceSubscribe`
  answers `Unsupported`.
- `PaymentPlatform`: pay from the user's balance to an account once the user
  approves, and stream each payment's status by its caller-chosen id.
  Installed with `set_payment_platform`; native hosts use
  `set_payment_callbacks`: the core asks the host to `follow_payment` an id
  and the host pushes each engine status with `notify_payment_status`. The
  core keeps the latest status per id, shows the pending one when the engine
  has not answered within 250 ms, keeps the last 512 final statuses, and
  ends a stream whose engine stopped early with the abandoned status. The
  core requires a session and hands the host each id hashed with the product,
  and offers only the main purse. Without it,
  `request` and `statusSubscribe` answer `Unsupported`.
- `TopUpPlatform`: claim a top-up source's funds into the user's balance and
  stream each top-up's status. Installed with `set_top_up_platform`; native
  hosts use `set_top_up_callbacks`, following ids through `follow_top_up` and
  `notify_top_up_status` by the same rules as payments. The core
  requires a session, checks the source keys, passes a `ProductAccount`
  source through for the host to derive, and hands the host each id hashed
  with the product. The host owns claiming, retries, partial claims and
  persistence. Without it, `topUp` and `topUpStatusSubscribe` answer
  `Unsupported`.
- `ExpandedCardHost`: show or hide the card face drawn above an opened card's
  Widget, and turn that card to its back or front. It is carried per product connection on `ConnectionAdapters`, so only
  the Widget under a card reaches that card.
- `ContactsPlatform`: resolve the handles a transaction names to contacts,
  render the picker that selects one, and list the dotNS usernames of the
  contacts the picker would offer. `contacts` is the only required method.
  `pick_contact` defaults to `Unsupported`, so a host serving no picker says so
  rather than looking like a user who declined, and `contact_usernames`
  defaults to `Unsupported` the same way. The host owns the UI, so no account
  on the list reaches the product. A product gets a handle for the selection,
  and the game product may also read the usernames. The core caches resolved
  handles. A host calls `notify_contacts_changed` on its runtime when a contact
  is removed or blocked.
  The core serves `contacts.usernames` only to the game product, `dim2` on
  every network, and answers `Unsupported` to any other product without
  calling the host. While nobody is signed in it answers a domain error
  with the reason `No active session`, also without calling the host. Each
  username is the bare dotNS label with no top-level domain, and the core
  drops any entry that is not one.
- `GamePlatform`: hold the game product's next-game reminder and drop it.
  The core serves Game only to `dim2`, on every network, and answers
  `Unsupported` to any other product without calling the host. A host
  keeps one reminder per product: a schedule replaces the reminder the same
  product already holds. The core asks for no per-product consent: the host
  asks the OS for what the reminder needs, rings an alarm where the OS allows
  one and delivers a notification otherwise, may add a calendar event, and
  keeps the reminder across app kill and reboot. A schedule the host cannot
  hold fails as a host failure carrying its reason.
- `ScannerPlatform`: open the host's QR and barcode viewfinder. Before calling
  it the core refuses an invalid request, a Worker the user has not tapped in
  the last 5 seconds, and a second open scan. After, it refuses a code the
  request does not accept and any pairing request. The trait's docs list what
  the host must do.

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
`Unsupported`, and `turn_card` answers `Unsupported` to every Widget, since
no host seam turns a card.
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
