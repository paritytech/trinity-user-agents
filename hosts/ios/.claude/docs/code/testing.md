# Testing

## Test Targets

- **Unit tests**: `polkadot-appTests/`
- **Integration tests**: `polkadot-appIntegrationTests/`
- **Test plan**: `polkadot-app.xctestplan`

## Running Tests

```bash
# Unit tests
xcodebuild test -project polkadot-app.xcodeproj \
    -scheme polkadot-appTests \
    -destination 'platform=iOS Simulator,name=iPhone 16'

# Integration tests
xcodebuild test -project polkadot-app.xcodeproj \
    -scheme polkadot-appIntegrationTests \
    -destination 'platform=iOS Simulator,name=iPhone 16'

# Via Fastlane
bundle exec fastlane run_unit_tests
```

## CI Pipeline

PR pipeline (`.github/workflows/pr.yml`):
1. **Build** (macos-26) — build with Fastlane (DevCI config)
2. **Test** (macos-26-xlarge) — unit tests
3. Skip with `skip-ci` label or `release/` branch prefix

## Test Patterns

### Prefer real implementations

**Always try to use the real implementation in tests; create as few stubs/mocks as
possible.** Reach for a test double only when the real type is genuinely impractical in
a unit test — it needs the network, real wall-clock time, a live chain, or key material
you cannot supply. A real type that only needs cheap in-memory inputs should be used
as-is — e.g. a real `CoinKeypairFactory(entropyManager: MockEntropyManager(entropy:))`
over a stubbed `CoinKeyDeriving`, `InMemoryKeychain`/in-memory storage facades over
faked stores, an injected fixed `Date` over a mocked clock. Fewer doubles means more
real code paths under test. When you must fake, fake the lowest-level injectable seam
and run the real production types above it (see below).

### Mocks

Reusable test doubles are shared, one type per file, never private per-suite:

- **Universal mocks** (cross-feature seams: chain registry, loggers, key managers,
  storage, runtime providers, …) → `polkadot-appTests/Helpers/Mocks/`
- **Feature mocks** (protocols owned by one feature) →
  `polkadot-appTests/<Feature>/Mocks/` (e.g. `Chat/Mocks/`, `Products/Mocks/`, `JWT/Mocks/`)

Rules:

- **Reuse or extend a shared mock before writing a new one.** If a shared mock is
  close but missing something, add the capability there (a new recorded field, a
  new `simulate…` helper) rather than forking a private copy into your suite.
- **Only fall back to a local double when the need is genuinely suite-specific**
  — and if it later proves reusable, promote it into the matching `Mocks/` folder.
- **Mock at the right seam.** Prefer mocking the lowest-level injectable protocol
  and exercising the real production types above it, over faking a whole
  high-level object. That keeps the real code paths under test.
- Keep mocks dumb: record inputs and expose simple accessors / `simulate…`
  helpers; put behavior/assertions in the test, not the mock.

Test data builders (e.g. `ChainMock`, `SsoTestData`) and in-memory storage facades
stay outside `Mocks/`. `polkadot-appIntegrationTests` keeps its own doubles until
a shared test-helpers target exists.

### Coding Tests

Only write encoding/decoding tests when they assert compatibility with a spec or
another platform: pinned test vectors, wire enum indices, or byte-level shapes.
Pure roundtrip tests of hand-written coding just re-execute the implementation
and add no signal.

### Test Data
- Use `Data.randomOrError` from SubstrateSdk for random/test data generation
- Use typed test helpers over raw primitives

### Framework: Swift Testing

All new and rewritten tests use Swift Testing (`import Testing`), not XCTest:

- `struct` suites with `@Test` functions; write `@Suite` only with a name or
  traits — a bare `@Suite` is redundant and removed by SwiftFormat
  (`redundantSwiftTestingSuite`)
- `#expect(…)` for assertions, `try #require(…)` for unwrapping/preconditions
- Error checks: `#expect(throws:)`, or the closure form
  `#expect { … } throws: { error in … }` for pattern matching
- Per-test state that needs cleanup → `final class` suite with `init()`/`deinit`
  (a fresh instance is created per test); make shared resources (e.g. UserDefaults
  suite names) unique per instance — tests run in parallel
- Tests do NOT run on the main thread by default — annotate the suite or test
  `@MainActor` when touching MainActor-isolated types; never `MainActor.assumeIsolated`
- No XCTest expectations: use async APIs or `withCheckedContinuation`; long-running
  external waits get `.timeLimit(…)` traits
- XCTest remains only in not-yet-migrated suites; migrate a suite when touching it

### Test Naming
Use camelCase for test functions (Swift convention), no underscores;
no `test` prefix on `@Test` functions — SwiftFormat strips it
(`swiftTestingTestCaseNames`):

```swift
@Test func behaviorWhenCondition() { }
```

### Test Structure
```swift
@Test func transferSucceedsWhenBalanceSufficient() {
    // Given
    let interactor = createInteractor(balance: .sufficient)

    // When
    let result = interactor.performTransfer(amount: testAmount)

    // Then
    #expect(result == .success)
}
```

## Benchmarks

Core Data benchmarks live in `polkadot-appIntegrationTests/CoreData/`. They are not in
`polkadot-app.xctestplan` and never run in CI; run them locally, on demand, one suite tree at a time:

```bash
set -o pipefail && TEST_RUNNER_COREDATA_BENCH_OUT="$PWD/docs/benchmarks/coredata/<date>-operation-ios-<version>" \
  xcodebuild test -project polkadot-app.xcodeproj -scheme polkadot-appIntegrationTests \
  -destination 'platform=iOS Simulator,id=<simulator udid>' \
  -parallel-testing-enabled NO \
  -only-testing:polkadot-appIntegrationTests/CoreDataBenchmarks \
  -only-testing:polkadot-appIntegrationTests/CoreDataTopologySpike 2>&1 | xcbeautify --quiet
```

`-parallel-testing-enabled NO` is required: the scheme allows parallel testing, which runs the scenarios
on several simulator clones at once and measures their contention instead of the store's. Use a
simulator udid (`xcrun simctl list devices available`); device names are ambiguous across runtimes.

Rules:

- Timing scenarios (B1–B4, S1) **record**, they never assert thresholds. Correctness gates (B5, B6) assert.
- Every timing test is `@Test(arguments: StackVariant.supported)`; a new Operation-iOS concurrency mode is
  enabled by appending to `StackVariant.supported`, not by editing scenarios.
- The store is SQLite on disk with history tracking (`BenchmarkStorageFacade`), not the in-memory
  `UserDataStorageTestFacade`: the coordinator lock and WAL are what is being measured.
- Reports are Swift Testing attachments (`<scenario>.<variant>.json`) and, when
  `TEST_RUNNER_COREDATA_BENCH_OUT` is set, files in that directory. Committed baselines live under
  `docs/benchmarks/coredata/` with a README naming the machine and library version.
- Sizes live in `BenchmarkScale`; the JSON embeds the scale used. Keep the whole set under three minutes.
- Compare p50/p95 across runs on the same machine only; run twice and keep the second run.

## Hard Rules

1. **Sentry must not run in tests** — guard with environment check
2. **Test actual behavior, not implementation** — mock boundaries, not internals
3. **Build scripts excluded for Debug** — CI scripts should be gated per config

## Debugging

- Shake gesture on `RootWindow` → Debug Settings (development only)
- `TESTNET_FEATURE` flag gates testnet-specific UI (`#if TESTNET_FEATURE`)
- `DEBUG` preprocessor conditionals for dev-only code
- SwiftyBeaver logging throughout
- Sentry error monitoring in non-debug builds
