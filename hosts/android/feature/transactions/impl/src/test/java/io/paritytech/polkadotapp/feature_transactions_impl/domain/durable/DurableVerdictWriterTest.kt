package io.paritytech.polkadotapp.feature_transactions_impl.domain.durable

import dagger.Lazy
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.AsyncDurableSubmissionPolicy
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.CheckpointBlock
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableFailureKind
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxEntry
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicy
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicyId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxDomainId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.Verdict
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.DurableTxRepository
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The one gate every verdict passes through. A failure written here is final and releases what the domain
 * locked, so the question worth pinning is exactly when a failure is handed back to its policy instead.
 */
class DurableVerdictWriterTest {
    private val repository: DurableTxRepository = mockk()
    private val policy: AsyncDurableSubmissionPolicy = mockk()

    private var policies = mapOf(POLICY_ID to policy)

    private val writer = DurableVerdictWriter(repository, Lazy { policies })

    @Test
    fun `a failure of a transaction whose policy wants it again is handed back to that policy`() = runBlocking<Unit> {
        withPolicy(REFERENCE)
        withPolicyRetrying(true)
        withWritesSucceeding()

        writer.write(ENTRY, FAILURE)

        verifyWritten(Verdict(DurableTxStatus.PENDING_SUBMISSION, successDetectedAt = null, failure = null))
    }

    /** The policy decides with the reason in hand, since a failure that would repeat must not loop. */
    @Test
    fun `the policy is told why the attempt failed`() = runBlocking<Unit> {
        withPolicy(REFERENCE)
        withWritesSucceeding()
        coEvery { policy.canRetry(any(), any(), any()) } returns false

        writer.write(ENTRY, Verdict(DurableTxStatus.FAILURE, successDetectedAt = null, DurableFailureKind.DISPATCH_FAILED))

        coVerify { policy.canRetry(ENTRY, REFERENCE.params, DurableFailureKind.DISPATCH_FAILED) }
    }

    @Test
    fun `a failure of a transaction registered without a policy stays final`() = runBlocking<Unit> {
        withPolicy(null)
        withWritesSucceeding()

        writer.write(ENTRY, FAILURE)

        verifyWritten(FAILURE)
    }

    @Test
    fun `a failure its policy declines to retry stays final`() = runBlocking<Unit> {
        withPolicy(REFERENCE)
        withPolicyRetrying(false)
        withWritesSucceeding()

        writer.write(ENTRY, FAILURE)

        verifyWritten(FAILURE)
    }

    /** A policy that is no longer bound can build nothing, so nothing is gained by holding the lock. */
    @Test
    fun `a failure whose policy is not registered stays final`() = runBlocking<Unit> {
        policies = emptyMap()
        withPolicy(REFERENCE)
        withWritesSucceeding()

        writer.write(ENTRY, FAILURE)

        verifyWritten(FAILURE)
    }

    /** A failure written on an unreadable policy could never be taken back; not writing costs one pass. */
    @Test
    fun `a failure is not written when its policy cannot be read`() = runBlocking<Unit> {
        coEvery { repository.getSubmissionPolicy(ENTRY.id) } returns Result.failure(IllegalStateException("db"))

        val result = writer.write(ENTRY, FAILURE)

        assertTrue(result.isFailure)
        coVerify(exactly = 0) { repository.compareAndSetStatus(any(), any(), any(), any()) }
    }

    @Test
    fun `a failure is not written when its policy throws deciding`() = runBlocking<Unit> {
        withPolicy(REFERENCE)
        coEvery { policy.canRetry(any(), any(), any()) } throws IllegalStateException("broken params")

        val result = writer.write(ENTRY, FAILURE)

        assertTrue(result.isFailure)
        coVerify(exactly = 0) { repository.compareAndSetStatus(any(), any(), any(), any()) }
    }

    @Test
    fun `a verdict other than failure is written as it is, without asking the policy`() = runBlocking<Unit> {
        withWritesSucceeding()
        val success = Verdict(DurableTxStatus.PENDING_SUCCESS, CheckpointBlock(12, "0x12"), failure = null)

        writer.write(ENTRY, success)

        verifyWritten(success)
        coVerify(exactly = 0) { policy.canRetry(any(), any(), any()) }
    }

    /** The compare-and-set carries the attempt, so a verdict about old bytes cannot land on a rebuilt row. */
    @Test
    fun `a verdict is written only against the attempt it was derived from`() = runBlocking<Unit> {
        withPolicy(null)
        withWritesSucceeding()

        writer.write(ENTRY, FAILURE)

        coVerify { repository.compareAndSetStatus(ENTRY.id, ENTRY.status, ENTRY.txHash, any()) }
    }

    private fun withPolicy(reference: SubmissionPolicy?) {
        coEvery { repository.getSubmissionPolicy(ENTRY.id) } returns Result.success(reference)
    }

    private fun withPolicyRetrying(retry: Boolean) {
        coEvery { policy.canRetry(ENTRY, REFERENCE.params, DurableFailureKind.EXPIRED) } returns retry
    }

    private fun withWritesSucceeding() {
        coEvery { repository.compareAndSetStatus(any(), any(), any(), any()) } returns Result.success(true)
    }

    private fun verifyWritten(verdict: Verdict) {
        coVerify(exactly = 1) { repository.compareAndSetStatus(ENTRY.id, ENTRY.status, ENTRY.txHash, verdict) }
    }

    private companion object {
        const val POLICY_ID = "test-policy"

        val REFERENCE = SubmissionPolicy(SubmissionPolicyId(POLICY_ID), byteArrayOf(1, 2).toDataByteArray())

        val FAILURE = Verdict(DurableTxStatus.FAILURE, successDetectedAt = null, DurableFailureKind.EXPIRED)

        val ENTRY = DurableTxEntry(
            id = DurableTxId(7),
            domainId = TxDomainId("test"),
            groupId = null,
            txHash = "0xattempt",
            checkpoint = CheckpointBlock(10, "0x10"),
            mortalityBlocks = 64,
            status = DurableTxStatus.PENDING,
            successDetectedAt = null,
        )
    }
}
