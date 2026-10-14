# Flow Operators Reference

> **Loaded on demand only.** The implementer/architect don't auto-load this file; `state-management.md` cites the most-used operators inline. Pull this doc when you need the full catalog for an unusual case.

Catalog of Flow helpers in `common/.../utils/Flows.kt`. Reach for these before writing your own — most recurring shapes already have a name.

## Threading

| Operator | What it does | When to use |
|---|---|---|
| `Flow<T>.inBackground()` | `flowOn(Dispatchers.Default)` | Wrap CPU/IO producers so the consumer never blocks the main thread. |
| `Flow<T>.shareInBackground(started = Eagerly)` (`context(CoroutineScope)`) | `inBackground()` + `share(started)` | One upstream computation feeding multiple consumers. |
| `Flow<T>.stateInBackground(initial)` (`context(CoroutineScope)`) | `inBackground()` + `stateIn(..., initial)` | Default for VM-exposed `StateFlow` derived from heavy upstreams. |
| `Flow<T>.stateInBackgroundWithLoading(failureLog?)` (`context(CoroutineScope)`) | `inBackground() + withLoading(...) + stateIn(..., Loading)` | **Most common VM terminal** for `Flow<Result<T>>`. |
| `Flow<T>.shareLazily()` / `Flow<T>.shareWhileSubscribed()` | Variants with different sharing semantics | `WhileSubscribed` when subscribers come and go; default to `Eagerly` for cached read paths. |

## Result wrapping and mapping

| Operator | What it does |
|---|---|
| `Flow<T>.wrapInResult()` | Catches exceptions emitted upstream and maps to `Flow<Result<T>>`. |
| `Flow<Result<T>>.mapResult { ... }` | Inline-map the success value; leaves failures untouched. |
| `Flow<Result<T>>.transformResult { ... }` | Reified flat-map: chain a `Flow<Result<R>>` from each success. |
| `Flow<Result<T>>.combineResult(other) { a, b -> r }` | `combine` over two `Result` streams, producing `Result<R>`. |
| `Flow<Result<T?>>.filterResultSuccess()` / `filterResultSuccessNotNull()` | Strip failures and (optionally) nulls. |
| `Flow<Result<T>>.logFailure("Tag")` | Timber-log failures as they pass through. |
| `Flow<Result<T>>.withLoading("Tag")` | Map to `Flow<LoadingState<T>>`; emit `Loading` on subscribe. |
| `Flow<T>.withMapLoading { x -> repo.fetch(x) }` | Switch to a `Flow<LoadingState<R>>` derived per upstream value; cancels on new upstream. |

## Producers

| Operator | What it does |
|---|---|
| `flowOf { producer() }` (suspend) | `flow { emit(producer()) }` — terse one-shot producer. |
| `flowOfAll { producer() }` | One-shot suspend producer returning a `Flow<T>` to switch into. |
| `Any.asFlow()` | `flowOf(this)` — emit a single value. |

## Combinators

| Operator | What it does |
|---|---|
| `combineToPair(flowA, flowB)` / `combineToTriple(a, b, c)` | `combine(...)` packaging the result as `Pair`/`Triple`. |
| `Flow<T>.zipWithPrevious()` | Emits `(previous?, current)` pairs — for transition-aware logic. |
| `Flow<T>.filterWithPrevious { prev, curr -> ... }` / `filterWithPreviousIgnoreFirst` | Filter using both current and prior value. |
| `Flow<T>.debounceIndexed { index, value -> Duration }` | Variable-duration debounce keyed by emission index. |
| `Flow<List<T : Identifiable>>.diffed()` | Emits `CollectionDiffer.Diff<T>` describing inserts/removes/changes against the previous list. |
| `Flow<List<T : Identifiable>>.transformLatestDiffed { ... }` | Per-element `transformLatest` driven by the diff — cancels the per-item flow when the item is removed. |
| `Flow<List<T>>.mapList { ... }` / `mapListNotNull { ... }` | Inline element mapping over a list inside a flow. |

## Lifecycle helpers

| Operator | What it does |
|---|---|
| `Flow<V>.observe(collector)` (`context(LifecycleOwner)`) | `launchWhenStarted { collect { … } }` — canonical "subscribe in a Fragment" call. |
| `Flow<V>.observeWhenCreated(collector)` (`context(LifecycleOwner)`) | Same, scoped to `CREATED`. |
| `OneShotEventChannel<T>()` | `Channel<T>(Channel.CONFLATED)` — standard one-shot event channel for VM → screen events. |

## Rules

1. **`major`** — Reach for the catalog first. Don't manually `flowOn(Dispatchers.Default)` in feature code; `inBackground()` is the idiom.
2. **`major`** — Don't open-code `.onStart { emit(Loading) } + .map { Loaded(it) }` chains; `.withLoading("Tag")` does it.
3. **`major`** — Don't open-code `combineToPair`; use the helper.
4. **`major`** — `transformLatestDiffed` over manual diffing when a list flow drives per-item subscriptions.
5. **`major`** — `OneShotEventChannel` over re-implemented `Channel(Channel.CONFLATED)`.
6. **`minor`** — `flowOf { suspend }` over `flow { emit(suspend()) }` for single-emission producers.
7. If you reach for a new operator 3+ times, **add it to `Flows.kt`** rather than duplicating. The catalog is descriptive of practice — new shared idioms belong there, and in this doc.
