# Host accounts and account holders

- The product host obtains allowance and delegated signing keys from the account holder, stores them securely, and uses them to sign transactions and other product payloads.
- The account holder owns wallet secrets, authorizes wallet operations and derives the keys it gives to the host.
- Native and paired hosts share the same host implementation. Their account holder is local or reached through SSO.

## Scope

- Share host account logic in one `HostAccounts` implementation.
- Introduce `AccountHolder`, implemented by `WalletAccountHolder` for native hosts and `SsoAccountHolderClient` for paired hosts.
- Introduce `SecretCoreStorage` for persistent host secrets, including native allowance keys, using iOS Keychain and Android's encrypted preferences.
- Own native runtime records and product data in Rust SQLite.
- Route native product and account-holder operations through Rust whenever the TrUAPI runtime feature is enabled.

The separation applies to all host-managed account operations. Internal Rust interfaces and SDK construction may change; update their consumers directly. Storage migration and compatibility with old persisted formats are out of scope.

## Architecture

```text
                 Paired product execution                                       Native product execution
                            |                                                                 |
                            |                                                                 |
+---------------------------+---------------------------+     +-------------------------------+-------------------------------+
| PairingHostRuntime        |                           |     | SigningHostRuntime            |                               |
|                           |                           |     |                               |  Activation / lifecycle       |
| +-------------------------v-------------------------+ |     | +-----------------------------v-----------------------------+ |
| | ProductRuntimeHost                                | |     | | ProductRuntimeHost                                        | |
| | Per product connection                            | |     | | Per product connection                                    | |
| +---------------------------------------------------+ |     | +-----------------------------------------------------------+ |
|                           |                           |     |                               |                               |
|                           |                           |     |                               |                               |
| +-------------------------v-------------------------+ |     | +-----------------------------v-----------------------------+ |
| | HostAccounts<SsoAccountHolderClient>              | |     | | HostAccounts<WalletAccountHolder>                         | |
| | One instance per runtime                          | |     | | One instance per runtime                                  | |
| | Owns grants, permissions and host caches          | |     | | Owns grants, permissions and host caches                  | |
| |                                                   | |     | |                                                           | |
| | secret access -> SecretCoreStorage                | |     | | secret access -> SecretCoreStorage                        | |
| | runtime records -> Rust repositories              | |     | | runtime records -> Rust repositories                      | |
| | holder: Arc<SsoAccountHolderClient>               | |     | | holder: Arc<WalletAccountHolder>                          | |
| |                                                   | |     | |                                                           | |
| +---------------------------------------------------+ |     | +-----------------------------------------------------------+ |
|                           |                           |     |                               |                               |
|                           |                           |     |                               +------------------------+      |
| +-------------------------v-------------------------+ |     | +---------------------------------------------+        |      |
| | SsoAccountHolderClient: AccountHolder             | |     | | SsoAccountHolderService                     |        |      |
| | Forward wallet requests / return results          | | SSO | | Dispatch authenticated remote requests      |        |      |
| |                                                   ---------->                                             |        |      |
| +---------------------------------------------------+ |     | +---------------------------------------------+        |      |
|                                                       |     |                        |                               |      |
| Pairing/session lifecycle                             |     |   remote               |                        local  |      |
+-------------------------------------------------------+     |                        |                               |      |
                                                              | +----------------------v-------------------------------v----+ |
                                                              | | WalletAccountHolder: AccountHolder                        | |
                                                              | | Shared by native host and incoming SSO                    | |
                                                              | | Owns active root entropy and wallet approval              | |
                                                              | | Derives keys; executes wallet operations                  | |
                                                              | | Wallet secret access is bound to activation               | |
                                                              | |                                                           | |
                                                              | +-----------------------------+-----------------------------+ |
                                                              |                               |                               |
                                                              +-------------------------------+-------------------------------+
                                                                                              |
                                                                                              | wallet-only access
                                                                +-----------------------------v-----------------------------+
                                                                | Native wallet secret storage                              |
                                                                | Existing protected root-entropy store                     |
                                                                +-----------------------------------------------------------+

Storage behind each host's adapters:
  SecretCoreStorage -> Keychain / encrypted preferences (native), browser / CLI backend
  Rust repositories -> core.sqlite3 (native), browser storage adapters (WASM)
HostAccounts shares implementation across runtimes; each instance owns separate state.
```

| Component | Responsibility |
| --- | --- |
| `ProductRuntimeHost` | Bind the calling product and execution context; adapt TrUAPI calls. |
| `HostAccounts` | Enforce product access, acquire and retain grants, execute delegated operations, and own host caches. |
| `AccountHolder` | Define the wallet operations needed by the host, independently of transport and host caches. |
| `WalletAccountHolder: AccountHolder` | Own active wallet secrets, authorize wallet operations, derive keys and issue capabilities. |
| `SsoAccountHolderClient: AccountHolder` | Translate holder calls and results through the existing SSO protocol. |
| `SsoAccountHolderService` | Dispatch authenticated remote requests directly to the wallet. |
| `SecretCoreStorage` | Persist host secrets through protected platform adapters. |
| Rust repositories | Own native runtime records and product data in SQLite; retain browser storage adapters for WASM. |

### Rust composition

- Implement `HostAccounts<H: AccountHolder>` with an `Arc<H>` dependency. Native runtimes use `HostAccounts<WalletAccountHolder>`; paired runtimes use `HostAccounts<SsoAccountHolderClient>`.
- Put interchangeable behavior behind `AccountHolder`. Keep grant acquisition and caching in the shared concrete service.
- A separate `HostAccounts` trait is unnecessary while there is one host-account implementation. Split its internals by responsibility instead.
- Each runtime owns its own host state. Shared implementation does not mean shared grants, permissions or caches between hosts.
- Carry `H` through `ProductRuntimeHost` and its internal owners. Keep UniFFI objects concrete; use the existing dispatcher boundary to erase execution types. Native host calls and inbound SSO share one `Arc<WalletAccountHolder>`.

### Runtime roles

- `PairingHostRuntime` composes `HostAccounts`, `SsoAccountHolderClient` and pairing lifecycle.
- `SigningHostRuntime` composes `HostAccounts`, `WalletAccountHolder`, wallet activation and optional `SsoAccountHolderService`.
- Both facades retain host lifecycle and administration. These do not belong in `AccountHolder`.
- Local product calls and inbound SSO may share the wallet instance. Inbound SSO bypasses native `HostAccounts`: remote operations cannot consume or populate its grants or inherit local product permissions.

### Native feature selection

- With the TrUAPI runtime feature enabled, product executions and account-holder entry points use Rust, including background operations and incoming pairing.
- Select the same runtime for browser/SPA, chat and worker calls, pairing/SSO, allowance renewal, permission administration and runtime-state cleanup.
- Swift/Kotlin supply approval UI, OS services, wallet activation and protected secret storage. They do not run legacy account logic or consult a second permission/grant database on the Rust path.
- A Rust startup or execution error is reported; it must not silently fall back to the legacy implementation. Feature-disabled behavior remains available.
- Resolve the choice consistently at runtime startup and stop the previous runtime's listeners/jobs before changing it. Do not run both renewal or SSO handlers against the same account.

## Host and wallet boundary

- `HostAccounts` owns product authorization and delegated execution. `WalletAccountHolder` owns wallet approval, derivation and wallet execution.
- Calls carry trusted invocation context, including product identity where available, local/remote origin, cancellation and the active owner/session.
- `AccountHolder` uses canonical domain types. SSO envelopes and transport state stay in the client/service and transport layer.
- Allocation results distinguish exported signing material from activation-bound wallet authorization. The latter is not a persistent private-key grant.
- Root entropy stays in `WalletAccountHolder`. Any purpose-specific entropy supplied to the host does not let it derive allowance keys.

### Shared grant flow

1. `HostAccounts` checks access and looks for a grant matching the active owner, product and resource.
2. On absence, it requests a grant through `AccountHolder`: directly on native hosts, through SSO on paired hosts.
3. It validates the result and persists eligible secrets before reporting success.
4. It executes with the retained capability. Later calls, including after a native restart, reuse it without requesting the secret again.

Key retention and allowance readiness are separate. Wallet-dependent checks and renewal stay in `WalletAccountHolder`; cached paired use adds no SSO round trip. Explicit allocation still reaches the holder. Host execution chooses between a retained capability and a wallet operation before requesting approval.

## Persistence

- Reuse the existing Rust `Db` and native `core.sqlite3`. Its production schema is currently empty; the tables in the [storage inventory](host-storage.md) are part of this implementation.
- Rust owns the repositories, transactions, subscriptions and cleanup. Native runtime state no longer goes through Swift/Kotlin core/product-storage callbacks. Settings and administration read and change it through Rust APIs.
- SQLite replaces CoreData, Room and preferences for the listed runtime records and product data, preserving existing payload encryption. Host private keys remain behind `SecretCoreStorage`.
- Keep browser persistence behind its existing adapters; native SQLite is not a new WASM dependency. Worker tables follow the separate [worker design](core-owned-workers.md).

### Secret storage

Add `SecretCoreStorage` beside `CoreStorage`, with typed `SecretCoreStorageKey` values and asynchronous callbacks:

| Operation | Contract |
| --- | --- |
| `read_secret_core_storage(key)` | `Result<Option<Vec<u8>>, GenericError>`; only `None` means absent. |
| `write_secret_core_storage(key, value)` | `Result<(), GenericError>`; success requires a committed persistent write. |
| `clear_secret_core_storage(key)` | `Result<(), GenericError>`; success requires persistent removal; absence is harmless. |

Rust owns encoding, scope and lifetime. Platform adapters protect and store bytes. Inaccessible or corrupt storage returns an error, never a cache miss or fallback to ordinary storage.

| Platform | Backend |
| --- | --- |
| iOS | Existing Keychain integration and accessibility policy. |
| Android | Existing encrypted preferences protected by Android Keystore, with committed writes and error propagation. |
| CLI | Existing persistent local storage; no OS keyring required. |
| Browser/WASM | Existing browser persistence and protection, exposed through the separate secret-storage interface. |

### Ownership and lifetime

- Route delegated keys, secret-bearing sessions and device secrets through `SecretCoreStorage`. Keep public caches, permissions and other nonsecret state in the Rust SQLite repositories on native hosts. Wallet root storage remains separate.
- Scope native allowance records by stable wallet identity, canonical product and resource. Use capability-specific owner/session binding for paired grants.
- Retain native allowances across restart and wallet lock. Require the matching active owner before use; local wallet authorizations expire with activation.
- Lock, owner changes and reset invalidate in-flight operations. Serialize grant writes and clearing so late results cannot restore revoked state.
- Product/account reset clears the relevant persisted grants. Report storage failures; never claim successful persistence or cleanup when it failed.

## Implementation milestones

Deliver these milestones on one experimental branch and one draft PR. Keep interface changes and their callers together in compiling commits. The experiment is the basis for reviewing and revising the design.

Use the existing native SQLite foundation (`Db`, database-directory configuration and `core.sqlite3`). Include any missing [worker lifecycle prerequisites](core-owned-workers.md) in this experiment, using the same account services and databases.

| Milestone | Review scope | Completion check |
| --- | --- | --- |
| 1. Wallet account holder | Extract `AccountHolder` and `WalletAccountHolder`; move wallet approval, derivation and execution out of `SigningHost`. | Existing local wallet operations execute through the new holder. |
| 2. SSO account holder | Extract `SsoAccountHolderClient` and `SsoAccountHolderService`; dispatch incoming SSO directly to the wallet. | Existing SSO messages interoperate; remote requests cannot inherit native grants or permissions. |
| 3. Shared host accounts | Introduce `HostAccounts<H>` and compose it into both runtimes and product adapters, including CLI and WASM; remove `ProductAuthority`. | Native and paired hosts use the same host logic while retaining separate state. |
| 4. Secret storage | Add `SecretCoreStorage`, typed keys, bindings and platform backends; move existing runtime secrets behind it. | iOS uses Keychain, Android uses encrypted preferences, and storage failures propagate; CLI and browser adapters remain functional. |
| 5. Wallet initialization | Replace entropy in runtime configuration with a wallet-only secret provider; activate by wallet identifier and update native callers. | Runtime creation needs no entropy; activation loads the selected wallet, and lock/switch clears its in-memory secrets. |
| 6. Durable host grants | Persist native allowance and delegated signing keys; bind reuse and cleanup to the correct owner, product and resource. | Grants survive restart; inactive owners and late results cannot restore or use invalidated grants. |
| 7. Product and core SQLite storage | Add `product_storage` and `core_state`; switch native adapters to Rust repositories with payload encryption and subscriptions. | Data survives reopen, cross-product access stays authorized, and only encrypted payloads reach these tables. |
| 8. Paired-device records | Add `paired_hosts` and `paired_host_metadata`; expose Rust roster and lifecycle APIs. | Pairing state, metadata and sync progress survive restart and can be removed together. |
| 9. Allowance journal | Add `allowance_records` and `statement_slots`; connect wallet allocation and renewal to the Rust journal. | Renewal uses persisted ownership, per-slot priority and timing without a competing native journal. |
| 10. Product catalog | Add `products` and Rust catalog query/update APIs. | Product metadata survives restart; authorization still uses the manifest. |
| 11. Notification records | Add `scheduled_notifications` and connect Rust ownership/reconciliation to native OS scheduling callbacks. | Registration and cancellation failures retain enough state to retry correctly. |
| 12. Worker integration | Connect host accounts, product records and reset operations to the worker repositories and lifecycle APIs. | Workers use the same account services and database; Android operations may omit labels. |
| 13. iOS product routing | Route enabled SPA/browser, chat and worker entry points through Rust; connect product permissions, catalog and notifications to Rust APIs. | Every product execution uses Rust; startup errors surface without a Swift fallback. |
| 14. iOS wallet routing | Route pairing/SSO, allowance renewal, wallet administration and cleanup through Rust; stop superseded native handlers/jobs. | The enabled iOS runtime has one owner for account operations and persistence, including across lock, restart and reset. |
| 15. Android product routing | Route enabled SPA/browser, chat and worker entry points through Rust; connect product permissions, catalog and notifications to Rust APIs. | Every product execution uses Rust; startup errors surface without a Kotlin fallback. |
| 16. Android wallet routing | Route pairing/SSO, allowance renewal, wallet administration and cleanup through Rust; stop superseded native handlers/jobs. | The enabled Android runtime has one owner for account operations and persistence, including across lock, restart and reset. |

Remove superseded enabled paths with their caller changes. Each platform requires both product and wallet routing; feature-disabled behavior remains available. Persisted-data migration stays out of scope.

The completion checks describe outcomes, not a quota of new tests. Reuse existing suites and add coverage only for a new failure mode or a demonstrated gap. Keep validation with the code that changes the behavior; do not add tests solely for extraction, forwarding or type layout.

## Definition of Done

- Native and paired paths use the same host logic with different account holders.
- With the native feature enabled, Rust owns execution and runtime persistence; legacy host/account-holder code and databases are not used for those operations.
- Remote operations remain isolated from native host grants and permissions.
- Native allowance reuse survives restart; inactive owners cannot use stored grants, and stale work cannot undo a successful reset.
- Existing SSO wire fixtures and host/holder interoperability checks pass.
- Rust, browser, CLI, native bindings, SDKs and embedding apps pass their required build/test checks; formatting and runtime documentation are current.

Use the repository's [build instructions](../../AGENTS.md) and [end-to-end testing guide](../local-e2e-testing.md) for validation commands.

## References

- [Allowance-management RFC](../rfcs/0010-allowance.md).
- [Current runtime](../../rust/crates/truapi/RUNTIME.md) and [authority interface](../../rust/crates/truapi/src/runtime/authority.rs).
- [iOS allowance storage](../../hosts/ios/Packages/Products/Sources/Products/Services/ProductResourceKeyManaging.swift) and [Android allowance storage](../../hosts/android/feature/products/impl/src/main/java/io/paritytech/polkadotapp/feature_products_impl/domain/hostApi/allowance/RealAllowanceKeyStorage.kt).
