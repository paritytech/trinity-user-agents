# Multi-Package Architecture

## Overview

The project uses local SPM packages under `Packages/`, with `AppDependencies` as the root package declaring all external dependencies. Feature UI lives in `polkadot-app/Modules/` (VIPER modules), while reusable logic, services, and UI components live in packages.

## Package Categories

### Infrastructure
- **AppDependencies** — root dependency manifest; all external SPM deps declared here
- **StructuredConcurrency** — async/await bridges for Operation-iOS, AsyncExtensions integration
- **BackgroundExecution** — bridges `UIApplication.beginBackgroundTask` into a structured `BackgroundExecuting.execute { }` API so a long operation keeps running for the window iOS grants after the app is folded (best-effort, ~30s; cancels the operation on expiration)
- **OperationExt** — Operation-iOS extensions and helpers
- **FoundationExt** — Foundation extensions
- **UIKitExt** — UIKit extensions and custom controls
- **UIDependencies** — UI layer dependencies aggregator
- **EventCenter** — Observer/visitor event bus shared by app and packages
- **CommonService** — Common service protocols
- **StateMachine** — Generic state machine primitives
- **JailbreakDetection** — Device integrity checks
- **IssueMonitoring** — Crash/issue reporting seam

### Blockchain & Crypto
- **SubstrateOperation** — Substrate blockchain operations
- **SubstrateSdkExt** — Reusable extensions to substrate-sdk-ios. Generic Substrate helpers (storage subscription wrappers, address conversion, metadata/coder type checks) live here, NOT in feature packages
- **ChainRegistry** — Chain catalog, runtime providers, connection pool, chain-sync events. Imports `SubstrateSdkExt`, `ChainStore`, `EventCenter`. App-specific config (API keys, etc.) is injected from the app via provider protocols, never hardcoded inside the package
- **KeyDerivation** — Key derivation and wallet management
- **CarParser** — Transaction metadata parsing
- **XcmDefinition** — Cross-consensus message definitions
- **XcmTransfer** — XCM transfer operations
- **Revive** — pallet-revive: the chain-bound contract API (`ReviveContractApi`), `Revive.call`, `EvmAddress` + H160 derivation, namehash, and the shared Solidity ABI encoder (the only web3swift link). Consumed by Coinage and Products; see architecture/revive.md

### Data & Storage
- **ChainStore** — Blockchain metadata caching
- **StatementStore** — Statement/signature storage
- **AssetsManagement** — Asset registry
- **ChatStorage** — Chat data persistence

### Specialized Services
- **ExtrinsicServiceExt** — Pluggable extrinsic submission on top of the `ExtrinsicService` package adds pre-submission validation + general fork protection
- **MessageExchangeKit** — Encrypted messaging protocol
- **HandoffService** — Handoff/continuity
- **Coinage** — Private Payment System implementation: coin models, denominations, transfer planning, and lifecycle
- **DurableTransactions** — Domain-neutral crash-durable transaction engine (ledger, submission watch, recovery pass, completion ladder, pinned chain view). Domains plug in through `TxCompletionOracle`; Coinage is the first. Ships `DurableTransactionsTestSupport` fakes
- **Individuality** — People, LitePeople, Members, Allowance, Pgas, Resources, the identity pallet and the origin restriction pallet
- **BulletinChain** — Bulletin/news chain integration
- **AssetExchange** — Asset exchange protocols
- **AssetHubSdk** — Asset Hub blockchain integration
- **HydrationSdk** — Hydration DeFi integration
- **Products** — Product account models and WebView JS bridge; the DotNs resolver reads its contracts through `Revive`
- **TrUAPIHost** *(remote SPM dependency, not a local package)* — TrUAPI Rust core
  (xcframework + uniffi bindings), the `TrUAPIHostCore` wrapper (`TrUAPIHostCoreProtocol`
  seam), and the bundled truapi lockdown container behind `ContainerScriptBundle.load()`.
  The container TypeScript source and its esbuild live in the truapi repo, not here.
  App-side glue — `RustRuntimeBridge`, `Connection/` (chain connection pool with injected
  genesis→engine resolver, rpc adapter, frames), `Storage/` (`TrUAPILocalStorage`),
  `Preimage/` (`TrUAPIPreimageCache` on `CoalescingTask`) — lives in
  `polkadot-app/Modules/Products/TrUAPI/`, not in a package

### UI & Design
- **PolkadotUI** — Design system: typography, colors, shared components
- **BlurHash** — BlurHash encode/decode using Wolt's reference implementation, with a typed `BlurHash` value boundary

## Hard Rules

1. **No circular dependencies** — packages must form a DAG
2. **Never build packages separately** — importing via `Packages/X` creates `.build` that inflates app size. All packages are imported through the Xcode project
3. **AppDependencies is the single source for external deps** — never add external package URLs in individual package manifests; reference them from AppDependencies
4. **Packages must not import app-level code** — packages are reusable; they cannot depend on `polkadot-app/` targets
5. **Prefer packages for reusable logic** — if a utility will be used across multiple modules, it belongs in a package, not `Common/`
6. **Generic SDK helpers go to SubstrateSdkExt, not feature packages** — when extracting code into a feature package (e.g., ChainRegistry), generic Substrate utilities (storage subscription wrappers, address conversion, metadata/coder type checks) MUST live in `SubstrateSdkExt`. Feature packages re-export or depend on them. Anti-pattern: bundling generic helpers into the first feature package that needs them (`AddressConversion`, `RawDataStorageSubscription`, `StorageSubscriptionContainer`, `CallMetadata+TypeCheck`, `CodingFactory+TypeCheck`, `RuntimeMetadata+Internal` all moved out of ChainRegistry into SubstrateSdkExt)
7. **App-specific configuration is injected, not embedded** — secrets, API keys, environment-dependent values must NOT be hardcoded inside a shared package. Define a provider protocol in the package (e.g., `ConnectionApiKeysProviding`) and inject the app's implementation from `polkadot-app/Common/{PackageName}/` (`ConnectionApiKeys` moved to app, `ConnectionApiKeysProvider` injected into `ChainRegistry`)

## When to Create a New Package

Create a new package when:
- Logic is reused across 3+ modules or other packages
- The domain is self-contained (e.g., a new blockchain SDK integration)
- You need to encapsulate a protocol + implementation boundary

Do NOT create a package for:
- A single module's internal logic
- Thin wrappers around a single external dependency
- Code that only makes sense within the app target

## Seams

| Seam                     | Where                                | When to touch                          |
|--------------------------|--------------------------------------|----------------------------------------|
| AppDependencies manifest | `Packages/AppDependencies/Package.swift` | Adding/removing external dependencies  |
| Package.swift            | `Packages/{Name}/Package.swift`      | Adding targets, dependencies to a package |
| Xcode project refs       | `polkadot-app.xcodeproj`             | Linking a new package to the app target |
