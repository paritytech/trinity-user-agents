# Time, Async and Flakiness

A unit test must give the same result on an idle laptop and on a CI runner at 100% CPU. Real
time and real scheduling are the main reasons it does not.

## Two kinds of time, two seams

| Need | Production dependency | Test double |
|------|-----------------------|-------------|
| Wait / sleep / timeout / retry backoff / debounce | `any Clock<Duration>`, default `ContinuousClock()` | `ManualClock` (`StructuredConcurrencyTestSupport`) |
| Read the wall-clock date (TTL, expiry, timestamps) | `DateProviding`, `FoundationExt` (default `NowDateProvider`) | Fixed or movable provider |

Rules for production code under test:

- Never call `Task.sleep`, `Task.sleep(nanoseconds:)`, `DispatchQueue.asyncAfter`, `Timer` or
  `Date()` in logic that has a test. Take the clock / date provider in `init` with the real
  default, as `StatementRenewalLoop` and `ChatAccountFunding` do.
- `withTimeout(_:clock:operation:)` accepts the injected clock; use it when the timeout is
  part of the behavior so the test can drive it.
- Pass `withTimeout` the injected clock if the timeout is part of the behavior.
- Prefer one `clock.sleep(for:)` per decision point, with the duration in a named constant the
  test can reference (`StatementRenewalLoop.interval`).

## ManualClock: how to use it

`ManualClock` finishes sleeps only when the test resumes them, by duration. It is
registration-safe: `resumeSleep` first waits until the code under test is actually suspended in
`sleep`, so there is no "advanced before the sleep was registered" race (the main flakiness
source with naive test clocks).

Reference test: `polkadot-appTests/StatementRenewal/StatementRenewalLoopTests.swift`. An hour-long
loop is tested in milliseconds, with no real wait and synchronization on events only:

```swift
@Suite(.timeLimit(.minutes(5)))
struct StatementRenewalLoopTests {
    @Test func renewsOnSetupAndAgainAfterEachInterval() async {
        let renewal = StatementDurabilityRenewalSpy()
        let clock = ManualClock()
        let loop = makeLoop(renewal: renewal, clock: clock)
        var events = renewal.events.makeAsyncIterator()

        loop.setup()
        #expect(await events.next() == .started)

        await clock.resumeSleep(for: StatementRenewalLoop.interval)
        #expect(await events.next() == .started)

        loop.throttle()
    }
}
```

Patterns:

- `waitForSleep(for:)` is a synchronization point: "the code reached its next wait". Use it
  instead of `Task.sleep` or polling to know that preceding work finished.
- `resumeSleeps(for:count:)` for fan-out (several retries waiting at once).
- Wrap each clock wait in `withTimeout(.seconds(60))` so a regression fails instead of hanging.

Limits (current implementation):

- `now` never moves. Code that measures elapsed time with `clock.now` cannot be driven.
  When you hit this, add an `advance(by:)` to `ManualClock` that moves `now` and resumes
  sleepers whose deadline passed (same semantics as Point-Free's `TestClock`), with its own
  tests in `ManualClockTests`. Do not work around it with real sleeps.
- Durations are matched exactly. Jittered backoff must take its jitter from an injected source
  so the test knows the duration.

## Waiting for async results without sleeping

In order of preference:

1. **Await the work directly**: call the async API and await its value / `Task.value`.
2. **Await an event**: next element of an `AsyncStream` with `withTimeout(.seconds(60))`;
   recorder mocks exposing `waitFor…` (`waitForRequests(_:)`, `waitForDeliveries(_:)`,
   `ManualDataChannelSendAttempts.waitForCallCount(_:)`); `confirmation(expectedCount:)` for callbacks.
3. **Await the clock**: `clock.waitForSleep(for:)` when the next thing the code does is wait.
4. **Poll** only as a last resort, and only with a helper that **fails** on timeout and uses a
   hang-guard budget. A poll that silently returns when the condition never holds turns a bug
   into a later, misleading assertion failure.

Never:

- `try await Task.sleep(for: .milliseconds(200))` "to let things settle".
- Assert that something did *not* happen after a short sleep. Instead make the absence
  observable: resume the clock to the point where it would have happened, then assert the
  recorder is empty; or wait for the next positive event and assert that it is the expected one.
- Tight `.timeLimit` or `withTimeout(.seconds(1))` as a correctness check. CI load stretches
  real time 5 to 20 times. Timeouts are hang guards: `withTimeout(.seconds(60))` or
  `.timeLimit(.minutes(1))`.

## MainActor and scheduling

- Annotate the suite `@MainActor` when the subject is MainActor-isolated; do not hop with
  `MainActor.run` inside assertions.
- `Task.yield()` is not a synchronization primitive. One yield is enough on an idle machine and
  not enough under load. Replace it with one of the event-based waits above.
- Unstructured `Task {}` started inside the subject is invisible to the test. Expose a handle
  (return the `Task`, or a `waitUntilIdle()` for tests via the protocol), or make the call
  `async` so the test awaits it.

## Flakiness causes (classify before fixing)

| Cause | Symptom | Fix |
|-------|---------|-----|
| Real sleep in test or subject | Passes locally, fails under load | Inject clock, `ManualClock` |
| Fixed-budget polling (`waitUntil` that gives up quietly) | Assertion fails with "stale" value | Event-based wait, or failing poll with hang-guard budget |
| Timeout used as assertion | `TimeoutError` / time limit exceeded on CI only | Make the wait event-based; keep timeout as hang guard |
| Shared mutable state between parallel tests | Order-dependent failures | Fresh instance per test; unique `UserDefaults` suite names; in-memory facades per test; no singletons |
| Wall-clock date | Fails near midnight / DST / month end | `DateProviding` with fixed date |
| Task not awaited | Assertion races the work | Await the task or an event |
| MainActor starvation | Hangs with many `@MainActor` suites | Move logic off the main actor; keep `@MainActor` tests few and fast |

## Finding offenders

```bash
# Real sleeps in tests
rg -n 'Task\.sleep|Thread\.sleep|usleep\(|asyncAfter' polkadot-appTests polkadot-appIntegrationTests Packages/*/Tests
# Production sleeps / dates that bypass injection
rg -n 'Task\.sleep\(|DispatchQueue\.main\.asyncAfter|[^A-Za-z]Date\(\)' polkadot-app Packages/*/Sources
# Short timeouts used as assertions
rg -n 'withTimeout\(\.(milliseconds|seconds)\(|timeout: *[0-9]' polkadot-appTests Packages/*/Tests
```

## Proving a fix

```bash
set -o pipefail && xcodebuild test -project polkadot-app.xcodeproj -scheme polkadot-appTests \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -only-testing:polkadot-appTests/<Suite> \
  -test-iterations 50 -run-tests-until-failure -parallel-testing-enabled YES 2>&1 | xcbeautify --quiet
```

It must pass all iterations. Run it while the full unit scheme runs in another simulator to
approximate CI load.
