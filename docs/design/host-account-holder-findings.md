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
| 12. Workers | `native/workers.rs` derives one worker per product from durable reasons/operations and live references, refreshes content hashes, defers updates during operations and retries engine failures. |
| 13. iOS products | SPA execution, chat views, worker engines, catalog, permission settings and notification callbacks select Rust when enabled. Chat views share the supervisor's execution. |
| 14. iOS wallet | The runtime provider activates by protected wallet identifier. Pairing/SSO, renewal, device administration and reset use Rust; protected-data lock clears wallet memory. |
| 15. Android products | Product scripts, browser/chat views and workers use Rust executions. Catalog, permission settings, integrations, operations and notification callbacks use Rust records. |
| 16. Android wallet | The runtime provider rebuilds on owner change. SSO, renewal, device administration, lock and reset use Rust when enabled. |

Rust paths above are relative to `rust/crates/truapi/src`. Native adapters live in `ios/truapi-host`, `android/truapi-host` and the embedding apps under `hosts/ios` and `hosts/android`.

## Decisions and spec conflicts

- Delivery is one experimental draft PR. The supplied stacked-PR wording and worker first-launch import paragraph are overridden. Received copies remain under `.agent/handoff/host-account-holder/spec-snapshots/` for comparison. There are no legacy-data importers.
- The account public key selects `<database_directory>/<owner hex>/core.sqlite3`. A native facade binds to its first owner and a switch creates another facade. Product executions retain the store token from their activation; reopening the same owner cannot revive an old execution's writes.
- One installation encryption key lives in `SecretCoreStorage`. Initialization is serialized across concurrent database opens. Product/core payload envelopes use ChaCha20-Poly1305, fresh nonces, a version and owner/table/row associated data. Wallet roots and delegated private keys stay outside SQL.
- Pairing orchestration belongs to Rust transport services and captures one wallet activation throughout provisioning, handshake and failure cleanup. The wallet owns approval and allocation, while replay locks and request cancellation stay outside it.
- Notification rows include state and revision, and IDs use a monotonic sequence. OS callbacks answer whether each record is still pending, which works with Android AlarmManager without a second native journal. OS identifiers include the native owner. Cancellation must succeed before product/account deletion; lock retains records for later restoration.
- Worker bundles remain content-addressed files. A cached-bundle callback can open a prior hash while an operation delays an update. Native file caches retain immutable manifest metadata needed to open that bundle. This metadata does not authorize wallet operations.
- Published DotNS workers and catalog overrides to DotNS bundles are supported. Arbitrary HTTP development scripts and iOS manual debug-script downloads fail explicitly on the enabled Rust path because they have no immutable bundle downloader. The disabled legacy path remains available. This is a prototype limitation, not an implicit fallback.
- Worker update eligibility is 24 hours, checked at activation/foreground and periodically while active. Engine failures use capped exponential retry. Native lifecycle and engine-stop callbacks are asynchronous so account switching awaits OS teardown without blocking the UI thread.
- The imported iOS app has no Pocket UI or native Pocket adapter. Its worker engine advertises no Pocket callback. Android routes its existing Pocket integrations and render streams through the shared worker.
- Rust's platform storage traits return the specified `GenericError`; UniFFI callbacks use `HostRejection` and map its reason at the boundary because foreign callback errors require an error enum.
- The SSO resource vector still carries one allocation policy. Ignore without a retained key versus Increase with one remains unresolved for mixed vectors. This experiment retains the existing wire and consent batching policy; it does not add a field or split consent silently.
- RFC7 local derivation from purpose-specific entropy, RFC22/24 derivation and ring entropy, and the tested blessed-product denial behavior retain their contracts. Remote requests never inherit native approval from a product name.

## Verification

- Focused checks pass: storage 12 tests, runtime operation coverage 247 tests, wallet operations 140 tests, native/remote isolation 12 tests, protected grants 8 tests, native integration 93 tests, CLI signing/pairing processes 16 tests, CLI platform 20 tests, and browser callback/worker/mock coverage 113 tests. Final workspace validation can subsume these overlapping suites.
- Protocol-only compilation, generated callback golden tests, production/test WASM target compilation, TypeScript builds and Swift/Kotlin binding generation pass.
- Android SDK, binding instrumentation sources, product/SSO sources, 316 product unit tests and the full `compileVanillaDebugKotlin` app/Dagger graph pass. The local command omits Google Services processing because this checkout has no `google-services.json`; it is a source compile, not an installable APK or device run. Existing Android deprecation and experimental Kotlin feature warnings remain outside this change.
- Full workspace tests, warning-denied clippy, packaged WASM/browser checks and final formatting are in progress. Their final results will be recorded before delivery.
- Linux cannot run Xcode, an iOS simulator or the embedding app's Swift typecheck. Swift consumers are updated and bindings are generated, but those platform checks remain required before treating the experiment as merge-ready.

Detailed command logs and work state are under `.agent/validation/`, `.agent/worklog/` and `.agent/STATUS.md`.
