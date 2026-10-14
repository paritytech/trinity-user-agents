# Result and Error Handling

Fallible operations return `Result<T>`. Errors carry domain meaning via sealed `XxxError` types. UI maps errors to string resources at the rendering layer, never inside the ViewModel.

## Rules at a glance

1. **`major`** — Fallible domain/data operations return `Result<T>`. Don't throw across a layer boundary.
2. **`major`** — `getOrThrow()` is forbidden everywhere except `Worker.doWork()` (the Result → WorkManager-Result seam) and test code. **`blocking`** when in a ViewModel, UI mapper, or any main-path code. (The PostToolUse hook will warn on `.kt` files outside `Worker` / tests.)
3. **`major`** — Compose `Result` chains via `flatMap` / `mapCatching` / `flatRecover`; don't unwrap with `.onSuccess { state.value = ... }` inside non-terminal flows.
4. **`major`** — `runCatching { throw ... }` ping-pong is forbidden. Return `Result.failure(...)` directly, or `runCatching` only ambient-throwing bodies.
5. **`major`** — Don't paper failures with a hidden default; propagate `Result` and let the caller decide.
6. **`major`** — ViewModels never carry user-facing text. Errors → sealed `XxxError` types → `@Composable` mapper that resolves to `stringResource(...)`.
7. **`major`** — Reuse `LoadingState<T>` + `.withLoading("Tag")`. Don't invent per-feature `Loading | Loaded | Failed` sealed hierarchies.
8. **`minor`** — Method name that implies success but returns `Result` and is easy to mis-call — make the returned object the only operate-able instance, or rename.
9. **`minor`** — `Throwable.message` straight into UI without a sealed-type mapping — wrap the failure or define an error variant.
10. **`major`** — `runCatching` around a suspending call that can be cancelled (waits, subscriptions, delays) is `runCancellableCatching`: plain `runCatching` turns cancellation into a failed `Result`.
11. **`minor`** — Use `.logFailure("label")` over `.onFailure { Timber.e(it, "label") }` for failure logging. Exception: when the log label requires non-trivial compute (string interpolation of an expensive call, formatting a large object) — then `.onFailure { Timber.e(it, expensiveLabel()) }` defers the compute to the failure path. `logFailure`'s `label: String` is eager.

---

## Result<T> — the contract

Every fallible domain or data operation returns `Result<T>`. The caller decides how to handle failure; the callee never throws across a layer boundary.

### Where to find the helpers

`common/src/main/.../utils/Result.kt` defines:

- `Result<T>.flatMap(transform: (T) -> Result<R>): Result<R>` — sequential chaining.
- `Result<T>.mapCatching(transform: (T) -> R): Result<R>` — wraps a synchronous step.
- `Result<T>.logFailure(label: String): Result<T>` — Timber-log on failure, pass-through.
- `Result<Result<T>>.flatten(): Result<T>` — collapse nested Results.
- `Result<List<T>>.mapList(mapper: (T) -> R): Result<List<R>>` — map inside list.
- `Result<T>.flatRecover(recover: (Throwable) -> Result<T>): Result<T>` — recover with another Result.

---

## ✓ Patterns

### Chain via `flatMap`

```kotlin
suspend fun approveHandshake(offer: HandshakeOffer, metadata: HostMetadata): Result<Unit> =
    ssoHandshakeUseCase.respondToHandshake(offer)
        .flatMap { saveSession(offer, metadata) }
```

### Wrap a side-effect with `mapCatching`

```kotlin
suspend fun load(): Result<Foo> =
    repository.fetchFoo()
        .mapCatching { foo -> validate(foo); foo }
```

### Recover with fallback only when fallback is semantically valid

```kotlin
suspend fun usernameFor(accountId: AccountId): Result<String> =
    repository.getUsername(accountId)
        .flatRecover { ex -> if (ex is NotFoundException) Result.success(DEFAULT_NAME) else Result.failure(ex) }
```

### Flow of Results → `withLoading`

```kotlin
override val state = combine(metadataFlow, approving) { metadata, approving ->
    metadata.map { PairRequestUiState(it.name, approving = approving) }
}
    .withLoading("PairRequest")
    .stateIn(this, SharingStarted.Eagerly, LoadingState.Loading)
```

`Flow<Result<T>>.withLoading("Tag"): Flow<LoadingState<T>>` emits `Loading` first, maps `Result.success` to `Loaded(value)`, `Result.failure` to `Error(throwable)`. The `Tag` is the Timber label for failure logs.

### Reuse `LoadingState<T>`

`LoadingState<T>` (in `common/.../presentation/loading/`) is the canonical three-state UI value. **Don't invent feature-specific `Loading | Loaded | Failed` sealed hierarchies** (memory `feedback_loading_state`).

---

## ✗ Anti-patterns

### `getOrThrow()` — forbidden except at two seams

```kotlin
// ✗ Anywhere on the main path
val metadata = metadataResult.getOrThrow()   // crashes on failure
```

`getOrThrow()` defeats the purpose of returning `Result<T>` — the caller's whole point of having a `Result` is that they decide how to handle failure. Composing with `flatMap` / `mapCatching` / `recoverCatching` surfaces failures explicitly to the next caller up.

```kotlin
// ✗ Anywhere
val account = accountResult.getOrThrow()
doSomething(account)

// ✓ Compose
return accountResult.flatMap { account -> doSomething(account) }
```

**The two legitimate seams** where `getOrThrow()` is allowed:

1. **Inside `Worker.doWork()`** — the seam where domain `Result<Unit>` becomes WorkManager `Result.retry / failure`. Centralize the unwrap there, log the throwable, return the appropriate WorkManager result.
2. **Test code** (`*/src/test/*`, `*/src/androidTest/*`) — assertions over `Result` are easier with `getOrThrow()` than with verbose `fold`.

Anywhere else: forbidden. Severity is `major` in source docs (PostToolUse hook will warn). **`blocking`** when the call is in a ViewModel / UI mapper / main-path code (the reviewer escalates).

The one legacy exception sometimes seen — `runCatching { … }` wrapping a body that needs to re-throw a checked exception into the wrapper — is rarely the cleanest form. Prefer `Result.failure(...)` directly.

(PR comments repeatedly: "avoid getOrThrow as much as possible".)

### `runCatching { throw ... }` ping-pong

```kotlin
// ✗
return runCatching {
    if (!ok) throw IllegalStateException("bad")
    doWork()
}
```

Throwing only to immediately catch is theater. Either:
```kotlin
// ✓ pure Result return
return if (!ok) Result.failure(IllegalStateException("bad")) else Result.runCatching { doWork() }
```
or, when the whole body is genuinely catching ambient exceptions:
```kotlin
// ✓ runCatching the whole body
return runCatching { doWork() }
```

### Returning a default to paper failure

```kotlin
// ✗
suspend fun getNotUsedCounterIndices(): List<Int> = runCatching { ... }.getOrDefault(emptyList())
```

The caller has no idea this silently swallowed an error. Return `Result<List<Int>>` and let the caller decide.

### Imperative `.onSuccess`/`.onFailure` cascading

```kotlin
// ✗
override fun backupOverrideIntention() {
    interactor.createAccountsAndOverrideBackup()
        .onSuccess { router.backWithResult(...) }
        .onFailure { showError(it) }
}
```

Acceptable for terminal actions (button presses), but inside a flow pipeline prefer composing with `flatMap` / `mapCatching`. When the action is genuinely terminal, this shape is fine — but don't reach for `.value =` inside the callbacks (see `state-management.md`).

### Method name implies success but returns Result

```kotlin
// ✗ Looks safe; isn't.
fun initialize(): Result<Engine> = ...
engine.someMethod()  // — caller forgot to unwrap
```

Make the consumer impossible to mis-call:
```kotlin
// ✓
suspend fun waitForReady(): Result<Engine>
// Caller pattern:
waitForReady().map { engine -> engine.someMethod() }
```

If the returned object is the *only* way to operate, callers can't skip the unwrap.

---

## Sealed error types — the UI mapping pattern

Domain operations that can fail in user-meaningful ways expose a **sealed error type**, not a bare `Throwable`:

```kotlin
// feature/transfers/api/.../domain/error/TransferError.kt
sealed interface TransferError {
    data object InsufficientBalance : TransferError
    data class AmountBelowMinimum(val minimum: Balance) : TransferError
    data class NetworkUnavailable(val cause: Throwable) : TransferError
    data class Unknown(val cause: Throwable) : TransferError
}

// Result carries the sealed type:
suspend fun submitTransfer(...): Result<TxHash> = ...
// where failure exceptions are wrapped:
typealias TransferException = DomainException<TransferError>
```

The VM exposes the sealed type via a `LoadingState.Error(TransferError.…)` or a one-shot `XxxFailed` event. The screen has a tiny **mapper** that resolves each variant to a `@StringRes`:

```kotlin
// feature/transfers/impl/.../presentation/transfer/TransferErrorMapper.kt
@Composable
fun TransferError.toUserMessage(): String = stringResource(when (this) {
    is TransferError.InsufficientBalance -> RCommon.string.transfer_error_insufficient
    is TransferError.AmountBelowMinimum  -> RCommon.string.transfer_error_below_minimum
    is TransferError.NetworkUnavailable  -> RCommon.string.transfer_error_network
    is TransferError.Unknown             -> RCommon.string.transfer_error_unknown
})
```

Rules:
- **ViewModels never carry user-facing text.** Not raw strings, not `@StringRes Int` plumbed through state (the resource id is presentation, but the *resolution* happens in Compose). The cleanest version is: VM exposes the sealed error; Compose resolves.
- **`Throwable`** still flows through `Result.failure` for unexpected/unmapped errors; the sealed type wraps the known cases.
- **One mapper per screen / feature**, co-located with the screen.

The Kiosk feature is the canonical reference for moving hardcoded VM error strings into a sealed + resource-mapped form.

---

## Logging discipline

Logging policy lives in `naming-and-hygiene.md § Logging`. Quick summary here:

- Timber-only. No bare `Log.x`.
- `error` — unexpected, actionable; alarms the developer.
- `warn` — expected failure path the user might see.
- `info` — lifecycle, important state transitions.
- `debug` — developer diagnostics; stripped from release.

Use `.logFailure("Tag")` on `Result`/`Flow<Result>` to centralize. Prefer it over the equivalent `.onFailure { Timber.e(it, "Tag") }` — same behavior, less noise.

**One exception:** `logFailure(label: String)` evaluates `label` eagerly at the call site. If the label involves non-trivial compute — a string template with a method call, a `toString()` on a large object, a formatter — write `.onFailure { Timber.e(it, expensive()) }` instead so the cost is paid only on the failure path. Static / cheap labels stay on `logFailure`.

---

## Result<Unit> — when

`Result<Unit>` is the standard return for fallible operations that produce no value. Prefer it over throwing.

`.coerceToUnit()` on a `Result<T>` discards the value when you genuinely don't care:
```kotlin
override fun onConfirm() {
    launch {
        interactor.onboard()
            .onSuccess { ... }
            .onFailure { showError(it) }
            // no Result<Unit> propagation needed — this is a terminal click handler
    }
}
```
