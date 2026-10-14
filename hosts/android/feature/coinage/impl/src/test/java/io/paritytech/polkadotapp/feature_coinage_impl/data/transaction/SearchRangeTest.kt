package io.paritytech.polkadotapp.feature_coinage_impl.data.transaction

import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ExtrinsicOutcome
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.CheckpointBlock
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainView
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TransactionSearchResult
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionHash
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.searchRange
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

private const val WANTED = "0xwanted"
private const val OTHER = "0xother"

/**
 * The search has to tell "not there" apart from "could not look": the first fails a transaction once its
 * window has closed, the second only leaves it pending.
 */
class SearchRangeTest {
    @Test
    fun `whole range read when every block was readable and the transaction is absent`() = runBlocking<Unit> {
        val view = chainOf(10L to listOf(OTHER), 11L to listOf(OTHER))

        val result = view.searchRange(fromBlockNumber = 10, toBlockNumber = 11, txHash = WANTED)

        assertEquals(TransactionSearchResult.NotFound(wholeRangeRead = true), result)
    }

    @Test
    fun `range is partial when a block hash cannot be resolved`() = runBlocking<Unit> {
        val view = chainOf(10L to listOf(OTHER), 11L to null)

        val result = view.searchRange(fromBlockNumber = 10, toBlockNumber = 11, txHash = WANTED)

        assertEquals(TransactionSearchResult.NotFound(wholeRangeRead = false), result)
    }

    @Test
    fun `range is partial when a block body cannot be read`() = runBlocking<Unit> {
        val view = chainOf(10L to listOf(OTHER), 11L to listOf(OTHER), unreadableBodies = setOf(11L))

        val result = view.searchRange(fromBlockNumber = 10, toBlockNumber = 11, txHash = WANTED)

        assertEquals(TransactionSearchResult.NotFound(wholeRangeRead = false), result)
    }

    @Test
    fun `reports the block the transaction was found in`() = runBlocking<Unit> {
        val view = chainOf(10L to listOf(OTHER), 11L to listOf(WANTED))

        val result = view.searchRange(fromBlockNumber = 10, toBlockNumber = 11, txHash = WANTED)

        assertFound(11L, result)
    }

    @Test
    fun `reports the first block when the transaction is at the very start of the range`() = runBlocking<Unit> {
        val view = chainOf(10L to listOf(WANTED), 11L to listOf(OTHER))

        val result = view.searchRange(fromBlockNumber = 10, toBlockNumber = 11, txHash = WANTED)

        assertFound(10L, result)
    }

    /** A hit is positive evidence regardless of what could not be read before it. */
    @Test
    fun `keeps searching past an unreadable block`() = runBlocking<Unit> {
        val view = chainOf(10L to listOf(OTHER), 11L to listOf(WANTED), unreadableBodies = setOf(10L))

        val result = view.searchRange(fromBlockNumber = 10, toBlockNumber = 11, txHash = WANTED)

        assertFound(11L, result)
    }

    @Test
    fun `carries the dispatch outcome of the block it was found in`() = runBlocking<Unit> {
        val view = chainOf(10L to listOf(WANTED), outcome = ExtrinsicOutcome.FAILURE)

        val result = view.searchRange(fromBlockNumber = 10, toBlockNumber = 10, txHash = WANTED)

        assertEquals(ExtrinsicOutcome.FAILURE, (result as TransactionSearchResult.Found).outcome)
    }

    /** An unreadable outcome is not a failed dispatch: inclusion alone decides nothing. */
    @Test
    fun `leaves the outcome null when the events could not be read`() = runBlocking<Unit> {
        val view = chainOf(10L to listOf(WANTED), outcome = null)

        val result = view.searchRange(fromBlockNumber = 10, toBlockNumber = 10, txHash = WANTED)

        assertEquals(null, (result as TransactionSearchResult.Found).outcome)
    }

    private fun assertFound(expectedBlockNumber: Long, result: TransactionSearchResult) {
        assertTrue("expected the transaction to be found but was $result", result is TransactionSearchResult.Found)
        assertEquals(expectedBlockNumber, (result as TransactionSearchResult.Found).block.blockNumber)
    }

    /** A null body list means the block hash itself could not be resolved. */
    private fun chainOf(
        vararg blocks: Pair<Long, List<TransactionHash>?>,
        unreadableBodies: Set<Long> = emptySet(),
        outcome: ExtrinsicOutcome? = ExtrinsicOutcome.SUCCESS,
    ) = FakeChainView(blocks.toMap(), unreadableBodies, outcome)
}

/**
 * A scripted chain of block bodies for [searchRange] to walk: [blocks] maps a block number to the hashes its
 * body holds, a null list meaning its hash cannot be resolved at all, and [unreadableBodies] names the
 * blocks whose hash resolves but whose body read fails.
 *
 * Hand-written rather than mocked because every read the search makes returns a `Result` — a value class
 * Mockito cannot stub — and answers a different value per block.
 */
private class FakeChainView(
    private val blocks: Map<Long, List<TransactionHash>?>,
    private val unreadableBodies: Set<Long>,
    private val outcome: ExtrinsicOutcome?,
) : PinnedChainView {
    override val finalizedHead = CheckpointBlock(0, "0xunused")

    override val bestHead = CheckpointBlock(0, "0xunused")

    override suspend fun blockHashAt(blockNumber: Long): Result<BlockHash?> =
        Result.success(blocks[blockNumber]?.let { "0xblock$blockNumber" })

    override suspend fun getAppliedExtrinsicHashes(at: BlockHash): Result<List<TransactionHash>> {
        val number = at.removePrefix("0xblock").toLong()
        if (number in unreadableBodies) return Result.failure(IllegalStateException("unreadable body"))

        return Result.success(blocks.getValue(number).orEmpty())
    }

    override suspend fun dispatchOutcomeAt(at: BlockHash, txHash: TransactionHash): Result<ExtrinsicOutcome?> =
        Result.success(outcome)

    override suspend fun searchForTransaction(
        fromBlockNumber: Long,
        toBlockNumber: Long,
        txHash: TransactionHash,
    ) = searchRange(fromBlockNumber, toBlockNumber, txHash)

    override suspend fun blockNumberAt(hash: BlockHash): Result<Long?> =
        Result.failure(UnsupportedOperationException("not scripted"))
}
