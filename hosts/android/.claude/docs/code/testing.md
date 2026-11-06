# Testing — Unit Tests in This Project

How we write unit tests so they're cheap to read and don't drift into framework debugging.

The patterns below are the result of repeated correction during recent reviews. Read them before adding a new test class; deviating without justification will surface as review comments.

---

## Stack

- **mockk** (`io.mockk:mockk`, on the test classpath of every module that depends on `test-shared`; other modules add `testImplementation(libs.mockk)`). Final classes, `data class`es, value classes and suspend functions work without extra setup. **Not Mockito.** Instrumentation tests use `mockk-android`.
- **JUnit 4**.
- **`kotlinx.coroutines.runBlocking`** for suspend-fn driving. **Not** `runTest` — the project's existing tests use plain `runBlocking`; match that.
- Shared test infrastructure lives in `test-shared`. Don't reinvent locally.

```kotlin
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
```

Add helpers to `test-shared` when you reach for a new matcher / fixture more than once. Don't keep copying them into each test class.

---

## Rules at a glance

1. **Stub every method the SUT calls.** A plain `mockk()` is strict: an unstubbed call throws `MockKException: no answer found`, naming the call. Use `mockk(relaxUnitFun = true)` when the SUT also calls `Unit` functions, suspend or not, that the test does not care about, and `mockk(relaxed = true)` only when the test genuinely ignores return values. A relaxed mock returns non-null defaults, so stub a nullable call explicitly when the test depends on `null`.

2. **Suspend functions go through `coEvery` / `coVerify`.** `every` / `verify` blocks cannot call suspend functions. Matchers such as `any()` work for value classes (`UInt`, `ULong`) and primitives directly.

3. **`runBlocking<Unit>` in `@Test` and `@Before`.** A non-Unit terminal expression (an `assertThrows(...)`, or a SUT call whose result the test ignores) makes the `runBlocking` block's inferred type non-Unit, which JUnit rejects with `InvalidTestClassError`. Always pin to `Unit`:
   ```kotlin
   @Test fun `foo`() = runBlocking<Unit> { ... }
   ```

4. **Per-test-explicit beats hidden defaults.** Only put stubs in `@Before` that are *truly universal* — needed by 100% of tests AND never overridden. If even one test wants the opposite behavior, the default belongs in each test, not in `@Before`. Hiding "the common case" in setup creates the false impression that the opposite case isn't tested.

   ```kotlin
   // ✓ Universal: every test needs a resolved chain context.
   @Before fun setUp() = runBlocking<Unit> { withChainContext() }

   // ✗ Hidden default: half the tests fail without it, the other half override it.
   @Before fun setUp() = runBlocking<Unit> {
       withChainContext()
       withNoStaleAllocations()   // some tests test the stale path → this belongs per-test
   }
   ```

5. **Hide stubs behind intent-revealing helpers.** A test body should read as state declarations, not mocking mechanics.
   ```kotlin
   // ✗
   coEvery { allocationRepository.hasStaleFor(any(), any(), target, any()) } returns false

   // ✓
   withNoStaleAllocations()
   ```

6. **Hide `verify { ... }` calls behind intent-revealing helpers.** Same reason: the test body asserts intent, not mockk syntax.
   ```kotlin
   // ✗
   coVerify(exactly = 0) { allocationRepository.insert(any(), any(), any(), any(), any(), any()) }

   // ✓
   verifyNoAllocationInsertions()
   ```
   A `verify { ... }` without a count passes when the call happened at least once. When the call must happen exactly once, write `verify(exactly = 1) { ... }`.

7. **Hide non-trivial fixture construction behind named helpers.** If a test arrange line reads like a constructor call salad, lift it.
   ```kotlin
   // ✗
   withChainSlots(slotsWith(taken = listOf(taken(seq = 0u, account = target))))

   // ✓
   withSlotTakenBy(target)
   ```

8. **Use mockk mocks over hand-rolled fakes for behavior-verification tests.** `verify { mock.insert(...) }` reads better than `recorder.inserts shouldContain ...` and avoids drift between fake and interface. Fakes are appropriate when the SUT *queries* a stateful collaborator across many calls (e.g., a real repository semantics matter); mocks suffice when you only need to confirm calls happened.

9. **`Result<T>` assertions go through `assertSuccess` / domain-specific helpers**, not `assertTrue(result.isSuccess)`. Surface the failure message:
   ```kotlin
   private fun assertSuccess(result: Result<*>) {
       assertTrue("expected Result.success but was ${result.exceptionOrNull()}", result.isSuccess)
   }

   private fun assertNoAllocationAvailable(result: Result<*>) {
       assertTrue(result.isFailure)
       assertTrue(
           "expected NoAllocationAvailable but was ${result.exceptionOrNull()}",
           result.exceptionOrNull() is StatementStoreSlotAllocationError.NoAllocationAvailable,
       )
   }
   ```

10. **Test name = behavior under condition.** Backticked: `` `<expected behavior> when <condition>` ``. Match house style; see `RealStatementStoreSlotAllocatorTest` and `TransferPlannerTest` for canonical examples.

11. **Audit coverage against `.claude/PLAN.md § Verification plan` before declaring done.** If the plan lists 8 unit cases, you have 8 tests (or a justified subset of them).

12. **Don't open classes purely for testability.** mockk handles final / data classes. Adding `open` only for tests violates `code/naming-and-hygiene.md § Don't expose mutable fields for testability` in spirit. If something can't be mocked, write a fake — don't change the production class.

---

## Helper naming conventions

Three prefixes; each carries a different intent:

| Prefix | Purpose | Examples |
|---|---|---|
| `with*` | State declaration / stub setup. Reads as "given the world is in this state." | `withChainContext()`, `withSlotTakenBy(target)`, `withNoStaleAllocations()`, `withZeroCooldown()`, `withSuccessfulSubmission()` |
| `assert*` | Domain-level assertion on a value the test holds. | `assertSuccess(result)`, `assertNoAllocationAvailable(result)` |
| `verify*` | mockk invocation check. | `verifyAllocationInsertedFor(target, priority)`, `verifyNoEvictionPerformed()`, `verifyNoExtrinsicSubmitted()` |

`assertRenewerInvokedWith(...)` and `assertRenewerNotInvoked()` straddle the line (they're verify-based but read as assertions about the SUT's behavior). Either prefix is fine when the helper exists for the test's narrative; pick the one that reads better in context.

When a setup helper takes ordering-sensitive varargs, name the parameter:

```kotlin
private suspend fun withSlotsTakenBy(vararg accountsOldestToNewest: AccountId) { … }

// reads as: "with slots taken by these accounts, oldest to newest"
withSlotsTakenBy(accountHigh, accountNormal)
```

---

## Test-affordance refactors

Some production seams are awkward to control from tests (e.g. `Clock.System.now()` is a static call; `WorkManager.getInstance(context)` reaches into Android plumbing). Pre-empt the test pain by extracting a tiny seam **as part of the feature work**, not as a later test-only patch.

The current canonical example is `CurrentPeriodProvider`:

```kotlin
fun interface CurrentPeriodProvider { fun current(): UInt }

class RealCurrentPeriodProvider @Inject constructor() : CurrentPeriodProvider {
    override fun current() = (Clock.System.now().epochSeconds / SECONDS_PER_PERIOD).toUInt()
}
```

The production allocator depends on `CurrentPeriodProvider` instead of calling `Clock.System` directly. Tests pass a `CurrentPeriodProvider { fixedPeriod }`. The production class stays small; tests don't need static mocking.

Trigger this refactor when one of the SUT's helpers calls a global / static / framework API the test can't intercept.

---

## Anatomy of a test in this project

```kotlin
class RealStatementStoreSlotAllocatorTest {

    // Fixtures — small, named, reused across tests.
    private val target: AccountId = byteArrayOf(0x01).toDataByteArray()
    private val accountHigh: AccountId = byteArrayOf(0x02).toDataByteArray()

    // Mock collaborators. One line each. No body.
    private val allocationRepository: StatementStoreSlotAllocationRepository = mockk()
    private val renewer: StatementStoreSlotRenewer = mockk()
    // …

    // SUT — last so its constructor is visible at a glance.
    private val allocator = RealStatementStoreSlotAllocator(
        // explicit named args; the constructor IS the test's API surface.
    )

    @Before
    fun setUp() = runBlocking<Unit> {
        // Only universal stubs. Per-test specifics stay per-test.
        withChainContext()
        withRenewerSucceeds()
        withAllocationWritesAccepted()
    }

    @Test
    fun `Normal caller evicts Normal slot - delete oldest + insert target row`() = runBlocking<Unit> {
        withNoStaleAllocations()
        withZeroCooldown()
        withSlotTakenBy(accountNormal)
        withTrackedPriorities(accountNormal to SlotPriority.Normal)
        withSuccessfulSubmission()

        val result = allocate(OnExistingAllocationStrategy.INCREASE, SlotPriority.Normal)

        assertSuccess(result)
        verifySlotDeletedFor(accountNormal, seq = 0u)
        verifyAllocationInsertedFor(target, seq = 0u, priority = SlotPriority.Normal)
    }

    // … helpers at the bottom of the class, grouped by purpose:
    //  - setup (`with*`)
    //  - verification (`verify*`, `assert*`)
    //  - value/object construction
}
```

Three visual blocks per test: arrange (one `with*` per line), act (one line), assert (one or two). If a test needs three asserts, it's two tests.

---

## When to add to `test-shared`

The bar: a helper used by 2+ test classes, OR a helper that wraps mocking boilerplate that's painful to remember. Don't pollute `test-shared` with one-off feature fixtures.

Current contents (`test-shared/src/main/java/io/paritytech/polkadotapp/test_shared/`):
- `Assertions.kt` — common assert wrappers.
- `LoggerHelpers.kt`, `FakeTimeProvider.kt`, `TestCoroutineDispatchers.kt`.
- `chain/FakeChain.kt` — chain fixtures.

---

## Reviewer flags

- **major** — Mockito in a new or changed test. Use mockk.
- **major** — `verify { ... }` / `coVerify { ... }` without a count where the test means the call happens exactly once. Use `exactly = 1`.
- **minor** — `mockk(relaxed = true)` where a strict `mockk()` or `relaxUnitFun = true` would cover the test.
- **major** — Production class made `open` purely for testability.
- **minor** — `assertTrue(result.isSuccess)` without a helpful failure message — should be `assertSuccess(result)` or equivalent.
- **minor** — mockk stub or `verify { ... }` invocation inline in a test body when a named `with*` / `verify*` helper would clarify intent (especially when used 2+ times across tests).
- **minor** — Non-universal stub placed in `@Before` (default for some tests, overridden by others). Move to per-test.
- **minor** — `runBlocking { … }` without `<Unit>` when the last expression is not `Unit` (the test won't be picked up by JUnit).
- **minor** — One-off matcher / fixture pattern reinvented in a test instead of added to `test-shared`.

---

## Canonical examples

- `feature/statement-store/impl/src/test/.../RealStatementStoreSlotAllocatorTest.kt` — mockk mocks + named helpers + `@Before` for truly-universal setup; covers the full allocator behavior contract.
- `feature/statement-store/impl/src/test/.../RealStatementStoreSlotRenewerTest.kt` — mixes pure-logic comparator tests with end-to-end mockk-based renewer flow tests.
- `feature/coinage/impl/src/test/.../TransferPlanTest.kt` (`TransferPlannerTest`) — some patterns predate this doc and don't follow every rule.
