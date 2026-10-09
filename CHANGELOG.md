# Changelog

All notable changes to the TrUAPI protocol are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
generated from [Conventional Commits](https://www.conventionalcommits.org/).

## [0.23.0] - 2026-09-29

### Added

- refresh the relay checkpoints, let the host choose peer connection types (#1046)

### Changed

- make storage callbacks and Pocket removal async (#1019)
- run native core tasks on one Tokio runtime (#1018)

### Fixed

- read txExtVersion as the transaction extension version (#1003)
- recover a product and its worker after the WebView renderer dies (#994)

## [0.22.0] - 2026-09-28

### Added

- hold light-client requests until the chain syncs, expose its lifecycle (#1030)
- draw the contact picker (#939)
- draw the contact picker (#907)
- pick a contact, contact them by an opaque handle (#17)
- stop prompting first-party products for remote access (#971)

### Changed

- @parity/truapi 0.22.0, @parity/truapi-host 0.22.0, @parity/truapi-provider 0.3.0, @parity/ios-host 0.22.0, @parity/android-host 0.22.0 (#1045)
- split native.rs into focused native/ submodules (#1027)
- build the native bridge and debug sink without feature flags (#1024)
- Adopt general AGENTS.md guidelines and refactor the Rust workspace to match (#1000)

### Fixed

- follow previewnet through its latest reset (#995)
- prompt blessed products only for device access (#999)
- rebind the port when the listener fails instead of spinning (#983)
- reload a page whose MessagePorts died with WebKit's networking process (#976)
- sign with a product account its owner granted context (#873)
- recover products after the app returns from the background (#974)
- protect permission checks without breaking libraries (#916)
- give the Android preview the real backend identifiers (#975)

## [0.21.0] - 2026-09-24

### Added

- load verifiable on demand in the browser (#922)
- host-supplied wire-debugger dial, plus a make debugger target (#604)
- let a withdrawal reach the paired host that is serving it (#933)
- stop a withdrawn call before it prompts, acts or reaches another party (#927)
- pass the requesting ProductContext to permission prompts (#936)
- stream product frames to a wire debugger behind --debugger (#656)

### Changed

- @parity/truapi 0.21.0, @parity/truapi-host 0.21.0, @parity/truapi-provider 0.2.2, @parity/ios-host 0.21.0, @parity/android-host 0.21.0 (#973)
- build smoldot's secp256k1 tables on first use in the browser (#928)

### Fixed

- mark the debugger private, since it is not published (#911)
- initialize built-in personhood keys (#924)

## [0.20.0] - 2026-09-23

### Added

- chat modality on the shared TrUAPI core (#840)

### Changed

- @parity/truapi 0.20.0, @parity/truapi-host 0.20.0, @parity/ios-host 0.20.0 (#920)

### Fixed

- repair source installs and gate rustdoc (#917)
- stop freezing built-in prototypes (#915)

## [0.19.0] - 2026-09-22

### Added

- add typed script projects (#549)
- a TrUAPI-native test host (#294)
- report a newly paired device to the host (#851)
- let an Android preview name the commit it came from (#882)
- distribute Android nightly and debug builds (#864)
- build an installable Android APK on demand (#858)

### Changed

- @parity/truapi 0.19.0, @parity/truapi-host 0.19.0 (#904)
- Sandbox CLI product scripts by default (#829)
- @parity/truapi-provider 0.2.1 (#871)

### Fixed

- mask the signed download link the distributor prints (#906)
- pass CONTACT_EMAIL to the Android builds (#903)
- balance the delivery preflight, and shellcheck inline action shell (#902)
- reconnect the localhost bridge after its socket dies (#872)
- say why the delivery check refused (#894)
- generate the bindings for every build type, not two (#884)
- hand each reusable workflow the secrets it reads (#860)
- use the application id the Google configuration is built for (#863)
- re-apply deleted paths by removing them, not by patching (#859)
- take the light client's full-node statement replay (#870)

## [0.18.0] - 2026-09-21

### RFCs

- **Accepted:** Wire message type: an explicit byte for trait, method, and leg
- **Withdrawn:** Wire message type: an explicit byte for trait, method, and leg

### Added

- report what a vendored tree owes its source (#824)
- localStorage.subscribe and worker pending operations (#603)
- cancel an in-flight one-shot request from the wire (#843)
- expose the device statement account to host applications (#842)
- resolve the Android core from this tree (#837)
- extend AutoSigning to the product signing APIs and statement proofs (#751)
- let hosts ask whether a product is trusted for remote access (#802)

### Changed

- @parity/truapi 0.18.0, @parity/truapi-host 0.18.0 (#869)
- Authorize browser APIs through Rust permissions (#828)
- Preserve permission decisions in the Rust core (#827)
- Share one localhost WS listener across native product executions (#600)
- route subscriptions through one state lock (#823)
- Add screenshot-triggered issue reports to mobile hosts (#771)

### Fixed

- admit the granting product's own proof context (#850)
- bound the pairing attempt with a deadline (#845)
- stamp the generated client with the host's codec version (#848)
- hand a product destination to the host as a polkadot URL (#832)
- build the core before the distribution workflows build the app (#833)
- record the revision hosts/ios is actually at (#822)

## [0.17.0] - 2026-09-17

### Added

- cap the light client's live connections (#817)
- a label-gated signed build that installs on a phone (#815)
- refresh a vendored host tree from its source repository (#811)
- read the statement renewal ledger, and untrack from a native host (#786)
- serve the Pocket card collection (#706)
- Pocket modality RFC and protocol spec (#609)

### Changed

- @parity/truapi 0.17.0, @parity/truapi-host 0.17.0, @parity/truapi-provider 0.2.0 (#819)
- require versioned wrappers for empty wire payloads (#785)
- restorable Rust cache, wire-table gate in the rust job (#779)
- remove genesis constants (#753)
- publish the iOS host and subscribe the app to its bumps (#758)
- parallel iOS slices, one rustdoc call, deterministic type names (#773)
- cut the Rust test suite from 2m12s to 21s (#767)

### Fixed

- install the Asset Hub genesis hash on the signing role (#729)
- move the published fallback forward and stop it rotting silently (#812)
- advertise the X25519 chat identity key on chain (#814)
- keep one base path on one signer identity (#810)
- keep a later push from cancelling a release commit's CI (#809)
- give the CLI build matrix the generated Rust it cannot compile without (#780)

## [0.16.0] - 2026-09-14

### RFCs

- **Accepted:** Wire message type: an explicit byte for trait, method, and leg

### Added

- Unified Renderer (#633)
- Worker Lifecycle (#632)
- Subscription interrupt (#631)
- notify removed devices (#584)
- add temporary deprecated unwrapped signing (#731)
- address every frame with a (trait, method, message_type) envelope (#357)
- generate typed dispatch and wire conversions (#651)
- resolve the core from this tree for the iOS host (#725)
- warm start on every client (#629)
- wire trace engine, standalone inspector, and in-app panel (#536)
- payload-blind wire-debug tap, sinks, and the codegen decode surface (#295)
- persist /log level (#583)

### Changed

- @parity/truapi 0.16.0, @parity/truapi-host 0.16.0 (#744)
- RFC: Scoped grants in trustedProducts (#454)
- @parity/truapi 0.15.0, @parity/truapi-host 0.12.0 (#732)
- serve inter-host requests through typed handlers (#628)
- @parity/truapi 0.14.0, @parity/truapi-host 0.11.0, @parity/ios-host 0.14.0, @parity/android-host 0.1.0 (#722)
- Clarify TrUAPI README and documentation paths (#721)
- move the request id into the call context (#718)
- @parity/truapi-provider 0.1.0 (#662)
- consolidate Rust boilerplate and move runtime modules (#617)
- speed up UniFFI binding generation (#590)

### Fixed

- bound iframe bootstrap and requests (#665)
- run the bootnode health check when a dispatch asks for it (#691)
- version the persisted session blob and decode the older layouts (#647)
- read lite PoP names in both their dotted and flattened forms (#602)
- derive the reserved person and identity keys under the network suffix (#627)
- remove legacy compatibility fallbacks (#585)
- adopt current People proof contexts (#587)
- announce the boot auth state after the initial session restore (#571)

## [0.13.1] - 2026-09-02

### Changed

- @parity/truapi 0.13.1, @parity/truapi-host 0.10.1, @parity/ios-host 0.13.1 (#581)

### Fixed

- strip the provider xcframework modulemaps (#553)
- resolve the dotNS controller whether the gateway stores a dispatcher or the controller (#564)
- follow previewnet and paseo-next-v2 through their wipes (#579)
- read Resources parameters through view functions (#577)
- clear the active UI slot only while it owns it (#567)
- page dotNS pendingClaims through its (address,uint256,uint256) view (#574)

## [0.13.0] - 2026-09-01

### Added

- rename PreviewNet dotNS TLD from .test to .testnet (#561)

### Changed

- @parity/truapi 0.13.0, @parity/truapi-host 0.10.0 (#568)
- Paste pairing QR images in the host CLI (#552)

## [0.12.0] - 2026-08-31

### Added

- `development_createAccountProof` for raw proof contexts to unblock Humanity as a Product (#457)
- open bump issues on consumer repos when a package is released (#529)

### Changed

- @parity/truapi 0.12.0, @parity/truapi-host 0.9.0, @parity/ios-host 0.12.0 (#555)
- RFC: Host locale subscription (#526)
- fix prebuilt script runner lookup (#532)

### Fixed

- reject unknown wire messages (#547)
- declare the chat worker in the product manifest (#541)
- downgrade response and error payloads to the caller's version (#525)

## [0.11.0] - 2026-08-27

### Added

- prebuilt truapi-host binaries, one-liner installer, and self-update (#516)
- local dev flow — run a product in a browser tab against the CLI host (#510)
- revalidate device permissions against OS state before use (#471)
- expose current product context (#504)
- publish io.parity:truapi-host-android as an AAR (#337)
- auto-grant remote permissions to trusted product labels (#446)
- improve signing-host session lifecycle (#495)

### Changed

- @parity/truapi 0.11.0, @parity/truapi-host 0.8.0 (#530)
- remove legacy single-execution core (#508)
- RFC: Host Identity and Version via `System.host_info` (#177)

### Fixed

- make the CLI release pipeline work end to end (#531)
- clear the sandbox client when the pipe closes (#509)
- persist and restore paired SSO hosts (#501)
- gate tags on a confirmed npm publish (#505)
- derive Pages base path from configure-pages (#500)

## [0.10.0] - 2026-08-24

### Added

- match manifest execution kinds and serve custom chat rendering from a JS host (#459)
- gate WebRTC on a decision resolved before the product realm (#444)
- let a host read what the last renewal pass achieved (#447)
- tell a listener which way its connection closed (#461)
- forward every message variant and add the Android host surface (#453)
- serve product frames headlessly with --serve (#439)
- export createWebSocketProvider for browser clients (#438)
- add the previewnet network preset (#440)
- serve Chat from a JS host as an optional capability (#400)
- implement Chat::register_bot in the shared Rust core (#430)
- native and wasm ChainProvider with WebSocket and embedded smoldot backends (#276)
- retain and expose session identity material (#403)
- gate external navigation on a per-host remote grant
- give AuthState::LoginFailed a typed kind (#401)
- pool allowance slots across personhood collections (#431)
- report renewal targets dropped by an identity change (#423)
- drive statement-store allowance renewal from native hosts (#417)
- yield the named theme from subscribe_theme (#396)
- serve Asset Hub as a chain role (#404)
- support PGAS allowances (#391)
- make ProductContext SCALE-encodable (#392)

### Changed

- @parity/truapi 0.10.0, @parity/truapi-host 0.7.0 (#494)
- Own-account subtree consent gate, deadline bound, and worker dispose fix (#469)
- Person's usernames: read from Asset Hub dotNS instead of the People Chain (rebase of #349) (#426)
- publish 0.7.0 (#450)
- SSO message handling bindings for Mobile hosts (#433)
- Allow webrtc connection for products (#399)
- update Package.swift (#421)
- Add product manifest RFC and host implementation guide

### Fixed

- raise the crate recursion limit for the trait solver (#475)
- accept the test dotNS TLD in product identifiers (#465)
- follow previewnet through its wipe (#455)
- stop a product or host from aborting the process (#452)
- track tunnel liveness by flag, not by dialling (#445)
- satisfy collapsible_match without changing Enter handling (#437)
- narrow the navigation grant to authorizable hosts
- satisfy collapsible_match without changing Enter handling
- advertise the People genesis the chain reports (#416)
- emit the opening auth state and forward session activation (#393)
- resolve product chains against the network preset (#402)
- import the wasm glue by a literal specifier (#394)
- skip v5 signing for supplied VerifyMultiSignature (#374)
- encode the transaction-extension version the runtime declares (#382)

## [0.9.0] - 2026-08-13

### RFCs

- **Accepted:** Proof of Personhood as a product

### Added

- replace the oldest slot when a period is full (#378)
- auto-renew statement-store allowances (#308)
- land RFC-0024 ring VRF key management (#360)

### Changed

- @parity/truapi 0.9.0, @parity/truapi-host 0.6.0 (#381)
- share one extension-info resolver in allowance metadata (#377)
- stop re-reading metadata and rings on every allowance call (#366)
- update ios library to 0.5.0 (#367)

### Fixed

- accept per-network dotNS TLDs in product identifiers (#369)
- deploy the playground under the new dotNS name format (#375)

## [0.8.0] - 2026-08-10

### RFCs

- **Accepted:** Host chain discovery and name resolution

### Added

- implement chain.getChainInfo in the core (#358)
- integrate the Chat modality with the shared Rust core (#326)
- support Extrinsic V5 transaction signing (#333)
- export parse_navigate to native hosts (#340)

### Changed

- @parity/truapi 0.8.0, @parity/truapi-host 0.5.0, @parity/ios-host 0.5.0 (#364)
- RFC 0026: Host chain discovery and name resolution (#354)
- Use canonical types over the native FFI (#345)
- iOS host integration (#330)

### Fixed

- isolate RFC-0022 session and AutoSigning capabilities (#329)

## [0.7.0] - 2026-08-04

### Added

- complete RFC-0022 host cutover (#327)

### Changed

- @parity/truapi 0.7.0, @parity/truapi-host 0.4.0 (#332)

## [0.6.0] - 2026-07-31

### RFCs

- **Accepted:** Account key derivations
- **Accepted:** sr25519 VRF signing for product accounts

### Added

- Android host adapter (android/truapi-host) + hosts/android submodule (#289)
- implement missing signing host fn (#288)

### Changed

- @parity/truapi 0.6.0, @parity/truapi-host 0.3.0 (#325)
- RFC 0023 - Sign VRF (#301)
- RFC 0022: Account key derivations (#296)
- cli host (#264)
- iOS host (#215)
- adopt Send async traits (#312)
- @parity/truapi-host 0.2.1 (#320)

### Fixed

- recheck inclusion instead of re-broadcasting on watch stop (#307)
- stop the broadcast operation returned by host (#318)

## [0.5.1] - 2026-07-21

### Changed

- @parity/truapi 0.5.1 (#302)

### Fixed

- run products on legacy hosts (#300)

## [0.5.0] - 2026-07-20

### RFCs

- **Accepted:** RFC 0004 — Redesign `host_account_create_proof`

### Changed

- @parity/truapi 0.5.0, @parity/truapi-host 0.2.0 (#293)
- RFC: Redesign RingLocation in host_account_create_proof (#18)

### Fixed

- cover Dotli signing regressions (#291)
- prevent stale generated bindings (#287)

## [0.4.1] - 2026-07-16

### Changed

- @parity/truapi 0.4.1 (#284)
- @parity/truapi-host 0.1.0 (#278)

### Fixed

- treat Firefox's masked "null" ancestor origin as hidden (#283)
- updates in diagnosis report + submit report button (#277)
- create tags through API (#282)
- unblock post-merge workflows (#281)

## [0.4.0] - 2026-07-15

### RFCs

- **Accepted:** Coinage Payment User Agent API

### Added

- build, sign, and submit Bulletin preimages in the core (#270)
- add @parity/truapi-host-wasm runtime (#252)
- generate wasm bridge callbacks (#265)
- add platform runtime and host bridge (#250)
- add wire and chain infrastructure (#256)
- add host logic primitives (#255)
- emit Rust dispatcher, wire table, and host callbacks (#254)
- add host capability traits (#249)
- add testing API and versioned wiring (#248)
- add explorer 0.3.2 version snapshot (#242)

### Changed

- @parity/truapi 0.4.0 (#279)

## [0.3.2] - 2026-06-26

### RFCs

- **Withdrawn:** Extended theme subscribe API

### Added

- add @parity/truapi/sandbox bootstrap entry point (#234)
- add explorer 0.3.1 version snapshot (#231)
- version lifecycle tooling (#145)

### Changed

- @parity/truapi 0.3.2
- @parity/truapi 0.3.2
- Playground e2e harness + SCALE codec additions (#238)
- rename Provider type to WireProvider (#235)
- Update diagnosis compatibility statuses (#233)
- Update playground transaction example for DotNS signer (#220)
- Revert "ci: remove redundant build skip env"
- fix
- Bump the actions group with 11 updates (#199)
- Add explorer v0.3.0 version snapshot
- Delete zizmor logs
- Apply zizmor --fix=all
- Preserve logs on timeout, float copy button, hide editor line numbers
- Annotate subscribe example statement and export generated types in Monaco dts
- Drop diagnosis-report changes; keep only the Rust example fixes
- Fix statement-store subscribe example and multi-line diagnosis details
- update ios report (from #189)
- route type-page links through typePath helper
- update ios report (from #182)
- update android report (from #180)
- update web report (from #178)
- update desktop report (from #175)
- improve examples
- fix
- fixes
- fixes
- fixes
- fixes
- fixes
- tmp
- test
- bundle the static export into a handful of chunks
- ss updates
- drop per-run report timestamp; fix docs
- Drop the per-method cancellation feature
- Cancel a processing diagnosis method from the UI
- Submit diagnosis reports via a pre-filled GitHub issue

### Fixed

- use GitHub API to create release tag
- align HostPaymentTopUpError SCALE indices with triangle-js-sdks (#223)
- Simplify the diagnosis run and fix the statement-store submit example (#174)
- remove version bump from cut-version.sh

### Removed

- roll back the CoinPayment (Coinage) host API

## [0.3.1] - 2026-06-17

### Changed

- @parity/truapi 0.3.1 (#228)
- @parity/truapi 0.3.1
- @parity/truapi@0.3.0
- @parity/truapi@0.3.0
- Revert "Add explorer v0.3.0 version snapshot"
- Add explorer v0.3.0 version snapshot

### Fixed

- use GitHub API to create release tag
- correct import paths in explorer 0.3.1 snapshot
- align HostPaymentTopUpError variant ordering with wire protocol
- add MIT license field to workspace and all crates
- remove prepare hooks and add deny.toml from main
- remove prepare hook from truapi-host package

## [0.3.0] - 2026-06-03

### RFCs

- **Accepted:** RFC-0020: Remove `context` from `create_transaction` and mirror in Accounts Protocol
- **Accepted:** Add Coins variant to PaymentTopUpSource
- **Accepted:** Extended theme subscribe API
- **Withdrawn:** Host API root account access
- **Withdrawn:** Simple Group Chat

### Added

- append to matrix instead of override
- extend theme subscribe API with named themes
- proc-macro envelopes + conversion traits
- add Next variant to Version enum
- diagnosis screen + host compatibility matrix (#143)
- show method examples and deep-link to the hosted playground
- require a ```ts example on every trait method
- add version lifecycle tooling and next/ staging module
- host compatibility matrix page
- codegen-driven explorer site with version snapshots (#130)
- implement RFC-0020 Rust types for create_transaction
- add host-side codegen and @parity/truapi-host package (#77)

### Changed

- Use Sr25519 secret keys in PaymentTopUpSource
- Always rebuild the compatibility matrix from the reports alone
- Regenerate compatibility matrix: drop skipped Coin Payment, restore host order
- fix
- fix
- update
- fixes
- Add 'make explorer' to run the explorer dev server locally
- update
- Add 'make matrix' to regenerate the compatibility matrix
- Track per-host diagnosis reports in version control
- update diagnostics
- one at a time
- clean up
- fixes
- improved errors
- update dotli sha
- Replay diagnosis methods and streamline the report action
- auto-test removal
- Add report error column, raise unary timeout, mark Signing/create_transaction web pass
- Expand diagnosis failures, link method breadcrumb to the index
- Deep-link diagnosis, fix host-mode detection, use chain const in signing examples
- improve playground ux
- fix up
- fix up compatibility tests
- @parity/truapi 0.3.0
- cut v0.3.0
- fix
- reduce payment
- IOS report
- simplify matrix builder and host-mode detection
- Update report matrix with android
- Set top_up example amount to 10000
- drop redundant playground link from method example
- Add compatibility parsed reports for web and desktop
- simplify for now
- remove unused Version enum and IntoVersion trait
- update versioned types
- Update RFC index
- Specify deployment environment for Playground (#142)
- Update README references
- Parse diagnosis reports into matrix
- remove JSON-RPC from 0.2.0 snapshot
- Generate diagnosis matrix
- align spec to actual implementation
- Remove RFC 0011-simple-group-chat
- Rfc skill (#131)
- small cleanups extracted from #96 prep work (#124)
- add WELL_KNOWN_CHAINS constants, use in examples (#128)
- Remove RFC 0010-get-root-account
- Notes from 12.05 Working Group Review
- Update 0020-create-transaction.md
- Create 0020-create-transaction.md
- Add Rust trait update checklist item to RFC PR template
- Auto-number RFCs on merge via CI
- complete withChainHeadFollow on Stop instead of erroring
- scope withChainHeadFollow subscriptionId per subscription
- extract withChainHeadFollow, drop `any` from examples, fix GenericError wire
- cargo-doc: serve static.files at site root so rustdoc CSS resolves (#119)
- fall back to ancestorOrigins when referrer is empty (#120)
- sync Cargo.toml version and auto-create GitHub Release (#113)
- render offline UI on GH Pages instead of stuck splash (#118)
- Playground: Monaco editor + rxjs + cargo-doc links + deep links (#116)
- Fix wire ID collision: shift CoinPayment IDs to 136+
- Rename listen_for to listen_for_payment
- RFC 0019: mark as breaking change
- Align RFC 0019 method names and error type with trait renames
- Rename push_notification methods for clarity
- Rename PushNotificationError to HostPushNotificationError
- RFC 0017: remove CoinPaymentInvoice type and align method names
- Remove version field from RFC pseudocode CoinPaymentCheque
- Fix codegen: collect error wrappers for ResultSubscription methods
- Address review comments: remove version field, error aliases, and Resolvable type
- Drop host_coin_payment_ prefix from CoinPayment trait methods
- move notification methods from System to Notifications trait
- implement RFC 0019 scheduled push notifications
- Remove unused PaymentPurse alias and CoinPaymentInvoice type
- Rename HostPaymentRequestRequest to HostPaymentRequest (#83)
- RFC 0017: add CoinPayment host API
- updated tokens with the ones from new design system

### Fixed

- match upstream ThemeName variant order (Custom before Default)
- keep version at 0.3.0 for release
- make PartialPayment top-up error a non-breaking append
- clean up cut-version.sh
- qualify method routes by service
- harden contiguity check and pin multi-version codec indices
- Update Paseo Next V2 Genesis hash
- make examples valid against the generated client
- remove unused HostCreateTransactionWithLegacyAccountRequest
- protocol document
- removed JAM codec mention
- restored indices
- display for RemotePermissionRequest
- fold RFC17 review cleanup
- rename host_push_notification_cancel to push_notification_cancel
- align CoinPayment trait with native async traits

### Removed

- roll back the CoinPayment (Coinage) host API
- remove Version::Next — unused until V2 types exist

## [0.1.0] - 2026-05-15

### RFCs

- **Accepted:** RFC Title
- **Accepted:** Permission Model for Host API
- **Accepted:** Payment Host API
- **Accepted:** RFC-0007: Deterministic Entropy Derivation for Products
- **Accepted:** Statement Store Host API v0.2
- **Accepted:** RFC-0009: Unauthenticated Product Access
- **Accepted:** RFC-0010: W3S Allowance Management in TrUAPI
- **Accepted:** Host API root account access
- **Accepted:** Simple Group Chat
- **Accepted:** RFC-0015: Get User Primary DotNS Name
- **Accepted:** Scheduled Push Notifications

### Added

- reject wire ids that collide with RESERVED_WIRE_IDS

### Changed

- @parity/truapi 0.1.0 — drop --ignore-scripts from install (#91)
- @parity/truapi-0.1.0 (#89)
- @parity/truapi-0.1.0
- Add release template
- Replace release bot with [release: PR title] gate
- Add release workflow to publish @parity/truapi via npm_publish_automation
- ignore generated TS outputs in git (#73)
- Refactor codegen (#68)
- remove public_key from HostGetUserIdResponse
- parse version from type prefix instead of hard-coding V01
- drop "0." from protocol version label
- rewrite to match actual repo structure and RFC CI requirements
- update generated types after macro doc comment changes
- address review: future-proof docs, auto-generate versioned wrapper doc comments
- drop v02 module: merge all types into v01, remove codegen discriminant hack
- tighten SubscriptionError assertions on malformed-receive and provider-close
- collapse observer error to single SubscriptionError type
- fix fmt and regenerate TS client
- Update rust/crates/truapi/src/api/calls.rs
- Update rust/crates/truapi-codegen/src/typescript.rs
- Bump next from 15.5.15 to 15.5.18 in /playground
- rename chainHeadFollow → chainHeadFollowSubscribe
- fix client example return types and regenerate
- regenerate TS client and examples
- fix legacy sign-payload example and fmt
- add V2 HostSignPayloadWithLegacyAccountRequest
- truapi-codegen: emit HexString import in generated client.ts
- add JsonRpc, Theme, ResourceAllocation traits + host_request_login
- add remote_preimage_submit + statement_store_create_proof_authorized
- add host_sign_*_with_legacy_account (wire 34–37)
- rename remote_chain_head_follow → remote_chain_head_follow_subscribe
- fix fmt
- drop host_chat_create_simple_group entirely
- move host_chat_create_simple_group off colliding wire ID 130
- emit plain HexString name + drop dead Uint8Array parsers
- update readme
- update
- align with host-product-sdk via HexString codec, drop dead helpers
- Require truapi interface changes in RFC PRs
- Add RFC validation CI workflow
- Rename deploy-playground CI file
- Fix submodule: recursive for workflows
- PR review
- simplify wire-table
- @parity/truapi: drop unused encodeWireMessage/decodeWireMessage from public surface
- rename /page diagnostics route to /diagnostics
- @parity/truapi: add publish metadata and dispose() handle
- tighten codegen and add v02 RemotePermission::PreimageSubmit
- fix ci
- fixes
- update types
- update
- fixes
- Address PR review findings
- add back doc site
- nit
- fixes
- renaming
- renaming
- rename stuff
- Address PR review findings
- updat rust code
- fix
- Fixes in RFC
- Propagate Rust doc comments to generated TS client
- Tighten review nits: deploy concurrency, BigInt regex, format args
- Pick V1 wrapper in codegen so legacy hosts decode every method
- Auto-respond to host_handshake_request in @truapi/client
- Fix handshake_response payload so the legacy decoder accepts it
- Answer the host's handshake_request to end the retry loop
- Pin handshake to V1 so legacy host-api accepts it
- Make playground reachable when no host is responding
- Add dotli submodule and top-level CLAUDE.md
- Surface subscription id; restore chain-head ephemeral-follow logic
- Add RFC 0012 for scheduled push notifications
- Promote truapi crate, add codegen, drop legacy docsite
- Add dev server proxy for legacy URL redirects
- Prepare for repo rename from truapi-explorer to truapi
- Align v0.2 API definitions with triangle-js-sdks implementation
- Migrate RFC-0014 (Get User Primary DotNS Name) as RFC-0015
- Fix TopicFilter to enum with MatchAll/MatchAny variants
- RFC-0011: Simple Group Chat
- Update RFC index with 0010-allowance entry
- RFC-0010: W3S Allowance Management in TrUAPI
- Migrate feature index and accepted RFCs from triangle-js-sdks
- Migrate PR templates and CONTRIBUTING guide from triangle-js-sdks
- Migrate host-api-protocol design doc from triangle-js-sdks
- Update Contacts API note in v02-changes.md
- Add draggable sidebar resizer with persisted width and double-click reset
- Bump vite to 8.0.8 to patch dev-server path traversal and file-read advisories
- Fix TypeScript narrowing in Fields and Variants map callbacks
- Use CSS grid for Fields and Variants tables to align columns across rows
- Fix long variant/field names overlapping right column on type pages
- Redirect legacy /host-api-explorer URLs to /truapi-explorer
- Promote v0.2 from preview to stable and make it the default version
- Update v02-changes.md with additional document links
- Add README.md
- Add Rust docs and v0.2 change doc
- Add v02 spec
- Change api-spec to v02
- Correct the vite path name
- Rename more thoroughly
- Rename host api to truAPI and add truapi-spec
- Bump brace-expansion
- Bump picomatch from 4.0.3 to 4.0.4
- Bump flatted from 3.4.1 to 3.4.2
- Make mobile friendly
- Clean up types (2)
- Clean up types
- Link types properly (2)
- Link types properly
- Improve readability
- UI improvements
- Replace iframe terminology with sandbox
- Fix GitHub Pages SPA routing
- Remove unused variable in TypesPage
- Add .npmrc to resolve Vite 8 / Tailwind peer dep conflict
- Initial commit: Host API Protocol Explorer

### Fixed

- clippy needless_borrow and stale type import

