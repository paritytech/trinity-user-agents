# Experimental host/account-holder implementation

This experiment implements the accepted ownership model on one branch and one draft PR. Native and paired products use concrete `HostAccounts<WalletAccountHolder>` and `HostAccounts<SsoAccountHolderClient>` services. Incoming SSO invokes the shared wallet holder directly, with independent wallet authorization. It cannot read or populate native host grants.

## Milestone map

| Milestone | Implementation |
| --- | --- |
| 1. Wallet holder | `runtime/authority.rs` defines domain operations and activation-bound authorization; `runtime/signing_host.rs` implements `WalletAccountHolder`. |
| 2. SSO boundary | `runtime/pairing_host.rs` implements the client; `runtime/signing_host/sso_service.rs` dispatches through the existing SSO macro to the wallet. Each service is bound to its wallet activation. |
| 3. Shared host accounts | `runtime/host_accounts.rs`, `host_core.rs`, `truapi_core.rs`, CLI frame dispatch and WASM composition carry the concrete account-holder generic. ProductAuthority is absent. |
| 4. Protected secrets | `platform/secrets.rs` defines typed asynchronous secret storage. Native Keychain/Keystore adapters, CLI storage and browser callbacks implement the same error and persistence contract. |
| 5. Wallet initialization | `native/storage.rs` separates host secrets from the wallet-only root provider. `prepare_wallet` returns an opaque activation with a public owner; native activation opens that owner's database before publishing the wallet. |
| 6. Durable grants | `runtime/host_accounts.rs` owns protected grants, owner/session validation, local delegated execution and serialized invalidation. Native grants survive lock; wallet authorizations do not. |
| 7. Encrypted records | `store/runtime_store.rs` encrypts product/core payloads and implements committed subscriptions over `Db`. Each account has its own database and activation token. Product removal revokes execution tokens before asynchronous cleanup and retains that fence until cleanup succeeds. |
| 8. Paired devices | `store/records.rs` owns paired-host and metadata rows. Pairing and native roster/removal APIs use them. |
| 9. Allocation journal | `runtime/signing_host.rs` and `signing_host/allowance_renewal.rs` persist allocation records and detailed statement slots. Renewal preserves each slot's priority and period. |
| 10. Product catalog | `RuntimeStore` owns product metadata and record subscriptions. Manifest resolution remains the authorization source. |
| 11. Notifications | `native/notifications.rs` persists registration/cancellation intent before OS work, reconciles actual pending delivery and retains failed work for retry. |
| 12. Existing worker callers | Native worker managers retain lifecycle ownership. Android retains native integration and operation persistence; iOS chat workers use Rust executions, with durable operation integration tracked separately below. |
| 13. iOS products | SPA execution, chat and worker product calls, catalog, permission settings and notification callbacks select Rust when enabled. Native managers retain worker scheduling and engines. |
| 14. iOS wallet | The runtime provider activates by protected wallet identifier. Pairing/SSO, renewal, device administration and reset use Rust; protected-data lock clears wallet memory. |
| 15. Android products | Product scripts, browser/chat views and workers use Rust executions. Catalog, permission settings and notification callbacks use Rust records; worker integrations and operations stay native. |
| 16. Android wallet | The runtime provider rebuilds on owner change. SSO, renewal, device administration, lock and reset use Rust when enabled. |

Rust paths above are relative to `rust/crates/truapi/src`. Native adapters live in `ios/truapi-host`, `android/truapi-host` and the embedding apps under `hosts/ios` and `hosts/android`.

## Decisions and spec conflicts

- Delivery is one experimental draft PR focused on host/account-holder separation. Worker lifecycle refactoring and legacy-data migration are outside its scope.
- The account public key selects `<database_directory>/<owner hex>/core.sqlite3`. A native facade binds to its first owner and a switch creates another facade. Product executions retain the store token from their activation; reopening the same owner cannot revive an old execution's writes.
- One installation encryption key lives in `SecretCoreStorage`. Initialization is serialized across concurrent database opens. Product/core payload envelopes use ChaCha20-Poly1305, fresh nonces, a version and owner/table/row associated data. Wallet roots and delegated private keys stay outside SQL.
- Pairing orchestration belongs to Rust transport services and captures one wallet activation throughout provisioning, handshake and failure cleanup. An owner-scoped cleanup intent is committed before tracking a new peer; interrupted attempts remove abandoned targets and statement records on disconnect or before reopening publishes the wallet. Completed pairings and preexisting slots are preserved. The wallet owns approval and allocation, while replay locks and request cancellation stay outside it.
- Notification rows include state and revision, and IDs use a monotonic sequence. OS callbacks answer whether each record is still pending, which works with Android AlarmManager without a second native journal. OS identifiers include the native owner. Cancellation must succeed before product/account deletion; lock retains records for later restoration.
- Native worker managers remain responsible for scheduling and engines. Android retains native integration and operation persistence. Host/account-holder initialization and account calls are adapted without introducing a Rust supervisor or worker tables.
- Native activation, lock and shutdown remain asynchronous for wallet cleanup, storage and notification reconciliation. This does not transfer native worker lifecycle policy to Rust.
- Rust's platform storage traits return the specified `GenericError`; UniFFI callbacks use `HostRejection` and map its reason at the boundary because foreign callback errors require an error enum.
- The SSO resource vector still carries one allocation policy. Ignore without a retained key versus Increase with one remains unresolved for mixed vectors. This experiment retains the existing wire and consent batching policy; it does not add a field or split consent silently.
- RFC7 local derivation from purpose-specific entropy, RFC22/24 derivation and ring entropy, and the tested blessed-product denial behavior retain their contracts. Remote requests never inherit native approval from a product name.

## iOS worker-operation scope boundary

The enabled iOS Rust path retains the SDK's process-local operation IDs and no-op end callback. These do not persist operations or keep native engines alive after chat teardown. The legacy operation reconciler runs only with Rust disabled because its factory constructs `ProductsNativeApi`; forwarding Rust operations to it would start a second worker using legacy account handling. Sharing the existing native worker between Rust chat and durable operations requires an execution adapter for typed rendering and messaging context, and remains a separate worker integration task. Chat engines are recreated through the existing extension store when the wallet runtime changes.

## Verification

The local branch without the worker refactor passes 1,898 Rust workspace tests, with 21 existing ignored cases. Warning-denied Clippy, nightly formatting, codegen/golden tests, TypeScript builds and fresh Swift/Kotlin binding generation pass. Android production and instrumentation-test sources compile, and 34 focused Android unit tests pass. The Swift SDK and its default-operation/WebSocket test sources typecheck against fresh bindings. Full iOS app/simulator tests and Android device instrumentation have not run on this local revision.

### Original experiment

The results below were recorded for commit `8fa52575658261da51e70e9a43d2dc55ca3402d9`, which included the separate worker refactor. They are historical evidence, not validation of the narrowed local changes.

- Focused checks cover storage encryption and isolation, protected grant reuse, wallet authorization, native/remote isolation, owner changes and stale completion fencing. Paired-session regressions verify that own writes and duplicate notifications preserve a committed login while external replacement/logout suspend access and reject old work. These checks are included in the workspace suite.
- Protocol-only compilation, generated callback golden tests, production/test WASM target compilation, TypeScript builds and Swift/Kotlin binding generation pass.
- Android SDK, binding instrumentation sources and the full `compileVanillaDebugKotlin` app/Dagger graph pass against the final generated bindings. All 422 affected unit tests pass (319 product, 10 SSO and 93 common), with no skips. The local command omits Google Services processing because this checkout has no `google-services.json`; it is a source compile, not an installable APK or device run. Existing Android deprecation and experimental Kotlin feature warnings remain outside this change.
- Both dev-profile WASM bundles build. Host browser tests pass 301, client tests 280, shared container tests 151 and packaged CLI runner tests 3. One release-sidecar packaging check is intentionally skipped for dev-profile WASM, which produces no compressed sidecars. Host/client TypeScript builds and the host harness typecheck pass.
- Live Dotli/CLI pairing, host sign-out and same-account reconnect pass against the prepared browser companion. Full diagnosis reports 54 successful method groups and 17 failures: 14 existing payment/WebRTC exclusions, unsupported contacts picking and two raw-sign examples that require a nonempty legacy-account list. Baseline comparison confirms contacts picking and empty legacy enumeration are unchanged. The diagnosis keeps its nonzero exit status; expectations are not relaxed. Allocation, delegated signing, ring-VRF, storage, notifications and statement operations pass.
- Mixed-version pairing, host sign-out and same-account reconnect also pass with the current browser and the installed 0.17.0 CLI signer, with no page errors. The signer uses isolated test state with its updater disabled; its binary checksum is unchanged.
- The all-target/all-feature Rust workspace build, nightly formatting and warning-denied Clippy pass. Workspace tests pass 1,905 with 24 existing skips for live-network, optional launcher and documentation cases. The launcher isolation case is run separately with Bun and passes. Bindgen is built/tested separately to avoid duplicate outputs; protocol-only compilation and warning-denied Rust documentation also pass. Native checks use Rust 1.98.1 because the baseline SQLite dependencies exceed the installed default 1.92 toolchain. Dependency future-incompatibility notices remain, with no project warning suppression.
- Android CI passes SDK, app/unit-test and detekt jobs. Linux cannot run Xcode or an iOS simulator locally. macOS CI compiles the Swift SDK and embedding app; six real WKWebView tests and 2,581 app tests pass. The iOS workflow also checks new Swift warnings and builds a simulator preview; current workflow status is available in the draft PR checks.

Detailed command logs and work state are under `.agent/validation/`, `.agent/worklog/` and `.agent/STATUS.md`.

## Outstanding browser consumer integration

The pinned Dotli submodule lacks SecretCoreStorage and has protocol skew predating this branch. A four-file companion implementation is prepared against Dotli main `5d9b8b5339b6e0634df2e63f57a00864ebb9f812`; its three-app build, UI source/test typecheck, 23 storage tests and focused lint pass. It retains the existing shared-auth policy for AuthSession and protects other secrets with the existing nonextractable IndexedDB AES-GCM key, fresh nonces and typed-key associated data. The companion is not published or pinned in this superproject. Review patches and the exact commit are preserved under `.agent/handoff/host-account-holder/`. The production browser consumer is not delivered until this integration is pinned.
