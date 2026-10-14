# Concurrency

## Overview

The project is mid-migration from Operation-iOS chains to structured concurrency (async/await). New code should prefer structured concurrency. Legacy code can be bridged.

## Priority Order

1. **Structured concurrency** (async/await) — preferred for all new code
2. **AsyncExtensions** — for reactive async streams (subjects, multicast)
3. **AsyncAlgorithms** (`swift-async-algorithms`) — for stream combinators (`debounce`, `throttle`, `merge`, `combineLatest`, `zip`, `chain`, `AsyncChannel`)
4. **Operation-iOS** (legacy) — bridge to async via `asyncExecute`
5. **Combine** — exists in some areas, not the primary pattern

## Structured Concurrency Package

`Packages/StructuredConcurrency/` provides:
- Bridges from Operation-iOS to async/await (`asyncExecute`)
- Stream wrappers for reactive patterns
- `withRetry` helper for retry logic — **use this instead of custom retry implementations**

## Async Streams

### Preferred Patterns
```swift
// Use AsyncPassthroughSubject from AsyncExtensions
let subject = AsyncPassthroughSubject<Value>()

// Or use AsyncStream.makeStream for custom streams
let (stream, continuation) = AsyncStream<Value>.makeStream()
```

### Don't
```swift
// Don't hack custom AsyncStream initializers
// Don't implement manual storage subscription flows when
// CallbackBatchStorageSubscription.asyncStream exists
```

## Bridging Legacy Code

```swift
// Bridge Operation-iOS to structured concurrency
let result = try await operation.asyncExecute()
```

When touching legacy Operation-iOS code, prefer migrating to async/await if the scope allows.

## AsyncBroadcast Pattern

Actor-based multi-subscriber event distribution:

```swift
// StructuredConcurrency package provides AsyncBroadcast<Event>
actor AsyncBroadcast<Event: Sendable> {
    func newSequence() -> AnyAsyncSequence<Event>
    func yield(_ event: Event)
}
```

Use for one-to-many event distribution where multiple consumers need the same stream.

## Debounce Pattern

When an endpoint is called in rapid succession (e.g., per-peer connection), add debounce:

```swift
// Prefer AsyncAlgorithms' debounce/throttle on the source AsyncSequence:
//   import AsyncAlgorithms
//   for await event in source.debounce(for: .milliseconds(300)) { ... }
// If a custom primitive is needed, add it to StructuredConcurrency.
// Review note: "This endpoint might be called quite frequently...WDYT if we add debounce here?"
```

## Context Actor Pattern

When a service needs several interrelated pieces of **mutable coordination state** — in-flight
tasks keyed by id, per-key subjects, a pending queue, a lifecycle task, a concurrency limit — do
**not** put that state (plus `@unchecked Sendable` and an `OSAllocatedUnfairLock`) in the service.
Extract an `actor` — the *context* — that owns all of it. Actor isolation replaces the lock
(non-reentrant, no `@unchecked Sendable`), the service keeps only immutable stored deps, and the
context is small and unit-testable on its own.

Keep business logic in the **service**; keep only scheduling/state in the **context**. The service
routes each lock-dependent operation through a context method and passes its own work back in as a
`@Sendable` callback the context invokes at the right moment. Results and streams flow back out.

```swift
// Context: owns the mutable state, exposes lock-free-at-the-call-site async methods.
actor IncomingPaymentContext {
    private var lifeCycleTask: Task<Void, Never>?
    private var tasks: [GroupId: Task<Void, Never>] = [:]
    private var subjects: [GroupId: AsyncCurrentValueSubject<Status>] = [:]

    func setup(_ makeTask: () -> Task<Void, Never>?) {           // service supplies the work
        lifeCycleTask?.cancel(); lifeCycleTask = makeTask()
    }
    func process(groupId: GroupId, run: @escaping @Sendable () -> Task<Void, Never>) { … } // dedup + schedule
    func liveStatusStream(for groupId: GroupId,
                          fallback: () async throws -> Status) async throws -> AnyAsyncSequence<Status> { … }
}

// Service: no lock, no mutable state — hands its logic to the context as callbacks.
func setup(with ctx: Context) {
    Task { [weak self, paymentContext] in
        await paymentContext.setup { self?.runSetup(ctx) }       // drive logic lives here, state in the context
    }
}
```

Reach for this when there are **several** interrelated mutable pieces with their own lifecycle. For a
single coalesced in-flight task use `CoalescingTask`; for one typed guarded value
`OSAllocatedUnfairLock<State>` is enough. (Real example: `IncomingPaymentService` +
`IncomingPaymentContext` in `Packages/Coinage`.)

## Injecting Time

Take `DateProviding` from `FoundationExt` (`NowDateProvider` in production) rather than a
`@Sendable () -> Date` closure. `read()` is `async`, so a caller must already be in an async context —
which is exactly where a wall-clock read belongs. Keep a `Clock` alongside it for *pacing* (sleeps,
timeouts): a clock instant cannot be compared against a deadline persisted on an earlier launch, and a
date cannot pace a sleep.

## Key Rules

0. **Check `StructuredConcurrency` and `AsyncExtensions` before writing any custom concurrency
   primitive.** Before hand-rolling a channel, buffer, multicast/broadcast, actor-based feed,
   debounce/throttle, retry, or coalescing task, search these two packages (and `AsyncAlgorithms`)
   first — the primitive almost always already exists:
   - `AsyncBufferedChannel` (AsyncExtensions) — a hot, buffered, **consume-once** channel whose
     buffer and lifetime are independent of any consumer, so a consumer cancelled by `withTimeout`
     does **not** terminate it. This is the Swift equivalent of Kotlin's `produceIn` /
     `ReceiveChannel`; reach for it instead of a bespoke actor + continuation feed.
   - `AsyncPassthroughSubject` / `AsyncCurrentValueSubject` (AsyncExtensions) — reactive subjects.
   - `AsyncBroadcast`, `CoalescingTask`, `withRetry`, `withTimeout` (StructuredConcurrency).
   - `debounce` / `throttle` / `merge` / `combineLatest` / `AsyncChannel` (AsyncAlgorithms).

   Only introduce a new primitive if none fits, and then add it to `StructuredConcurrency` rather
   than inline in a feature. (Lesson: a bespoke `OnChainCoinFeed` actor was replaced by
   `AsyncBufferedChannel`, which it had been re-implementing.)

1. **Use existing `withRetry` from StructuredConcurrency** — don't implement custom retry with manual delays
2. **Use `AsyncPassthroughSubject` or `AsyncStream.makeStream`** — never use `var continuation: AsyncStream.Continuation!` with a separate init
3. **Scope `@MainActor` narrowly** — don't mark an entire Task as `@MainActor`. Keep background work off the main thread, and only `await` the `@MainActor`-bound presenter call:
   ```swift
   // GOOD
   Task {
       let result = try await interactor.fetchData()
       await presenter.didReceive(result: result)  // @MainActor method
   }
   
   // BAD
   Task { @MainActor in
       let result = try await interactor.fetchData()  // Runs on main thread!
       presenter.didReceive(result: result)
   }
   ```
   (review: "No need to make the whole task MainActor, just await presenter")
4. **Don't block startup on network** — separate async setup from required initialization; use cached data when available. Don't await reachability when chains may already be cached
5. **Cancel tasks when replacing** — when a new response replaces a previous one, cancel the old task
6. **Keep original initialization call sites** — when restructuring init flows, keep critical operations (like `fetchRemoteConfig`) at their original call sites to prevent race conditions
7. **Add debounce for frequently called endpoints** — when an endpoint is called per-peer or in rapid succession
8. **Prefer `OSAllocatedUnfairLock` over `NSLock`** for new or refactored lock-based code —
   typed guarded state (`OSAllocatedUnfairLock<State>` + `withLock`), non-reentrant by design,
   no obj-c allocation. `NSLock` remains only in code not yet migrated. For coalescing
   concurrent async calls, reuse `CoalescingTask` (StructuredConcurrency) instead of a manual
   in-flight `Task` map.
9. **Async cleanup from `deinit` captures properties via a capture list, never `self`** —
   capture lists are evaluated eagerly in the deinit body, so the value is copied out before
   the object dies and `self` never escapes:
   ```swift
   // GOOD
   deinit {
       setupTask?.cancel()
       Task { [runtime] in await runtime?.dispose() }
   }

   // BAD — local-copy detour is redundant noise
   deinit {
       let runtime = runtime
       Task { await runtime?.dispose() }
   }

   // BROKEN — captures self in an escaping closure from deinit
   deinit {
       Task { await self.runtime?.dispose() }
   }
   ```
   Swift-6 note: deinit may only read *Sendable* stored properties, and operator/assert
   autoclosures inside deinit count as nonisolated — read flags into locals before combining
   them (`let started = started; assert(!started || disposed, …)`).

## Retry Pattern

```swift
// Good: use StructuredConcurrency's withRetry
try await withRetry(maxAttempts: 3) {
    try await performOperation()
}

// Bad: custom retry with hardcoded delays
// "8 sec will never be used, maybe worth to replace delay seconds with some func"
// Use: func retryDelaySeconds(forAttempt attempt: Int) -> Int { 1 << attempt }
```

## Race Conditions

- Use separate CoreData mappers instead of fetch-modify-save to prevent race conditions
- Fix request factory access race conditions at the source
- Be careful about removing initialization steps that guard against races

## Staleness Reporting

A generic subsystem in `Packages/StructuredConcurrency/Sources/StalenessReport/` for surfacing "this is taking longer than expected" to the user.

**How it works:** Producers publish `StallActivity` values through a `StallActivitySource`. `StallBoard` merges every source, decides visibility, and emits `StallReportSnapshot`. The UI layer maps the snapshot to a view model — the domain package never imports UI.

### Producing Activities

**Option 1: Mark stall regions in call sites**
```swift
try await markStallRegion(
    title: "Submitting extrinsic",
    body: { try await extrinsicService.submit(...) }
)
```
Regions wrap an `async` call and close on return, throw, or cancellation. Nested regions inherit the parent activity.

**Option 2: Custom source adapter**
For domains that already track their own work (e.g., `ExtrinsicSubmissionTracker`), implement a `StallActivitySource`:
```swift
class ExtrinsicStallActivitySource: StallActivitySource {
    // Adapts existing work tracking, emits StallActivity values
}
```

### Task-Local Collector

`StalenessDiagnostics.collector` is a `@TaskLocal`. It propagates into child tasks and unstructured `Task {}`, but **NOT** into `Task.detached`. Work started with `Task.detached` inside an instrumented region reports nothing and vanishes from the report. If detached work is unavoidable, rebind the collector explicitly inside it. Default is `NoOpStalenessCollector.shared`, so uninstrumented code costs nothing.

### Visibility & Reveal Rules

`StallActivity.Visibility` has three states:

- **`.whenStale`** — Revealed once it outlives `StallBoard.revealAfter`
- **`.immediate`** — Revealed at once (use for failures the user must see)
- **`.never`** — Tracked but never revealed on its own

Reveal is **latched**: once an activity is visible it stays on screen until dismissed or it disappears from every source. A later flip to `.never` does not retract it.

**Ownership rule:** A `.never` activity is never reclaimed by the board, so the component that owns the underlying work must remove it when it ends, or it accumulates forever.

### Localization

Region titles (`StallableRegion.title`) are display strings. Localize at the call site, not in the report.
