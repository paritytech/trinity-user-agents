package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies

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
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

@OptIn(ExperimentalTime::class)
class ExactMatchStrategyTest {
    private val transactionService: CoinageTransactionService = mock()
    private val handoffCommit: CoinageHandoffCommit = mock()
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

    private suspend fun withHandoffAccepted() {
        whenever(transactionService.prepareHandoff(any())).thenReturn(Result.success(handoffCommit))
        whenever(transactionService.preCommitHandoff(any())).thenReturn(Result.success(handoffCommit))
    }

    private suspend fun verifyHandoffPrepared() {
        verify(transactionService).prepareHandoff(eq(handedOff()))
        verify(transactionService, never()).preCommitHandoff(any())
    }

    private suspend fun verifyHandoffPreCommitted() {
        verify(transactionService).preCommitHandoff(eq(handedOff()))
        verify(transactionService, never()).prepareHandoff(any())
    }

    private fun handedOff() = listOf(OwnAsset.Coin(coin.derivationIndex))

    private fun coinOf(derivationIndex: Int) = Coin(
        derivationIndex = testKey(derivationIndex),
        valueExponent = ValueExponent(1),
        age = Coin.Age.Known(3),
        isOnChain = true,
        accountId = mock(),
        provenance = CoinProvenance.UNKNOWN,
    )

    private companion object {
        val PARAMS = TransferSubmissionParams(buildUntil = Instant.fromEpochSeconds(1_000), retryFailures = true)
    }
}
