# Testing — Unit Tests in This Project

How we write unit tests so they're cheap to read and don't drift into framework debugging.

The patterns below are the result of repeated correction during recent reviews. Read them before adding a new test class; deviating without justification will surface as review comments.

---

## Stack

- **Mockito 5** (`org.mockito:mockito-core` — inline mock-maker is the default, final classes including `data class` work without `open`). **Not MockK.**
- **JUnit 4**.
- **`kotlinx.coroutines.runBlocking`** for suspend-fn driving. **Not** `runTest` — the project's existing tests use plain `runBlocking`; match that.
- Test infrastructure (matchers, `whenever`, common helpers) lives in `test-shared/.../MockitoHelpers.kt`. Don't reinvent locally.

```kotlin
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.anyLong
import io.paritytech.polkadotapp.test_shared.anyUInt
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.whenever
```

Add helpers to `test-shared` when you reach for a new matcher / fixture more than once. Don't keep copying them into each test class.

---

## Rules at a glance

1. **Stub every method the SUT calls.** Mockito returns `null` for unstubbed suspend functions; Kotlin then unboxes `null` to a primitive / value class and NPEs. The failure looks like a mysterious `NullPointerException` deep inside the SUT, not a missing stub. If a test fails with NPE, the first hypothesis is "I forgot to stub something."

2. **Value-class arg matchers go through `test-shared` helpers.** Use `anyUInt()` / `anyLong()` from `MockitoHelpers.kt`. `Mockito.any<UInt>()` NPEs because Mockito returns `null` cast to `UInt` and the JVM unboxes that.

3. **`runBlocking<Unit>` in `@Test` and `@Before`.** A `verify(...)` call (or any non-Unit terminal expression) makes the `runBlocking` block's inferred type non-Unit, which JUnit rejects with `InvalidTestClassError`. Always pin to `Unit`:
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

5. **Hide stubs behind intent-revealing helpers.** A test body should read as state declarations, not Mockito mechanics.
   ```kotlin
   // ✗
   whenever(allocationRepository.hasStaleFor(any(), any(), eq(target), anyUInt())).thenReturn(false)

   // ✓
   withNoStaleAllocations()
   ```

6. **Hide `verify(...)` calls behind intent-revealing helpers.** Same reason: the test body asserts intent, not Mockito syntax.
   ```kotlin
   // ✗
   verify(allocationRepository, never()).insert(any(), any(), any(), anyUInt(), anyLong(), any())

   // ✓
   verifyNoAllocationInsertions()
   ```

7. **Hide non-trivial fixture construction behind named helpers.** If a test arrange line reads like a constructor call salad, lift it.
   ```kotlin
   // ✗
   withChainSlots(slotsWith(taken = listOf(taken(seq = 0u, account = target))))

   // ✓
   withSlotTakenBy(target)
   ```

8. **Use Mockito mocks over hand-rolled fakes for behavior-verification tests.** `verify(mock).insert(...)` reads better than `recorder.inserts shouldContain ...` and avoids drift between fake and interface. Fakes are appropriate when the SUT *queries* a stateful collaborator across many calls (e.g., a real repository semantics matter); mocks suffice when you only need to confirm calls happened.

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

12. **Don't open classes purely for testability.** Mockito 5 inline mock-maker handles final / data classes. Adding `open` only for tests violates `code/naming-and-hygiene.md § Don't expose mutable fields for testability` in spirit. If something can't be mocked, write a fake — don't change the production class.

---

## Helper naming conventions

Three prefixes; each carries a different intent:

| Prefix | Purpose | Examples |
|---|---|---|
| `with*` | State declaration / stub setup. Reads as "given the world is in this state." | `withChainContext()`, `withSlotTakenBy(target)`, `withNoStaleAllocations()`, `withZeroCooldown()`, `withSuccessfulSubmission()` |
| `assert*` | Domain-level assertion on a value the test holds. | `assertSuccess(result)`, `assertNoAllocationAvailable(result)` |
| `verify*` | Mockito invocation check. | `verifyAllocationInsertedFor(target, priority)`, `verifyNoEvictionPerformed()`, `verifyNoExtrinsicSubmitted()` |

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
    private val allocationRepository: StatementStoreSlotAllocationRepository =
        mock(StatementStoreSlotAllocationRepository::class.java)
    private val renewer: StatementStoreSlotRenewer = mock(StatementStoreSlotRenewer::class.java)
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
    }

    @Test
    fun `Normal caller evicts Normal slot - delete oldest + insert target row`() = runBlocking<Unit> {
        withNoStaleAllocations()
        withZeroCooldown()
        withSlotTakenBy(accountNormal)
        withAccountPriority(accountNormal, SlotPriority.Normal)
        withSuccessfulSubmission()

        val result = allocator.allocate(target, OnExistingAllocationStrategy.INCREASE, SlotPriority.Normal)

        assertSuccess(result)
        verifyEvictedRowDeletedFor(accountNormal)
        verifyAllocationInsertedFor(target, SlotPriority.Normal)
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

The bar: a helper used by 2+ test classes, OR a helper that wraps Mockito boilerplate that's painful to remember (value-class matchers fit here). Don't pollute `test-shared` with one-off feature fixtures.

Current contents (`test-shared/src/main/java/io/paritytech/polkadotapp/test_shared/`):
- `MockitoHelpers.kt` — `whenever`, `any`, `eq`, `argThat`, `anyUInt`, `anyLong`, `thenThrowUnsafe`.
- `Assertions.kt` — common assert wrappers.
- `DiffHelpers.kt`, `LoggerHelpers.kt`.

---

## Reviewer flags

- **major** — `Mockito.any<UInt>()` or other value-class-typed `any()` (will NPE at runtime). Use `anyUInt()` from `test-shared`.
- **major** — Production class made `open` purely for testability.
- **minor** — `assertTrue(result.isSuccess)` without a helpful failure message — should be `assertSuccess(result)` or equivalent.
- **minor** — Mockito stub or `verify(...)` invocation inline in a test body when a named `with*` / `verify*` helper would clarify intent (especially when used 2+ times across tests).
- **minor** — Non-universal stub placed in `@Before` (default for some tests, overridden by others). Move to per-test.
- **minor** — `runBlocking { … }` without `<Unit>` when the last expression is a `verify(...)` (the test won't be picked up by JUnit).
- **minor** — One-off matcher / fixture pattern reinvented in a test instead of added to `test-shared`.

---

## Canonical examples

- `feature/statement-store/impl/src/test/.../RealStatementStoreSlotAllocatorTest.kt` — Mockito mocks + named helpers + `@Before` for truly-universal setup; covers the full allocator behavior contract.
- `feature/statement-store/impl/src/test/.../RealStatementStoreSlotRenewerTest.kt` — mixes pure-logic comparator tests with end-to-end Mockito-based renewer flow tests.
- `feature/coinage/impl/src/test/.../TransferPlannerTest.kt` — older `runBlocking` + Mockito style; some patterns predate this doc and don't follow every rule.
