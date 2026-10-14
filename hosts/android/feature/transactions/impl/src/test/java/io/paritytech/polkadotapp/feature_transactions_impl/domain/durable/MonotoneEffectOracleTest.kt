package io.paritytech.polkadotapp.feature_transactions_impl.domain.durable

import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ExtrinsicOutcome
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.CheckpointBlock
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxEntry
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.LedgerView
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.MonotoneEffectOracle
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainView
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TransactionSearchResult
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxDomainId
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionHash
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

private const val CHECKPOINT = 100L
private const val MORTALITY = 64L
private const val MORTALITY_END = CHECKPOINT + MORTALITY

/**
 * A whole consumer, written the way a new one would be: a list the app appends to, and one batched read of
 * it. Everything else — pre-finality success, terminal failure, the search underneath — comes from the
 * engine.
 *
 * This is the reference a new domain copies, and it is small on purpose: if it needs more than this, the
 * seam is wrong.
 */
private class ListMembershipOracle(
    /** Values in the list at each block, as the contract would report them. */
    private val listAt: Map<Long, Set<String>>,
    private val expectedValueOf: (DurableTxId) -> String,
    /** Heights whose read fails, so a transport error can be told apart from an empty list. */
    private val unreadable: Set<Long> = emptySet(),
) : MonotoneEffectOracle() {
    override val chainId = "test-chain"

    var reads = 0
        private set

    override suspend fun effectsAt(
        transactions: List<DurableTxEntry>,
        at: CheckpointBlock,
    ): Map<DurableTxId, Boolean> {
        reads++

        // A read that did not answer leaves every transaction out, which decides nothing.
        if (at.blockNumber in unreadable) return emptyMap()

        val values = listAt[at.blockNumber].orEmpty()

        return transactions.associate { it.id to (expectedValueOf(it.id) in values) }
    }
}

class MonotoneEffectOracleTest {
    @Test
    fun `a value present at the finalized head finalizes`() = runBlocking<Unit> {
        val outcome = evaluate(listAt = mapOf(FINALIZED to setOf(VALUE)))

        assertDecided(DurableTxStatus.FINALIZED_SUCCESS, outcome)
    }

    /** What the block search cannot give a consumer: a verdict above the finalized head. */
    @Test
    fun `a value present only at the best head is optimistic success`() = runBlocking<Unit> {
        val outcome = evaluate(listAt = mapOf(BEST to setOf(VALUE)))

        assertDecided(DurableTxStatus.PENDING_SUCCESS, outcome)
    }

    @Test
    fun `an absent value after mortality fails the transaction`() = runBlocking<Unit> {
        val outcome = evaluate(listAt = emptyMap(), finalized = MORTALITY_END + 1)

        assertDecided(DurableTxStatus.FAILURE, outcome)
    }

    /** Absence only means something once the transaction can no longer run. */
    @Test
    fun `an absent value before mortality keeps the transaction pending`() = runBlocking<Unit> {
        val outcome = evaluate(listAt = emptyMap(), finalized = CHECKPOINT + 1)

        assertDecided(DurableTxStatus.PENDING, outcome)
    }

    /**
     * The distinction the whole three-valued contract exists for: a failed read must not read as an empty
     * list, or a network error past mortality would fail a transaction that succeeded.
     */
    @Test
    fun `an unreadable list never fails the transaction`() = runBlocking<Unit> {
        val outcome = evaluate(
            listAt = emptyMap(),
            finalized = MORTALITY_END + 1,
            unreadable = setOf(MORTALITY_END + 1, MORTALITY_END + 11),
            search = TransactionSearchResult.NotFound(wholeRangeRead = false),
        )

        assertDecided(DurableTxStatus.PENDING, outcome)
    }

    /** Even with the domain silent, history decides it — the floor every consumer gets. */
    @Test
    fun `an unreadable list still finalizes when the search finds the transaction`() = runBlocking<Unit> {
        val outcome = evaluate(
            listAt = emptyMap(),
            finalized = MORTALITY_END + 1,
            unreadable = setOf(MORTALITY_END + 1, MORTALITY_END + 11),
            search = TransactionSearchResult.Found(CheckpointBlock(110, "0xblock"), ExtrinsicOutcome.SUCCESS),
        )

        assertDecided(DurableTxStatus.FINALIZED_SUCCESS, outcome)
    }

    /** Two reads for the pass, not two per transaction: one head each, however many are in flight. */
    @Test
    fun `the list is read once per head no matter how many transactions are in flight`() = runBlocking<Unit> {
        val oracle = ListMembershipOracle(mapOf(FINALIZED to setOf(VALUE)), { VALUE })
        val transactions = (1L..25L).map { tx(it) }

        oracle.openPass(transactions, ledgerOf(transactions), view()).getOrThrow()

        assertEquals(2, oracle.reads)
    }

    // ---- fixtures ----

    private suspend fun evaluate(
        listAt: Map<Long, Set<String>>,
        finalized: Long = FINALIZED,
        unreadable: Set<Long> = emptySet(),
        search: TransactionSearchResult = TransactionSearchResult.NotFound(wholeRangeRead = true),
    ): RuleOutcome {
        val oracle = ListMembershipOracle(listAt, { VALUE }, unreadable)
        val transactions = listOf(tx())
        val view = view(finalized, search)

        val scope = oracle.openPass(transactions, ledgerOf(transactions), view).getOrThrow()

        return evaluateLadder(transactions.single(), scope, view, recordedStillCanonical = null)
    }

    private fun assertDecided(expected: DurableTxStatus, outcome: RuleOutcome): RuleOutcome.Decided {
        assertTrue("expected a decision but was $outcome", outcome is RuleOutcome.Decided)
        outcome as RuleOutcome.Decided
        assertEquals(expected, outcome.verdict.status)

        return outcome
    }

    private fun view(
        finalized: Long = FINALIZED,
        search: TransactionSearchResult = TransactionSearchResult.NotFound(wholeRangeRead = true),
    ) = SearchOnlyChainView(finalized, search)

    private fun tx(id: Long = 1) = DurableTxEntry(
        id = DurableTxId(id),
        domainId = TxDomainId("list-consumer"),
        groupId = null,
        txHash = "0xtx$id",
        checkpoint = CheckpointBlock(CHECKPOINT, "0xcheckpoint"),
        mortalityBlocks = MORTALITY,
        status = DurableTxStatus.PENDING,
        successDetectedAt = null,
    )

    private fun ledgerOf(entry: List<DurableTxEntry>) = object : LedgerView {
        override val transactions = entry

        override fun statusOf(id: DurableTxId) = entry.firstOrNull { it.id == id }?.status
    }

    private companion object {
        const val VALUE = "the-value"
        const val FINALIZED = 130L
        const val BEST = FINALIZED + 10
    }
}

private class SearchOnlyChainView(
    finalizedNumber: Long,
    private val search: TransactionSearchResult,
) : PinnedChainView {
    override val finalizedHead = CheckpointBlock(finalizedNumber, "0xfinalized")

    override val bestHead = CheckpointBlock(finalizedNumber + 10, "0xbest")

    override suspend fun blockHashAt(blockNumber: Long): Result<BlockHash?> = notScripted()

    override suspend fun blockNumberAt(hash: BlockHash): Result<Long?> = notScripted()

    override suspend fun dispatchOutcomeAt(at: BlockHash, txHash: TransactionHash) = notScripted<ExtrinsicOutcome?>()

    override suspend fun getAppliedExtrinsicHashes(at: BlockHash) = notScripted<List<TransactionHash>>()

    override suspend fun searchForTransaction(
        fromBlockNumber: Long,
        toBlockNumber: Long,
        txHash: TransactionHash,
    ): TransactionSearchResult = search

    private fun <T> notScripted(): Result<T> = Result.failure(UnsupportedOperationException("not scripted"))
}
