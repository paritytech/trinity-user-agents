# Native host storage

Part of the [host/account-holder specification](host-account-holder.md). Scope: TrUAPI, pairing and account-holder runtime state. App-only chat history, payment records, browser tabs and native worker lifecycle stores remain outside this refactor. Existing data migration is not required.

## Ownership

- Native Rust owns runtime records and product data in the existing `core.sqlite3`. Reuse `Db` and version schemas through `core_migrations()`.
- Swift/Kotlin provide the database directory, protected secret storage and OS services. They do not keep a second authoritative database for the records transferred to Rust. Existing worker integration and operation stores remain native.
- `SecretCoreStorage` retains delegated keys, session secrets and device keys in Keychain or Android's Keystore-protected encrypted preferences. Root wallet secrets stay in the wallet's existing secure-storage boundary.
- Runtime settings screens use Rust query/update APIs. Product-storage changes are published to all executions after the database transaction commits.

## Existing stores and their replacements

CoreData entities are prefixed `CD`; Android entities below use Room unless preferences are named. This inventory maps native stores to the Rust repositories used when the feature is enabled.

| State | iOS today | Android today | Rust owner/store |
| --- | --- | --- | --- |
| Product key/value data | `ProductsLocalStorage`; `TrUAPILocalStorage` product preferences | `ProductLocalStorage` preferences; `EncryptedTrUAPIStorage` product adapter | `product_storage`; product-owned bytes and Rust subscriptions. |
| Persistent permission decisions | `ProductPermissionRepository`, `CDProductPermissionGrant` | `ProductPermissionGrantLocal` | `core_state`, canonical `PermissionAuthorization` keys. One-time decisions stay in memory. |
| Paired devices and lifecycle | `CDPolkadotSignInHost` via `PolkadotSignInHostRepositoryFactory` | `SsoSessionLocal`, `SsoSessionMetadataLocal` | `paired_hosts`, `paired_host_metadata`; includes peer identity, status and sync progress. |
| Handled SSO requests | `CDSSOHandledRequest` | `SsoHandledRequestLocal` | `core_state`, existing wallet/peer-scoped `SsoResponderRequestLedger`, including Started/Completed and expiry. |
| Allowance allocation/renewal history | `CDAllowanceRecord` via `AllowanceRepositoryFactory` | `StatementStoreSlotAllocationLocal` | `allowance_records`, `statement_slots`; public account/slot records, not signing keys. |
| Renewal target recipes | Rust `StatementRenewalTargets` through the native core adapter | Same Rust slot; native allocator also uses the slot journal above | `core_state`, `StatementRenewalTargets`; keep recipes and fixed-account ownership distinct. |
| Ring-VRF registry | Rust `RingVrfRegistry` through the native core adapter | `RingVrfKeyRegistrationLocal` and the Rust core adapter | `core_state`, wallet-scoped `RingVrfRegistry`, including completeness and provider selections. |
| Public subtree cache, manifest cache, pairing cursor | Rust `ProductSubtree`, `ProductManifest`, `LastProcessedPairingStatement` through the native core adapter | Same Rust slots; native resolved-product cache is also in memory | `core_state`, the corresponding typed keys and canonical Rust records. |
| Product catalog/display metadata | `CDProduct` via `ProductRepositoryFactory` | `ProductLocal` | `products`; authorization continues to use the manifest, not catalog labels. |
| Installed product integrations and worker operations | `CDProduct`, `CDProductOperation`, `CoreDataProductOperationStore` | `ProductIntegrationLocal`, `ProductFundingOperationLocal` via `ProductOperationService` | Existing native stores; outside the host/account-holder refactor. |
| Scheduled product notifications | `CDScheduledNotification`; OS owns scheduled payloads | `ScheduledProductNotificationLocal`; AlarmManager | `scheduled_notifications`; Rust owns IDs, ownership, limits and reconciliation; native code schedules/cancels OS delivery. |
| Native allowance private keys | `ProductResourceKeyManager`, Keychain | `RealAllowanceKeyStorage`, encrypted `ALLOWANCE_KEY:*` preferences | `SecretCoreStorage`, scoped by wallet/product/resource. |
| Rust session and delegated secrets | Secret-bearing `CoreStorageKey` values through the generic adapter | Same, through `EncryptedTrUAPIStorage` | `SecretCoreStorage`: `AuthSession`, `PairingDeviceIdentity`, allowance keys, delegated signing keys and device encryption key. |
| Platform wallet/device secrets | `RootEntropyManager`, `DeviceEncryptionKeyManager`, Keychain | `RealAccountSecretsStorage`, `RealBandersnatchSecretsStorage`, `RealOurDeviceKeypairProvider`, encrypted preferences | Wallet root storage remains native; runtime device-key access uses `SecretCoreStorage`. No private bytes in SQLite. |

`ProductFundingOperationLocal` is the Android worker-operation ledger despite its name; it remains owned by the native worker manager. Shared device identities used by chat remain shared through the secret provider, not duplicated or removed by product cleanup.

## SQLite tables

These are target schema definitions, not implemented tables. `BLOB` stores canonical Rust encodings or fixed-byte identifiers; `TEXT` product IDs use the existing canonical normalization. Timestamps are integer Unix milliseconds. Nullable columns omit `NOT NULL`. Links below each definition identify the corresponding native stores; differences in granularity are noted.

```sql
-- Product-owned data, isolated by normalized product ID.
CREATE TABLE product_storage (
    product_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value BLOB NOT NULL, -- Versioned ciphertext envelope.
    PRIMARY KEY (product_id, key)
);
```

iOS: [ProductsLocalStorage](../../hosts/ios/polkadot-app/Modules/Products/ProductsLocalStorage.swift) and [TrUAPILocalStorage](../../hosts/ios/polkadot-app/Modules/Products/TrUAPI/Storage/TrUAPILocalStorage.swift). Android: [ProductLocalStorage](../../hosts/android/feature/products/impl/src/main/java/io/paritytech/polkadotapp/feature_products_impl/data/storage/ProductLocalStorage.kt) and [EncryptedHostStorage](../../hosts/android/feature/products/impl/src/main/java/io/paritytech/polkadotapp/feature_products_impl/domain/truapi/EncryptedTrUAPIStorage.kt).

```sql
-- Typed runtime records: permissions, replay ledger, registries and caches.
-- Secret-bearing records belong to SecretCoreStorage.
CREATE TABLE core_state (
    key BLOB NOT NULL PRIMARY KEY,
    value BLOB NOT NULL -- Versioned ciphertext envelope.
);
```

Core adapters: iOS [TrUAPILocalStorage](../../hosts/ios/polkadot-app/Modules/Products/TrUAPI/Storage/TrUAPILocalStorage.swift), Android [EncryptedHostCoreStorage](../../hosts/android/feature/products/impl/src/main/java/io/paritytech/polkadotapp/feature_products_impl/domain/truapi/EncryptedTrUAPIStorage.kt). Also consolidates iOS [permissions](../../hosts/ios/polkadot-app/Modules/Products/Permissions/Repository/ProductPermissionRepository.swift) and [handled requests](../../hosts/ios/polkadot-app/Modules/PolkadotSignIn/Models/SSOHandledRequestMapper.swift), plus Android [permissions](../../hosts/android/database/src/main/java/io/paritytech/polkadotapp/database/model/ProductPermissionGrantLocal.kt), [handled requests](../../hosts/android/database/src/main/java/io/paritytech/polkadotapp/database/model/SsoHandledRequestLocal.kt) and [VRF registrations](../../hosts/android/database/src/main/java/io/paritytech/polkadotapp/database/model/RingVrfKeyRegistrationLocal.kt).

```sql
-- Authenticated peer identity and lifecycle/sync progress, scoped to a wallet.
CREATE TABLE paired_hosts (
    wallet BLOB NOT NULL,
    peer_statement BLOB NOT NULL,
    peer_encryption BLOB NOT NULL,
    status TEXT NOT NULL,
    added_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    outgoing_update_at INTEGER,
    last_sync_offer_id TEXT,
    PRIMARY KEY (wallet, peer_statement, peer_encryption)
);
```

iOS: [CDPolkadotSignInHost mapping](../../hosts/ios/polkadot-app/Modules/PolkadotSignIn/Models/PolkadotSignInHostMapper.swift) stores the peer identity; Android [SsoSessionLocal](../../hosts/android/database/src/main/java/io/paritytech/polkadotapp/database/model/SsoSessionLocal.kt) also records lifecycle and sync progress.

```sql
-- Peer names, icons and other metadata disappear with the pairing record.
CREATE TABLE paired_host_metadata (
    wallet BLOB NOT NULL,
    peer_statement BLOB NOT NULL,
    peer_encryption BLOB NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (wallet, peer_statement, peer_encryption, key),
    FOREIGN KEY (wallet, peer_statement, peer_encryption)
        REFERENCES paired_hosts (wallet, peer_statement, peer_encryption)
        ON DELETE CASCADE
);
```

iOS: name/icon fields on [PolkadotSignInHost](../../hosts/ios/polkadot-app/Modules/PolkadotSignIn/Models/PolkadotSignInHost.swift), stored by the [CoreData mapper](../../hosts/ios/polkadot-app/Modules/PolkadotSignIn/Models/PolkadotSignInHostMapper.swift). Android: separate [SsoSessionMetadataLocal](../../hosts/android/database/src/main/java/io/paritytech/polkadotapp/database/model/SsoSessionMetadataLocal.kt) rows.

```sql
-- Allocation history per resource account. Contains no private keys.
CREATE TABLE allowance_records (
    wallet BLOB NOT NULL,
    chain BLOB NOT NULL,
    resource TEXT NOT NULL,
    account BLOB NOT NULL,
    allocated_at INTEGER NOT NULL,
    -- Null when these values are derived from detailed statement_slots rows.
    priority INTEGER,
    last_renewed_period INTEGER,
    PRIMARY KEY (wallet, chain, resource, account)
);
```

iOS: [AllowanceRecord](../../hosts/ios/Packages/Individuality/Sources/Allowance/AllowanceRecord.swift) and its [CoreData mapper](../../hosts/ios/polkadot-app/Modules/Products/AllowanceRecordMapper.swift). Android: related account history is recorded per slot in [StatementStoreSlotAllocationLocal](../../hosts/android/database/src/main/java/io/paritytech/polkadotapp/database/model/StatementStoreSlotAllocationLocal.kt).

```sql
-- Device-owned Statement Store slots; priority and renewal time guide renewal.
CREATE TABLE statement_slots (
    wallet BLOB NOT NULL,
    chain BLOB NOT NULL,
    collection TEXT NOT NULL,
    period INTEGER NOT NULL,
    slot INTEGER NOT NULL,
    account BLOB NOT NULL,
    priority INTEGER NOT NULL,
    last_allocated_or_renewed_at INTEGER NOT NULL,
    PRIMARY KEY (wallet, chain, collection, period, slot)
);
```

Android: [StatementStoreSlotAllocationLocal](../../hosts/android/database/src/main/java/io/paritytech/polkadotapp/database/model/StatementStoreSlotAllocationLocal.kt). iOS: [AllowanceRecord](../../hosts/ios/Packages/Individuality/Sources/Allowance/AllowanceRecord.swift) tracks account-level renewal state, with no equivalent per-slot row.

```sql
-- Product display metadata and worker override, not authorization decisions.
CREATE TABLE products (
    product_id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    icon_cid TEXT,
    icon_format TEXT,
    worker_url_override TEXT
);
```

iOS: [CDProduct mapping](../../hosts/ios/polkadot-app/Modules/Products/ProductMapper.swift). Android: [ProductLocal](../../hosts/android/database/src/main/java/io/paritytech/polkadotapp/database/model/ProductLocal.kt).

```sql
-- Notification ownership and reconciliation state; the OS performs delivery.
CREATE TABLE scheduled_notifications (
    product_id TEXT NOT NULL,
    notification_id INTEGER NOT NULL,
    text TEXT NOT NULL,
    deeplink TEXT,
    scheduled_at INTEGER, -- Null means immediate delivery.
    PRIMARY KEY (product_id, notification_id),
    FOREIGN KEY (product_id) REFERENCES products (product_id) ON DELETE CASCADE
);
```

iOS: [ScheduledNotificationEntry](../../hosts/ios/polkadot-app/Modules/Products/ScheduledNotification/ScheduledNotificationEntry.swift) stores IDs; [ProductNotificationScheduler](../../hosts/ios/polkadot-app/Modules/Products/ScheduledNotification/ProductNotificationScheduler.swift) keeps payloads in OS requests. Android: [ScheduledProductNotificationLocal](../../hosts/android/database/src/main/java/io/paritytech/polkadotapp/database/model/ScheduledProductNotificationLocal.kt) stores IDs and payloads together.

- `core_state` backs the existing typed nonsecret `CoreStorage` records, retaining their canonical encodings and scopes. Do not create a competing normalized table for the same permission, registry or replay facts. Secret-bearing keys are not accepted by this repository.
- `wallet` is the stable root public key. Peer tables include both authenticated peer public keys; names and icons are entries in `paired_host_metadata`. Product storage and permission records keep their existing product scope, without adding wallet ownership arbitrarily.
- Allowance records represent accounts; statement slots represent individual assignments. Preserve each slot's priority and last allocation/renewal time, which determine renewal ordering. Update related rows in one transaction. For accounts with detailed slot rows, derive renewal period and effective priority from those rows and leave the account-level fields null. Those fields serve resources without detailed slot rows.
- Product-storage keys represent the decoded product-local key. Cross-product access checks select the authorized owner before querying its row.
- A null `scheduled_at` means immediate delivery. Notification delivery remains an OS operation. Rust reconciles the database with pending OS requests after interruption; a failed OS registration must not be reported as a scheduled notification. Cancel OS requests before deleting their rows or the owning product. Keep the records and retry if cancellation fails; a SQL cascade alone is not OS cleanup.
- Add no worker tables or Rust worker repository. Native worker managers retain integration reasons, operation records and their existing lifecycle policy, including Android's optional operation labels.

Product values are arbitrary bytes and can be sensitive. Preserve Android's existing at-rest protection when replacing its encrypted preferences. Rust encrypts `product_storage` and `core_state` payloads using the existing AEAD dependency, fresh nonces and associated data binding the table/row key. Keep the installation-scoped storage encryption key in `SecretCoreStorage`, separate from wallet and device-signing/encryption keys. It survives ordinary logout. Persist only the versioned ciphertext envelope in SQLite; decryption failure is an error. No plaintext fallback.

## State that stays outside SQLite

- Keychain/Keystore protection, wallet root secrets and secret-store namespace identifiers remain native. The runtime feature flag is application configuration.
- Native worker managers retain bundle caching, metadata and update policy, together with their existing integration and operation stores. Do not put bundle bytes into SQLite.
- One-time permissions, live sessions, approval queues, subscriptions and running engine handles remain in memory. Persisted peers and replay records are the restart state, not serialized pending approvals.
- Shared native chain caches and unrelated app databases remain with their existing owners. Rust performs its own account-runtime chain reads and renewal decisions; the native renewal job cannot remain a second policy owner.

## Source inventory

- Rust: [SQLite foundation](../../rust/crates/truapi/src/store.rs), [typed core slots](../../rust/crates/truapi/src/platform.rs), [native storage callbacks](../../rust/crates/truapi/src/native/platform.rs).
- iOS: [product/core preferences](../../hosts/ios/polkadot-app/Modules/Products/TrUAPI/Storage/TrUAPILocalStorage.swift), [permission repository](../../hosts/ios/polkadot-app/Modules/Products/Permissions/Repository/ProductPermissionRepository.swift), [CoreData entities](../../hosts/ios/polkadot-app/Common/Storage/UserDataModel.xcdatamodeld/UserDataModel52.xcdatamodel/contents), [allowance records](../../hosts/ios/Packages/Individuality/Sources/Allowance/AllowanceRecord.swift), [notification scheduler](../../hosts/ios/polkadot-app/Modules/Products/ScheduledNotification/ProductNotificationScheduler.swift).
- Android: [Room entities](../../hosts/android/database/src/main/java/io/paritytech/polkadotapp/database/model), [Rust storage adapter](../../hosts/android/feature/products/impl/src/main/java/io/paritytech/polkadotapp/feature_products_impl/domain/truapi/EncryptedTrUAPIStorage.kt), [product storage](../../hosts/android/feature/products/impl/src/main/java/io/paritytech/polkadotapp/feature_products_impl/data/storage/ProductLocalStorage.kt), [allowance key storage](../../hosts/android/feature/products/impl/src/main/java/io/paritytech/polkadotapp/feature_products_impl/domain/hostApi/allowance/RealAllowanceKeyStorage.kt).
