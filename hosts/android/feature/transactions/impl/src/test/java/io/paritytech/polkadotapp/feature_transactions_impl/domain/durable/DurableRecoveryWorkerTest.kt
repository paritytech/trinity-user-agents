package io.paritytech.polkadotapp.feature_transactions_impl.domain.durable

import android.content.Context
import androidx.work.ListenableWorker
import androidx.work.WorkerParameters
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainViewFactory
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxCompletionOracle
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.DurableTxRepository
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import io.paritytech.polkadotapp.common.R as RCommon

/** Every launch enqueues the worker, so its ongoing notification must only show when a transaction is live. */
class DurableRecoveryWorkerTest {
    private val context: Context = mockk(relaxed = true)
    private val repository: DurableTxRepository = mockk()
    private val recoveryPass: DurableRecoveryPass = mockk()
    private val chainViewFactory: PinnedChainViewFactory = mockk<PinnedChainViewFactory>().also {
        every { it.finalizedHeads(any()) } returns emptyFlow()
        every { it.bestHeads(any()) } returns emptyFlow()
    }
    private val oracle: TxCompletionOracle = mockk<TxCompletionOracle>().also {
        every { it.chainId } returns "test-chain"
    }
    private val oracles = mapOf("test" to oracle)

    private var passesRun = 0

    private val worker = DurableRecoveryWorker(
        appContext = context,
        params = mockk<WorkerParameters>(relaxed = true),
        recoveryLoop = DurableRecoveryLoop(repository, recoveryPass, chainViewFactory, oracles),
        submissionExecutor = mockk(relaxed = true),
        chainConnectionRefCounter = mockk<ChainConnectionRefCounter>(relaxed = true),
        oracles = oracles,
    )

    @Test
    fun `an empty ledger finishes without a notification`() = runBlocking<Unit> {
        coEvery { repository.hasLiveTransactions() } returns Result.success(false)
        givenPassesSucceed()

        val result = worker.doWork()

        assertEquals(ListenableWorker.Result.success(), result)
        verify(exactly = 0) { context.getString(RCommon.string.coinage_recovery_worker_notification_title) }
    }

    @Test
    fun `a live transaction runs in the foreground`() = runBlocking<Unit> {
        coEvery { repository.hasLiveTransactions() } answers { Result.success(passesRun == 0) }
        givenPassesSucceed()

        val result = worker.doWork()

        assertEquals(ListenableWorker.Result.success(), result)
        verify(exactly = 1) { context.getString(RCommon.string.coinage_recovery_worker_notification_title) }
    }

    private fun givenPassesSucceed() {
        coEvery { recoveryPass.run() } answers { passesRun++; Result.success(Unit) }
    }
}
