# Diagnostics and Stall Reporting

A **one-time, user-initiated operation that can get stuck** must report what it is doing. "Stuck" here means the user pressed a button and is now staring at a spinner while the app waits on something it does not control — a chain read, an extrinsic landing, a remote device answering, a value propagating between chains.

The subsystem lives in `common/.../utils/progressStallReport/` and has two halves:

- **write side** — `StalenessReportCollector`, taken as a context parameter, written to with `markRegion`.
- **read side** — `StalenessReport` (a `StalenessReportCollector` + `StalenessReportDisplay`), owned by a ViewModel, rendered by the screen.

Nothing reaches the user while the operation keeps to time. Once it overruns the report's budget (`DEFAULT_REVEAL_AFTER`, 5s), a plain-language notice appears naming the operations still in flight, with the full step tree behind a **Details** sheet. Design rationale: past ~10s an indeterminate spinner reads as "frozen" (NN/g), so the report exists to replace a dead spinner with evidence of life for non-technical users, and with a diagnosable step list for technical ones.

---

## When this applies

**Instrument** — one-shot operations behind a user action:

- extrinsic submission and awaiting execution
- chain state reads on the critical path of that action
- waiting for a value to appear on another chain
- a remote handshake / peer response
- slot, voucher and allowance allocation

**Do not instrument** — nothing renders the report there and the context parameter is pure cost:

- WorkManager jobs and background sync (pass `StalenessReportCollector.NoOp`)
- long-lived subscriptions and feeds
- anything without a screen waiting on it

---

## Write side

### Regions belong to the implementation, not the call site

Whoever knows what a step does is who knows what to call it. A function that can stall takes the collector as a context parameter and names its own step:

```kotlin
context(diagnostics: StalenessReportCollector)
suspend fun claim(destination: AccountId): Result<Unit> = diagnostics.markRegion(RCommon.string.pgas_stall_claiming) {
    resolveClaimContext(destination)
        .flatMap { ctx -> submitClaim(ctx, destination) }
}
```

✗ Never open a region *around* someone else's call at the call site:

```kotlin
// ✗ the caller is guessing at a label for work it does not own
diagnostics.markRegion(RCommon.string.some_label) { pgasClaimer.claim(destination) }
```

### Interface methods take the context parameter directly

When a function on an `api` interface can stall, declare the context parameter on the interface method. Callers with no UI attached supply `NoOp`:

```kotlin
interface PgasClaimer {
    /**
     * Reports its progress into [diagnostics]; callers with no UI attached pass
     * [StalenessReportCollector.NoOp].
     */
    context(diagnostics: StalenessReportCollector)
    suspend fun claim(destinationAccountId: AccountId, strategy: OnExistingAllocationStrategy): Result<Unit>
}
```

```kotlin
// worker / background caller
with(StalenessReportCollector.NoOp) {
    slotAllocator.allocate(target, strategy)
}
```

Do **not** add a parallel `…WithDiagnostics` extension to dodge the signature change — two entry points for one operation is worse than the `with(NoOp)` at the few silent call sites.

### Nesting comes from the callees

Regions form a tree, and nesting is produced by calling another function that declares its own context parameter — **one region per function**. Sub-regions therefore fall out of ordinary decomposition.

A context parameter is *not* an implicit receiver, so `markRegion(...)` cannot be called bare inside a region body. If one function genuinely owns two levels, use `startRegion` and its handle:

```kotlin
val handle = diagnostics.startRegion(ReportAsText(RCommon.string.some_label))
try { … } finally { handle.end() }
```

Prefer `markRegion` everywhere else — it closes the region when the body throws or the coroutine is cancelled.

### Granularity

This is the part that goes wrong. The details sheet is a diagnostic tool, not a trace log.

1. **One region for the operation** — the outermost region is the unit of work phrased for a user ("Allocating Bulletin allowance"). Its label is what the collapsed summary shows.
2. **Combine ordinary chain reads into one region per operation.** Resolving a chain, a period, a collection and reading one balance is *one* `stall_reading_chain_state` region, not four.
3. **Split a read out only when it is clearly huge and carries its own stall risk** — a scan that derives a bandersnatch alias per candidate and then bulk-queries them earns its own step, because it is the thing that actually hangs. See `pickFreeCounter` / `pickFreeSlotIndex` / `loadSlots`.
4. **One region for submission** (`stall_submitting_transaction`), and one for any post-submit wait (awaiting propagation).
5. **Never emit the same label twice in a row within one operation.** Two adjacent identical rows is the signal that rule 2 was skipped.
6. **Do not duplicate a progress model the screen already has.** If the screen renders a stepper/`ProgressCard` naming the phases, regions must cover only the sub-steps that model cannot see — otherwise the user reads the same words twice, in two styles.

Extract each region into its own small private function so the top-level body stays readable:

```kotlin
context(diagnostics: StalenessReportCollector)
private suspend fun resolveClaimContext(destination: AccountId): Result<ClaimContext> =
    diagnostics.markRegion(RCommon.string.stall_reading_chain_state) { … }

context(diagnostics: StalenessReportCollector)
private suspend fun submitClaim(ctx: ClaimContext, destination: AccountId): Result<Unit> =
    diagnostics.markRegion(RCommon.string.stall_submitting_transaction) { … }
```

### Labels

- Live in `common/src/main/res/values/strings.xml` like every other UI string.
- Generic steps shared across features use the unprefixed pair: `stall_reading_chain_state`, `stall_submitting_transaction`. Anything feature-specific is prefixed (`pgas_stall_…`, `statement_store_stall_…`).
- Phrased for the user, in the app's domain vocabulary — "Allocating statement store usage voucher", not "allocate slot seq".
- **No trailing ellipsis.** Every row already carries a progress indicator; `…` is redundant noise.

---

## Read side — UI integration

Instrumenting without wiring the UI is half a feature. The ViewModel owns the report, the Contract exposes it, the screen renders it.

```kotlin
class SendTransferViewModel(private val interactor: SendTransferInteractor) : BaseViewModel(), SendTransferContract {

    override val stalenessReport = StalenessReport(this)

    override fun onConfirmClicked() {
        launchWithDiagnostics(stalenessReport) {
            interactor.sendTransfer(transfer)
        }
    }
}
```

```kotlin
interface SendTransferContract {
    val state: StateFlow<LoadingState<SendTransferUiState>>

    val stalenessReport: StalenessReportDisplay
}
```

```kotlin
@Composable
fun SendTransferScreen(contract: SendTransferContract) {
    …
    contract.stalenessReport.DisplayReport()
}
```

Rules:

1. **One field, not two.** `override val stalenessReport = StalenessReport(this)` — the Contract declares it as `StalenessReportDisplay`, the VM's concrete type serves `launchWithDiagnostics`.
2. **`launchWithDiagnostics(stalenessReport) { … }`** instead of `launch { with(report) { … } }`. It is a `CoroutineScope` extension, so a `BaseViewModel` is already the receiver.
3. **`DisplayReport()` is safe to place unconditionally** — it emits nothing while the operation is on time. Place it where the progress UI already is.
4. **Pass `background` only when the host surface differs** from the default (`bg.surface.nested`).
5. **The work must not run on the main thread.** `launchWithDiagnostics` uses `viewModelScope` (`Dispatchers.Main.immediate`), so the interactor is responsible for `withContext(coroutineDispatchers.computation)` (or `.io`) around anything doing crypto, parsing or blocking I/O. This is the single easiest way to turn this feature into jank.
6. **Previews** use `previewStallReportOperations()` / `previewStallReportSteps()` with `StallReportContent` — a real `StalenessReport` never reveals in a preview, because nothing waits.

### Lifetime

Work driven from `launchWithDiagnostics` dies with the ViewModel. For a prompt that owns the operation, that is a deliberate trade — and the prompt must answer whatever is waiting on it in `onCleared()` so a caller cannot hang forever. If the operation must outlive the screen (an extrinsic is already in flight), keep it in a caller-scoped component and hand the report *to* it rather than moving the work into the ViewModel.

---

## Known limitation

The reveal budget is armed when the report goes from idle to active and cleared when the last root region finishes. **Sequential** root regions therefore restart the budget: an operation of five 4-second phases never reveals, though it took 20s, and a revealed panel can disappear between phases. Until `launchWithDiagnostics` marks a real operation boundary, prefer a single root region per operation, with phases as children.

---

## Rules at a glance

1. One-shot, user-initiated, can-stall → instrument. Background/streaming → `NoOp`.
2. `markRegion` in the implementation; never wrap someone else's call at the call site.
3. `context(diagnostics: StalenessReportCollector)` goes on the `api` interface method; silent callers pass `NoOp`. No `…WithDiagnostics` twins.
4. Nesting comes from callees — one region per function.
5. All ordinary chain reads of an operation collapse into one `stall_reading_chain_state` region.
6. A read gets its own region only if it is clearly huge and stall-prone.
7. One region for submission, one for any post-submit wait.
8. Never two identical labels in a row inside one operation.
9. Never duplicate a progress model the screen already renders.
10. Labels in `common` strings.xml, user-phrased, no trailing `…`.
11. VM: one `stalenessReport` field, `launchWithDiagnostics`, Contract exposes `StalenessReportDisplay`.
12. The instrumented work must leave the main thread in the interactor.

## Canonical examples

- Allocator with the full read/submit/wait shape: `feature/transaction-storage/impl/.../RealTransactionStorageSlotAllocator.kt`
- Same shape, no post-submit wait: `feature/pgas/impl/.../RealPgasClaimer.kt`
- Conditional sub-step (renewer) inside an operation: `feature/statement-store/impl/.../RealStatementStoreSlotAllocator.kt`
- UI integration end to end: `feature/products/impl/.../resourceAllocationRequest/` (ViewModel, Contract, screen, previews)
