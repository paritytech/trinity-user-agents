package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.StrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageHandoffCommit
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.domain.model.CoinageTransaction
import io.paritytech.polkadotapp.feature_coinage_impl.domain.model.CoinageTransactionAssets
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission.TransferSubmissionParams
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

@OptIn(ExperimentalTime::class)
class SplitCoinStrategyTest {
    private val transactionService = mockk<CoinageTransactionService>()
    private val handoffCommit = mockk<CoinageHandoffCommit>()
    private val transaction = mockk<CoinageTransaction>(relaxed = true)
    private val handedOff = listOf(OwnAsset.Coin(testKey(2)))

    private val strategy = SplitCoinStrategy(
        transactionService = transactionService,
        payload = StrategyType.Split(
            splitFrom = coin(index = 1, exponent = 3),
            recipientDenominations = listOf(ValueExponent(2)),
            changeDenominations = listOf(ValueExponent(2)),
            exactCoins = emptyList(),
        ),
        coinageTransactionFactory = mockk { every { newTransaction() } returns transaction },
    )

    @Test
    fun `a mint cancelled while it waits is a cancellation, not a failed result`() = runBlocking<Unit> {
        withMintHeld()

        var result: Result<ScheduledTransfer>? = null
        val scheduling = launch { result = schedule(CoinReservationMode.PRECOMMIT) }
        yield()

        scheduling.cancelAndJoin()

        assertTrue("the scheduling was not cancelled", scheduling.isCancelled)
        assertNull("a cancelled scheduling produced a result", result)
        coVerify(exactly = 0) { transactionService.preCommitHandoff(any()) }
    }

    @Test
    fun `prepares the handoff of its outputs when scheduled in PREPARE mode`() = runBlocking<Unit> {
        withSplitBuilt()
        withHandoffAccepted()

        schedule(CoinReservationMode.PREPARE)

        verifyHandoffPrepared()
    }

    @Test
    fun `pre-commits the handoff of its outputs when scheduled in PRECOMMIT mode`() = runBlocking<Unit> {
        withSplitBuilt()
        withHandoffAccepted()

        schedule(CoinReservationMode.PRECOMMIT)

        verifyHandoffPreCommitted()
    }

    private suspend fun schedule(mode: CoinReservationMode) =
        with(StalenessReportCollector.NoOp) { strategy.schedule(PARAMS, mode) }

    private fun withMintHeld() {
        coEvery { transaction.mintCoins(any(), any()) } coAnswers { awaitCancellation() }
    }

    private fun withSplitBuilt() {
        coEvery { transaction.mintCoins(any(), any()) } returns Result.success(emptyList())
        every { transaction.build() } returns CoinageTransactionAssets(emptyList(), emptyList(), handedOff)
    }

    private fun withHandoffAccepted() {
        coEvery { transactionService.prepareHandoff(any()) } returns Result.success(handoffCommit)
        coEvery { transactionService.preCommitHandoff(any()) } returns Result.success(handoffCommit)
    }

    private fun verifyHandoffPrepared() {
        coVerify { transactionService.prepareHandoff(handedOff) }
        coVerify(exactly = 0) { transactionService.preCommitHandoff(any()) }
    }

    private fun verifyHandoffPreCommitted() {
        coVerify { transactionService.preCommitHandoff(handedOff) }
        coVerify(exactly = 0) { transactionService.prepareHandoff(any()) }
    }

    private fun coin(index: Int, exponent: Int) = Coin(
        derivationIndex = testKey(index),
        valueExponent = ValueExponent(exponent),
        age = Coin.Age.Known(0),
        isOnChain = true,
        accountId = byteArrayOf(index.toByte()).intoAccountId(),
        provenance = CoinProvenance.UNKNOWN,
    )

    private companion object {
        val PARAMS = TransferSubmissionParams(buildUntil = Instant.DISTANT_FUTURE, retryFailures = false)
    }
}
