# DI, Scopes, and Lifecycle

Hilt-based DI with strong conventions around startup work, singletons, scope discipline, and background-vs-foreground execution.

## Rules at a glance

1. **`blocking`** — Injecting a class into `App.kt` only to trigger its `init {}` block is forbidden. Use `AppInitializer @IntoSet`.
2. **`blocking`** — Submitting extrinsics in background work without `ChainConnectionRefCounter.withConnectionEnabled(...)` is forbidden. The default chain connection isn't active off-screen.
3. **`blocking`** — Reusing a keypair across roles (identity ≠ device key ≠ wallet) is forbidden. Each role derives from its own path.
4. **`blocking`** — State holder without a `clear()` / reset method, causing leaks across sessions.
5. **`major`** — `@AssistedInject` parameters typed as `@JvmInline value class` break KSP. Pass the underlying type and wrap internally.
6. **`major`** — Don't `@Singleton`-annotate stateless / cheap-to-construct classes.
7. **`major`** — Don't inject a service's `CoroutineScope` into a VM to launch a flow. Expose `Flow<X>` from the service and subscribe.
8. **`major`** — Long-lived chain connections use `requestConnectionEnabled()` + `release()` (ref counting). Don't abuse `withSessionEnabled { awaitCancellation() }`.
9. **`major`** — Notification cancellation logic lives in the owning feature, not in `App.kt`.
10. **`major`** — Expedited `Worker`s must override `getForegroundInfo()`. Otherwise WorkManager demotes them silently. See `code/workers-and-background-sync.md`.
11. **`major`** — One terminal cleanup entry point per state holder; not two cleanup verbs with subtle semantic differences.
12. **`major`** — `ComputationalScope` is a specialization tied to `ComputationalCache` consumer counting; pass a VM (which implements it) or wrap `ProcessLifecycleOwner.lifecycleScope` deliberately. Don't hand-roll `SupervisorJob() + Dispatchers.Default` inside a singleton.
13. **`minor`** — `try { ... } finally { dispose() }` at every caller — prefer one explicit `dispose()` owned by the lifecycle owner.

---

## Hilt setup

- DI graph rooted at `app/.../di/AppModule.kt`.
- Each `impl` module ships its own `di/<Feature>FeatureModule.kt` that `@Binds` interfaces from `api/` to `Real*` implementations.
- Cross-feature plug-ins use multibindings: `@IntoSet` (for `Set<ChatExtension>`, `Set<AppInitializer>`) and `@IntoMap` for keyed lookups.

```kotlin
@Module @InstallIn(SingletonComponent::class)
interface MyFeatureModule {

    @Binds @Singleton
    fun bindFooRepository(impl: RealFooRepository): FooRepository

    @Binds @IntoSet
    fun bindMyInitializer(impl: MyAppInitializer): AppInitializer
}
```

### `@AssistedInject` for runtime params

When a class needs both injected deps AND a runtime parameter (e.g. a state machine that's per-game, per-room, per-config), use `@AssistedInject` + `@AssistedFactory`.

```kotlin
class GameStateMachine @AssistedInject constructor(
    private val repository: GameRepository,
    @Assisted private val gameId: Long,                  // — NOTE: see value-class caveat below
) { ... }

@AssistedFactory
interface GameStateMachineFactory {
    fun create(gameId: Long): GameStateMachine
}
```

**Caveat (KSP):** Don't use `@JvmInline value class` as the `@Assisted` parameter type — KSP fails. Pass the underlying primitive and wrap internally. Memory: `feedback_ksp_assisted_value_class`.

```kotlin
// ✗ — KSP fails
fun create(@Assisted gameId: GameId): GameStateMachine

// ✓
@AssistedInject constructor(
    @Assisted gameIdRaw: Long,
) {
    private val gameId = GameId(gameIdRaw)
}
@AssistedFactory interface Factory { fun create(gameIdRaw: Long): GameStateMachine }
```

---

## Singletons — only when stateful or expensive

Don't reflexively annotate everything `@Singleton`. Stateless and cheap-to-construct classes should be created on demand.

**Singleton ⇒ Hilt manages one instance for the SingletonComponent's lifetime.** Reserve for:
- Classes holding cached state (registries, state holders, caches).
- Classes that establish connections (sockets, services).
- Classes with non-trivial construction cost.

For pure dispatchers/mappers/converters — no `@Singleton`. Hilt will construct one per injection request, the cost is negligible, and the GC story is simpler.

---

## `AppInitializer` — the only allowed startup-side-effect pattern

In `common/src/.../presentation/AppInitializer.kt`:

```kotlin
interface AppInitializer {
    context(ComputationalScope)
    fun initialize(): Result<Unit>
}
```

Bind via `@IntoSet`:

```kotlin
@Binds @IntoSet
fun bindFooInitializer(impl: FooAppInitializer): AppInitializer
```

`AppInitializerPipeline` runs them all at app start (failures are logged and don't stop the rest).

### What goes here

- Warm-up subscriptions that must run for the app lifetime regardless of UI.
- Cancellation of stale notifications, alarms, work.
- Registry priming.

### What does NOT go here

- Feature work that should be triggered by user navigation.
- Anything that blocks app startup waiting on network/storage; perform it asynchronously in `RootViewModel`.

### Why this pattern exists

The temptation is to inject a class into `App.kt` just to get Dagger to construct it, so its `init {}` block runs. This is an anti-pattern — it creates implicit dependencies and is easy to break by removing the injection. The `AppInitializer` interface makes the contract explicit.

---

## Coroutine scopes

### `viewModelScope`

Default for any VM-tied work.

### `ComputationalScope`

`ComputationalScope` is a **specialization of `CoroutineScope`** that exists so `ComputationalCache` (in `common/.../data/memory/ComputationalCache.kt`) can do automatic consumer counting for cached computations and shared flows.

```kotlin
interface ComputationalScope : CoroutineScope
```

The cache uses the scope as the **identity of a consumer**: each subscriber passes its scope, the cache reference-counts subscribers, and when the last scope cancels the underlying computation/flow is torn down. If callers could pass *any* `CoroutineScope`, it would be easy to accidentally hand in a global or wrong-lifecycle scope and leak the cached computation. Requiring a `ComputationalScope` forces the caller to opt in to "yes, this scope's lifetime should drive consumer counting".

In practice, almost all `ComputationalScope`s are **ViewModel scopes** — the base `ViewModel` implements `ComputationalScope`, so a VM is itself a valid `ComputationalScope` and can be passed to the cache directly:

```kotlin
context(ComputationalScope)
fun <T> ComputationalCache.useSharedFlow(key: String, flowLazy: Computation<Flow<T>>): Flow<T>

// At a call site inside a VM (VM is ComputationalScope):
val accountInfo = computationalCache.useSharedFlow("accountInfo:$accountId") {
    storage.observe(accountId)
}
```

You may also wrap an arbitrary `CoroutineScope` for ad-hoc use:
```kotlin
val computational = ComputationalScope(processLifecycleOwner.lifecycleScope)
```
…but this is rare and intentional. `AppInitializer.initialize()` uses `context(ComputationalScope)` for the same reason — `AppInitializerPipeline` provides a process-scoped instance.

### `context(CoroutineScope)` extensions

Flow utilities like `shareInBackground()` take a context-receiver `CoroutineScope`:

```kotlin
context(CoroutineScope)
fun <T> Flow<T>.shareInBackground(started: SharingStarted = SharingStarted.Eagerly) =
    inBackground().share(started)
```

Call from a place that has a scope in context — usually inside a VM init or a Mixin constructor.

### `launchUnit { ... }`

Project's terse, `Job`-less launch helper. Preferred for VM action handlers and one-shot side effects.

---

## Service scopes — what NOT to inject

Don't inject a service's `CoroutineScope` into a ViewModel just to start a flow. Expose `Flow<X>` from the service and subscribe from the VM.

```kotlin
// ✗
class CallControlsViewModel @Inject constructor(
    private val serviceScope: CallServiceScope,  // — ugh
) {
    init { serviceScope.launch { ... } }
}

// ✓
class CallControlsViewModel @Inject constructor(
    private val reader: CallStateReader,
) {
    override val state = reader.observeState().map { ... }.stateIn(viewModelScope, ...)
}
```

If the only reason a dependency needs a scope is to launch the flow, expose `Flow<X>` to the ViewModel and subscribe there.

---

## Connection reference counting (chains, sessions)

For long-lived chain connections in services or app-level features, **use reference counting**, not `withSessionEnabled { awaitCancellation() }`.

```kotlin
// ✓
class GameService : Service() {
    @Inject lateinit var connection: BackgroundChainConnection
    private var ref: ConnectionRef? = null

    override fun onCreate() {
        super.onCreate()
        ref = connection.requestConnectionEnabled()
    }

    override fun onDestroy() {
        ref?.release()
        super.onDestroy()
    }
}

// ✗
class GameService : Service() {
    override fun onCreate() {
        scope.launch {
            connection.withSessionEnabled { awaitCancellation() }  // — abuse of withSessionEnabled
        }
    }
}
```

### When to use `withSessionEnabled { ... }`

When the work is genuinely scoped to a lambda body — submit an extrinsic and then release:

```kotlin
connection.withSessionEnabled {
    extrinsicService.submitExtrinsic(...)
}
```

If the lambda needs to outlive a single suspending block, use `requestConnectionEnabled` instead.

---

## Background chain work

Default `ExtrinsicService` operates on the foreground chain connection. **In background contexts** (WorkManager, foreground Services), use `BackgroundChainConnection.Session`:

```kotlin
backgroundChainConnection.withSession {
    extrinsicService.submitExtrinsic(...)
}
```

---

## Background work conventions

| Use… | When |
|---|---|
| **WorkManager** | Deferred / retryable / OS-scheduled work — sync, upload, periodic refresh. Survives process death. |
| **Foreground Service** | User-initiated active sessions — call, in-progress game, ongoing recording. Visible to the user via a notification. |
| **In-process flow** (`viewModelScope`, `ComputationalScope`) | Anything that lives only while the app is foreground. |

### WorkManager details

- Workers that are **expedited** must override `getForegroundInfo()` — otherwise the worker is silently demoted to non-expedited and may not run promptly.
- Worker scoped to a feature lives in `feature/<X>/impl/.../work/`.
- One-shot vs periodic: pick based on the OS scheduling needs, not on convenience.

### Foreground Service

- Notification with appropriate `foregroundType` (`mediaPlayback` for calls, `dataSync` for sync, etc.).
- Notification cancellation logic lives **in the owning feature**, not in `App.kt`.
- Single source of truth for service state — a `@Singleton` `*StateHolder` rather than the Service class itself, so state remains accessible regardless of whether the service is running.

### Long-running flows

When neither WorkManager nor a Service fits — purely in-app reactive logic — keep the flow inside a feature-owned `@Singleton` class with `context(ComputationalScope)` init. Don't pollute `App.kt`.

---

## Cryptographic key derivation — separate per role

Never share a keypair across roles. Each role (wallet, chat device, identity, alias) gets a derivation path:

- Wallet: `//wallet`
- Chat device key: `//wallet//chat/deviceKey`
- Identity: `//identity` (or whatever the people module defines)
- Aliases: `//alias/<n>`

Sharing keypairs:
- Compromises one role compromises all.
- Identity keypair shared to multiple devices means every device can decode every device's PAPP messages — leaks the threat model.

If you find yourself reusing a keypair for a new role, **stop and derive instead**.

---

## State cleanup — `clear()` and reset

State holders that survive across user sessions (logout, account switch, game session restart) must offer an explicit reset:

```kotlin
@Singleton
class FooStateHolder @Inject constructor() {
    private val internalState = MutableStateFlow(FooState.Empty)
    val state: StateFlow<FooState> = internalState

    fun clear() {
        internalState.value = FooState.Empty
    }
}
```

Forgetting to clear creates leaks across sessions.

---

## Explicit `dispose()` over `try/finally`

For resources that need cleanup, prefer an explicit `dispose()` (or `close()`) method called by the owner, over inline `try { } finally { }` patterns scattered around callers (memory `feedback_explicit_dispose`).

```kotlin
// ✓ — owner controls lifecycle
class ProductTrUAPIHostBridge(...) {
    fun stop() {
        chainProvider.detach()
        chainProvider.closeAll()
        execution?.stopWsBridge()
        execution?.close()
    }
}

// — caller pattern
val bridge = ProductTrUAPIHostBridge(...)
try { ... } finally { bridge.stop() }
```

Internally the owner might still use `try/finally`, but the API surface is a single explicit teardown call.
