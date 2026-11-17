package io.paritytech.polkadotapp.feature_wallet_impl.domain.transactionSuccess

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageExportId
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentState
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentStatus.AwaitingClaim
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentStatus.Claimed
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentStatusUseCase
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

class TransactionSuccessInteractorTest {
    private val exportId = CoinageExportId.of("payment")

    private val statusUseCase = mockk<CoinagePaymentStatusUseCase>()
    private val interactor = TransactionSuccessInteractor(statusUseCase)

    @Test
    fun `completes once every claim is finalized`() = runTest {
        givenReadings(
            listOf(Claimed(finalized = false), Claimed(finalized = false)),
            listOf(Claimed(finalized = true), Claimed(finalized = false)),
            listOf(Claimed(finalized = true), Claimed(finalized = true)),
        )

        assertEquals(listOf(false, true), interactor.subscribeClaimsFinalized(exportId).toList())
    }

    @Test
    fun `a claim forked away is not finalized`() = runTest {
        givenReadings(
            listOf(Claimed(finalized = false)),
            listOf(AwaitingClaim),
            listOf(Claimed(finalized = true)),
        )

        assertEquals(listOf(false, true), interactor.subscribeClaimsFinalized(exportId).toList())
    }

    @Test
    fun `an empty reading is not finalized`() = runTest {
        givenReadings(
            emptyList(),
            listOf(Claimed(finalized = true)),
        )

        assertEquals(listOf(false, true), interactor.subscribeClaimsFinalized(exportId).toList())
    }

    private fun givenReadings(vararg readings: List<CoinagePaymentStatus>) {
        every { statusUseCase.subscribeStatuses(exportId) } returns flow {
            readings.forEach { statuses ->
                emit(statuses.map { status -> CoinagePaymentState(mockk(), Balance.ONE, status) })
            }
            // A storage subscription stays open, so only the interactor can end the watch.
            awaitCancellation()
        }
    }
}
