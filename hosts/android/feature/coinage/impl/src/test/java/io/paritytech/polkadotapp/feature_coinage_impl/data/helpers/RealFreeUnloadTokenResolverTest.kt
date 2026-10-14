package io.paritytech.polkadotapp.feature_coinage_impl.data.helpers

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.context.CoinageSigningContextProvider
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Before
import org.junit.Test
import kotlin.time.Duration.Companion.seconds

class RealFreeUnloadTokenResolverTest {
    private var maxCounter: Long = DEFAULT_MAX_COUNTER
    private val takenCounters = mutableSetOf<Long>()

    // Records the counters queried per `getNotUsedCounterIndices` call, so tests can assert batching.
    private val queriedBatches = mutableListOf<List<Long>>()

    private val periodCalculator: UnloadTokenPeriodCalculator = mockk()
    private val contextProvider: CoinageSigningContextProvider = mockk()
    private val resolverSource: UnloadTokenResolverSource = mockk()
    private val consumedTokenChecker: ConsumedTokenChecker = mockk()

    private val resolver = RealFreeUnloadTokenResolver(
        consumedTokenChecker = consumedTokenChecker,
        periodCalculator = periodCalculator,
        contextProvider = contextProvider,
        unloadTokenResolverSource = resolverSource,
    )

    @Before
    fun setUp() {
        every { periodCalculator.currentPeriod(PERIOD_DURATION_SECONDS.seconds) } returns PERIOD
        coEvery { resolverSource.getPeriodDuration(any()) } returns PERIOD_DURATION_SECONDS
        coEvery { resolverSource.getFreeUnloadTokenLimit(any()) } answers { Result.success(maxCounter) }

        // The context (and thus the identity alias) encodes the counter, so the consumed-token stub can map
        // a query back to the concrete counter it was built from.
        every { contextProvider.freeUnloadTokenContext(any(), any()) } answers {
            BandersnatchContext(secondArg<Int>().toString().encodeToByteArray())
        }
        coEvery { resolverSource.generateAlias(any()) } answers { firstArg() }

        coEvery { consumedTokenChecker.getNotUsedCounterIndices(any(), any()) } answers {
            notUsedCounterIndices(secondArg())
        }
    }

    @Test
    fun `selects the lowest available counters`() {
        withMaxUnloadTokens(500)

        val selected = resolveCounters(requiredQuantity = 3)

        assertEquals(listOf(0L, 1L, 2L), selected)
    }

    @Test
    fun `fetches only the first batch when low indices satisfy the request`() {
        withMaxUnloadTokens(500)

        resolveCounters(requiredQuantity = 3)

        assertEquals(1, queriedBatches.size)
        assertEquals((0L until BATCH_SIZE).toList(), queriedBatches.single())
    }

    @Test
    fun `skips taken counters`() {
        withMaxUnloadTokens(500)
        withTakenCounters(0..4)

        val selected = resolveCounters(requiredQuantity = 3)

        assertEquals(listOf(5L, 6L, 7L), selected)
    }

    @Test
    fun `selects counters spanning a batch boundary`() {
        withMaxUnloadTokens(500)
        withTakenCounters(0..98)

        val selected = resolveCounters(requiredQuantity = 3)

        assertEquals(listOf(99L, 100L, 101L), selected)
        assertEquals(2, queriedBatches.size)
    }

    @Test
    fun `fetches the next batch when the first batch is fully taken`() {
        withMaxUnloadTokens(500)
        withTakenCounters(0..99)

        val selected = resolveCounters(requiredQuantity = 2)

        assertEquals(listOf(100L, 101L), selected)
        assertEquals(2, queriedBatches.size)
    }

    @Test
    fun `throws when not enough free counters are available`() {
        withMaxUnloadTokens(3)
        withTakenCounters(0..1)

        assertThrows(IllegalStateException::class.java) {
            resolveCounters(requiredQuantity = 2)
        }
    }

    // -- precondition helpers --

    private fun withMaxUnloadTokens(max: Int) {
        maxCounter = max.toLong()
    }

    private fun withTakenCounters(counters: IntRange) {
        takenCounters += counters.map(Int::toLong)
    }

    private fun resolveCounters(requiredQuantity: Int): List<Long> = runBlocking {
        resolver.resolve(CHAIN_ID, requiredQuantity).map { it.counter }
    }

    // Mirrors the real checker: returns positional indices of counters that are not yet taken.
    private fun notUsedCounterIndices(queries: List<ConsumedTokenChecker.Query>): Result<List<Long>> {
        val counters = queries.map { it.counter() }
        queriedBatches += counters

        val notUsedIndices = counters.indices
            .filter { counters[it] !in takenCounters }
            .map { it.toLong() }

        return Result.success(notUsedIndices)
    }

    private fun ConsumedTokenChecker.Query.counter(): Long =
        alias.value.decodeToString().toLong()

    private companion object {
        const val CHAIN_ID = "test-chain-id"
        const val PERIOD = 42L
        const val PERIOD_DURATION_SECONDS = 60L
        const val DEFAULT_MAX_COUNTER = 500L

        // Mirrors RealFreeUnloadTokenResolver.BATCH_SIZE.
        const val BATCH_SIZE = 100L
    }
}
