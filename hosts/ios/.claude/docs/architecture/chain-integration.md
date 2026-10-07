# Chain Integration

## Overview

Substrate/Polkadot blockchain interaction via `substrate-sdk-ios`. Covers storage reads, runtime API calls, SCALE codec, and chain registry management.

## Key Components

### External
- **substrate-sdk-ios** — core Substrate SDK: JSON-RPC, storage subscriptions, SCALE, signing
- **ExtrinsicService** — extrinsic building

### Internal Packages
- **ChainRegistry** — Chain catalog, connection pool, runtime providers, chain-sync events. Owns `ChainRegistry`, `ConnectionPool`, `RuntimeProviderPool`, `SpecVersionSubscription`, and `ChainModel`/`AssetModel` types. Imports `SubstrateSdkExt`, `ChainStore`, `EventCenter`, `OperationExt`, `CommonService`
- **SubstrateOperation** — Substrate operation utilities
- **SubstrateSdkExt** — Reusable extensions to substrate-sdk-ios. Hosts generic helpers used by ChainRegistry and other features: `AddressConversion`, `RawDataStorageSubscription`, `StorageSubscriptionContainer`, `StorageSubscriptionProtocols`, `WebSocketSubscribing`, `CallMetadata+TypeCheck`, `CodingFactory+TypeCheck`, `RuntimeMetadata+Internal`. New generic helpers belong here
- **ChainStore** — Chain metadata caching
- **AssetsManagement** — Asset registry and management
- **AssetHubSdk** — Asset Hub integration
- **HydrationSdk** — Hydration DeFi chain integration

### App-Level
- `Common/Substrate/` — Substrate helpers
- `Common/ChainRegistry/` — App wiring for the ChainRegistry package: `ChainRegistryFacade`, `ChainRegistryFactory`, `ChainSyncService`, and `ConnectionApiKeysProvider` (the app's implementation of the provider protocol declared in the package — injects API keys without hardcoding them in shared code)
- `Inherited/Substrate/` — Legacy substrate operations (being migrated)

## Using ChainRegistry from a Feature

**Canonical entry point:** `ChainRegistryFacade.sharedRegistry` (in `polkadot-app/Common/ChainRegistry/`). Features inject this `ChainRegistryProtocol` into interactors/services rather than reaching into the package directly. The facade owns one shared instance backed by `SubstrateDataStorageFacade`; `ChainRegistryFactory.createDefaultRegistry(from:)` is also exposed for tests that need an in-memory database.

Current `ChainRegistryProtocol` surface (defined in the package):
- `availableChainIds`, `allAvailableChains` — snapshot of synced chains
- `getChain(for:)`, `getChainByGenesis(for:)`, `getChainOrError(for:)` — chain lookup
- `getConnection(for:)`, `getOneShotConnection(for:)`, `getConnectionOrError(for:)` — JSON-RPC engines (one-shot for short-lived calls)
- `getRuntimeProvider(for:)`, `getRuntimeProviderOrError(for:)` — runtime coder/metadata access
- `chainsSubscribe(...)` / `chainsUnsubscribe(_:)` — observe chain catalog changes
- `subscribeChainState(_:chainId:)` / `unsubscribeChainState(_:chainId:)` — connection-state observation
- `switchSync(mode:chainId:)`, `retainConnections(_:)`, `syncUp()` — lifecycle control

Convenience helpers (from `ChainRegistry+ChainAsset`, `+ChainStore`, `+AsyncWait`, `+Combine`) layer on top: `getChainAsset`, async `awaitChain(for:)`, Combine publishers.

### Rules
- **Inject `ChainRegistryProtocol`, not `ChainRegistry` (concrete)** — keeps modules testable with `MockChainRegistry`
- **Use the facade in app code, the factory in tests** — `ChainRegistryFactory.createDefaultRegistry(from:)` with an in-memory facade is the test entry point
- **Don't recreate ChainRegistry per feature** — `ChainRegistryFacade.sharedRegistry` is the single live instance

### Connection retention (background)

`ConnectionPool` sleeps every connection when the app backgrounds (`didEnterBackground` → `disconnect(true)`) and reconnects on foreground. Work that must survive a fold keeps connections alive by holding a retain via `retainConnections(_ scope:)`:

- `.chains([chainId])` — retain the current connections for those chains (chains with no live connection are skipped). Refcounted per connection; the token holds them strongly, so identity is stable while retained.
- `.all` — retain **every** connection and suppress background sleep pool-wide, including connections created *while the retain is held* (a "live" scope). Use this for multi-chain work whose exact connection set isn't known up front.

`retainConnections(_:)` returns a `ConnectionRetainToken` (RAII): connections are kept (and woken if already sleeping) while any token is held, and slept again when the last token releases — release is automatic on `deinit`, so a thrown error can't leak the retain. The per-connection refcount machinery underpins `.chains` and is retained for future retain-driven APIs.

For background operations, prefer wrapping the whole operation in `ConnectionRetainingExecutor` (in the `BackgroundExecution` package) rather than retaining by hand: it takes a `ConnectionRetentionProviding` (satisfied by `ChainRegistryFacade.sharedRegistry`), retains `.all` before the operation and releases when it finishes — see `AssetDetailsViewFactory` for the wiring.

**What to retain — and what not to.** Retain only long, non-idempotent work.
**Plain RPC reads are not retained** — `JSONRPCOptions` defaults to `resendOnReconnect: true`, so an in-flight read is requeued on the forced disconnect and resent when the app foregrounds. The awaiting call just suspends across the fold and resumes. Only `resendOnReconnect: false` requests fail (with `clientCancelled`) on background.

## Storage Queries

### Canonical Pattern
Use `substrate-sdk-ios` storage subscription (`CallbackBatchStorageSubscription`) and query APIs (`StorageRequestFactory`):

```swift
// One-shot query via StorageRequestFactory
let requestFactory = try await StorageRequestFactory.asyncInit(
    remoteFactory: StorageKeyFactory(),
    operationManager: operationManager
)

let queryWrapper: CompoundOperationWrapper<[StorageResponse<UInt32>]> =
    requestFactory.queryItems(
        engine: connection,
        keyParams: { [accountId] },
        factory: { try runtimeProvider.fetchCoderFactoryOperation().extractNoCancellableResultData() },
        storagePath: SystemPallet.accountPath
    )

// Prefix query (replaces keysFactory.createKeysFetchWrapper)
let prefixWrapper = requestFactory.queryByPrefix(
    engine: connection,
    keyParam: chatId,
    factory: { try codingFactoryOperation.extractNoCancellableResultData() },
    storagePath: ChatPallet.messagesPath,
    at: nil
)
```

For reactive subscriptions, prefer `CallbackBatchStorageSubscription.asyncStream` with the `resultDecoder` parameter so the SDK handles SCALE decoding for any `ScaleDecodable` type.

### Rules
1. **Subscription > polling** — prefer storage subscriptions over periodic polling
2. **Use SDK's built-in decoders** — `resultDecoder` parameter handles standard SCALE types. When the type conforms to `ScaleDecodable` (like `Bool`), use `resultDecoder` instead of manual decoding with `queryType: "bool"`
3. **Use `AsyncPassthroughSubject` or `AsyncStream.makeStream`** — for custom async streams. Specifically, don't use `var continuation: AsyncStream.Continuation!` with a separate init — use `AsyncStream.makeStream()` which returns both together
4. **Derive on-chain values, don't hardcode** — prices and limits should come from chain state
5. **Use `StorageRequestFactory.asyncInit()`** — for proper async initialization, not the sync initializer
6. **Use `requestFactory.queryByPrefix`** — instead of `keysFactory.createKeysFetchWrapper` when querying by prefix
7. **Use `getChainAsset`** — from chain registry instead of manually constructing chain + asset combinations

## SCALE Codec

- `substrate-sdk-ios` provides SCALE encoding/decoding
- Use `hexString.fromHex()` (SubstrateSdkExt) to convert hex strings to Data. It wraps `Data(hexString:)` and reads left to right at the call site; both are preferred over the underlying `NSData(hexString:)` from NovaCrypto
- Use `toHex()` from SubstrateSdk for Data-to-hex conversion — prefer the `Data` wrapper over the underlying `NSData.toHexString` from NovaCrypto
- Implement `ScaleEncodable`/`ScaleDecodable` at the type level for reusability
- Use `Data.randomOrError` from SubstrateSdk for random/test data generation

## Chain Configuration

Defined in `AppConfig/AppConfig.swift`:
- Chat chain, username chain, bulletin chain, asset hub chain
- UNSTABLE build variant support for testnet chains
- Chain assets: main asset, fiat onramp asset, PGAS, funding assets

## Anti-Patterns from Reviews

| Anti-pattern                              | Do instead                                                |
|-------------------------------------------|-----------------------------------------------------------|
| Hardcoding on-chain values (prices, scores)| Derive from chain state via storage queries              |
| Custom async stream initializers          | Use `AsyncPassthroughSubject` or `AsyncStream.makeStream` |
| Fetching remotely what's available locally | Check local data first; only fetch if stale              |
| Using `keysFactory` for prefix queries    | Use `requestFactory.queryByPrefix` instead                |
| Sync StorageRequestFactory init           | Use `StorageRequestFactory.asyncInit()`                   |
| Manual chain+asset construction           | Use `getChainAsset` from chain registry                   |
