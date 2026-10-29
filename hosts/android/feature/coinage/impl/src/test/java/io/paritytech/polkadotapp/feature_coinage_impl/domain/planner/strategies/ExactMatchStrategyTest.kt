package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.StrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageHandoffCommit
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission.TransferSubmissionParams
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import kotlinx.coroutines.runBlocking
import org.junit.Test
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

@OptIn(ExperimentalTime::class)
class ExactMatchStrategyTest {
    private val transactionService: CoinageTransactionService = mockk()
    private val handoffCommit: CoinageHandoffCommit = mockk()
    private val coin = coinOf(derivationIndex = 1)

    private val strategy = ExactMatchStrategy(StrategyType.ExactCoins(listOf(coin)), transactionService)

    @Test
    fun `prepares the handoff of its coins when scheduled in PREPARE mode`() = runBlocking<Unit> {
        withHandoffAccepted()

        schedule(CoinReservationMode.PREPARE)

        verifyHandoffPrepared()
    }

    @Test
    fun `pre-commits the handoff of its coins when scheduled in PRECOMMIT mode`() = runBlocking<Unit> {
        withHandoffAccepted()

        schedule(CoinReservationMode.PRECOMMIT)

        verifyHandoffPreCommitted()
    }

    private suspend fun schedule(mode: CoinReservationMode) =
        with(StalenessReportCollector.NoOp) { strategy.schedule(PARAMS, mode) }

    private fun withHandoffAccepted() {
        coEvery { transactionService.prepareHandoff(any()) } returns Result.success(handoffCommit)
        coEvery { transactionService.preCommitHandoff(any()) } returns Result.success(handoffCommit)
    }

    private fun verifyHandoffPrepared() {
        coVerify(exactly = 1) { transactionService.prepareHandoff(handedOff()) }
        coVerify(exactly = 0) { transactionService.preCommitHandoff(any()) }
    }

    private fun verifyHandoffPreCommitted() {
        coVerify(exactly = 1) { transactionService.preCommitHandoff(handedOff()) }
        coVerify(exactly = 0) { transactionService.prepareHandoff(any()) }
    }

    private fun handedOff() = listOf(OwnAsset.Coin(coin.derivationIndex))

    private fun coinOf(derivationIndex: Int) = Coin(
        derivationIndex = testKey(derivationIndex),
        valueExponent = ValueExponent(1),
        age = Coin.Age.Known(3),
        isOnChain = true,
        accountId = mockk(),
        provenance = CoinProvenance.UNKNOWN,
    )

    private companion object {
        val PARAMS = TransferSubmissionParams(buildUntil = Instant.fromEpochSeconds(1_000), retryFailures = true)
    }
}
