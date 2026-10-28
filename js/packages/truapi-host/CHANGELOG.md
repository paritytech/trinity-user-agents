# @parity/truapi-host

## 0.27.0

### Major Changes

- 0bd0f9b: Host integrations implement `requestWorkerTransition(productId, transition)` for worker lifecycle requests.
  `Start` requires the worker to run; `Stop` allows it to stop. Native Rust integrations use
  `request_worker_transition`, and the ledger controller is `WorkerController` with `install_worker_controller`.

### Minor Changes

- ff34f26: The core builds its own extrinsics through one builder: statement-store and long-term-storage allowances,
  PGAS claims, Bulletin preimages and the CLI's dotNS registrations. Every extrinsic is mortal, born at the finalized
  head for 64 blocks, and signs the genesis the host configured. Its version follows the new
  `transactionExtensionVersions` host callback, in the `extrinsicVersion` group: the chains the host builds as V5
  general transactions and the transaction extension pipeline each uses; every other chain is V4. A web host
  implementing `RequiredHostCallbacks` has to answer it; an empty list builds every chain as V4, which leaves the core
  unable to build the allowance, PGAS and dotNS extrinsics, since those exist only as V5. The `truapi-host` CLI answers
  People and Asset Hub on pipeline 1.
- d61da55: `system.info()` reports `truapiVersion`, the TrUAPI release whose runtime the host embeds, so a product can
  tag telemetry with it. The runtime fills it in on every host, including iOS and Android. A product built before this
  release ignores the field. A product built with it cannot decode the answer of an older host, so it should treat that
  error as the version being unknown.
- 8c23b5e: Add the `NftPurse` API. Native hosts create purses owned by the calling product and list an owner's purses,
  asking for the new `NftPurseRead` permission before showing Root's or another product's. Its other methods answer
  unavailable.
- e9b6d73: Native hosts serve `NftPurse.createItems` and `NftPurse.queryPurse`. `createItems` derives 1 to 1000
  receiving keys in a purse, on the hard path `//nft//<purse>//<installation>//<index>` under the installation the host
  reports through the new `currentInstallation` host callback. `queryPurse` pages through the keys that hold an NFT on
  Asset Hub at the best block, at most 100 at a time. Another owner's purse needs the `NftPurseRead` grant for both. The
  browser core answers both `Unsupported`.
- e6621cd: Account access, operation reviews and cross-product signature approvals go through one `ProductConsent` per
  runtime. An Allow once answer to an account-access prompt now covers that product's later requests until restart,
  lock, a wallet switch or that product's reset, only one account-access prompt is shown at a time, and a request that
  waited for it reuses the answer. Requests from a paired host share the phone's stored decisions and are always
  reviewed on the phone.
- 747865c: If a product sends a request in a version the host doesn't know, the host rejects it with an
  `UnsupportedVersion` error instead of failing to decode it, and the call fails as `Unsupported`. Older hosts still
  answer `MalformedFrame`.

### Patch Changes

- f67e9cb: The core closes a chain connection on which a request has waited 30 s with no frame arriving. The waiting
  requests fail with `json-rpc connection closed: no frame for 30s while a request waited`, the host is told to close
  the socket, and the next call opens a new connection without waiting for the host to miss a pong. The deadline
  outlasts the largest frame a chain sends on a slow link, the 1.3 MB Asset Hub runtime metadata, so a slow answer is
  not mistaken for a dead socket.
- 662da7f: Native hosts screen a product-supplied chat room icon through the core rather than reimplementing it.
  `screen_chat_icon` applies the rules `create_room` and `register_bot` already apply, for a host to call wherever it
  takes a room icon from a product, and raises `NativeChatFieldError`.
- f67e9cb: The SDK warns `[truapi] host link lost: <reason>` with the close reason each time its host connection drops,
  including each refused redial, and the host container warns `[truapi] host link back after N ms` when the link
  returns, timed from the first loss. The core warns when a json-rpc reply takes more than 5 s, when a json-rpc
  connection closes while requests wait, naming each with its age, and when `createTransaction` waits more than 2 s for
  the current block or cannot get it.
- f67e9cb: Every connection the runtime opens to a chain shares the output of the runtime calls that depend only on the
  runtime, such as the metadata. Products that start together and the transaction builder of the host download the 1 to
  1.5 MB metadata of each runtime once between them, keyed by the hash of the runtime code at the block they ask about.
  A product that asks while the download is on its way waits for it, and is told the operation is inaccessible if that
  download fails, so it asks again.
- c18c267: Runtime, CLI, and generated WASM host callbacks use shared error conversions while retaining protocol error
  variants and JavaScript rejection values. CLI storage and session failures retain nested causes, and allowance
  exhaustion is recognized through the error chain.
- f67e9cb: A call made while the SDK has lost its host connection waits for the connection it redials and is sent once
  that connection completes its handshake. A call or subscription whose frame already reached the host fails with
  `ConnectionResetError`, since the host may have started it. The waiting call fails with the same error when the SDK
  stops redialing: the page is hidden, the retries are spent, or the connection was disposed. The 120 s request deadline
  still bounds the wait.
- Updated dependencies [f67e9cb]
- Updated dependencies [b5e1aa4]
- Updated dependencies [0eb93a9]
- Updated dependencies [ff34f26]
- Updated dependencies [d61da55]
- Updated dependencies [f67e9cb]
- Updated dependencies [f231850]
- Updated dependencies [dce190f]
- Updated dependencies [8c23b5e]
- Updated dependencies [e9b6d73]
- Updated dependencies [0c77379]
- Updated dependencies [747865c]
- Updated dependencies [f67e9cb]
- Updated dependencies [c18c267]
- Updated dependencies [f67e9cb]
  - @parity/truapi@0.27.0

## 0.26.0

### Major Changes

- 8462d88: `parse_navigate` reads a Pocket deeplink as the host's own URL.
  `polkadotapp://pocket/add?product=<id>&card=<id>` and `polkadotapp://pocket/open?product=<id>&card=<id>` classify as
  `NavigateDecision::Pocket`, naming the product from the `product` parameter, and `polkadotapp://pocket` classifies as
  `NavigateDecision::PocketCollection`, which carries no `identifier`. A Pocket link with an action the core does not
  serve, or with no product or no card, is rejected. A product's own paths are App routes:
  `polkadot://<product>.<tld>/-/pocket/...` classifies as `DotName` and opens the product, so a product that links to
  the Pocket uses the new form. `navigate_to` passes a Pocket link without the `OpenUrl` grant other `polkadotapp://`
  links need.
- 8c6f4c9: `preimage.submit` raises no per-upload confirmation. The `PreimageSubmit` permission authorizes it:
  `AllowAlways` covers every upload, and `AllowOnce` covers a single upload. Allocating or renewing the Bulletin
  allowance may still ask the paired wallet. `UserConfirmationReview` has no `PreimageSubmit` variant and
  `PreimageSubmitReview` is not exported, so hosts that match on either remove that case; the remaining review variants
  keep their wire discriminants.
- 1a2efef: When the user refuses a resource allocation on the product's own host, the product receives a response whose
  outcomes are all `Rejected`, as RFC 0010 defines, instead of an error. A refusal on the phone for a paired host
  already answered this way.
- 7472b77: `development_createAccountProof` is not exported, and signing and pairing hosts derive every ring-VRF proof
  context from its product id. A `raw:` product id has no special meaning: its context is derived like any other
  product's, so a proof can no longer be bound to 32 caller-chosen bytes.

### Minor Changes

- 073b4f2: When a product signs with another product's account, the user is asked once for each kind of signature and
  each owning product, rather than for every signature. The approval lasts until the calling product is closed. This
  applies to payload, watermarked raw, transaction and statement proof signatures. If the user declines, the next
  signature asks again. Transactions that name contacts and unwatermarked raw bytes still ask every time. Only one of
  these prompts is open at a time, and a signature the user already approved does not wait for it.
- 7bf5a3e: Durable transactions can be scheduled before they are signed. A domain registers a `SubmissionPolicy`; a
  scheduled transaction waits in the ledger until the core's submission executor asks that policy to build it, then is
  broadcast and followed like a presigned one. A presigned transaction may name a policy too. When a transaction with a
  policy is proven failed, the policy decides whether it is built again instead of failing. `durable_work_changed(true)`
  also covers transactions waiting to be built, so the host keeps recovery running until they are. No domain schedules
  transactions yet.
- 28fa81c: A wallet-local session reads its usernames from the dotNS contracts on Asset Hub, the same lookup a paired
  session uses. Activation does not wait for it: the session starts under the lite username the host passes, if any, and
  when the dotNS record holds a name, the record's lite and full usernames replace it and the core reports them through
  `authStateChanged`. Hosts no longer need to read usernames from the People chain.
- 743f0ba: Add the `Motion` device permission. Products request it through `requestDevicePermission("Motion")` or the
  standard `DeviceMotionEvent.requestPermission()`, and the iOS host answers WebKit's motion request from the product's
  saved, one-use, or prompted decision. A host built before this release cannot decode `Motion` and rejects the request
  with a malformed-frame error, so products should treat that error as motion being unsupported.

### Patch Changes

- f1cfc42: Build and dry-run a Bulletin preimage transaction at a best block no older than the finalized block its
  mortality is anchored to, so the first submission of a session no longer fails with `BadProof` or `AncientBirthBlock`,
  or reads a stale nonce.
- 1a2efef: Before signing a Bulletin preimage submission, check that the product's Bulletin authorization can hold the
  whole payload, and ask for more first when it cannot, instead of waiting for the chain to reject the transaction.
- 10a36af: Anchor Bulletin preimage transaction mortality to finalized blocks so discarded forks do not invalidate
  signatures. Keep nonce lookup and dry-run validation at the best block.
- 19f5b48: Keep capability logic on latest protocol payloads and return responses, subscription items, and domain errors
  in the requested version. Local storage reads preserve V1 and V2 at the handler boundary.
- 9064140: Each product runtime holds its own chain connection, so a node's four-follow budget per connection is spent
  per product rather than shared across all of them. A product runtime's connections are torn down when it ends.
- eb57f87: A chain-head call or follow that fails because the chain connection closed, the follow was closed, or the
  product's connections were released reports that cause in its `HostFailure` reason, after the method name, instead of
  the method name alone.
- 11c6ceb: Keep each product's chain follows and transaction operations to itself: a product resolves only its own
  follow ids, cannot stop another product's transaction by its operation id, and releasing a product drops both.
- abfc1dd: Native hosts recognise a development server through the core. `parse_dev_server` reads an address a developer
  typed, such as `localhost:3000`, `10.0.2.2:3000` or `192.168.1.59:3000?product_id=humanity.dot`, and returns a
  `DevServerProduct`: the identifier the product runs under, the `http` origin it is served from, and the address to
  load. The identifier is the normalized `product_id` the address names, otherwise `localhost[:port]`. Only `localhost`
  and loopback or private IPv4 hosts over `http` qualify, and an address that does not raises `DevServerError`. Rust
  callers parse the address with `str::parse::<DevServerProduct>()`. A `localhost` identifier holds the core's
  development wildcard and a named one gets that product's accounts and storage, so hosts call it only in development
  builds. Native builds only.
- 8b58680: A paired session resolves its dotNS username again for accounts onboarded more than a week ago. Pending
  gateway claims count however old they are, because the PoP controller no longer expires them.
- 28fa81c: A session whose identity account holds only dotNS labels the PoP controller did not issue, such as a publicly
  registered name, resolves its username from the root key. Those labels are not usernames, so they no longer end the
  lookup with an empty result.
- 94628e3: Compile the automatic allowance renewal driver only on native targets while retaining allowance tracking in
  browser hosts.
- def1bb9: `payment.balanceSubscribe` checks the product's `BalanceAccess` again before passing on each balance, so
  revoking the grant ends the stream with `PermissionDenied`. The new `AlreadyExists` and `InvalidAmount` errors follow
  the existing variants, so `HostPaymentError` keeps the indexes it had before. The test host says why it cannot serve
  payments: the payment platforms are served only by native and Rust hosts.
- 81008a4: Statement Store, Bulletin and PGAS allowance allocation sign personhood operations through a
  `PersonhoodSigner`, so callers pass a signer instead of raw entropy.
- 6864e42: Allow `polkadotapp://` links through `system.navigateTo`, preserving payment parameters and requiring
  `OpenUrl` permission. Reject external `polkadot:` and `dot:` links.
- 33963a5: Keep each product connection's adapters, permissions and action channels in a `ProductConnection`, reach
  account operations through a `ProductAccounts` bound to that connection's product, and route login, logout and
  connection state through a `HostSession` trait split out of `ProductAuthority`.
- 93fee0a: `truapi-host` trims and lowercases the `productName` of a local product config before storing its grants, so
  `Peopl.Paseo` grants the same as `peopl.paseo`. The name must be a full product id: a bare label such as `peopl`, or a
  name such as `example.com`, stops the CLI at startup with an error, before any config is stored.

  Reading the cross-product signature permission for a target that is not a valid product id returns "not determined",
  and setting it returns an error.

- 93fee0a: A product id must be a name dotNS can register, or `localhost[:port]`. Each label is lowercase ASCII letters,
  digits and hyphens, does not start or end with a hyphen, and is at most 63 bytes, as the dotNS registrar requires. Any
  case is accepted and lowercased. An id such as `tést.dot`, `my_app.dot`, or one carrying an invisible character is
  refused wherever the runtime parses a product id.
- 1a2efef: A VRF request relayed from a paired host is always confirmed on the phone, even when the product it names
  holds a local AutoSigning grant.
- 33963a5: Apply every ring-VRF registry change through one prepared update under the registry's storage lock, and skip
  the storage write when the registry did not change. Test-host allocation controls live in one `TestResourceControls`.
- 822e310: Preserve the requested protocol version for all runtime responses, subscription items, and domain errors,
  including chat message failures and subscription interruptions, through one shared conversion boundary.
- 0e195c9: Run the pairing host's session selection, login and request channel in an `SsoRequestService`, and the
  signing host's pairing answers and served sessions in an `SsoResponderService`.
- a8623e2: Statement submissions and the Statement Store allowance paths run over the People-chain connection the chain
  runtime keeps open, so publishing a statement costs no WebSocket handshake. Statement subscriptions open their own
  connection.
- d24cf65: A product granted another product's ring-VRF key may now prove in, and read its alias in, the context of a
  third product whose manifest grants it `context` (or `all`). The third product must be on the key owner's network, and
  a refusal the user stored for that pair still applies. A third product that grants nothing stays refused:
  `NotAllowlisted` on the proof, `Rejected` on the alias read.
- 6ce480f: Trusted products (`peopl`, `dim2` and `stash`) get device permissions, such as notifications, without a
  prompt: the core does not call `device_permission` for them. They still need the OS grant. Stored device decisions do
  not apply to them, so per-product device toggles in the host have no effect.
- c843529: A blessed product signs raw bytes with the user's legacy account without a prompt when the bytes are signed
  inside the `<Bytes>` watermark, which cannot pass for a transaction's signing payload. The deprecated unwatermarked
  raw signing, payload signing and transaction creation with a legacy account still ask the user, as does every
  legacy-account signature for a product that is not blessed.
- Updated dependencies [10a36af]
- Updated dependencies [19f5b48]
- Updated dependencies [9a172e1]
- Updated dependencies [9064140]
- Updated dependencies [eb57f87]
- Updated dependencies [a8623e2]
- Updated dependencies [5ee4351]
- Updated dependencies [5ee4351]
- Updated dependencies [ddb729f]
- Updated dependencies [82db857]
- Updated dependencies [073b4f2]
- Updated dependencies [8b58680]
- Updated dependencies [28fa81c]
- Updated dependencies [7bf5a3e]
- Updated dependencies [eb035fe]
- Updated dependencies [eb035fe]
- Updated dependencies [eb035fe]
- Updated dependencies [eb035fe]
- Updated dependencies [eb035fe]
- Updated dependencies [28fa81c]
- Updated dependencies [743f0ba]
- Updated dependencies [98e4550]
- Updated dependencies [98e4550]
- Updated dependencies [98e4550]
- Updated dependencies [98e4550]
- Updated dependencies [def1bb9]
- Updated dependencies [6864e42]
- Updated dependencies [8c6f4c9]
- Updated dependencies [93fee0a]
- Updated dependencies [93fee0a]
- Updated dependencies [7472b77]
- Updated dependencies [822e310]
- Updated dependencies [6ce480f]
  - @parity/truapi@0.26.0

## 0.25.0

### Major Changes

- fd7926a: Add `chat.setRoomFooter`, which sets what a room the product created shows below its messages: the text
  input, or nothing for a room the product drives through actions alone. A host keeps the footer until the product sets
  another. A room the product did not create answers `UnknownRoom`, and a host that predates the method answers
  `Unsupported`. A host serving chat now implements `setChatRoomFooter`, and native hosts implement
  `ChatHostBridge.setRoomFooter`. Neither needs to check the room, because the core answers `UnknownRoom` before asking
  the host.

### Minor Changes

- 9fece5c: Native signing hosts follow presigned transactions to a verdict in the core. The core database gains a
  `durable_tx` ledger: a domain registers presigned mortal extrinsics, the core broadcasts them once committed, watches
  them, and decides every one it no longer watches from the chain, including those a previous process left live.
  `HostCallbacks` gains the required `durable_work_changed(pending)`, which the Swift and Kotlin `HostBridge` default to
  a no-op; while it reports `true` the host keeps a background task running that awaits `runDurableRecovery()`. No
  domain registers transactions yet.
- 84282d0: Serve `scanner.scan`. A host supplies the optional `scanner` callbacks to draw its viewfinder, and the core
  checks each request and each answer. The mock test host serves a scanner when created with a `scanner` answer, and
  `setScanAnswer` changes the next one. `contacts.pick` now stops waiting when the product cancels.
- 34ba9f8: `parse_navigate` classifies `polkadot://<product>.<tld>/-/pocket`, with no action, as
  `NavigateDecision::PocketCollection`: a product's link to the Pocket itself rather than to one card. Like a card link
  it resolves into the host's own surface, so `navigate_to` passes it without an `OpenUrl` grant. A host that matches
  `NavigateDecision` exhaustively needs an arm for the new variant.
- 7944c68: Native hosts read the `@parity/truapi-host` release their core was built from with `truapi_host_version`. The
  iOS and Android debug menus show it beside the TrUAPI Runtime switch.

### Patch Changes

- 14ef0cb: Answer a Widget's `expandedCard.setFaceShown` with `Unsupported`, since a browser host has no expanded cards.
  The generated host-callback types include `ExpandedCardHost`, the hook native hosts implement; a browser host supplies
  nothing for it. `HostCallbacks` gains a required `setExpandedCardFaceShown`, so a native embedder that implements it
  directly, rather than through `HostBridge`, has to add it; `HostBridge` defaults it to `Unsupported`.
- 10a48ea: A Web Worker host booted with `role: 'signing'` serves the same host surface as a pairing host:
  - `getPermissionAuthorizationStatus`, `getPermissionAuthorizationStatuses` and `setPermissionAuthorizationStatus`
    answer, so a signing host can read stored device permissions when it mounts a product. `WasmSigningHostRuntime`
    exposes `permissionAuthorizationStatus`, `permissionAuthorizationStatuses`, `setPermissionAuthorizationStatus` and
    `notifyContactsChanged` with the same signatures as `WasmPairingHostRuntime`.
  - The optional `chat`, `contacts` and `permissionStatus` callback groups reach the core. A device capability the
    browser blocks reads as `Denied` whatever is stored, and on the test host a Worker product's chat calls reach the
    mock, so the Playwright fixture's `getChatRooms`, `getChatBots` and `getChatMessageLog` record them.
  - A request only a pairing host can answer, such as `getSessionChatIdentityKey` or `activateStoredSession`, rejects
    with an error naming the request. `cancelPairing` and `notifySessionStoreChanged` do nothing, since a signing host
    has no pairing and no stored session.

- 59ea7aa: Zeroize SSO session and pairing secrets on drop and redact responder and handshake secrets from debug output.
- 92f7032: An SSO request waiting on the paired signing host skips a peer message that does not decode, with a debug
  log, instead of failing. A stale reply left on the session channel, such as an AutoSigning allocation without
  `ring_vrf_domain_entropy`, is skipped, so later backups, identity publishing and `getProductAccount` calls complete,
  and the idle peer-disconnect monitor keeps watching past it.
- d512037: The core uses subxt 0.51.1 and jsonrpsee 0.26.
- Updated dependencies [c75de1b]
- Updated dependencies [fd7926a]
- Updated dependencies [9fece5c]
- Updated dependencies [1c4a63e]
- Updated dependencies [84282d0]
  - @parity/truapi@0.25.0

## 0.24.0

### Minor Changes

- 96c002c: Give native signing hosts a core-owned SQLite database. `HostRuntimeConfig.database_directory`
  (`databaseDirectory` in Swift and Kotlin) is required and names an existing, writable directory kept out of backups.
  The runtime opens `core.sqlite3` there at startup and refuses to start when it cannot. `coreDatabaseStatus()` reports
  the SQLite version, schema version and path.
- 2c7efee: Serve the `game` service from the host runtime. A host supplies the optional `game` callbacks,
  `scheduleGameReminder` and `cancelGameReminder`, to hold the calling product's next-game reminder.
  `scheduleGameReminder` receives the product and the start time (Unix milliseconds, as a `bigint`); the core asks for
  no per-product consent, so the host asks the platform for what the reminder needs. A host that supplies none answers
  both `Unsupported`. The core serves `game` to the game product, `dim2`, alone, so the mock test host answers
  `Unsupported` to its default `mock.dot` product; a suite that exercises `game` passes `productId: "dim2.dot"`.
- d8c83f5: Add the `scanner` service. `scan` asks the host to open its own QR and barcode viewfinder and returns the
  scanned code's text and format. No host serves it yet, so the runtime answers `Unsupported`.
- a0cd409: The test host answers a product account's address from the fixture, derived from the active session's root,
  so a suite can fund or assert on that account without reading it out of the product's own UI. `productAccountAddress`
  works the same address out with no host running, which is what a suite funding that account once, in setup, reaches
  for.
- 7747430: The test host's statement store reads the field vector the core sends, so a topic filter matches what a
  product submitted, and a suite reads statements back as decoded entries. `behaviors.resourceAllocation` withholds a
  named resource across every way a product reaches it, including the allowance keys the statement-proof and preimage
  paths ask for without requesting an allocation. A changed permission answer reaches the core, and product storage is
  readable under the name `@parity/host-api-test-sdk` gives it.

  Surface a migrating suite has to match:
  - `PermissionLogEntry` carries `decision` and `timestamp` as required fields, so an assertion comparing a whole entry
    names both.
  - `injectStatement` answers the `StatementEntry` the store retained, and takes `{topics, data}` as well as the SCALE
    wire bytes.
  - `getSubmittedStatements` answers `StatementEntry[]`, so a read of a statement's payload goes through `entry.data`
    rather than the `0x` hex.
  - The loopback statement store rides on the People chain's proxy, or on the single proxy of a one-chain suite. Two or
    more chains with no People among them carry no store: a suite that passed `loopbackStatements: true` is refused and
    told to declare the People chain or drop to one chain, and a suite that only took the default gets no store and
    builds.

### Patch Changes

- b1105f5: Native hosts read and screen a Pocket card's fields through the core rather than reimplementing them.
  `parse_renderer_node_json` reads a product-declared face from the JSON shape the generated client describes, bounded
  at the same nesting the renderer subscription carries, and `encode_renderer_node` / `decode_renderer_node` keep one
  under the SCALE encoding it already travels in, so a face kept at one version reads back at the next.
  `screen_pocket_card_id` and `screen_pocket_card_title` apply the rules every Pocket call already applies, so a card is
  refused where it is declared rather than at its first wire call, and a title keeps the display rules rather than the
  stricter identifier ones. Both raise `NativeRendererError` or `NativeChatFieldError`.
- b89e96e: `parse_renderer_node_json` reads a card face on a thread of its own with an 8 MiB stack, so a face nested as
  deep as the reader admits is refused instead of overflowing the stack of the thread a host calls it from, which took
  the app down. A face at that bound needs between 1 and 2 MiB, more than an Android background thread or an iOS
  secondary thread carries. A host that cannot start that thread gets `NativeRendererError::ReaderUnavailable`.
  `decode_renderer_node` refuses bytes past the end of the kept tree, as `Malformed`.
- 254aa63: Document that `ProductStorage.read` can be handed another product's key. On a granted cross-product read the
  key names the owner, not the caller, so a host that keeps a separate store per product must read from the owner the
  key names; one that keys a single store by the whole key needs no change.
- 3281e17: The local signing host caches a product's statement-store allowance key for the current allowance period,
  including a key allocated by an explicit resource allocation request, so proofs after it in a period skip the on-chain
  slot scan. The key is looked up again when the period changes, the local session is cleared or replaced, the product's
  state is cleared, or the statement store still rejects a statement signed with it for having no allowance after the
  submit retries. Statement submissions retry a `noAllowance` rejection up to 10 times, 2 seconds apart, as the native
  iOS and Android hosts do.
- 3281e17: Statement-store allowance lookups read a period's slot row in one request per collection, and read the People
  and LitePeople rows at the same time, instead of one request per slot.
- 5c8914d: A test host can keep preimage submissions in the core instead of sending them to the Bulletin chain, where a
  `store` signed with an allowance that was never authorized on chain is always refused at dry-run. Pairing and signing
  runtimes built with `test-host` take `setSubmitPreimagesLocally(true)`, and a signing host that grants allowances
  unchecked (`setGrantAllowancesUnchecked`) does it without being asked. The product gets the content key back and reads
  the value through the same lookup, from a store the core keeps for the rest of the run, and a refused
  `BulletinAllowance` still refuses the submission. Real hosts, which never set either, are unchanged.
- Updated dependencies [1481c42]
- Updated dependencies [aa6ae62]
- Updated dependencies [711d030]
- Updated dependencies [96c002c]
- Updated dependencies [2c7efee]
- Updated dependencies [d8c83f5]
- Updated dependencies [3281e17]
- Updated dependencies [3281e17]
  - @parity/truapi@0.24.0

## 0.23.0

### Minor Changes

- 7663ece: `create_transaction` and `create_transaction_with_legacy_account` read `txExtVersion` as the version of the
  transaction extensions in `extensions`, as the runtime numbers them. With `0` the host builds a V5 general transaction
  when transaction extension version 0 includes `VerifyMultiSignature`, and a signed V4 transaction otherwise. A
  non-zero value builds V5 with that version. A version the runtime does not declare returns `NotSupported` naming the
  declared versions.

### Patch Changes

- ca44c7f: Native host storage and Pocket card removal are asynchronous. The core awaits `core_storage_read`,
  `core_storage_write`, `core_storage_clear`, `local_storage_read`, `local_storage_write`, `local_storage_clear` and
  `NativePocketCallbacks::remove_card`, so a host backend that waits on disk, a keystore or a database no longer holds a
  core thread. On Android, `HostStorage`, `HostCoreStorage` and `PocketHostBridge.removeCard` are `suspend` functions,
  which is a breaking change for `@parity/android-host` implementers. On iOS the protocols are unchanged, since
  synchronous implementations satisfy the async requirements. `chain_send` and `chain_close` stay synchronous so
  requests keep their order, and must only enqueue work.
- 57475ce: Native hosts run the whole core on one process-wide tokio runtime. Subscriptions and background loops spawned
  by the core and the localhost WebSocket bridge's connections share its workers, so a bridged product's request and the
  subscriptions it opens run on the same executor. The runtime constructor fails with `RuntimeUnavailable` when that
  runtime cannot start.
- Updated dependencies [7663ece]
  - @parity/truapi@0.23.0

## 0.22.0

### Patch Changes

- d3d3312: Every native build of the core includes the localhost WebSocket bridge and the debug sink. There are no
  `ws-bridge` or `debug-sink` Cargo features: native targets compile both with the default `runtime` feature, and wasm32
  builds never include them.
- 336be6e: The shared Rust core asks blessed products only for device permissions and legacy-account signing. Other
  product operations bypass permission prompts and recorded decisions.
- 4ef4efc: Build the core WASM without wasm-opt's one-caller inlining, which merges functions into bodies that compress
  poorly. The `web` core is 719.5 KiB brotli and 952.5 KiB gzip, down from 775.4 KiB and 1.02 MiB; the raw size is
  unchanged at 2.66 MiB.
- Updated dependencies [a285523]
- Updated dependencies [60940ff]
- Updated dependencies [de343d4]
- Updated dependencies [3ef2191]
  - @parity/truapi@0.22.0

## 0.21.0

### Minor Changes

- 579f450: Load `verifiable` on demand in the browser. The core WASM is 2.64 MiB raw and 775 KiB brotli, down from 7.76
  MiB and 5.40 MiB: `verifiable` and its 4.5 MiB of powers of tau live in a separate module, `truapi_verifiable.js` and
  `truapi_verifiable_bg.wasm` beside the core's files in each bundle, fetched in the background once a pairing session
  connects, or when a ring-VRF operation first runs. Bundlers that follow `new URL(…, import.meta.url)`, such as Vite,
  emit both files, and a host serving a bundle directory as a whole needs no change; a host that copies individual files
  out of it must copy both too.
- db004aa: SSO request cancellation. A pairing host whose caller withdraws a request it has published sends a `Cancel`
  naming it, when that request is still the newest one on the session; a host timeout sends nothing. The core's
  signing-host responder reads `Cancel` while it is still serving earlier requests: a running request stops at its
  confirmation prompt or before its next allocation step and posts no response, and one not yet started never runs.
  `Cancel` is appended to the SSO catalog at the next index, so a peer that predates it logs the message and serves the
  request as before.
- 0e56d37: Take the wire debugger's dial from the embedding host instead of `localStorage`.

  `createWebWorkerPairingHostRuntime` accepts a `debugger` option; a dev build may carry a default in
  `VITE_TRUAPI_DEBUGGER_URL`. The host's own value wins, and `null` or `""` refuses the dial outright. The dial is
  resolved once at construction and cannot be changed from the page afterwards, so whether a session is observed is a
  property of the build and the host rather than of anything typed into a console later.

  While a dial is live the host shows a small dev-only badge naming every endpoint frames are going to, suppressible
  with `debuggerIndicator: false` for a host that renders its own. The badge belongs to the runtimes that are dialling,
  so an embedder with one worker runtime per product surface can give a dial to some of them without the rest taking the
  badge down.

  A dial URL must be `ws://` on `localhost`, 127.0.0.0/8 or `[::1]`, the same set the native sink accepts. An
  IPv4-mapped literal such as `ws://[::ffff:127.0.0.1]` is refused.

  **Breaking for anyone enabling the debugger today:** the `truapi:debugger` `localStorage` key is no longer read, and
  setting it has no effect. Pass the `debugger` option, or build with `VITE_TRUAPI_DEBUGGER_URL`.

- 5a9f4b9: The `Permissions` host callbacks name the product that asked: `devicePermission(product, request)` and
  `remotePermission(product, request)` take the requesting `ProductContext` first, like the other product-scoped
  callbacks, so a host can title the prompt with the product and key any grant it keeps itself by the product. Stored
  decisions stay keyed by `productId` alone.

  The `truapi-host` CLI names the requesting product in its permission approvals.

### Patch Changes

- cf1702f: Initialize built-in personhood ring keys when an authorized product lists the personhood owner's keys, so
  full and lite handles are available without prior registration on the device.
- dadcabd: A withdrawn request stops on the host, not only at the product. A call cancelled while its confirmation
  prompt is open stops waiting, and an answer given afterwards authorizes nothing. A withdrawn call never publishes its
  paired-host request, never sends a broadcast, statement, notification or navigation it had not yet sent, and a
  broadcast already sent is stopped when the node gave it an operation id. Permission prompts still finish and record
  their answer. Every stop reaches the product as `CallError.Cancelled`.
- Updated dependencies [ffdd9b4]
- Updated dependencies [5a9f4b9]
- Updated dependencies [cf1702f]
  - @parity/truapi@0.21.0

## 0.20.0

### Patch Changes

- Updated dependencies [5f3dc71]
- Updated dependencies
  - @parity/truapi@0.20.0

## 0.19.0

### Minor Changes

- 5709e2b: `@parity/truapi-host/testing` exposes a mock host products can be tested against. `createMockHost` answers
  the host callbacks from memory and records what the core asked for, so a test asserts on the confirmation reviews,
  permission answers, navigations and storage writes the core produced rather than on a host's internals.
  `./testing/playwright` provides a fixture that runs the real core in a Web Worker with the product in an iframe,
  `./testing/server` the node server behind it, `./testing/client` a no-iframe variant for unit tests, and
  `./testing/dev-accounts` named accounts that sign with real sr25519 from fixed entropy.

  A second WASM bundle at `./wasm/testing` carries the `wasm-signing-host` and `test-host` Cargo features, which is what
  lets the test host own keys and answer resource allocation as granted without allocating anything. Neither feature is
  on in the `./wasm/web` production bundle, so no shipping browser host has an entry point to either.

  Statement Store and Bulletin allowance allocation compiles for `wasm32` as well as native, so a browser signing host
  reaches the same on-chain allocation path a native one does. That adds about 1.7 KB to the `./wasm/web` bundle.

  Statement-store controls are served through the chain connection the host owns: `injectStatement` publishes a
  SCALE-encoded statement into the product's subscriptions, `getSubmittedStatements` reads submissions back off the
  transport, and `injectChatAction` publishes a host-authored Chat action into the product's action stream.

  Payments throw with the reason rather than returning a plausible value: the protocol declares them but no host
  implements them, so a test reaching that path learns why instead of passing against a fake. `setLoginBehavior` throws
  too, pointing at the `loginBehavior` fixture option, which is where the test host takes it.

### Patch Changes

- Updated dependencies [a78d73c]
- Updated dependencies [303b163]
- Updated dependencies [5d5fd1c]
  - @parity/truapi@0.19.0

## 0.18.0

### Major Changes

- 26b957f: `navigate_to` hands a host a dotNS product destination as `polkadot://<product_id>.<tld>/<path>` rather than
  rewriting it to `https://`. That is the form the Pocket deeplink grammar already defines for a product URL, and the
  one a Pocket target already arrives as, so a host routes on the scheme instead of guessing from the domain. An
  `http(s)` destination is unchanged and still gated on a per-host grant.

  A host that recognised `https://<name>.<tld>` and converted it back should match `polkadot://` instead.

### Minor Changes

- 9b54ceb: A `context` grant admits the grantee acting in the granting product's own proof context as well as in its
  own. Both are matched by product label within one network, so a product's other executables count and a namesake on
  another network does not.

  Refusing the granting product's context left the scope unusable: `PeopleLite.set_alias_account` verifies against
  `Score.score_context`, which names the personhood product for every prover, so a grantee held to its own context alone
  can produce no proof such a chain accepts. A context naming a third product is still refused, as is the `raw:`
  development context. A grantee's own namesake on another network is refused too, which is new. Both the ring-VRF proof
  and the contextual alias read follow the same rule, because they come out of one VRF evaluation.

- 448c1d4: - Host `device_permission` and `remote_permission` callbacks return `PermissionDecision` (`AllowOnce`,
  `AllowAlways`, or `Deny`). Native callbacks leave grant storage and consumption to Rust. Embedders must update their
  callbacks; product-facing responses remain boolean.
  - Keep one-use grants in memory. Internal `authorize_remote_permission` and `authorize_device_permission` methods
    consume them using the public request types, without exposing the methods in the public SDK or API documentation.
  - Normalize remote domains, match legacy wildcard coverage, and keep a shared blessed-domain list.
  - Require `OpenUrl` for external navigation, including allowed application schemes. A lasting grant covers all
    external destinations; domain permissions govern outbound network requests.
  - Require `Notifications` before `send_push_notification`, prompting when undecided and rejecting delivery when
    denied.
  - Present per-action confirmations without misleading persistent-permission choices.
- e9c45b8: Authorize browser fetch, asynchronous XHR, WebSocket connections, media capture and WebRTC through the
  internal `authorize_remote_permission` and `authorize_device_permission` APIs. Install the shared sandbox in native
  product views. Synchronous XHR, `Worker`, `WebTransport` and `getDisplayMedia` screen capture are unavailable.

  Camera and microphone grants are consumed per capture attempt, camera first. A later microphone denial or native
  capture failure does not restore an already consumed grant. Product consent is enforced by the container; native media
  delegates enforce OS permission separately.

  Update native integrations: Swift `installProductScripts(into:endpoint:)` now takes a `WKWebView` and is synchronous,
  without an execution argument. Swift and Kotlin `LocalhostBridgeBootstrap.script` no longer take `webRtcAllowed`.

- ea5e2f9: Add `localStorage.subscribe` and worker pending operations.

  `localStorage.subscribe(key)` streams a key's value within the product's own namespace, emitting the current value
  immediately and then one item per later write or clear from any of the product's runtimes. A write that leaves the
  stored bytes unchanged emits nothing.

  `worker.beginOperation` / `worker.endOperation` keep a product's worker runtime alive while it holds at least one open
  operation. Both are gated to the Worker execution kind. `endOperation` is idempotent. An open operation counts as
  demand on the product's worker, so it reaches a host through the same worker-demand signal an on-screen surface
  produces.

  Hosts implement `ProductOperations` and `ProductStorage.subscribeStorage` to back these.

### Patch Changes

- 27b8f07: A pairing attempt is bounded. Reading the stored pairing identity, connecting to the statement store,
  subscribing to the pairing topic and waiting for the wallet's handshake are each raced against a single 300s deadline,
  beside the cancellation they already honoured. Resolving the session and persisting it carry their own budgets and are
  not covered by it.

  A peer answering on a different SSO envelope publishes statements this host cannot open, which is indistinguishable
  from a peer that has not answered yet: both are silence on the topic. The attempt now ends with a reason naming that
  as the likely cause.

- b7b2c94: Product executions under one host runtime share a single localhost WebSocket listener, each with its own
  token. Closing a connection disposes the runtime that served it, so its host-core subscriptions and chat state are
  released rather than held for the life of the listener. Admitted connections are bounded per execution and
  listener-wide, with the per-execution cap a share of the listener-wide one; handshakes in flight are bounded
  separately, and a full backlog evicts its oldest entry so a stalled peer cannot lock out other executions.
- Updated dependencies [d9eaece]
- Updated dependencies [9b54ceb]
- Updated dependencies [33f9222]
- Updated dependencies [c2e5674]
- Updated dependencies [a63b0e8]
- Updated dependencies [448c1d4]
- Updated dependencies [d3ec891]
- Updated dependencies [e9c45b8]
- Updated dependencies [ea5e2f9]
  - @parity/truapi@0.18.0

## 0.17.0

### Minor Changes

- 221d972: Make the `context` scope effective. A `trustedProducts` grant of `context` now admits a cross-product
  ring-VRF proof or signature against the granting product's key, where before the runtime admitted the caller and the
  authority holding the keys refused it again with the same error a product granting nothing would produce. Each
  authority resolves the grant from the owner's published manifest itself rather than accepting a verdict relayed by the
  caller, because on a paired host the calling product id arrives over the wire as a self-asserted field.

  The authority derives the key from the identity it authorised rather than the spelling it was handed, so authorisation
  and derivation cannot diverge. A granted call is held to the caller's own proof context: the contextual alias is a
  function of the owner's key and the context, so an unconstrained context would let a grantee produce the alias the
  owner presents to a third product that granted nothing and cannot consent.

  The identity read `context` names is covered by the grant rather than prompted for again. The contextual alias and the
  ring-VRF proof come out of one VRF evaluation, so a grantee that may `create_account_proof` already holds the alias
  that proof attests; `get_account_alias` accepting the same grant makes the two calls agree about what the scope means.
  A product without a grant still takes the prompt.

  A stored account-access refusal still overrides a grant, and is now recorded against the product on both sides rather
  than one spelling of it, matching the granularity a manifest grant uses: a refusal for `peopl.dot` also covers
  `app.peopl.dot`. Decisions written by earlier releases under the full product id are still honoured, so upgrading does
  not discard a refusal a user has already given.

  `create_account_proof` with a foreign key handle and no active session now answers `Rejected` where it previously
  answered `NotAllowlisted`. The session is consulted before the grant so that the pair of refusals cannot be used to
  probe which product granted which; a product branching on the old tag for that case sees a different one.

  Product identifiers are rejected when they carry no name: an empty label, control characters, invisible bidirectional
  or zero-width formatting, whitespace, or path separators. Each of these reached the grant key, the manifest cache key,
  the account-access prompt and the logs. Internationalised names are unaffected.

  A granted cross-product access is logged. It raises no prompt and writes no stored decision, so previously only the
  refusal was audible: a publisher's grant could let one product act with another's keys leaving no trace on the device.
  This does not make the access revocable, which needs a surface for the user to record a decision about a pair they
  were never asked about.

  Resolving a grant can require reading a product manifest from dotNS, so the lookup is bounded by the caller's timeout
  and cancellation rather than running outside both. A manifest is cached under the label it resolves by, so every
  executable of one product shares one entry, and an entry stamped in the future is re-read rather than treated as fresh
  forever. Entries written by earlier releases under the full product id are no longer read and nothing evicts them, so
  they sit in core storage until the cache is cleared.

- b0d95f0: Serve the `pocket` service from the host runtime. A host supplies the optional `pocket` callbacks to back
  `listSubscribe` and `removeCard` over the calling product's cards; a host that supplies none answers both with
  `Unsupported`.
- 221d972: Cap product identifiers at 256 bytes. `has_dotns_tld` only inspects the suffix after the last `.`, so every
  length of `aaa...aaa.dot` was a distinct valid id, and a cross-product call carries that string from the wire where it
  is self-asserted. An identifier longer than the cap after NFC normalisation is now rejected, reported by length rather
  than by echoing the value back into an error string and a log line. This bounds the size of one identifier, not how
  many exist.

  Signing hosts also require an Asset Hub genesis hash. Product manifests are read from the dotNS contracts deployed
  there, so without one no manifest resolves and every cross-product `trustedProducts` grant not already cached is
  refused, indistinguishably from the other product having granted nothing. A custom build enabling the Rust
  `wasm-signing-host` feature must supply `runtimeConfig.assetHub` alongside `runtimeConfig.networkSuffix`; the shipped
  bundle is built `--no-default-features` and carries no signing constructor.

### Patch Changes

- Updated dependencies [4034118]
- Updated dependencies [221d972]
- Updated dependencies [befde58]
- Updated dependencies [75a9f2e]
- Updated dependencies [cc1823d]
- Updated dependencies [0e86a3a]
- Updated dependencies [a9731b1]
- Updated dependencies [a9731b1]
- Updated dependencies [a9731b1]
  - @parity/truapi@0.17.0

## 0.16.0

### Minor Changes

- 4c20296: Subscriptions can fail at any point, not only at start. A stream that breaks ends with a typed reason that
  reaches the product's `error` handler, so a platform failure surfaces instead of freezing the last value or passing
  for a clean finish; a method the host has not implemented, and a chain follow that cannot be opened, say so rather
  than completing quietly. A product that serves a host-initiated subscription, today only the chat custom renderer, now
  takes a handler of the request plus `send` and `interrupt` callbacks instead of returning an observable, which retires
  `ObservableSource`. Subscription consumers keep the shapes they had, and the wire bytes are unchanged.
- 4a93ac4: Add temporary deprecated unwatermarked signing for product and legacy accounts to unblock runtime ownership
  proofs while runtimes adopt watermarked verification (#612). Existing signing APIs retain their names and wrapping
  behavior. The temporary implementations log a deprecation warning when called. Raw signing reviews now include a
  `watermarked` flag; host confirmation UIs must display the matching bytes and warn that unwatermarked signatures may
  authorize transactions.
- 9252985: `Renderer` is the one service through which a product draws a body inside a host surface. The host starts
  `renderer.onRender` with a `RenderContext` (`ChatMessage`, `InputWidget`, `PocketCard`) and an opaque payload; the
  product streams `RendererNode` trees, and a press inside a tree reaches `renderer.actionSubscribe` as
  `{ context, actionId, payload }`. `Chat` has no `custom_message_render`; a `Custom` chat message renders through
  `Renderer` with a `ChatMessage` context, and `ChatActionPayload.ActionTriggered` carries only host-drawn `Actions`
  button presses.

  `RendererNode` replaces `CustomRendererNode` with `Image` (`ImageSource`, `ImageFit`), `Effect`, `Shape.Square`,
  `Modifier.Opacity` and `Modifier.BlendingMode`; `Spacer`, `TextField` and `Image` carry no `children`, and the
  single-field `Modifier` and `Shape` variants are tuple variants.

  Hosts call `provider.render(request, sink)` and `provider.publishRendererAction(item)`; `publishChatAction` is the
  path for posted messages, commands and host-drawn `Actions` buttons.

- fa67882: The core counts references on each product's worker and tells the host when that count crosses zero, so a
  worker runs only while something needs it.

  `acquireWorker(productId)` takes one reference for a modality holder that is on screen or in flight, and
  `releaseWorker(productId)` gives it back; releasing with none held is a no-op. The first reference and the last
  release are the only ones that report anything. `subscribeWorkerDemand(listener)` is where that report arrives: the
  listener receives every product wanted right now, then each change as it happens, and `wanted: false` for everything
  still wanted when the runtime is disposed. Starting and stopping the worker executable stays with the host, and a
  `wanted: false` is permission to stop rather than an order, so a host may keep one warm. The core keeps no clock and
  runs no timers.

### Patch Changes

- Updated dependencies [4c20296]
- Updated dependencies [4a93ac4]
- Updated dependencies [9252985]
  - @parity/truapi@0.16.0

## 0.12.0

### Minor Changes

- e8ee375: Address every frame with a two-byte `(trait, method)` wire discriminant. The trait byte names the API trait
  and the method byte addresses a method within it, so each trait owns a full 256-slot method space and method ids
  restart at 0 in every trait.

  A third envelope byte, `message_type`, names which leg of a method's exchange a frame carries, so a method costs one
  id whatever its shape. The payload is the plain SCALE encoding of that leg's type and carries its own version.

  `TrUApiTransport.codecVersion`, `CreateTransportOptions.codecVersion` and `GeneratedClientTransport` are removed.
  Generated handshake calls read `TRUAPI_CODEC_VERSION` directly, so there is no longer a way to advertise a codec
  version that differs from the one the envelope is actually framed in. `CreateTransportOptions` itself remains,
  carrying `requestTimeoutMs` alone, and `createClient` takes a `TrUApiTransport` (every value that satisfied
  `GeneratedClientTransport` satisfies it unchanged).

  This is wire codec version 2. A codec version 1 peer cannot exchange frames with a codec version 2 peer in either
  direction: the handshake itself rides the changed envelope, so the mismatch cannot be negotiated in band. Hosts and
  products must move together.

### Patch Changes

- Updated dependencies [d36911f]
- Updated dependencies [e8ee375]
  - @parity/truapi@0.15.0

## 0.11.0

### Minor Changes

- Reserved person and identity keys derive under the network's dotNS suffix. `SigningHostConfig` carries that suffix,
  and every reserved derivation (`uid.<suffix>`, `peopl.<suffix>`) scopes to it, so one seed resolves to the same person
  across the WASM, native and CLI hosts on a given network. Derivation vectors for `.paseo` and `.testnet` are pinned
  against an independent RFC-0022 implementation.

  One seed therefore resolves to a different person than it did under the unsuffixed derivation, and the CLI requires
  version 2 account and pairing stores: existing signer state must be discarded and devices paired again.

### Patch Changes

- Updated dependencies
  - @parity/truapi@0.14.0

## 0.10.1

### Patch Changes

- Preserve buffered subscription event order.
- Resolve the dotNS controller whether the gateway stores a dispatcher or the controller.
- Page dotNS `pendingClaims` through its `(address,uint256,uint256)` view, and retain claims from complete earlier pages
  when a later page reverts.
- Read Resources parameters through runtime view functions.
- Updated dependencies
  - @parity/truapi@0.13.1

## 0.10.0

### Minor Changes

- Rename the PreviewNet dotNS top-level domain from `.test` to `.testnet`.

### Patch Changes

- Updated dependencies
- Updated dependencies
  - @parity/truapi@0.13.0

## 0.9.0

### Minor Changes

- 8983638: Support the development-only raw proof context used by `development_createAccountProof`.
- 654c0cf: Expose the host's selected language through `locale.subscribe()`.

### Patch Changes

- Updated dependencies [654c0cf]
  - @parity/truapi@0.12.0

## 0.8.0

### Minor Changes

- fa7d8db: Expose the current canonical product identifier through `system.getProductContext()`.

### Patch Changes

- Updated dependencies [fa7d8db]
  - @parity/truapi@0.11.0

## 0.7.0

### Minor Changes

- Host runtime over the current Rust core. A JS host can serve Chat as an optional capability: bot registration, every
  message variant forwarded, manifest execution-kind matching, and custom chat rendering. The runtime retains and
  exposes session identity material, emits the opening auth state with a typed `LoginFailed` kind, forwards session
  activation, yields the named theme from `subscribe_theme`, and reads person usernames from Asset Hub dotNS. External
  navigation is gated on a per-host remote grant, own-account subtree consent is gated with a bounded deadline, and
  statement-store allowance renewal pools PGAS slots and reports what the last pass achieved. Fixes: workers are
  disposed cleanly, a misbehaving product or host no longer aborts the process, and the wasm glue is imported by a
  literal specifier.

### Patch Changes

- Updated dependencies [d872d64]
- Updated dependencies [d49f253]
  - @parity/truapi@0.10.0

## 0.6.0

### Minor Changes

- The host runtime backs the ring-VRF registry with a product-scoped key store, so registration, listing, and direct
  signing resolve against registered member keys, and a foreign key is refused unless its owner allowlisted the caller.

  Statement-store allowances renew themselves as they approach expiry and replace the oldest slot once a period is full,
  so long-lived products keep a usable slot without a manual top-up. Allowance operations reuse cached chain metadata,
  rings, and a single shared extension-info resolver instead of re-reading them per call.

  Product identifiers accept per-network dotNS TLDs, so a product name resolves against the host's configured network
  rather than a single hard-coded suffix.

### Patch Changes

- Updated dependencies
  - @parity/truapi@0.9.0

## 0.5.0

### Minor Changes

- Host callbacks gain `supportedChains()`, returning the host's environment plus one `(ChainIdentifier, genesisHash)`
  entry per chain role. The core answers `chain.getChainInfo` (RFC 0026) from this single callback; web hosts implement
  it on the `features` callback group.

### Patch Changes

- Updated dependencies
  - @parity/truapi@0.8.0

## 0.4.0

### Minor Changes

- Publish the RFC-0022 mobile host cutover and completed RFC-0023 account VRF signing runtime. Pairing hosts persist
  product-scoped AutoSigning keys, sign matching same-product requests locally, and require structured host and Account
  Holder confirmations before forwarding every other request.

### Patch Changes

- Updated dependencies
  - @parity/truapi@0.7.0

## 0.3.0

### Minor Changes

- Update the WASM host runtime and generated callbacks for tagged 32-byte product-account derivation indexes. Implement
  sr25519 VRF signing through both local AutoSigning authorization and account-holder confirmation flows.

### Patch Changes

- Updated dependencies
  - @parity/truapi@0.6.0

## 0.2.1

### Patch Changes

- Update the WASM host runtime so Bulletin preimage submission survives `chainHead_follow` interruptions without
  double-storing: an interrupted watch re-checks finalized blocks for the already-broadcast transaction before any
  retry, retries re-broadcast the identical signed bytes instead of re-signing with a fresh nonce, and a bounced
  re-broadcast surfaces as inclusion-unverified rather than a failure. Allowance propagation waits are now bounded by
  wall-clock time instead of a best-block count, keeping the budget stable across changes in Bulletin's block cadence.

## 0.2.0

### Minor Changes

- Update the WASM host runtime for junction-based ring locations and contextual alias/proof reviews. The runtime also
  exposes login progress after wallet approval, routes product and DotNS identity raw signing through their matching
  account-holder messages, and retries transient preimage inclusion lookups.

### Patch Changes

- Updated dependencies
  - @parity/truapi@0.5.0

## 0.1.0

### Minor Changes

- Initial public release of `@parity/truapi-host`: a WASM-backed TrUAPI host runtime that embeds the Rust core. Subpath
  entries expose the shared host types (`.`), the browser iframe + Web Worker runtime (`/web`), the Worker entry
  (`/worker-runtime`), and the packaged WASM bundle (`/wasm/web`).
