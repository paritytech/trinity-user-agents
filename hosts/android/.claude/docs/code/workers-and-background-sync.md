# Workers and Background Sync

Long-running, retryable, or OS-scheduled work runs as **WorkManager `CoroutineWorker`**. Many of these jobs are stateful — they're a sequence of stages that must resume mid-flight after process death — and use the project's `WorkerStateMachine` abstraction. This doc codifies both the WorkManager conventions and the state-machine recipe.

For decision rules between WorkManager / foreground Service / in-process flow, see `code/di-and-lifecycle.md § Background work conventions`.

---

## Plain `CoroutineWorker` rules

`androidx.work.CoroutineWorker` is the base for every background job. With Hilt, use the `@HiltWorker` annotation and the `HiltWorkerFactory` already wired in `App.kt`.

```kotlin
@HiltWorker
class FooSyncWorker @AssistedInject constructor(
    @Assisted context: Context,
    @Assisted params: WorkerParameters,
    private val interactor: FooSyncInteractor,
) : CoroutineWorker(context, params) {

    override suspend fun doWork(): Result = runCatching {
        interactor.runSync().getOrThrow()
    }.fold(
        onSuccess = { Result.success() },
        onFailure = { failure ->
            Timber.e(failure, "FooSyncWorker failed")
            if (runAttemptCount < MAX_RETRIES) Result.retry() else Result.failure()
        }
    )

    companion object {
        const val NAME = "foo-sync"
        private const val MAX_RETRIES = 5
    }
}
```

### Rules

1. **`@HiltWorker` + `@AssistedInject`** with `@Assisted Context` and `@Assisted WorkerParameters` — never inject deps via `WorkerParameters.inputData`. Use a typed wrapper if you need parameters.
2. **`getOrThrow()` here is one of the two legitimate seams** (see `code/results-and-errors.md § getOrThrow`). `doWork()` is where domain `Result<Unit>` becomes WorkManager `Result.retry/failure`. Centralize the unwrap, log the throwable, never let it bubble unwrapped.
3. **Always return `Result.retry()` for transient failures** (network, chain disconnect, lock contention). Bound the retries with `runAttemptCount` to avoid infinite loops.
4. **Always return `Result.failure()` for unrecoverable failures** (invalid state, deleted account). Don't silently `Result.success()` to swallow.
5. **Use `Constraints.Builder()`** to declare network / battery / charging needs at enqueue time:
   ```kotlin
   val request = OneTimeWorkRequestBuilder<FooSyncWorker>()
       .setConstraints(Constraints.Builder().setRequiredNetworkType(CONNECTED).build())
       .build()
   ```
6. **Periodic vs one-shot** — `PeriodicWorkRequestBuilder` only when the OS scheduling is what you want. For "run when X happens", an event-driven path (subscription + one-shot enqueue) is usually better.
7. **Unique work names** — every enqueue must specify a unique name and a `ExistingWorkPolicy` (`KEEP` for idempotent, `REPLACE` when re-enqueuing supersedes). Constants in the worker companion.
8. **Foreground / expedited workers** — if you mark a worker as expedited via `setExpedited(...)`, you **must** override `getForegroundInfo()`. Otherwise WorkManager silently demotes the worker to non-expedited and it may not run promptly.
9. **In-process executors a worker keeps alive are started in `doWork()`** — a worker restarted after process death runs without the UI-driven starters, so anything it only hosts (e.g. `DurableSubmissionExecutor`) must be started by the worker itself.
10. **A Room query `Flow` used as a work trigger must not be `distinctUntilChanged`** — Room drops intermediate invalidations, so two equal emissions can hide a row that left and came back.
11. **Background chain access** — if the worker submits extrinsics or holds a long subscription, wrap in `ChainConnectionRefCounter.withConnectionEnabled(...)` (`architecture/transactions.md § Background chain work`).
12. **No UI work in `doWork`** — notification posting belongs in a separate helper that takes the result. Workers should be pure logic.

### Enqueue from a feature

The worker class lives in `feature/<X>/impl/.../data/worker/`. The enqueue helper lives next to it:

```kotlin
class FooSyncEnqueuer @Inject constructor(@ApplicationContext private val context: Context) {

    fun enqueue() {
        val request = OneTimeWorkRequestBuilder<FooSyncWorker>()
            .setConstraints(...)
            .build()

        WorkManager.getInstance(context).enqueueUniqueWork(
            FooSyncWorker.NAME,
            ExistingWorkPolicy.KEEP,
            request,
        )
    }
}
```

Inject `FooSyncEnqueuer` into the feature's interactor / event handler. **Don't enqueue from a ViewModel directly** — that bakes in lifecycle assumptions; route through the domain layer.

---

## `WorkerStateMachine` — stateful resumable work

For workers whose work is a **sequence of stages** that must survive process death (claim → confirm → notify; allocate → sign → submit; download → decrypt → import), use `BaseWorkerStateMachine` from `common/.../data/worker/stateMachine/`.

### Shape

```kotlin
interface WorkerStateMachine<S : WorkerStateMachineState<S, *>> {
    suspend fun createCurrentState(): Result<S>
    suspend fun performTransition(state: S): TransitionResult<S>
}
```

A state is a concrete sealed-class branch (`Initial`, `WaitingForConfirmation(txHash)`, `Completed(result)`, `UnrecoverableFailure(error)`). Each branch knows how to transition forward, or signals terminal success/failure.

### The driver

```kotlin
suspend fun WorkerStateMachine<S>.executeUntilPossible(): Result<Unit> {
    var current: TransitionResult<S> = TransitionResult.TransitionPerformed(createCurrentState())
    while (current is TransitionResult.TransitionPerformed && current.outcome.isSuccess) {
        current = performTransition(current.outcome.getOrThrow())
    }
    return when (current) {
        TransitionResult.StateTerminal -> Result.success(Unit)
        is TransitionResult.TransitionPerformed -> current.outcome.map { Unit }
    }
}
```

Loop until terminal state or a transition fails. Inside `doWork`, call:
```kotlin
override suspend fun doWork(): Result = stateMachine.executeUntilPossible().fold(
    onSuccess = { Result.success() },
    onFailure = { Result.retry() },
)
```

### Persistence

`WorkerStateMachineLocalSession<S>` is the storage interface for "what state were we in?". Implementations typically back to `SharedPreferences` (or `EncryptedPreferences` for sensitive state). The `BaseWorkerStateMachine` reads on entry and writes after every successful transition — so when the OS kills the process and WorkManager retries, the next run picks up where the previous one stopped.

### Unrecoverable failures

A state can transition to `UnrecoverableFailureState<S>` carrying a `retryState: S`. On the next `createCurrentState()` call, the base class detects this, logs, and restarts from `retryState` (typically `Initial`). Use this when a failure is bad enough that resuming from the current state is meaningless (e.g. signature failed → restart from the very beginning) but the *job itself* isn't dead.

### When to use

- Multi-stage on-chain operations where each stage is a separate extrinsic or wait.
- Upload/download with chunked progress that must resume.
- Anything where the process can die between stages and re-running from scratch would be wrong (double-spend risk, duplicate state changes).

### When NOT to use

- Single-step jobs — overkill. Use a plain `CoroutineWorker`.
- Workflows entirely inside one suspend call with no observable intermediate state.

---

## Foreground workers and `getForegroundInfo`

Some workers need to surface progress as a notification (long uploads, syncs the user is watching). These workers:

1. Call `setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)` on the request builder, **or** the OS will demote them.
2. Override `getForegroundInfo()`:
   ```kotlin
   override suspend fun getForegroundInfo(): ForegroundInfo {
       val notification = notificationFactory.createForWorker(
           channelId = NotificationChannels.SYNC,
           title = context.getString(RCommon.string.sync_in_progress),
       )
       return ForegroundInfo(WORKER_NOTIFICATION_ID, notification)
   }
   ```
3. Optionally call `setForeground(getForegroundInfo())` from inside `doWork` to surface the notification immediately.

Failing to override `getForegroundInfo` while requesting expedited execution causes WorkManager to throw — and even when it doesn't throw, the worker won't run as expedited.

---

## Where new things live

| Concept | Goes in |
|---|---|
| `CoroutineWorker` class | `feature/<X>/impl/.../data/worker/<Name>Worker.kt` |
| Enqueue helper | `feature/<X>/impl/.../data/worker/<Name>Enqueuer.kt` |
| `WorkerStateMachine` implementation | `feature/<X>/impl/.../data/worker/stateMachine/<Name>StateMachine.kt` |
| State sealed class | `feature/<X>/impl/.../data/worker/stateMachine/<Name>State.kt` |
| Local session (persistence) | `feature/<X>/impl/.../data/worker/stateMachine/<Name>LocalSession.kt` |
| Hilt module binding the enqueuer / state machine | the feature's `di/` package |

---

## Anti-patterns

| Anti-pattern | Fix |
|---|---|
| `Result.success()` after catching a failure | use `Result.retry()` / `Result.failure()` correctly |
| Injecting deps via `inputData` instead of `@AssistedInject` | use the Hilt-worker entry point |
| Enqueueing from a ViewModel directly | route through an interactor / domain entry point |
| Expedited worker without `getForegroundInfo` override | always override |
| Submitting an extrinsic without `ChainConnectionRefCounter` | wrap chain calls in `withConnectionEnabled` |
| Storing state in a `var` field inside the worker | use the `WorkerStateMachineLocalSession` so it survives process death |
| Two cleanup verbs on the state holder (e.g. `clear()` and `endSession()`) | one terminal transition, named for what it represents |
| Worker that grows into a multi-stage flow without adopting the state machine | refactor to `BaseWorkerStateMachine` once stages ≥ 2 |

---

## Reviewer flags

- **blocking** — extrinsic submission inside a `Worker` without `ChainConnectionRefCounter`.
- **blocking** — expedited worker request without `getForegroundInfo()` override.
- **major** — multi-stage worker storing intermediate state in local fields instead of `WorkerStateMachineLocalSession`.
- **major** — `runCatching { ... }.getOrNull()` in `doWork` that swallows failures into `Result.success()`.
- **major** — dependency injected through `WorkerParameters.inputData` typed primitives.
- **major** — worker enqueued from a ViewModel directly.
- **minor** — missing `runAttemptCount` cap; missing unique work name; missing `ExistingWorkPolicy`.

---

## Canonical examples

- Stateful worker: `ExternalPaymentWorker` (`feature/coinage/impl/.../domain/externalPayment/`) — drives `ExternalPaymentStateMachine`, a `BaseWorkerStateMachine`.
- File-pipeline worker: `FileUploadWorker` (`feature/chats/impl/.../data/hop/upload/`), HOP upload with a `getForegroundInfo()` notification surface.
- Simple one-shot: any of the notification cancellation enqueuers, plain `CoroutineWorker` with no state machine.
