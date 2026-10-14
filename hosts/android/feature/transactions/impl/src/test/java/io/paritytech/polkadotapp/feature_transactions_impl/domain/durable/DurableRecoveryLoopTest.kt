package io.paritytech.polkadotapp.feature_transactions_impl.domain.durable

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainViewFactory
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxCompletionOracle
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.DurableTxRepository
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The loop's whole job is deciding when to stop: too early strands a lock nothing else will release, too
 * late holds a foreground service up for nothing.
 */
class DurableRecoveryLoopTest {
    private val repository: DurableTxRepository = mockk()
    private val recoveryPass: DurableRecoveryPass = mockk()
    private val chainViewFactory: PinnedChainViewFactory = mockk()
    private val oracle: TxCompletionOracle = mockk<TxCompletionOracle>().also {
        every { it.chainId } returns "test-chain"
    }

    private val loop = DurableRecoveryLoop(repository, recoveryPass, chainViewFactory, mapOf("test" to oracle))

    private var passesRun = 0

    @Test
    fun `a settled ledger costs one pass`() = runBlocking<Unit> {
        givenPassesSucceed()
        givenLiveWhilePassesBelow(1)
        givenHeads(1, 2, 3)

        loop.runUntilSettled()

        coVerify(exactly = 1) { recoveryPass.run() }
    }

    @Test
    fun `one pass per head until nothing is live`() = runBlocking<Unit> {
        givenPassesSucceed()
        // The launch pass plus two heads: the second head's pass is the one that settles the ledger.
        givenLiveWhilePassesBelow(3)
        givenHeads(1, 2, 3, 4)

        loop.runUntilSettled()

        coVerify(exactly = 3) { recoveryPass.run() }
    }

    @Test
    fun `a best head drives a pass just as a finalized one does`() = runBlocking<Unit> {
        givenPassesSucceed()
        givenLiveWhilePassesBelow(2)
        every { chainViewFactory.finalizedHeads(any()) } returns emptyFlow()
        every { chainViewFactory.bestHeads(any()) } returns blocks(1, 2, 3)

        loop.runUntilSettled()

        coVerify(exactly = 2) { recoveryPass.run() }
    }

    @Test
    fun `a lost head subscription fails the loop so its host can retry`() = runBlocking<Unit> {
        givenPassesSucceed()
        givenLiveWhilePassesBelow(Int.MAX_VALUE)
        every { chainViewFactory.finalizedHeads(any()) } returns flow { throw IllegalStateException("socket closed") }
        every { chainViewFactory.bestHeads(any()) } returns emptyFlow()

        val result = loop.runUntilSettled()

        assertTrue(result.isFailure)
    }

    @Test
    fun `an unreadable ledger keeps the loop running rather than abandoning entries`() = runBlocking<Unit> {
        givenPassesSucceed()
        coEvery { repository.hasLiveTransactions() } returns Result.failure(IllegalStateException("no database"))
        givenHeads(1, 2)

        // It never settles by design, so the loop has to be cut off rather than awaited: the point is that
        // it ran every trigger and did not give up on the first unreadable answer.
        val settled = withTimeoutOrNull(TIMEOUT_MILLIS) { loop.runUntilSettled() }

        assertNull(settled)
        coVerify(exactly = 3) { recoveryPass.run() }
    }

    private fun givenPassesSucceed() {
        coEvery { recoveryPass.run() } answers { passesRun++; Result.success(Unit) }
    }

    private fun givenLiveWhilePassesBelow(threshold: Int) {
        coEvery { repository.hasLiveTransactions() } answers { Result.success(passesRun < threshold) }
    }

    private fun givenHeads(vararg numbers: Int) {
        every { chainViewFactory.finalizedHeads(any()) } returns blocks(*numbers)
        every { chainViewFactory.bestHeads(any()) } returns emptyFlow()
    }

    private fun blocks(vararg numbers: Int) =
        flowOf(*numbers.map { BlockNumber(it.toBigInteger()) }.toTypedArray())

    private companion object {
        const val TIMEOUT_MILLIS = 500L
    }
}
