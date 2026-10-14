package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness

import io.paritytech.polkadotapp.chains.extrinsic.ExtrinsicStatus
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ExtrinsicOutcome
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.TestActionFinality.FINALIZED
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.TestActionFinality.IN_BEST
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.PreSubmissionValidationFailed
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FAILURE
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FINALIZED_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING_SUCCESS
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * What the watcher writes, as opposed to what a pass derives.
 *
 * The watcher is the only writer of `successDetectedAt` before finality, and that record is what keeps an
 * entry's outputs selectable in the window between inclusion and finalization. Everything here is a status
 * the node reports, so the flow is driven directly rather than through the chain.
 */
class WatcherScenariosTest {
    @Test
    fun `an inclusion with a successful dispatch records the block it was seen in`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val (id, txHash, events) = watchedEntry()

        val block = includeInBlock(txHash, ExtrinsicOutcome.SUCCESS, finality = IN_BEST)
        events.tryEmit(ExtrinsicStatus.InBlock(block.hash, txHash))
        releaseSubmissions()

        assertEquals(PENDING_SUCCESS, statusOf(id))
        assertEquals(block.number, repository.getEntry(id).getOrThrow()!!.successDetectedAt?.blockNumber)
    }

    /**
     * The app goes to background while a transaction is in flight.
     * The watch is the only thing following it, so the connection has to stay up for as long as it lasts.
     * Once the transaction is decided the reference goes, and the chain may disconnect again.
     *
     * Losing the subscription is not a correctness failure — the recovery pass would still decide the entry —
     * but it would decide it the slow way, over a whole mortality window with the inputs locked.
     */
    @Test
    fun `a watch holds the chain connection open until its transaction is decided`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val (id, txHash, events) = watchedEntry()

        val block = includeInBlock(txHash, ExtrinsicOutcome.SUCCESS, finality = FINALIZED)
        events.tryEmit(ExtrinsicStatus.Finalized(block.hash, txHash))
        releaseSubmissions()

        assertEquals(FINALIZED_SUCCESS, statusOf(id))
        assertEquals("the watch must keep the connection while it runs", 1, connections.peak)
        assertEquals("a finished watch must let the connection go", 0, connections.held)
    }

    /**
     * The node's pre-submission check rejects the extrinsic, so it is never handed to a pool.
     * Nothing can ever include it, and the entry is failed immediately.
     *
     * Every other terminal verdict here waits for finalized evidence. This one does not have to: validation
     * runs only ahead of the first submission, so a rejection proves the bytes never left the device. Waiting
     * would cost the entry a full mortality window with its inputs locked, for an outcome already known.
     */
    @Test
    fun `a transaction the node refuses before submission is failed without waiting for its window`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val (id, txHash, events) = watchedEntry()

        events.tryEmit(ExtrinsicStatus.FailedToSubmit(PreSubmissionValidationFailed()))
        releaseSubmissions()

        assertEquals(FAILURE, statusOf(id))
        assertEquals(1, submissionCount)
    }

    /**
     * The same terminal failure, but from a submission that did reach the node. Those bytes may still be in
     * someone's pool, so the entry stays live and its own window decides it.
     */
    @Test
    fun `a submission that failed after reaching the node is left to the pass`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val (id, txHash, events) = watchedEntry()

        events.tryEmit(ExtrinsicStatus.FailedToSubmit(IllegalStateException("socket closed")))
        releaseSubmissions()

        assertEquals(PENDING, statusOf(id))
    }

    @Test
    fun `a finalized block with a successful dispatch finalizes the entry`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val (id, txHash, events) = watchedEntry()

        val block = includeInBlock(txHash, ExtrinsicOutcome.SUCCESS, finality = FINALIZED)
        events.tryEmit(ExtrinsicStatus.Finalized(block.hash, txHash))
        releaseSubmissions()

        assertEquals(FINALIZED_SUCCESS, statusOf(id))
    }

    /**
     * A transaction that finalized needs nothing from the pass: the watcher already wrote the terminal
     * verdict, and a terminal entry is never rewritten.
     *
     * Asking anyway is pure cost — a worker scheduled and a chain view pinned to re-derive a decision that
     * has already been made — and on the happy path it happens for every transaction the app submits.
     */
    @Test
    fun `a transaction that finalized asks for no recovery`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val (id, txHash, events) = watchedEntry()

        val block = includeInBlock(txHash, ExtrinsicOutcome.SUCCESS, finality = FINALIZED)
        events.tryEmit(ExtrinsicStatus.Finalized(block.hash, txHash))
        releaseSubmissions()

        assertEquals(FINALIZED_SUCCESS, statusOf(id))
        assertRecoveryWasNotRequested()
    }

    /** The same for the other terminal verdict a watcher can write itself. */
    @Test
    fun `a transaction refused before submission asks for no recovery`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val (id, txHash, events) = watchedEntry()

        events.tryEmit(ExtrinsicStatus.FailedToSubmit(PreSubmissionValidationFailed()))
        releaseSubmissions()

        assertEquals(FAILURE, statusOf(id))
        assertRecoveryWasNotRequested()
    }

    /**
     * A watch that ends without deciding anything is the case recovery exists for, so it must still ask —
     * here the node dropped the transaction and said nothing about whether it executed.
     */
    @Test
    fun `a transaction left undecided when its watch ends is handed to recovery`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val (id, txHash, events) = watchedEntry()

        events.tryEmit(ExtrinsicStatus.Dropped(txHash))
        releaseSubmissions()

        assertEquals(PENDING, statusOf(id))
        assertRecoveryWasRequested()
    }

    /** No block number, no record: a verdict must never name a block the view could not resolve. */
    @Test
    fun `an inclusion in a block that cannot be read records nothing`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val (id, txHash, events) = watchedEntry()

        val block = includeInBlock(txHash, ExtrinsicOutcome.SUCCESS, finality = IN_BEST)
        makeBlocksUnreadable(block.number)
        events.tryEmit(ExtrinsicStatus.InBlock(block.hash, txHash))
        releaseSubmissions()

        assertEquals(PENDING, statusOf(id))
        assertNull(repository.getEntry(id).getOrThrow()!!.successDetectedAt)
    }

    @Test
    fun `a retraction of the recorded block clears the record`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val (id, txHash, events) = watchedEntry()

        val block = includeInBlock(txHash, ExtrinsicOutcome.SUCCESS, finality = IN_BEST)
        // Both before the watcher is let run: it releases the entry after a silence timeout, so anything
        // emitted after the first release would reach nobody.
        events.tryEmit(ExtrinsicStatus.InBlock(block.hash, txHash))
        events.tryEmit(ExtrinsicStatus.Retracted(block.hash, txHash))
        releaseSubmissions()

        assertEquals(PENDING, statusOf(id))
        assertNull(repository.getEntry(id).getOrThrow()!!.successDetectedAt)
    }

    /** A retraction elsewhere in the chain says nothing about the block this entry was seen in. */
    @Test
    fun `a retraction naming another block leaves the record alone`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val (id, txHash, events) = watchedEntry()

        val block = includeInBlock(txHash, ExtrinsicOutcome.SUCCESS, finality = IN_BEST)
        advanceBlocks(1, finality = IN_BEST)

        events.tryEmit(ExtrinsicStatus.InBlock(block.hash, txHash))
        events.tryEmit(ExtrinsicStatus.Retracted(chain.chain.bestHead.hash, txHash))
        releaseSubmissions()

        assertEquals(PENDING_SUCCESS, statusOf(id))
        assertEquals(block.number, repository.getEntry(id).getOrThrow()!!.successDetectedAt?.blockNumber)
    }

    /**
     * The resubmission path keeps consuming the same flow, so a `Ready` can arrive after an `InBlock`.
     *
     * It must not be read as "back in the pool": the record it would clear is what keeps this entry's outputs
     * selectable, and losing it withdraws them for the rest of the mortality window on no evidence at all.
     */
    @Test
    fun `a Ready arriving after an inclusion does not wipe the record`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val (id, txHash, events) = watchedEntry()

        val block = includeInBlock(txHash, ExtrinsicOutcome.SUCCESS, finality = IN_BEST)

        events.tryEmit(ExtrinsicStatus.InBlock(block.hash, txHash))
        events.tryEmit(ExtrinsicStatus.Ready(txHash))
        releaseSubmissions()

        assertEquals(PENDING_SUCCESS, statusOf(id))
        assertNotNull(repository.getEntry(id).getOrThrow()!!.successDetectedAt)
    }

    /**
     * The app submits a transaction and the connection dies while it is subscribed to it.
     * Nothing is ever reported about that transaction.
     * The entry must stay undecided, keep its input locked, and be handed back for a pass to resolve.
     *
     * A dead subscription is not evidence about the transaction — it may well have executed. The watcher's
     * only job here is to let go, exactly as it would on a silence timeout, so recovery picks the entry up.
     */
    @Test
    fun `a subscription that fails hands the entry to recovery with its lock intact`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        submissionStatuses = { flow { throw IllegalStateException("connection lost") } }

        val id = register(COIN_A, COIN_B).getOrThrow()
        releaseSubmissions()

        assertEquals(PENDING, statusOf(id))
        assertFalse("ownership must not be held by a watcher that died", ownedEntries.isOwnedBySubmission(id))
        assertRecoveryWasRequested()
        assertEquals("the input stays locked to the entry", PENDING, assetStateOf(COIN_A).consumerStatus)
    }

    /**
     * The same, except the subscription call itself throws rather than the stream it returns.
     * The entry must end up in the same place: undecided, locked, released to recovery.
     */
    @Test
    fun `a subscription that cannot even be opened is handled like one that died`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        submissionStatuses = { throw IllegalStateException("no connection to subscribe with") }

        val id = register(COIN_A, COIN_B).getOrThrow()
        releaseSubmissions()

        assertEquals(PENDING, statusOf(id))
        assertFalse(ownedEntries.isOwnedBySubmission(id))
        assertRecoveryWasRequested()
    }

    /**
     * Two transactions are in flight and the subscription for the first one throws.
     * The second is reported included and finalized as normal.
     * The second must still get its verdict.
     *
     * The watchers share a scope, so a failure that escaped one would cancel the other and the app would
     * silently stop learning about every transaction it had in flight.
     */
    @Test
    fun `a failed subscription does not take down the watcher of another transaction`() = scenario {
        mintCoinsOnChain(COIN_A, COIN_C, finality = FINALIZED)
        val events = MutableSharedFlow<ExtrinsicStatus>(replay = 4, extraBufferCapacity = 8)
        submissionStatuses = { n -> if (n == 0) flow { throw IllegalStateException("connection lost") } else events }

        val failed = register(COIN_A, COIN_B).getOrThrow()
        val healthy = register(COIN_C, COIN_D).getOrThrow()
        val txHash = repository.getEntry(healthy).getOrThrow()!!.txHash

        val block = includeInBlock(txHash, ExtrinsicOutcome.SUCCESS, finality = FINALIZED)
        events.tryEmit(ExtrinsicStatus.Finalized(block.hash, txHash))
        releaseSubmissions()

        assertEquals(FINALIZED_SUCCESS, statusOf(healthy))
        assertEquals(PENDING, statusOf(failed))
    }

    /**
     * The node refuses a payment's split before it reaches a pool, which proves it can never land.
     * The payment was registered with a policy that builds it again.
     * The same entry is rebuilt and submitted once more, minting into the same coin.
     *
     * Failing it instead would release the input and leave the recipient holding a key to a coin that will
     * never exist — the partial payment retries are there to prevent.
     */
    @Test
    fun `a failure the watch proves is handed back to its policy instead of failing`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val events = MutableSharedFlow<ExtrinsicStatus>(replay = 4, extraBufferCapacity = 8)
        submissionStatuses = { n -> if (n == 0) events else emptyFlow() }
        val policy = givenSubmissionPolicy(PolicyBehaviour.BUILD)

        val id = registerRetriable(inputCoin = COIN_A, COIN_B)
        val firstAttempt = txHashOf(id)

        events.tryEmit(ExtrinsicStatus.FailedToSubmit(PreSubmissionValidationFailed()))
        releaseSubmissions()

        assertEquals(PENDING, statusOf(id))
        assertEquals(listOf(listOf(id)), policy.prepared)
        assertNotEquals("the rebuilt entry carries a new attempt", firstAttempt, txHashOf(id))
        assertEquals(2, submissionCount)
        assertEquals("the input stays locked to the same entry", PENDING, assetStateOf(COIN_A).consumerStatus)
        assertEquals("the rebuild mints the same coin", PENDING, assetStateOf(COIN_B).minterStatus)
    }

    /** A policy being installed changes nothing for a transaction that was registered without one. */
    @Test
    fun `a failure without a policy stays final`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val policy = givenSubmissionPolicy(PolicyBehaviour.BUILD)
        val (id, _, events) = watchedEntry()

        events.tryEmit(ExtrinsicStatus.FailedToSubmit(PreSubmissionValidationFailed()))
        releaseSubmissions()

        assertEquals(FAILURE, statusOf(id))
        assertTrue(policy.prepared.isEmpty())
        assertEquals(1, submissionCount)
    }

    private suspend fun DurabilityHarness.watchedEntry(): Triple<CoinageTransactionId, String, MutableSharedFlow<ExtrinsicStatus>> {
        val events = MutableSharedFlow<ExtrinsicStatus>(replay = 4, extraBufferCapacity = 8)
        submissionStatuses = { events }
        val id = register(COIN_A, COIN_B).getOrThrow()

        return Triple(id, repository.getEntry(id).getOrThrow()!!.txHash, events)
    }
}

private const val COIN_A = 1
private const val COIN_B = 2
private const val COIN_C = 3
private const val COIN_D = 4
