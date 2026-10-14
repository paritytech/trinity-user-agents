package io.paritytech.polkadotapp.feature_transactions_impl.domain.durable

import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ExtrinsicOutcome
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.CheckpointBlock
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableFailureKind
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxEntry
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.HeadKind
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainView
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TransactionSearchResult
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxCompletionOracle
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
 * The ladder, with no domain in sight.
 *
 * Coinage pins the same rules through its own evidence; these pin them through an oracle that answers
 * whatever the case says, which is what a new consumer gets to write. Anything asserted here holds for a
 * domain that reads a storage value, a list membership, or nothing at all.
 */
class CompletionLadderTest {
    // ---- Rule 0 — a recorded inclusion ----

    @Test
    fun `a canonical record at or below the finalized head finalizes`() = runBlocking<Unit> {
        val outcome = evaluate(
            tx = tx(successDetectedAt = block(120)),
            oracle = says(),
            finalized = 130,
            recordedStillCanonical = true,
        )

        assertDecided(DurableTxStatus.FINALIZED_SUCCESS, outcome)
    }

    @Test
    fun `a canonical record above the finalized head holds at PENDING_SUCCESS`() = runBlocking<Unit> {
        val outcome = evaluate(
            tx = tx(successDetectedAt = block(140)),
            oracle = says(),
            finalized = 130,
            recordedStillCanonical = true,
        )

        assertDecided(DurableTxStatus.PENDING_SUCCESS, outcome)
    }

    @Test
    fun `a record whose block is gone demotes to PENDING when nothing is visible any more`() = runBlocking<Unit> {
        val outcome = evaluate(
            tx = tx(successDetectedAt = block(120)),
            oracle = says(),
            finalized = 130,
            recordedStillCanonical = false,
        )

        val decided = assertDecided(DurableTxStatus.PENDING, outcome)
        assertEquals(null, decided.verdict.successDetectedAt)
    }

    @Test
    fun `a record whose block is gone re-records the best head when completion is still visible there`() =
        runBlocking<Unit> {
            val outcome = evaluate(
                tx = tx(successDetectedAt = block(120)),
                oracle = says(completedAtBest = true),
                finalized = 130,
                recordedStillCanonical = false,
            )

            assertDecided(DurableTxStatus.PENDING_SUCCESS, outcome)
        }

    /** A read that did not answer is never a verdict: the transaction keeps its status and is retried. */
    @Test
    fun `an unreadable canonicality check leaves the transaction undecided`() = runBlocking<Unit> {
        val outcome = evaluate(
            tx = tx(successDetectedAt = block(120)),
            oracle = says(),
            finalized = 130,
            recordedStillCanonical = null,
        )

        assertTrue("expected Undecided but was $outcome", outcome is RuleOutcome.Undecided)
    }

    // ---- Rules 1 and 2 — proven completion ----

    @Test
    fun `completion proven at the finalized head finalizes`() = runBlocking<Unit> {
        val outcome = evaluate(tx = tx(), oracle = says(completedAtFinalized = true))

        assertDecided(DurableTxStatus.FINALIZED_SUCCESS, outcome)
    }

    @Test
    fun `completion proven only at the best head is PENDING_SUCCESS`() = runBlocking<Unit> {
        val outcome = evaluate(tx = tx(), oracle = says(completedAtBest = true))

        assertDecided(DurableTxStatus.PENDING_SUCCESS, outcome)
    }

    @Test
    fun `the finalized head wins over the best head on the same evidence`() = runBlocking<Unit> {
        val outcome = evaluate(
            tx = tx(),
            oracle = says(completedAtFinalized = true, completedAtBest = true),
        )

        assertDecided(DurableTxStatus.FINALIZED_SUCCESS, outcome)
    }

    // ---- Rule 3 — proven non-completion ----

    @Test
    fun `proven non-completion after mortality fails the transaction`() = runBlocking<Unit> {
        val outcome = evaluate(
            tx = tx(),
            oracle = says(notCompletedAtFinalized = true),
            finalized = MORTALITY_END + 1,
        )

        assertDecided(DurableTxStatus.FAILURE, outcome)
    }

    /**
     * The window is what makes non-completion mean anything: before it closes the transaction can still run,
     * so the same evidence has to reach the search instead.
     */
    @Test
    fun `proven non-completion before mortality does not fail the transaction`() = runBlocking<Unit> {
        val outcome = evaluate(
            tx = tx(),
            oracle = says(notCompletedAtFinalized = true),
            finalized = CHECKPOINT + 1,
            search = TransactionSearchResult.NotFound(wholeRangeRead = true),
        )

        assertDecided(DurableTxStatus.PENDING, outcome)
    }

    /** Completion is asked first, so a domain that answers both ways cannot fail a transaction that ran. */
    @Test
    fun `proven completion beats proven non-completion`() = runBlocking<Unit> {
        val outcome = evaluate(
            tx = tx(),
            oracle = says(completedAtFinalized = true, notCompletedAtFinalized = true),
            finalized = MORTALITY_END + 1,
        )

        assertDecided(DurableTxStatus.FINALIZED_SUCCESS, outcome)
    }

    // ---- Rule 4 — the short circuit ----

    @Test
    fun `proven non-completion at the best head short circuits the search while the window is open`() =
        runBlocking<Unit> {
            val view = FakePinnedChainView(finalizedNumber = CHECKPOINT + 1)

            val outcome = evaluateLadder(
                tx = tx(),
                scope = says(notCompletedAtBest = true).scope(),
                view = view,
                recordedStillCanonical = null,
            )

            assertDecided(DurableTxStatus.PENDING, outcome)
            assertEquals("the search is what the short circuit exists to avoid", 0, view.searches)
        }

    /**
     * Past mortality the search is the only thing left that can decide it, so the short circuit must not
     * keep it away from one.
     */
    @Test
    fun `the short circuit does not fire once mortality has expired`() = runBlocking<Unit> {
        val view = FakePinnedChainView(finalizedNumber = MORTALITY_END + 1)

        evaluateLadder(
            tx = tx(),
            scope = says(notCompletedAtBest = true).scope(),
            view = view,
            recordedStillCanonical = null,
        )

        assertEquals(1, view.searches)
    }

    // ---- Rule 5 — the block search ----

    @Test
    fun `a successful dispatch found in the window finalizes`() = runBlocking<Unit> {
        val outcome = evaluate(
            tx = tx(),
            oracle = says(),
            search = TransactionSearchResult.Found(block(110), ExtrinsicOutcome.SUCCESS),
        )

        assertDecided(DurableTxStatus.FINALIZED_SUCCESS, outcome)
    }

    /** Inclusion is not success — an extrinsic can be applied and its dispatch still fail. */
    @Test
    fun `a failed dispatch found in the window fails the transaction`() = runBlocking<Unit> {
        val outcome = evaluate(
            tx = tx(),
            oracle = says(),
            search = TransactionSearchResult.Found(block(110), ExtrinsicOutcome.FAILURE),
        )

        assertDecided(DurableTxStatus.FAILURE, outcome)
    }

    /** A policy decides whether to build again from this: a dispatch that failed would most likely fail again. */
    @Test
    fun `a failed dispatch is reported as a dispatch failure`() = runBlocking<Unit> {
        val outcome = evaluate(
            tx = tx(),
            oracle = says(),
            search = TransactionSearchResult.Found(block(110), ExtrinsicOutcome.FAILURE),
        )

        assertEquals(DurableFailureKind.DISPATCH_FAILED, assertDecided(DurableTxStatus.FAILURE, outcome).verdict.failure)
    }

    @Test
    fun `a transaction that never ran within its window is reported as expired`() = runBlocking<Unit> {
        val provenAbsent = evaluate(tx = tx(), oracle = says(notCompletedAtFinalized = true), finalized = MORTALITY_END + 1)
        val searchedAbsent = evaluate(
            tx = tx(),
            oracle = says(),
            finalized = MORTALITY_END + 1,
            search = TransactionSearchResult.NotFound(wholeRangeRead = true),
        )

        assertEquals(DurableFailureKind.EXPIRED, assertDecided(DurableTxStatus.FAILURE, provenAbsent).verdict.failure)
        assertEquals(DurableFailureKind.EXPIRED, assertDecided(DurableTxStatus.FAILURE, searchedAbsent).verdict.failure)
    }

    @Test
    fun `an unreadable outcome leaves the transaction PENDING`() = runBlocking<Unit> {
        val outcome = evaluate(
            tx = tx(),
            oracle = says(),
            search = TransactionSearchResult.Found(block(110), outcome = null),
        )

        assertDecided(DurableTxStatus.PENDING, outcome)
    }

    @Test
    fun `absence fails the transaction only once the whole window was read and mortality expired`() =
        runBlocking<Unit> {
            val outcome = evaluate(
                tx = tx(),
                oracle = says(),
                finalized = MORTALITY_END + 1,
                search = TransactionSearchResult.NotFound(wholeRangeRead = true),
            )

            assertDecided(DurableTxStatus.FAILURE, outcome)
        }

    @Test
    fun `a partially read window leaves the transaction PENDING`() = runBlocking<Unit> {
        val outcome = evaluate(
            tx = tx(),
            oracle = says(),
            finalized = MORTALITY_END + 1,
            search = TransactionSearchResult.NotFound(wholeRangeRead = false),
        )

        assertDecided(DurableTxStatus.PENDING, outcome)
    }

    // ---- a consumer that observes nothing ----

    /**
     * What a domain gets before it writes an oracle at all: correct, and decided by history alone. This is
     * the floor the seam guarantees, and the reason `provenNotCompleted` is safe to leave unimplemented.
     */
    @Test
    fun `a domain with no oracle is still decided by the search`() = runBlocking<Unit> {
        val scope = TxCompletionOracle.unobservableOn("test-chain")
            .openPass(emptyList(), ledgerOf(), FakePinnedChainView())
            .getOrThrow()

        val outcome = evaluateLadder(
            tx = tx(),
            scope = scope,
            view = FakePinnedChainView(
                finalizedNumber = MORTALITY_END + 1,
                search = TransactionSearchResult.Found(block(110), ExtrinsicOutcome.SUCCESS),
            ),
            recordedStillCanonical = null,
        )

        assertDecided(DurableTxStatus.FINALIZED_SUCCESS, outcome)
    }

    // ---- fixtures ----

    private suspend fun evaluate(
        tx: DurableTxEntry,
        oracle: ScriptedOracle,
        finalized: Long = 130,
        recordedStillCanonical: Boolean? = null,
        search: TransactionSearchResult = TransactionSearchResult.NotFound(wholeRangeRead = false),
    ): RuleOutcome = evaluateLadder(
        tx = tx,
        scope = oracle.scope(),
        view = FakePinnedChainView(finalizedNumber = finalized, search = search),
        recordedStillCanonical = recordedStillCanonical,
    )

    private fun assertDecided(expected: DurableTxStatus, outcome: RuleOutcome): RuleOutcome.Decided {
        assertTrue("expected a decision but was $outcome", outcome is RuleOutcome.Decided)
        outcome as RuleOutcome.Decided
        assertEquals(expected, outcome.verdict.status)

        return outcome
    }

    private fun tx(
        successDetectedAt: CheckpointBlock? = null,
        status: DurableTxStatus = DurableTxStatus.PENDING,
    ) = DurableTxEntry(
        id = DurableTxId(1),
        domainId = TxDomainId("test"),
        groupId = null,
        txHash = "0xtx",
        checkpoint = CheckpointBlock(CHECKPOINT, "0xcheckpoint"),
        mortalityBlocks = MORTALITY,
        status = status,
        successDetectedAt = successDetectedAt,
    )
}

private fun block(number: Long) = CheckpointBlock(number, "0xblock$number")

private fun ledgerOf(vararg entry: DurableTxEntry) =
    object : io.paritytech.polkadotapp.feature_transactions.api.domain.durable.LedgerView {
        override val transactions = entry.toList()

        override fun statusOf(id: DurableTxId) = transactions.firstOrNull { it.id == id }?.status
    }

private fun says(
    completedAtFinalized: Boolean = false,
    completedAtBest: Boolean = false,
    notCompletedAtFinalized: Boolean = false,
    notCompletedAtBest: Boolean = false,
) = ScriptedOracle(completedAtFinalized, completedAtBest, notCompletedAtFinalized, notCompletedAtBest)

/** A domain that answers whatever the case says, which is all the ladder is allowed to know about one. */
private class ScriptedOracle(
    private val completedAtFinalized: Boolean,
    private val completedAtBest: Boolean,
    private val notCompletedAtFinalized: Boolean,
    private val notCompletedAtBest: Boolean,
) {
    fun scope() = object : TxCompletionOracle.PassScope {
        override fun provenCompleted(tx: DurableTxEntry, head: HeadKind) = when (head) {
            HeadKind.FINALIZED -> completedAtFinalized
            HeadKind.BEST -> completedAtBest
        }

        override fun provenNotCompleted(tx: DurableTxEntry, head: HeadKind) = when (head) {
            HeadKind.FINALIZED -> notCompletedAtFinalized
            HeadKind.BEST -> notCompletedAtBest
        }
    }
}

/**
 * Counts searches, because two of the rules exist to avoid one and asserting the verdict alone would not
 * notice if they stopped.
 */
private class FakePinnedChainView(
    finalizedNumber: Long = 130,
    private val search: TransactionSearchResult = TransactionSearchResult.NotFound(wholeRangeRead = false),
) : PinnedChainView {
    var searches = 0
        private set

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
    ): TransactionSearchResult {
        searches++

        return search
    }

    /** The ladder reads the heads and searches; a call to anything else is a bug in the test. */
    private fun <T> notScripted(): Result<T> = Result.failure(UnsupportedOperationException("not scripted"))
}
