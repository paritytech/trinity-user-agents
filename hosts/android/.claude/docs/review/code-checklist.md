# Reviewer: Code Checklist

Walk this for every file the diff touches. Cite the rule's doc and section. Tag severity:

- **blocking** — runtime correctness, data loss, leak, or migration safety.
- **major** — clearly violates a documented code pattern.
- **minor** — naming, comment noise, redundancy.

---

## Result and errors (`code/results-and-errors.md`)

- **blocking** — `getOrThrow()` in a ViewModel, UI mapper, or any main-path code. **`major`** anywhere else outside `Worker.doWork()` and test code. (See `code/results-and-errors.md § getOrThrow — forbidden except at two seams`.)
- **major** — `runCatching { throw … }` ping-pong.
- **major** — `runCatching` around a cancellable suspending call instead of `runCancellableCatching` (cancellation becomes a failed `Result`). (`code/results-and-errors.md § Rules at a glance #10`.)
- **major** — Function returns a hidden default to paper over failure (`.getOrDefault(emptyList())` swallows errors silently).
- **major** — Imperative `.onSuccess { state.value = ... }.onFailure { state.value = ... }` inside non-terminal flows (use functional composition).
- **major** — ViewModel surfacing user-facing **strings** or `@StringRes` resolution; should expose sealed `XxxError` and let Compose resolve.
- **minor** — Method name implies success but returns `Result` and is easy to mis-call.
- **minor** — Custom `Loading | Loaded | Failed` sealed type per feature instead of `LoadingState<T>` + `.withLoading("Tag")`.
- **minor** — `.onFailure { Timber.e(it, "static label") }` instead of `.logFailure("static label")`. The exception is a label that needs non-trivial compute (string interpolation calling a function, formatting a large object); there `onFailure` defers the cost to the failure path while `logFailure`'s `label: String` is eager. (`code/results-and-errors.md § Rules at a glance #10`.)

## State management (`code/state-management.md`)

- **major** — `MutableStateFlow` exposed as state with values pushed from a `launch { ... interactor(...).onSuccess { state.value = ... } }` pattern (push state instead of deriving).
- **major** — UI state patched with `.copy(...)` from multiple methods instead of constructed once via `combine`.
- **major** — `_backing` field with `.asStateFlow()` boilerplate when `MutableStateFlow` can shadow the `StateFlow` Contract property directly.
- **major** — `replayCache[0]` access (use `flow.first()` or convert to `StateFlow`).
- **major** — `combine(...)` with 6+ sources crammed into one block; should pre-combine groups first.
- **major** — `MutableStateFlow` for derived data that could be produced via `combine` / `map`.
- **major** — Reusable cross-screen state (amount, fee, recipient) not extracted into a Mixin (interface in `api/`, factory bound in `impl/`).
- **major** — One-shot UI event from a VM consumed via a hand-rolled `LaunchedEffect(Unit) { flow.collect { ... } }` instead of `Flow<T>.collectAsEffect { ctx, event -> ... }` from `design/utils/Flows.kt`. The extension wraps `repeatOnLifecycle(STARTED)`; the hand-rolled form leaks events while the screen is backgrounded. (See `code/state-management.md § One-shot UI events`.)
- **minor** — Granular states without a clear independence story; check if they produce inconsistent UI (one updates, another stale).
- **minor** — Timer/subscription flow at coarser display precision than the rendered value, without `distinctUntilChangedBy { displayKey }` dedup. (`code/state-management.md § Dedup high-frequency flows on the displayed value`)

## UI / Compose (`code/ui-compose.md`)

- **blocking** — Always-on `Modifier.blur` without an API 31+ guard.
- **major** — Raw Material widget (`Button`, `TextField`, `Surface`, `Text`, `Icon`) instead of Nova wrapper.
- **major** — Modifier mutation inside a Composable (`Box(modifier = modifier.fillMaxWidth())`).
- **major** — Early return inside a `@Composable`.
- **major** — `modifier` parameter not the first parameter.
- **major** — Hardcoded user-facing string (literal `Text("Send")`) instead of `stringResource(RCommon.string.…)`.
- **major** — Hardcoded `Color(0x…)` / `Color.White` instead of `NovaTheme.colors.…`.
- **major** — `Spacer(Modifier.height(...))` instead of `VerticalSpacer { spacingN }`.
- **major** — `NovaTheme.spacings.spacingN` used for corner radius / stroke / element size (only paddings/margins).
- **major** — `BottomSheet` / `AlertDialog` placed in the *internal* screen instead of the public screen.
- **major** — Using `BaseComposeBottomSheet` for a sheet that's purely in-screen with no own ViewModel (use `NovaModalBottomSheet` instead).
- **major** — Using `NovaModalBottomSheet` for a sheet that has its own navigation entry and `BaseViewModel` (use `BaseComposeBottomSheet`).
- **major** — Canvas drawing where an SVG/`ImageVector` would have sufficed.
- **major** — `reverseLayout = false` for the chat feed.
- **major** — Custom item-entry animation instead of `LazyListScope.animateItem()`.
- **major** — `if (canX) Modifier.clickable {...} else this` instead of `Modifier.clickable(enabled = canX) {...}`.
- **major** — `BackHandler` in the Fragment instead of in the screen.
- **major** — Fully-qualified type used inline (`androidx.compose.ui.graphics.Color` without an import).
- **major** — VM Contract / state exposes a raw Android framework widget (`View`, `Bitmap`, `Drawable`, `Window`). Use a domain holder; do the framework interop in the screen.
- **major** — Plain `List`/`Set`/`Map` field in Compose-state, or `ImmutableList<T>` with unstable `T` (sealed model with a plain `List` field, or non-`data class` model). (`code/ui-compose.md § Recomposition and stability`)
- **major** — Compose-state model **or any of its field types** is a plain `class` without value `equals`. A `data class` whose field is an interface backed by a plain class still won't compare equal — check the leaf types, not just the variant. (`code/ui-compose.md § Recomposition and stability`)
- **major** — Scroll-derived `State` (`isScrollInProgress`, `layoutInfo`, `derivedStateOf` result) read at the `LazyColumn` host and passed by value, not pushed to the consumer. (`code/ui-compose.md § Recomposition and stability`)
- **major** — Raw `firstVisibleItemIndex`/`layoutInfo` in composition without `derivedStateOf`. (`code/ui-compose.md § Recomposition and stability`)
- **major** — Per-item `LaunchedEffect` that mutates the VM and re-emits the list (scroll feedback loop). (`code/ui-compose.md § Recomposition and stability`)
- **minor** — `WebView` exposed on a VM Contract (e.g. `StateFlow<WebView?>`) where preloading forces VM ownership. Acceptable but prefer a `WebViewSlot` / `WebViewHolder` wrapper.
- **minor** — Compose file >400 lines that should be split into `components/`.
- **minor** — Per-frame allocations inside a Canvas (data-class copies in a mapNotNull); should back with float arrays.
- **minor** — Odd pixel sizes (3.dp, 5.dp) that will subpixel-jump on different DPIs.

## Naming and hygiene (`code/naming-and-hygiene.md`)

- **major** — `Service`-suffixed class that isn't an Android `Service`.
- **major** — `UseCase` that's a single-line passthrough — propose inlining at the caller.
- **major** — Default value on a data class constructor (request, payload, metadata, domain model).
- **major** — Class-level `var` / `val` only used inside one method.
- **major** — Exposing a mutable field for testability instead of constructor-injecting the collaborator.
- **major** — Bare `Log.x()` instead of Timber.
- **major** — Hardcoded magic constant (timeout, retry count, threshold) without an extracted name.
- **major** — Hand-rolled JSON encoding/decoding (`buildJsonObject { … }` walks, manual `JSONObject`, Gson) instead of `@Serializable` + kotlinx-serialization. Custom wire shapes use a `KSerializer`. Rationale: `code/naming-and-hygiene.md § Rules at a glance #10`; parallels the SCALE rule in `code/database-and-scale.md`.
- **major** — Comment that restates the code ("// loads the user" on a function called `loadUser`), or any non-essential comment. Minimal comments are mandatory: keep one only where the code is genuinely specific and a reader could misread it without it (non-obvious invariant, workaround, platform quirk).
- **minor** — Public method name doesn't reflect what the function actually does.
- **minor** — Trailing positional `Boolean` parameter without a named call site.
- **minor** — `Long` for time when `Duration` / `Instant` would do.
- **minor** — KDoc on an `impl/` class or a self-explanatory method.
- **minor** — `error` log level on an expected branch.
- **minor** — PII in logs (mnemonics, signatures, user-typed content). Account IDs are **not** PII; they may be logged.
- **minor** — Enum representing an ordered / comparable category without a numeric backing field. Comparisons, filters, sorts, or SQL orderings that branch on enum identity / name instead of going through the numeric field. Rationale: `code/naming-and-hygiene.md § Rules at a glance #21`.

## DI and lifecycle (`code/di-and-lifecycle.md`)

- **blocking** — Injecting a class into `App.kt` (or other constructor) **only** to trigger its `init {}` block. Should be an `AppInitializer @IntoSet`.
- **blocking** — Submitting an extrinsic in background work without `BackgroundChainConnection.Session`.
- **blocking** — Reusing a keypair across roles (identity keypair = device keypair).
- **blocking** — State holder missing a `clear()` / reset, causing leaks across sessions.
- **major** — `@AssistedInject` parameter typed as a `@JvmInline value class` (KSP fails).
- **major** — `@Singleton` on a stateless / cheap-to-construct class.
- **major** — Injecting a service's `CoroutineScope` into a VM just to launch a flow (expose `Flow<X>`).
- **major** — `withSessionEnabled { awaitCancellation() }` abuse for long-lived connections; use `requestConnectionEnabled` ref counting.
- **major** — Notification cancellation logic in `App.kt` instead of the owning feature.
- **major** — Expedited Worker without `getForegroundInfo()` override.
- **minor** — `try { ... } finally { dispose() }` patterns at callers when an explicit `dispose()` lifecycle is cleaner.

## Navigation / Routers (`code/navigation-and-routers.md`)

- **major** — ViewModel / Compose / Interactor depending on `NavController`, `Fragment`, or `app/` types.
- **major** — Router interface placed in `feature/<X>/impl/` instead of `api/`.
- **major** — Navigator class placed outside `app/.../navigation/`.
- **major** — Router method named generically (`navigate`, `handleAction`) instead of semantic intent (`openContactDetails`).
- **major** — Router method that does more than navigate (state mutation, suspending business logic).
- **major** — Loose primitives in router method signatures where a payload class is appropriate.
- **major** — Feature directly invoking another feature's router instead of going through a shared mix-in (`SigningRouter`, `ScanRouter`).
- **minor** — Returning data from `router.openX()` instead of using `backWithResult` / `SavedStateHandle`.

## Workers / Background sync (`code/workers-and-background-sync.md`)

- **blocking** — Extrinsic submission inside a `Worker` without `ChainConnectionRefCounter.withConnectionEnabled(...)`.
- **blocking** — Expedited `Worker` request without overriding `getForegroundInfo()`.
- **blocking** — A worker that keeps an in-process executor alive without starting it in `doWork()` (a restart after process death would run with nothing working).
- **blocking** — `distinctUntilChanged` on a Room query `Flow` that triggers work (dropped invalidations can strand a row).
- **major** — Multi-stage worker storing intermediate state in local fields instead of `WorkerStateMachineLocalSession`.
- **major** — `runCatching { ... }.getOrNull()` in `doWork()` that masks failures as `Result.success()`.
- **major** — `Worker` dependencies passed via `WorkerParameters.inputData` primitives instead of `@HiltWorker` / `@AssistedInject`.
- **major** — `Worker` enqueued directly from a ViewModel — route through an interactor / domain entry point.
- **major** — Two cleanup verbs on the state holder (`clear()` and `endSession()`) with subtly different semantics.
- **minor** — Missing `runAttemptCount` cap, unique work name, or `ExistingWorkPolicy`.

## Diagnostics / stall reporting (`code/diagnostics-and-stall-reporting.md`)

- **blocking** — Work driven by `launchWithDiagnostics` (i.e. `viewModelScope`, `Dispatchers.Main.immediate`) doing crypto, parsing or blocking I/O without `withContext(coroutineDispatchers.computation/io)` in the interactor. (`§ Read side — UI integration #5`.)
- **major** — A new one-shot, user-initiated operation that can stall (extrinsic submission, critical-path chain read, remote handshake, cross-chain wait) with no regions at all. (`§ When this applies`.)
- **major** — Regions added on the write side with no UI integration: no `StalenessReport` on the ViewModel, no `StalenessReportDisplay` on the Contract, or no `DisplayReport()` on the screen. Half a feature.
- **major** — `markRegion` wrapping a *collaborator's* call at the call site instead of living in that collaborator's implementation. (`§ Regions belong to the implementation`.)
- **major** — A `…WithDiagnostics` extension twin added next to an interface method instead of putting `context(diagnostics: StalenessReportCollector)` on the method itself. (`§ Interface methods take the context parameter directly`.)
- **major** — Two adjacent steps with the same label inside one operation — ordinary chain reads were not collapsed into a single `stall_reading_chain_state` region. (`§ Granularity #2, #5`.)
- **major** — A region per individual chain read (resolve chain, resolve period, read balance as three steps) where one read region is the rule; the exception is a genuinely huge, stall-prone scan. (`§ Granularity #2, #3`.)
- **major** — Regions duplicating a progress model the screen already renders (stepper / `ProgressCard` phases named twice). (`§ Granularity #6`.)
- **major** — Region label hardcoded in Kotlin instead of `common` strings.xml, or a `StalenessReport` UI string added outside it.
- **major** — Two fields for one report (`private val diagnostics = StalenessReport(this)` plus `override val stalenessReport = diagnostics`). (`§ Read side #1`.)
- **major** — A prompt that owns an operation and does not answer its waiting caller in `onCleared()`. (`§ Lifetime`.)
- **minor** — Trailing `…` on a stall label; the row already carries a progress indicator. (`§ Labels`.)
- **minor** — `startRegion` + manual `end()` where `markRegion` would close the region on throw/cancellation.
- **minor** — `launch { with(report) { … } }` instead of `launchWithDiagnostics(report) { … }`.
- **minor** — New `StallReportContent` preview building its own fixture instead of `previewStallReportOperations()` / `previewStallReportSteps()`.

## Database / SCALE (`code/database-and-scale.md`)

- **blocking** — Modifying a persisted SCALE schema in place (no `*V<N>.kt` copy, no migration).
- **blocking** — Removing / reordering / retyping fields or inserting an enum variant in the middle without a SCALE migration.
- **blocking** — Editing existing hex in a SCALE conformance test.
- **blocking** — Room schema bump without a corresponding `Migration(N, N+1)` and schema test.
- **major** — New SCALE type added without a conformance test.
- **major** — Manual binary encoder when `BinaryScale` would work.
- **major** — Repository accessing `Preferences` directly instead of via a typed `XxxStorage` interface.
- **major** — Feature-specific entity in shared `database/.../entity/` without a feature-prefixed class name.
- **major** — DAO returning `List<…>` to check existence; should be `SELECT EXISTS(...)`.
- **minor** — New entity using legacy `Entry<N>Encoders` backward-compat layer (only for migration of old data).

---

## Git / PR hygiene (always minor)

- **minor** — PR mixes 2+ unrelated tasks without a justification in the description.
- **minor** — Visible UI change without a demo video / screenshot attached to the PR.
- **minor** — Commit count / scope inconsistent with the PR description (looks like leftover work from another branch).
- **minor** — Test file present but unrelated to the change (drift from prior work).

---

## How to write a comment

Bad:
```
This code is bad. Please refactor.
```

Good:
```
**blocking** `<file>:<line>` —
Renderer state held inside the bot. (`architecture/chat-extension.md § Bot rules`)

Fix: move the overlay state to a `@Singleton` state holder and have the
overlay renderer host its own `*OverlayViewModel` that reads from the holder.
```

Always:
1. Tag severity.
2. Quote `file:line`.
3. Cite the doc section.
4. Propose the concrete fix in one or two sentences.

---

## Verdict format

```
### Code
- 1 blocking, 6 major, 11 minor.
- UI section: 4 major (raw Material widgets, hardcoded strings, modifier mutation, no NovaSurface).
- State management: 1 blocking (push-state pattern from a coroutine) + 2 major.
- Naming/hygiene: 11 minor — primarily noise comments and magic constants.

**Verdict:** Not mergeable as-is.
- Address blocking first (state push pattern).
- Major UI cleanups before second-pass review.
- Minors can be batched in a follow-up if the architecture-blocking items extend the PR scope.
```
