package io.paritytech.polkadotapp.feature_transactions_impl.domain.durable

import dagger.Lazy
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.AsyncDurableSubmissionPolicy
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxRegistrationError
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.OperationGroupId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.ScheduledDurableTx
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicy
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicyId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPreparation
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxDomainId
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.DurableTxRepository
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds

/**
 * The executor decides when a waiting transaction is built and what happens to it next. The two ways to get
 * that wrong are opposite: failing a transaction a later call could still have built loses a payment, and
 * building one before its row is committed builds against a lock that may never exist.
 */
class DurableSubmissionExecutorTest {
    private val repository: DurableTxRepository = mockk()
    private val launcher: DurableSubmissionLauncher = mockk()
    private val recoveryScheduler: DurableRecoveryScheduler = mockk(relaxed = true)
    private val connections: ChainConnectionRefCounter = mockk(relaxed = true)

    private val policyA: AsyncDurableSubmissionPolicy = policy()
    private val policyB: AsyncDurableSubmissionPolicy = policy()
    private var policies = mapOf(POLICY_A to policyA, POLICY_B to policyB)

    private val pending = MutableStateFlow<List<ScheduledDurableTx>>(emptyList())
    private val started = mutableListOf<DurableTxId>()
    private val abandoned = mutableListOf<DurableTxId>()

    private var executor: DurableSubmissionExecutor? = null

    @After
    fun closeExecutor() {
        executor?.close()
    }

    @Test
    fun `a scheduled transaction is built once the ledger shows it`() = runTest {
        withLedger()
        val tx = scheduled(1, POLICY_A, GROUP_1)
        withPolicyReady(policyA)

        start()
        advanceUntilIdle()
        assertEquals(emptyList<DurableTxId>(), started)

        pending.value = listOf(tx)
        advanceUntilIdle()

        assertEquals(listOf(tx.id), started)
    }

    @Test
    fun `transactions are grouped by policy and then by operation group`() = runTest {
        withLedger()
        val calls = mutableListOf<List<DurableTxId>>()
        withPolicyReady(policyA, recordingInto = calls)
        withPolicyReady(policyB, recordingInto = calls)
        pending.value = listOf(
            scheduled(1, POLICY_A, GROUP_1),
            scheduled(2, POLICY_A, GROUP_1),
            scheduled(3, POLICY_B, GROUP_1),
            scheduled(4, POLICY_A, GROUP_2),
        )

        start()
        advanceUntilIdle()

        assertEquals(
            setOf(listOf(DurableTxId(1), DurableTxId(2)), listOf(DurableTxId(3)), listOf(DurableTxId(4))),
            calls.toSet(),
        )
    }

    @Test
    fun `a policy waiting on one group does not hold up another`() = runTest {
        withLedger()
        val waiting = scheduled(1, POLICY_A, GROUP_1)
        val ready = scheduled(2, POLICY_A, GROUP_2)
        coEvery { policyA.prepareSubmission(any()) } coAnswers {
            val transactions = firstArg<List<ScheduledDurableTx>>()
            if (waiting in transactions) awaitCancellation()
            transactions.readyAll()
        }
        pending.value = listOf(waiting, ready)

        start()
        advanceUntilIdle()

        assertEquals(listOf(ready.id), started)
    }

    @Test
    fun `a failed preparation is tried again after a backoff and never given up`() = runTest {
        withLedger()
        val tx = scheduled(1, POLICY_A, GROUP_1)
        var calls = 0
        coEvery { policyA.prepareSubmission(any()) } coAnswers {
            calls++
            if (calls < 4) Result.failure(IllegalStateException("node unreachable")) else firstArg<List<ScheduledDurableTx>>().readyAll()
        }
        pending.value = listOf(tx)

        start()
        advanceUntilIdle()

        assertEquals(4, calls)
        assertEquals(listOf(tx.id), started)
        assertEquals(emptyList<DurableTxId>(), abandoned)
    }

    @Test
    fun `a preparation that throws is treated like a failed one`() = runTest {
        withLedger()
        val tx = scheduled(1, POLICY_A, GROUP_1)
        var calls = 0
        coEvery { policyA.prepareSubmission(any()) } coAnswers {
            calls++
            if (calls == 1) throw IllegalStateException("bug") else firstArg<List<ScheduledDurableTx>>().readyAll()
        }
        pending.value = listOf(tx)

        start()
        advanceUntilIdle()

        assertEquals(listOf(tx.id), started)
        assertEquals(emptyList<DurableTxId>(), abandoned)
    }

    @Test
    fun `a call that decides nothing backs off rather than spinning`() = runTest {
        withLedger()
        var calls = 0
        coEvery { policyA.prepareSubmission(any()) } coAnswers {
            calls++
            Result.success(emptyMap())
        }
        pending.value = listOf(scheduled(1, POLICY_A, GROUP_1))

        start()
        runCurrent()
        advanceTimeBy(1.seconds)

        assertEquals(1, calls)

        advanceTimeBy(10.minutes)

        assertTrue("expected a bounded number of calls, got $calls", calls in 2..12)
        assertEquals(emptyList<DurableTxId>(), abandoned)

        // A bucket that never decides keeps backing off, and runTest would drain that forever on exit.
        executor!!.close()
    }

    @Test
    fun `a transaction its policy gives up on is failed`() = runTest {
        withLedger()
        val tx = scheduled(1, POLICY_A, GROUP_1)
        coEvery { policyA.prepareSubmission(any()) } returns Result.success(mapOf(tx.id to SubmissionPreparation.GiveUp))
        pending.value = listOf(tx)

        start()
        advanceUntilIdle()

        assertEquals(listOf(tx.id), abandoned)
        assertEquals(emptyList<DurableTxId>(), started)
    }

    @Test
    fun `an attempt the engine refuses to track fails the transaction`() = runTest {
        withLedger(attemptOutcome = Result.failure(DurableTxRegistrationError.NotMortal))
        val tx = scheduled(1, POLICY_A, GROUP_1)
        withPolicyReady(policyA)
        pending.value = listOf(tx)

        start()
        advanceUntilIdle()

        assertEquals(listOf(tx.id), abandoned)
    }

    /** A database hiccup while starting the attempt says nothing about the transaction, so it must stay waiting. */
    @Test
    fun `an attempt that could not be started stays waiting`() = runTest {
        withLedger()
        val tx = scheduled(1, POLICY_A, GROUP_1)
        withPolicyReady(policyA)
        coEvery { launcher.startAttempt(any(), any()) } returns Result.failure(IllegalStateException("database locked"))
        pending.value = listOf(tx)

        start()
        runCurrent()
        advanceTimeBy(1.minutes)

        assertEquals(emptyList<DurableTxId>(), abandoned)
        assertEquals(listOf(tx), pending.value)

        executor!!.close()
    }

    /**
     * A Room flow can skip the states between two emissions, so the set of waiting transactions can read the
     * same twice while one of them was started and handed back in between.
     */
    @Test
    fun `a transaction handed back between two identical emissions is still built`() = runTest {
        withLedger()
        val tx = scheduled(1, POLICY_A, GROUP_1)
        withPolicyReady(policyA)
        pending.value = listOf(tx)

        start()
        advanceUntilIdle()
        assertEquals(listOf(tx.id), started)

        pending.value = listOf(tx)
        advanceUntilIdle()

        assertEquals(listOf(tx.id, tx.id), started)
    }

    /** A failure that repeats on every rebuild must not turn into a build loop as fast as the chain answers. */
    @Test
    fun `a transaction built again waits longer each time`() = runTest {
        withLedger()
        val tx = scheduled(1, POLICY_A, GROUP_1)
        withPolicyReady(policyA)
        pending.value = listOf(tx)

        start()
        advanceUntilIdle()
        assertEquals(1, started.size)

        pending.value = listOf(tx)
        runCurrent()
        advanceTimeBy(4.seconds)
        assertEquals(1, started.size)
        advanceTimeBy(2.seconds)
        assertEquals(2, started.size)

        pending.value = listOf(tx)
        runCurrent()
        advanceTimeBy(9.seconds)
        assertEquals(2, started.size)
        advanceTimeBy(2.seconds)
        assertEquals(3, started.size)
    }

    @Test
    fun `a transaction that becomes pending during a call is built in the next one`() = runTest {
        withLedger()
        val first = scheduled(1, POLICY_A, GROUP_1)
        val second = scheduled(2, POLICY_A, GROUP_1)
        val calls = mutableListOf<List<DurableTxId>>()
        coEvery { policyA.prepareSubmission(any()) } coAnswers {
            val transactions = firstArg<List<ScheduledDurableTx>>()
            calls += transactions.map { it.id }
            if (second !in pending.value) pending.value = pending.value + second
            transactions.readyAll()
        }
        pending.value = listOf(first)

        start()
        advanceUntilIdle()

        assertEquals(listOf(listOf(first.id), listOf(second.id)), calls)
        assertEquals(listOf(first.id, second.id), started)
    }

    @Test
    fun `a transaction whose policy is not registered is failed`() = runTest {
        withLedger()
        policies = emptyMap()
        val tx = scheduled(1, POLICY_A, GROUP_1)
        pending.value = listOf(tx)

        start()
        advanceUntilIdle()

        assertEquals(listOf(tx.id), abandoned)
    }

    @Test
    fun `recovery is kept scheduled while anything waits`() = runTest {
        withLedger()
        coEvery { policyA.prepareSubmission(any()) } coAnswers { awaitCancellation() }

        start()
        advanceUntilIdle()
        coVerify(exactly = 0) { recoveryScheduler.ensureRunning() }

        pending.value = listOf(scheduled(1, POLICY_A, GROUP_1))
        advanceUntilIdle()

        coVerify(atLeast = 1) { recoveryScheduler.ensureRunning() }
    }

    @Test
    fun `starting twice builds each transaction once`() = runTest {
        withLedger()
        val calls = mutableListOf<List<DurableTxId>>()
        withPolicyReady(policyA, recordingInto = calls)
        pending.value = listOf(scheduled(1, POLICY_A, GROUP_1))

        start()
        executor!!.ensureStarted()
        advanceUntilIdle()

        assertEquals(1, calls.size)
    }

    @Test
    fun `closing leaves waiting transactions waiting`() = runTest {
        withLedger()
        coEvery { policyA.prepareSubmission(any()) } coAnswers { awaitCancellation() }
        pending.value = listOf(scheduled(1, POLICY_A, GROUP_1))

        start()
        advanceUntilIdle()
        executor!!.close()
        advanceUntilIdle()

        assertEquals(emptyList<DurableTxId>(), abandoned)
        assertEquals(1, pending.value.size)
    }

    // ---- harness ----

    private fun TestScope.start() {
        executor = DurableSubmissionExecutor(
            repository = repository,
            policies = Lazy { policies },
            launcher = launcher,
            recoveryScheduler = recoveryScheduler,
            chainConnectionRefCounter = connections,
            dispatchers = TestCoroutineDispatchers(StandardTestDispatcher(testScheduler)),
        ).also { it.ensureStarted() }
    }

    /** A ledger over [pending]: starting or abandoning a transaction takes it out of the waiting set. */
    private fun withLedger(attemptOutcome: Result<Boolean> = Result.success(true)) {
        every { repository.subscribePendingSubmissions() } returns pending
        coEvery { repository.getPendingSubmissions(any(), any()) } answers {
            val policyId = firstArg<String>()
            val groupId = secondArg<String?>()?.let(::OperationGroupId)

            Result.success(pending.value.filter { it.policy.id.value == policyId && it.groupId == groupId })
        }
        coEvery { repository.abandonSubmission(any()) } answers {
            val id = DurableTxId(firstArg())
            abandoned += id
            pending.value = pending.value.filterNot { it.id == id }
            Result.success(true)
        }
        coEvery { launcher.startAttempt(any(), any()) } answers {
            val id = DurableTxId(firstArg())
            if (attemptOutcome.isSuccess) started += id
            pending.value = pending.value.filterNot { it.id == id }
            attemptOutcome
        }
    }

    private fun withPolicyReady(
        policy: AsyncDurableSubmissionPolicy,
        recordingInto: MutableList<List<DurableTxId>>? = null,
    ) {
        coEvery { policy.prepareSubmission(any()) } answers {
            val transactions = firstArg<List<ScheduledDurableTx>>()
            recordingInto?.add(transactions.map { it.id })
            transactions.readyAll()
        }
    }

    private fun List<ScheduledDurableTx>.readyAll(): Result<Map<DurableTxId, SubmissionPreparation>> =
        Result.success(associate { it.id to SubmissionPreparation.Ready(EXTRINSIC) })

    private fun policy(): AsyncDurableSubmissionPolicy = mockk<AsyncDurableSubmissionPolicy>().also {
        every { it.chainId } returns "test-chain"
    }

    private fun scheduled(id: Long, policyId: String, groupId: OperationGroupId) = ScheduledDurableTx(
        id = DurableTxId(id),
        domainId = TxDomainId("test"),
        groupId = groupId,
        policy = SubmissionPolicy(SubmissionPolicyId(policyId), byteArrayOf(id.toByte()).toDataByteArray()),
    )

    private companion object {
        const val POLICY_A = "policy-a"
        const val POLICY_B = "policy-b"

        val GROUP_1 = OperationGroupId("group-1")
        val GROUP_2 = OperationGroupId("group-2")

        val EXTRINSIC: EnrichedSendableExtrinsic = mockk()
    }
}
