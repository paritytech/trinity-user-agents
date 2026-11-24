---
name: tester
description: Decide what deserves a test, write it deterministically, and prune what doesn't. Use when planning or writing tests for a change, auditing an existing suite (trivial / humble-object / duplicate tests), or fixing a flaky test (clock, sleep, timeout, load).
user_invocable: true
---

# Tester Skill

The goal is a small suite that catches real regressions in risky code, runs fast under CI
load, and survives refactoring. Test count is a cost, not a metric.

Three modes. Pick one from the request:

| Request                                              | Mode                   |
|------------------------------------------------------|------------------------|
| "write/plan tests for …", implementer handoff        | **A. Plan & write**    |
| "audit/clean up/reduce tests in …", CI too slow      | **B. Audit a suite**   |
| "this test is flaky / times out / fails on CI"       | **C. Deflake**         |

Always load `.claude/docs/code/testing.md` first (framework conventions, mocks policy).
Load `code/concurrency.md` for anything async.

## Core Rules (apply in every mode)

1. **Test behavior through the public seam, never structure.** No 1:1 `FooTests` per `Foo`
   by default; a test suite describes a behavior, and refactoring internals must not break it.
2. **Only risky code gets unit tests.** Classify first (`references/risk-triage.md`). Tier 3
   code (trivial mappings, pass-through wiring, field copies, rendering) gets **no** test of its own.
3. **Humble objects stay untested; their logic moves out.** ViewControllers, ViewLayouts,
   SwiftUI views, Wireframes, ViewFactories, cells and plain mappers are humble. If a humble
   object contains a decision worth testing, extract it into a pure type and test that
   (`references/humble-objects.md`). Never assert on `UILabel.text`, colors, fonts or view hierarchy.
4. **Time is an injected dependency.** Production code never calls `Task.sleep`, `Date()`,
   `DispatchQueue.asyncAfter` or `Timer` directly in tested logic: it takes `any Clock<Duration>`
   for waiting and `DateProviding` for wall-clock reads. Tests drive time
   with `ManualClock` / fixed date providers (`references/time-and-async.md`).
5. **No real waiting in unit tests.** No `Task.sleep`, no fixed-duration polling, no
   `XCTestExpectation` with short timeouts. Synchronize on events (`clock.waitForSleep`,
   `confirmation`, `AsyncStream` element, recorded-call awaiters). Timeouts exist only as hang
   guards (`withTimeout(.seconds(60))`, `.timeLimit(.minutes(1))`) and must never decide pass/fail of
   a healthy run.
6. **One behavior, one test; variations → `@Test(arguments:)`.** Enum-case round-trips,
   formatter tables and boundary sets are one parameterized test, not N copies.
7. **Real types over doubles; double only the impractical seam** (network, chain, clock,
   keychain), per `testing.md`.

## Mode A: Plan & Write

1. **Inventory the change.** List the types/functions the diff adds or changes.
2. **Triage each** with `references/risk-triage.md` → Tier 1 / 2 / 3. Write the table into the
   reply (or PLAN.md `test_strategy` section when called from the architect):

   ```
   | Unit | Tier | Why (risk driver) | Behaviors to cover | Seam / doubles |
   ```
3. **Fix testability before writing tests.** For Tier 1/2 logic that lives in a humble object
   or reads time directly, refactor first (extract pure type, inject clock). This is in scope
   for the change; record it in the plan.
4. **Choose behaviors, not methods.** For each Tier 1 unit cover: the happy path, each failure
   branch that changes user-visible outcome or persisted state, boundaries (zero, max, overflow,
   empty, duplicate), cancellation, retry/timeout transitions, and idempotency/reentrancy where
   relevant. Tier 2: happy path + the single most likely failure. Tier 3: nothing.
5. **Write the tests** per `testing.md` conventions (Swift Testing, Given/When/Then,
   behavior-named `@Test` functions, shared mocks). Prefer one test exercising a meaningful
   scenario through the real stack over several tests of each internal layer.
6. **Self-check** with the checklist at the end of this file. Run only the touched suites with
   `-only-testing:`, then run them again with `-test-iterations 20 -run-tests-until-failure`
   if they involve async or time. A flaky test must not be merged.

## Mode B: Audit a Suite

Follow `references/audit.md`. Output is a table of verdicts per test/suite
(**keep / parameterize / merge / move-to-extracted-type / rewrite-deterministic / delete**)
with the reason, followed by the patch. Never delete a test whose behavior is not covered
elsewhere *and* is Tier 1; rewrite it instead. Report before/after test count and suite
runtime.

## Mode C: Deflake

1. Reproduce under load: `-test-iterations 50 -run-tests-until-failure` with
   `-parallel-testing-enabled YES`; if it doesn't fail, add CPU pressure (run the full unit
   scheme in parallel). CI is a loaded machine.
2. Classify the cause with `references/time-and-async.md` § Flakiness causes.
3. Fix the **cause**, in this order of preference: inject/drive the clock → replace sleep/poll
   with an event-based wait → remove shared state (unique `UserDefaults` suite, fresh facade
   per test) → move logic out of the async/humble boundary so it is tested synchronously.
   Raising a timeout or adding `.serialized` is not a fix; it is allowed only as a temporary
   quarantine with a linked issue.
4. Prove it: the reproduction command above passes 50/50.

## Self-Check Before Handoff

- [ ] Every new test targets a Tier 1/2 unit; no test exists for Tier 3 code
- [ ] No test touches a humble object's views/labels/colors/layout
- [ ] No `Task.sleep`, `Thread.sleep`, `usleep`, `asyncAfter` in tests
- [ ] No production `Task.sleep`/`Date()` added in tested logic; clock/date provider injected
- [ ] Every wait is event-based; any timeout is a generous hang guard (60 s or more) and only guards hangs
- [ ] Case tables use `@Test(arguments:)`
- [ ] Test names describe behavior (`transferFailsWhenBalanceInsufficient`), not methods
- [ ] Async/time tests passed 20 iterations locally
- [ ] Net test count change and reason stated in the PR description

## References

- `references/risk-triage.md`: what to test, tiers, coverage targets
- `references/humble-objects.md`: extracting logic from VIPER humble layers
- `references/time-and-async.md`: ManualClock, date providers, event-based waits, flakiness causes
- `references/audit.md`: suite audit procedure and report format
