package io.paritytech.polkadotapp.feature_coinage_impl.data.helpers

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.context.CoinageSigningContextProvider
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.anyInt
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
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
    private val consumedTokenChecker: ConsumedTokenChecker = mock()
    private val periodCalculator: UnloadTokenPeriodCalculator = mock()
    private val contextProvider: CoinageSigningContextProvider = mock()
    private val source: UnloadTokenResolverSource = mock()

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
        whenever(consumedTokenChecker.getNotUsedCounterIndices(any(), any()))
            .thenReturn(Result.failure(RuntimeException("chain down")))

        assertTrue(resolver.estimateRemainingUnloadQuota(CHAIN_ID).isFailure)
    }

    @Test
    fun `an unreadable limit fails the estimate`() = runBlocking<Unit> {
        whenever(source.getPeriodDuration(CHAIN_ID)).thenReturn(60)
        whenever(periodCalculator.currentPeriod(60.seconds)).thenReturn(PERIOD)
        whenever(source.getFreeUnloadTokenLimit(CHAIN_ID)).thenReturn(Result.failure(RuntimeException("no view fn")))

        assertTrue(resolver.estimateRemainingUnloadQuota(CHAIN_ID).isFailure)
    }

    private suspend fun withLimit(limit: Long) {
        whenever(source.getPeriodDuration(CHAIN_ID)).thenReturn(60)
        whenever(periodCalculator.currentPeriod(60.seconds)).thenReturn(PERIOD)
        whenever(source.getFreeUnloadTokenLimit(CHAIN_ID)).thenReturn(Result.success(limit))
        whenever(source.generateAlias(any())).thenReturn(byteArrayOf(1))
        whenever(contextProvider.freeUnloadTokenContext(anyInt(), anyInt()))
            .thenReturn(BandersnatchContext(byteArrayOf(1)))
    }

    private suspend fun withFreeIndicesInFirstBatch(vararg indices: Long) {
        whenever(consumedTokenChecker.getNotUsedCounterIndices(eq(CHAIN_ID), any()))
            .thenReturn(Result.success(indices.toList()))
    }
}
