package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageAssetLedger
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.EntryAssets
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.COINAGE_DOMAIN
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableFailureKind
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.OperationGroupId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.ScheduledDurableTx
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicy
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicyId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPreparation
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import kotlin.time.Duration
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * When a coinage transaction is built, left waiting or given up — the same for every kind of transaction.
 *
 * The kind-specific half (how a transaction is read back from the ledger, what it waits for, how it is built) is a
 * fake here; each real rebuild has its own suite. Several cases keep the names they had when this lived in the
 * claim loop: "claimed" means the policy hands back an extrinsic, "not claimed" or "open" means it leaves the
 * transaction waiting, and "ends" means it gives up for good.
 */
@OptIn(ExperimentalTime::class)
class InputGatedSubmissionPolicyTest {
    private val chainAssetProvider: ChainAssetProvider = mockk()
    private val assetLedger: CoinageAssetLedger = mockk()
    private val timeProvider: TimeProvider = mockk()

    private val rebuild = FakeRebuild()

    private val policy = InputGatedSubmissionPolicy(
        policyId = SubmissionPolicyId("test-policy"),
        rebuild = rebuild,
        chainAssetProvider = chainAssetProvider,
        assetLedger = assetLedger,
        timeProvider = timeProvider,
    )

    @Before
    fun openTheWindow() {
        every { chainAssetProvider.chainId() } returns "test-chain"
        every { timeProvider.now() } returns WINDOW_OPEN
        coEvery { assetLedger.assetsOf(any()) } answers {
            Result.success(firstArg<List<DurableTxId>>().associateWith { EntryAssets(emptyList(), emptyList()) })
        }
    }

    // ---- when a transaction is built ----

    /** The inputs are present, so whatever made the last attempt fail, a rebuild can land. It is built. */
    @Test
    fun `a claim that failed is submitted again while its coin is still on chain`() = runTest {
        val tx = transactionOf(1)
        givenChain(look("in-1"))

        val outcome = prepare(tx)

        assertBuilt(outcome, tx)
    }

    /** Nothing present yet: the input may still be landing, so the transaction is neither built nor given up. */
    @Test
    fun `a coin that is not on chain yet is not claimed`() = runTest {
        val tx = transactionOf(1)
        givenChain(look())

        val outcome = prepare(tx)

        assertLeftWaiting(outcome, tx)
        verifyNothingBuilt()
    }

    @Test
    fun `a coin that appears later is claimed when it does`() = runTest {
        val tx = transactionOf(1)
        givenChain(look(), after(10.seconds), look("in-1"))

        val outcome = prepare(tx)

        assertBuilt(outcome, tx)
    }

    /**
     * One input shows up before the other. Building the first right away would leave the second for a separate
     * call over the same wait, so the call holds out for the whole set while it keeps arriving.
     */
    @Test
    fun `claiming holds out for the whole set before submitting`() = runTest {
        val first = transactionOf(1)
        val second = transactionOf(2)
        givenChain(look("in-1"), after(10.seconds), look("in-1", "in-2"))

        val outcome = prepare(first, second)

        assertBuilt(outcome, first)
        assertBuilt(outcome, second)
        assertEquals(listOf(listOf(first.tx, second.tx)), rebuild.buildCalls)
    }

    /** Holding out is bounded: an input that never arrives must not hold up the one that did. */
    @Test
    fun `a partial detection claims only the coins that arrived`() = runTest {
        val arrived = transactionOf(1)
        val missing = transactionOf(2)
        givenChain(look("in-1"))

        val outcome = prepare(arrived, missing)

        assertBuilt(outcome, arrived)
        assertLeftWaiting(outcome, missing)
    }

    /**
     * The input was there, then a fork took it away. The newest look wins outright: building against the widest
     * view ever seen would spend what the chain no longer has.
     */
    @Test
    fun `a coin a fork took away is not claimed on the strength of an older look`() = runTest {
        val forked = transactionOf(1)
        val missing = transactionOf(2)
        givenChain(look("in-1"), after(5.seconds), look())

        val outcome = prepare(forked, missing)

        assertLeftWaiting(outcome, forked)
        assertLeftWaiting(outcome, missing)
        verifyNothingBuilt()
    }

    /** A read that could not be taken emits nothing, so the look before it still stands. */
    @Test
    fun `a failed read does not erase what the chain last showed`() = runTest {
        val seen = transactionOf(1)
        val missing = transactionOf(2)
        givenChain(look("in-1"), after(5.seconds))

        val outcome = prepare(seen, missing)

        assertBuilt(outcome, seen)
    }

    /** Every transaction ready in one call shares whatever building costs once, so they are built together. */
    @Test
    fun `every transaction ready in one call is built in a single build`() = runTest {
        val first = transactionOf(1)
        val second = transactionOf(2)
        givenChain(look("in-1", "in-2"))

        prepare(first, second)

        assertEquals(listOf(listOf(first.tx, second.tx)), rebuild.buildCalls)
    }

    /** A transaction spending several inputs is built only once all of them are there. */
    @Test
    fun `a transaction is not built while any of its inputs is missing`() = runTest {
        val tx = transactionOf(1, inputs = setOf("in-a", "in-b"))
        givenChain(look("in-a"))

        val outcome = prepare(tx)

        assertLeftWaiting(outcome, tx)
        verifyNothingBuilt()
    }

    // ---- when a transaction ends ----

    /** The window closed and a look shows the input gone: nothing further can make this transaction land. */
    @Test
    fun `claiming ends when the window closes on coins that never arrived`() = runTest {
        val tx = transactionOf(1)
        givenWindowClosed()
        givenChain(look())

        val outcome = prepare(tx)

        assertGaveUp(outcome, tx)
    }

    /** One input proven gone past the deadline is enough: the transaction can never spend all of them. */
    @Test
    fun `a transaction is given up once any of its inputs is gone past the deadline`() = runTest {
        val tx = transactionOf(1, inputs = setOf("in-a", "in-b"))
        givenWindowClosed()
        givenChain(look("in-a"))

        val outcome = prepare(tx)

        assertGaveUp(outcome, tx)
    }

    /** The window bounds how long we wait for an input to appear, never whether one that is there is used. */
    @Test
    fun `a coin still on chain is claimed however long ago the payment arrived`() = runTest {
        val tx = transactionOf(1)
        givenWindowClosed()
        givenChain(look("in-1"))

        val outcome = prepare(tx)

        assertBuilt(outcome, tx)
    }

    /**
     * Past the window, one input is still there and one is gone. The one still there is built rather than
     * abandoned alongside the one that is gone.
     */
    @Test
    fun `claiming carries on past the window while the coin is still there`() = runTest {
        val stillThere = transactionOf(1)
        val gone = transactionOf(2)
        givenWindowClosed()
        givenChain(look("in-1"))

        val outcome = prepare(stillThere, gone)

        assertBuilt(outcome, stillThere)
        assertGaveUp(outcome, gone)
    }

    /** A transaction first seen after its window closed is still built if its input is there. */
    @Test
    fun `a first build is made even when the window has already closed`() = runTest {
        val tx = transactionOf(1)
        givenWindowClosed()
        givenChain(look("in-1"))

        val outcome = prepare(tx)

        assertBuilt(outcome, tx)
    }

    /** Inside the window, an input that never showed up is left waiting: giving up here would strand it. */
    @Test
    fun `a coin that has not appeared keeps the claim open`() = runTest {
        val tx = transactionOf(1)
        givenChain(look(), after(1.seconds), look())

        val outcome = prepare(tx)

        assertLeftWaiting(outcome, tx)
    }

    /** A transaction built only once — the merchant path — is abandoned the moment a look shows its input missing. */
    @Test
    fun `a transfer scheduled without a retry window gives up as soon as its input is seen missing`() = runTest {
        val tx = transactionOf(1, deadline = WINDOW_OPEN, retriesFailures = false)
        givenChain(look())

        val outcome = prepare(tx)

        assertGaveUp(outcome, tx)
    }

    /**
     * Nothing changes on a quiet chain, so waiting for the next look would notice a deadline only whenever the call
     * happens to return. The policy wakes at the deadline instead.
     */
    @Test
    fun `a deadline that passes on a quiet chain is noticed without a new look`() = runTest {
        val tx = transactionOf(1, deadline = WINDOW_OPEN + QUIET_DEADLINE)
        every { timeProvider.now() } answers {
            if (testScheduler.currentTime >= QUIET_DEADLINE.inWholeMilliseconds) WINDOW_CLOSED else WINDOW_OPEN
        }
        givenChain(look())

        val outcome = prepare(tx)

        assertGaveUp(outcome, tx)
        assertTrue(
            "the deadline was noticed only at ${testScheduler.currentTime}ms",
            testScheduler.currentTime < IDLE_LIMIT.inWholeMilliseconds,
        )
    }

    /** A build that failed is the executor's to try again later; calling it a give-up would lose the transaction. */
    @Test
    fun `a build failure is reported as a failure, not a give-up`() = runTest {
        val tx = transactionOf(1)
        givenChain(look("in-1"))
        rebuild.buildOutcome = { Result.failure(IllegalStateException("no runtime")) }

        val outcome = policy.prepareSubmission(listOf(tx.scheduled))

        assertTrue("expected a failure but was $outcome", outcome.isFailure)
    }

    /** Nothing recorded can ever make it buildable, so waiting on the chain for it would only hold its lock. */
    @Test
    fun `a transaction that cannot be resolved is given up without waiting`() = runTest {
        val tx = transactionOf(1, resolvable = false)

        val outcome = prepare(tx)

        assertGaveUp(outcome, tx)
        assertEquals(0, rebuild.presenceRequests)
    }

    @Test
    fun `a transaction whose params cannot be read is given up without waiting`() = runTest {
        val tx = transactionOf(1, readableTerms = false)

        val outcome = prepare(tx)

        assertGaveUp(outcome, tx)
        assertEquals(0, rebuild.presenceRequests)
    }

    /** An unbuildable transaction does not stop the others in its call from being built. */
    @Test
    fun `an unbuildable transaction does not hold up the others`() = runTest {
        val unbuildable = transactionOf(1, resolvable = false)
        val buildable = transactionOf(2)
        givenChain(look("in-2"))

        val outcome = prepare(unbuildable, buildable)

        assertGaveUp(outcome, unbuildable)
        assertBuilt(outcome, buildable)
    }

    // ---- whether a failure is retried ----

    /** The merchant path builds its transfer once: no failure, however it happened, is built again. */
    @Test
    fun `a transfer scheduled to be built once is never retried`() = runTest {
        val tx = transactionOf(1, retriesFailures = false)

        DurableFailureKind.entries.forEach { failure ->
            assertFalse("$failure was retried", policy.canRetry(mockk(), tx.params, failure))
        }
    }

    /** An attempt that simply never got included may land if built again, however late that is. */
    @Test
    fun `an attempt that expired is retried even after the window`() = runTest {
        val tx = transactionOf(1)
        givenWindowClosed()

        assertTrue(policy.canRetry(mockk(), tx.params, DurableFailureKind.EXPIRED))
    }

    /** A dispatch failure would most likely repeat, so only the window bounds how often it is rebuilt. */
    @Test
    fun `a dispatch failure is retried only while the window is open`() = runTest {
        val tx = transactionOf(1)

        assertTrue(policy.canRetry(mockk(), tx.params, DurableFailureKind.DISPATCH_FAILED))

        givenWindowClosed()

        assertFalse(policy.canRetry(mockk(), tx.params, DurableFailureKind.DISPATCH_FAILED))
    }

    @Test
    fun `a rejected attempt is retried only while the window is open`() = runTest {
        val tx = transactionOf(1)

        assertTrue(policy.canRetry(mockk(), tx.params, DurableFailureKind.REJECTED))

        givenWindowClosed()

        assertFalse(policy.canRetry(mockk(), tx.params, DurableFailureKind.REJECTED))
    }

    @Test
    fun `a transaction whose params cannot be read is never retried`() = runTest {
        val tx = transactionOf(1, readableTerms = false)

        assertFalse(policy.canRetry(mockk(), tx.params, DurableFailureKind.EXPIRED))
    }

    // ---- harness ----

    private suspend fun prepare(vararg transactions: TestTransaction): Map<DurableTxId, SubmissionPreparation> =
        policy.prepareSubmission(transactions.map { it.scheduled }).getOrThrow()

    private fun assertBuilt(outcome: Map<DurableTxId, SubmissionPreparation>, tx: TestTransaction) {
        val ready = outcome[tx.id]
        assertTrue("expected ${tx.id} to be built but was $ready", ready is SubmissionPreparation.Ready)
        assertEquals(rebuild.builtFor[tx.tx], (ready as SubmissionPreparation.Ready).extrinsic)
    }

    private fun assertLeftWaiting(outcome: Map<DurableTxId, SubmissionPreparation>, tx: TestTransaction) {
        assertTrue("expected ${tx.id} to keep waiting but was ${outcome[tx.id]}", tx.id !in outcome)
    }

    private fun assertGaveUp(outcome: Map<DurableTxId, SubmissionPreparation>, tx: TestTransaction) {
        assertEquals(SubmissionPreparation.GiveUp, outcome[tx.id])
    }

    private fun verifyNothingBuilt() {
        assertTrue("expected no build but got ${rebuild.buildCalls}", rebuild.buildCalls.isEmpty())
    }

    private fun givenWindowClosed() {
        every { timeProvider.now() } returns WINDOW_CLOSED
    }

    private fun givenChain(vararg steps: ChainStep) {
        rebuild.steps = steps.toList()
    }

    private class TestTransaction(val scheduled: ScheduledDurableTx, val tx: FakeTx) {
        val id: DurableTxId get() = scheduled.id
        val params: DataByteArray get() = scheduled.policy.params
    }

    /** By default one input, `in-<seed>`, retried until [RETRY_UNTIL]. */
    private fun transactionOf(
        seed: Int,
        inputs: Set<String> = setOf("in-$seed"),
        deadline: Instant = RETRY_UNTIL,
        retriesFailures: Boolean = true,
        resolvable: Boolean = true,
        readableTerms: Boolean = true,
    ): TestTransaction {
        val params = byteArrayOf(seed.toByte()).toDataByteArray()
        val scheduled = ScheduledDurableTx(
            id = DurableTxId(seed.toLong()),
            domainId = COINAGE_DOMAIN,
            groupId = GROUP,
            policy = SubmissionPolicy(SubmissionPolicyId("test-policy"), params),
        )
        val tx = FakeTx(seed, inputs)

        if (resolvable) rebuild.resolvable[scheduled.id] = tx
        if (readableTerms) rebuild.terms[params] = RebuildTerms(deadline, retriesFailures)

        return TestTransaction(scheduled, tx)
    }

    private data class FakeTx(val seed: Int, val inputs: Set<String>)

    private sealed interface ChainStep

    private class Look(val present: Set<String>) : ChainStep

    private class Pause(val duration: Duration) : ChainStep

    private fun look(vararg present: String): ChainStep = Look(present.toSet())

    private fun after(duration: Duration): ChainStep = Pause(duration)

    /** The kind-specific half, scripted: what resolves, what the chain shows, and what building returns. */
    private class FakeRebuild : CoinageRebuild<FakeTx, String> {
        val resolvable = mutableMapOf<DurableTxId, FakeTx>()
        val terms = mutableMapOf<DataByteArray, RebuildTerms>()
        var steps: List<ChainStep> = emptyList()

        val buildCalls = mutableListOf<List<FakeTx>>()
        val builtFor = mutableMapOf<FakeTx, EnrichedSendableExtrinsic>()
        var presenceRequests = 0

        var buildOutcome: (List<FakeTx>) -> Result<List<EnrichedSendableExtrinsic>> = { transactions ->
            Result.success(transactions.map { tx -> mockk<EnrichedSendableExtrinsic>().also { builtFor[tx] = it } })
        }

        override fun termsOf(params: DataByteArray): RebuildTerms? = terms[params]

        override suspend fun resolve(
            transactions: List<ScheduledDurableTx>,
            assets: Map<DurableTxId, EntryAssets>,
        ): Map<DurableTxId, FakeTx> = transactions.mapNotNull { tx -> resolvable[tx.id]?.let { tx.id to it } }.toMap()

        override fun inputsOf(transaction: FakeTx): Set<String> = transaction.inputs

        /** One emission per look, and a subscription that stays open after the last one. */
        override suspend fun presence(inputs: Set<String>): Flow<Set<String>> {
            presenceRequests++

            return flow {
                steps.forEach { step ->
                    when (step) {
                        is Pause -> delay(step.duration)
                        is Look -> emit(step.present)
                    }
                }
                awaitCancellation()
            }
        }

        override suspend fun build(transactions: List<FakeTx>): Result<List<EnrichedSendableExtrinsic>> {
            buildCalls += transactions

            return buildOutcome(transactions)
        }
    }

    private companion object {
        val GROUP = OperationGroupId("group")

        val RETRY_UNTIL = Instant.fromEpochSeconds(1_000)
        val WINDOW_OPEN = Instant.fromEpochSeconds(500)
        val WINDOW_CLOSED = Instant.fromEpochSeconds(1_500)

        val QUIET_DEADLINE = 60.seconds
        val IDLE_LIMIT = 5.minutes
    }
}
