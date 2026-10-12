# Architecture Review Checklist

Walk through each applicable section. For each rule violated, note the severity (blocking/major/minor), quote the file:line, and suggest a fix.

## Module Boundaries

- [ ] New code in correct location: Modules/ for screens, Packages/ for reusable logic, Common/ for shared app-level utilities
- [ ] No new code added to `Inherited/` (deprecated; migrate the file you're touching, no big-bang refactors)
- [ ] VIPER contracts complete: Protocols.swift updated for any new inter-layer methods
- [ ] Packages don't import app-level code
- [ ] No circular dependencies between packages
- [ ] File not growing too large (>400 lines → split)
- [ ] Generic Substrate helpers (knowing only SDK primitives: AccountId, hex, SCALE, runtime metadata) live in `SubstrateSdkExt`, not in feature packages
- [ ] App-specific config (API keys, secrets, env values) injected via provider protocol, not hardcoded inside a shared package
- [ ] EventCenter events live where they're emitted; the `…EventVisiting` extension protocol is co-located
- [ ] ChainRegistry consumed via `ChainRegistryFacade.sharedRegistry` (app) or `ChainRegistryFactory` (tests), not by recreating the registry per feature

**Ref:** architecture/multi-package.md, architecture/viper.md

## Layer Boundaries (VIPER)

- [ ] ViewController doesn't call Interactor directly
- [ ] Interactor doesn't import UIKit
- [ ] Layout code in ViewLayout, not ViewController
- [ ] Navigation only in Wireframe
- [ ] All cross-layer communication via protocols (no concrete type references)
- [ ] Private methods in separate extensions
- [ ] Interactor→Presenter uses OutputProtocol callbacks, NOT publishers/Combine
- [ ] @MainActor scoped narrowly — on OutputProtocol, not on entire Tasks
- [ ] WebView features have proper VIPER modules (async tasks in Interactor, not Wireframe)
- [ ] Base class inheritance used when shared logic exists (TokensPresenter, etc.)

**Ref:** architecture/viper.md

## Reuse Before Build

- [ ] Checked existing utilities (SubstrateSdk, SubstrateSdkExt, StructuredConcurrency, FoundationExt, UIKitExt, PolkadotUI) before writing new code
- [ ] HTTP constants use typed values (HttpMethod, HttpContentType, HttpHeaderKey)
- [ ] Async retry uses `withRetry` from StructuredConcurrency
- [ ] Storage subscriptions use SDK helpers (CallbackBatchStorageSubscription.asyncStream)
- [ ] Async streams use `AsyncPassthroughSubject` or `AsyncStream.makeStream` (not custom continuation hacks)
- [ ] Hex conversion uses `Data.toHex()` and `String.fromHex()`, not `Data(hexString:)` directly or the underlying `NSData` variants from NovaCrypto
- [ ] Reusable conversions extracted to appropriate package extensions
- [ ] `UsernameStorage` used for own username (not remote queries)
- [ ] `StorageRequestFactory.asyncInit()` used (not sync init)
- [ ] `requestFactory.queryByPrefix` used (not keysFactory)
- [ ] `getChainAsset` used (not manual chain+asset construction)
- [ ] `UserNotificationServicing` used (not direct UNUserNotificationCenter)
- [ ] No unnecessary provider/wrapper/factory classes

**Ref:** architecture/maintainability.md (rule #1)

## Service Architecture

- [ ] Services are non-optional in coordinator/manager inits
- [ ] Single responsibility per service (no unrelated concerns injected)
- [ ] Services post state; consumers interpret (not the other way around)
- [ ] No init{} side effects in services
- [ ] User not blocked on network when cached data is available
- [ ] Internal sub-services not exposed through protocol properties
- [ ] Logger injected through init (not singleton access)
- [ ] StorageFacade stored for lazy repository creation (when multiple repo types needed)
- [ ] Features disabled at factory/registration level, not via early returns inside the feature
- [ ] Temporary behavior changes behind feature flags

**Ref:** code/di-and-services.md, architecture/maintainability.md

## Data Persistence

- [ ] Partial entity updates use separate mappers (not fetch-modify-save)
- [ ] UserDefaults access via SettingsManager
- [ ] New CoreData schema changes add a version migration
- [ ] Repository pattern used for data access

**Ref:** code/data-persistence.md

## Chain Integration

- [ ] On-chain values derived, not hardcoded
- [ ] Storage subscriptions preferred over polling
- [ ] SDK built-in decoders used where applicable
- [ ] ScaleEncodable/ScaleDecodable conformance at type level for reusable types

**Ref:** architecture/chain-integration.md

## Transactions

- [ ] Extrinsic extension ordering correct (base first, custom overrides after)
- [ ] Payload types implement Decodable (not manual JSON parsing)
- [ ] Generic models where multiple signer/account types exist
- [ ] Errors thrown on unexpected data (no silent raw-bytes fallback)

**Ref:** architecture/transactions.md

## Chat & Messaging

- [ ] Chat extensions don't block main chat flow
- [ ] Message rendering state in dedicated handlers
- [ ] Encryption via MessageExchangeKit

**Ref:** architecture/chat-extension.md

## Data Transport (WebRTC)

- [ ] Reconnect policy owned by consumer, not transport
- [ ] Config naming reflects actual scope

**Ref:** architecture/data-transport.md

## Device Sync

- [ ] Device sync owns exactly one peer engine per supported remote device; membership removal disposes it and re-addition creates a fresh engine
- [ ] Disposed device-sync engines/contexts are terminal and cannot schedule retries or create another connection generation
- [ ] WebRTC connection generations are sequential per peer; exchange and flow shutdown are awaited before replacement starts
- [ ] Device-sync exchange iterators are created before the flow starts publishing update, ACK, or state events
- [ ] Accepted reconnect identity is persisted before its signaling offer/answer is exposed to WebRTC
- [ ] Service throttle cancels and awaits device reconciliation, removes owned engines, and awaits every engine disposal
- [ ] Retry and ACK-timeout tests inject manual sleepers or event gates; they never depend on wall-clock sleeps or exact timeouts

**Ref:** architecture/device-sync.md

## Coinage

- [ ] Key derivation via CoinKeypairDerivation (never hand-rolled)
- [ ] SharedSecretDerivationDomain per-feature (never reused)
- [ ] Edge cases handled (zero balance, exact match)

**Ref:** architecture/coinage.md

## Maintainability

- [ ] Docs updated in this PR for any new rule, package, pattern, or removed subsystem (`.claude/docs/**`, `CLAUDE.md`)
- [ ] Dead code removed (unused methods, debug leftovers, preview Tasks)
- [ ] Structured concurrency used for new code (Operation-iOS migrated when touched)
- [ ] All user-visible strings localized
- [ ] Build config guards (Sentry not running in tests/previews)
- [ ] Build phase scripts exclude Debug configuration
- [ ] File size reasonable (<500 lines per file)
- [ ] Type aliases used for domain concepts (ProductId, not String)
- [ ] API boundary params parsed into typed models immediately
- [ ] Configuration URLs in AppConfig (not hardcoded in modules)
- [ ] Debounce added for frequently called endpoints
- [ ] Logger injected through init (not `Logger.shared` singleton inside methods)
- [ ] TODOs left for pending integrations (e.g., TURN node config) with explicit unblocking condition
- [ ] "mark" used for state transitions, "save" for persistence

**Ref:** architecture/maintainability.md

## Verdict Format

```
## Architecture Review

**Verdict:** X blocking / Y major / Z minor

### Blocking
- [file:line] Description. Fix: ...

### Major
- [file:line] Description. Fix: ...

### Minor
- [file:line] Description. Suggestion: ...
```
