package io.paritytech.polkadotapp.feature_coinage_impl.data.helpers

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.context.CoinageSigningContextProvider
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration.Companion.seconds

private const val BATCH_SIZE = 100L
private const val CHAIN_ID = "chain"
private const val PERIOD = 7L

/**
 * The count stops early on purpose: tokens are claimed in index order, so a batch that ends on a free
 * counter proves the rest of the range is free too, and walking it would cost a query per hundred counters
 * to learn nothing.
 */
class UnloadQuotaEstimateTest {
    private val consumedTokenChecker: ConsumedTokenChecker = mockk()
    private val periodCalculator: UnloadTokenPeriodCalculator = mockk()
    private val contextProvider: CoinageSigningContextProvider = mockk()
    private val source: UnloadTokenResolverSource = mockk()

    private val resolver = RealFreeUnloadTokenResolver(
        consumedTokenChecker,
        periodCalculator,
        contextProvider,
        source,
    )

    @Test
    fun `a free counter at the end of a batch ends the count and the rest of the range is free`() = runBlocking<Unit> {
        withLimit(250)
        // Batch 0 covers counters 0..99; index 99 free means everything from 100 up is free too.
        withFreeIndicesInFirstBatch(50, 99)

        val quota = resolver.estimateRemainingUnloadQuota(CHAIN_ID).getOrThrow()

        assertEquals(2 + (250 - BATCH_SIZE), quota.remaining)
    }

    @Test
    fun `a fully consumed range leaves nothing`() = runBlocking<Unit> {
        withLimit(250)
        withFreeIndicesInFirstBatch()

        val quota = resolver.estimateRemainingUnloadQuota(CHAIN_ID).getOrThrow()

        assertEquals(0L, quota.remaining)
    }

    @Test
    fun `the period limit is reported alongside what is left`() = runBlocking<Unit> {
        withLimit(250)
        withFreeIndicesInFirstBatch(99)

        val quota = resolver.estimateRemainingUnloadQuota(CHAIN_ID).getOrThrow()

        assertEquals(250L, quota.limit)
        assertEquals(PERIOD, quota.period)
    }

    @Test
    fun `an empty allowance is not walked at all`() = runBlocking<Unit> {
        withLimit(0)

        val quota = resolver.estimateRemainingUnloadQuota(CHAIN_ID).getOrThrow()

        assertEquals(0L, quota.remaining)
    }

    /** A count we could not take is not a count of zero, so the caller keeps whatever it had. */
    @Test
    fun `a failed query fails the estimate rather than reporting an empty allowance`() = runBlocking<Unit> {
        withLimit(250)
        coEvery { consumedTokenChecker.getNotUsedCounterIndices(any(), any()) } returns
            Result.failure(RuntimeException("chain down"))

        assertTrue(resolver.estimateRemainingUnloadQuota(CHAIN_ID).isFailure)
    }

    @Test
    fun `an unreadable limit fails the estimate`() = runBlocking<Unit> {
        coEvery { source.getPeriodDuration(CHAIN_ID) } returns 60
        every { periodCalculator.currentPeriod(60.seconds) } returns PERIOD
        coEvery { source.getFreeUnloadTokenLimit(CHAIN_ID) } returns Result.failure(RuntimeException("no view fn"))

        assertTrue(resolver.estimateRemainingUnloadQuota(CHAIN_ID).isFailure)
    }

    private fun withLimit(limit: Long) {
        coEvery { source.getPeriodDuration(CHAIN_ID) } returns 60
        every { periodCalculator.currentPeriod(60.seconds) } returns PERIOD
        coEvery { source.getFreeUnloadTokenLimit(CHAIN_ID) } returns Result.success(limit)
        coEvery { source.generateAlias(any()) } returns byteArrayOf(1)
        every { contextProvider.freeUnloadTokenContext(any(), any()) } returns BandersnatchContext(byteArrayOf(1))
    }

    private fun withFreeIndicesInFirstBatch(vararg indices: Long) {
        coEvery { consumedTokenChecker.getNotUsedCounterIndices(CHAIN_ID, any()) } returns Result.success(indices.toList())
    }
}
