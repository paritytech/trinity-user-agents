package io.paritytech.polkadotapp.feature_transactions_impl.data

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.novasama.substrate_sdk_android.runtime.extrinsic.signer.SendableExtrinsic
import io.paritytech.polkadotapp.chains.extrinsic.ExtrinsicStatus
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.rpc.RpcCalls
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.Abort
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicRecoveryContext
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicSubmissionFailure
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicSubmissionFailureRecovery
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicSubmissionFailureRecoveryStrategy
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.PreSubmissionValidationFailed
import io.paritytech.polkadotapp.feature_transactions_impl.data.validation.PreSubmissionValidator
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * What a pre-submission rejection is allowed to mean.
 *
 * Callers treat `FailedToSubmit(PreSubmissionValidationFailed)` as proof that the extrinsic never reached a
 * node — the coinage ledger fails an entry outright on it, releasing its inputs. That is only sound while this
 * service never submits before validating, and never reports a pre-submission rejection for bytes it has
 * already sent. Both are pinned here rather than left to be re-derived by reading the loop.
 */
class RealExtrinsicServiceSubmissionTest {
    private val rpcCalls: RpcCalls = mockk()
    private val preSubmissionValidator: PreSubmissionValidator = mockk()

    private val service = RealExtrinsicService(
        rpcCalls = rpcCalls,
        chainRegistry = mockk(relaxed = true),
        extrinsicBuilderFactory = mockk(relaxed = true),
        preSubmissionValidator = preSubmissionValidator,
        extrinsicValidator = mockk(relaxed = true),
        chainStateRepository = mockk(relaxed = true),
        signerProvider = mockk(relaxed = true),
        chainEventsRepositoryFactory = mockk(relaxed = true),
        coroutineDispatchers = mockk(relaxed = true),
        resubmitWhenValidFactory = mockk(relaxed = true),
        defaultExtensionProviders = emptySet(),
    )

    private val chain: Chain = mockk<Chain>().also { every { it.id } returns CHAIN_ID }
    private val extrinsic: SendableExtrinsic = mockk<SendableExtrinsic>()
        .also { every { it.extrinsicHex } returns "0x00" }

    /**
     * Validation rejects the extrinsic and recovery declines to retry.
     * Nothing is ever handed to the node, and the caller is told exactly why.
     */
    @Test
    fun `a rejected extrinsic is never submitted`() = runTest {
        coEvery { preSubmissionValidator.mightBeValid(CHAIN_ID, extrinsic) } returns false

        val statuses = service.submitAndWatchBuiltExtrinsic(chain, extrinsic, Abort).toList()

        coVerify(exactly = 0) { rpcCalls.submitAndWatchExtrinsic(any(), any()) }
        assertEquals(1, statuses.size)
        assertTrue((statuses.single() as ExtrinsicStatus.FailedToSubmit).exception is PreSubmissionValidationFailed)
    }

    /**
     * The same rejection, but recovery insists on resubmitting.
     * The extrinsic does reach the node this time — and the caller must not be told it was never sent.
     *
     * This is the property the coinage ledger leans on: a pre-submission rejection is reported only when the
     * bytes stayed here, so reporting one can never strand a transaction that is actually in flight.
     */
    @Test
    fun `a rejection is not reported once recovery has resubmitted`() = runTest {
        coEvery { preSubmissionValidator.mightBeValid(CHAIN_ID, extrinsic) } returns false
        coEvery { rpcCalls.submitAndWatchExtrinsic(CHAIN_ID, "0x00") } returns
            flowOf(ExtrinsicStatus.Dropped("0xhash"))

        val statuses = service.submitAndWatchBuiltExtrinsic(chain, extrinsic, AlwaysResubmitOnce()).toList()

        coVerify(exactly = 1) { rpcCalls.submitAndWatchExtrinsic(CHAIN_ID, "0x00") }
        assertTrue(statuses.none { it is ExtrinsicStatus.FailedToSubmit })
    }

    /**
     * Validation passes, so the extrinsic is handed to the node, and the node is the one that rejects it.
     * The caller sees the node's own verdict and everything that preceded it.
     *
     * Nothing here may surface as a pre-submission rejection: these bytes were sent, and a caller that read
     * them as never-sent would free inputs a transaction may still be spending.
     */
    @Test
    fun `a failure from the node is reported as the node's, not as a rejection before sending`() = runTest {
        coEvery { preSubmissionValidator.mightBeValid(CHAIN_ID, extrinsic) } returns true
        coEvery { rpcCalls.submitAndWatchExtrinsic(CHAIN_ID, "0x00") } returns flowOf(
            ExtrinsicStatus.Ready(TX_HASH),
            ExtrinsicStatus.Broadcast(TX_HASH),
            ExtrinsicStatus.Invalid(TX_HASH),
        )

        val statuses = service.submitAndWatchBuiltExtrinsic(chain, extrinsic, Abort).toList()

        coVerify(exactly = 1) { rpcCalls.submitAndWatchExtrinsic(CHAIN_ID, "0x00") }
        assertTrue(statuses.none { it is ExtrinsicStatus.FailedToSubmit })
        assertEquals(ExtrinsicStatus.Invalid(TX_HASH), statuses.last())
    }

    /**
     * Validation passes and the extrinsic executes.
     * The whole progression reaches the caller, ending in the finalized block.
     */
    @Test
    fun `a validated extrinsic is submitted and its statuses are passed through`() = runTest {
        coEvery { preSubmissionValidator.mightBeValid(CHAIN_ID, extrinsic) } returns true
        coEvery { rpcCalls.submitAndWatchExtrinsic(CHAIN_ID, "0x00") } returns flowOf(
            ExtrinsicStatus.Ready(TX_HASH),
            ExtrinsicStatus.Broadcast(TX_HASH),
            ExtrinsicStatus.InBlock(BLOCK_HASH, TX_HASH),
            ExtrinsicStatus.Finalized(BLOCK_HASH, TX_HASH),
        )

        val statuses = service.submitAndWatchBuiltExtrinsic(chain, extrinsic, Abort).toList()

        assertEquals(
            listOf(
                ExtrinsicStatus.Ready(TX_HASH),
                ExtrinsicStatus.Broadcast(TX_HASH),
                ExtrinsicStatus.InBlock(BLOCK_HASH, TX_HASH),
                ExtrinsicStatus.Finalized(BLOCK_HASH, TX_HASH),
            ),
            statuses,
        )
    }

    /** Resubmits the first failure, then lets the next one stand. */
    private class AlwaysResubmitOnce : ExtrinsicSubmissionFailureRecoveryStrategy {
        private var resubmitted = false

        override suspend fun recoverSubmissionFailure(
            context: ExtrinsicRecoveryContext,
            failure: ExtrinsicSubmissionFailure,
        ): ExtrinsicSubmissionFailureRecovery {
            if (resubmitted) return ExtrinsicSubmissionFailureRecovery.Abort
            resubmitted = true

            return ExtrinsicSubmissionFailureRecovery.Resubmit
        }
    }

    private companion object {
        const val CHAIN_ID = "test-chain"
        const val TX_HASH = "0xtx"
        const val BLOCK_HASH = "0xblock"
    }
}
