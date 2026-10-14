package io.paritytech.polkadotapp.feature_wallet_impl.domain.transactionSuccess

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
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
    private val first: AccountId = byteArrayOf(1).intoAccountId()
    private val second: AccountId = byteArrayOf(2).intoAccountId()

    private val statusUseCase = mockk<CoinagePaymentStatusUseCase>()
    private val interactor = TransactionSuccessInteractor(statusUseCase)

    @Test
    fun `completes once every claim is finalized`() = runTest {
        givenReadings(
            mapOf(first to Claimed(finalized = false), second to Claimed(finalized = false)),
            mapOf(first to Claimed(finalized = true), second to Claimed(finalized = false)),
            mapOf(first to Claimed(finalized = true), second to Claimed(finalized = true)),
        )

        assertEquals(listOf(false, true), interactor.subscribeClaimsFinalized(listOf(first, second)).toList())
    }

    @Test
    fun `a claim forked away is not finalized`() = runTest {
        givenReadings(
            mapOf(first to Claimed(finalized = false)),
            mapOf(first to AwaitingClaim),
            mapOf(first to Claimed(finalized = true)),
        )

        assertEquals(listOf(false, true), interactor.subscribeClaimsFinalized(listOf(first)).toList())
    }

    @Test
    fun `an empty reading is not finalized`() = runTest {
        givenReadings(
            emptyMap(),
            mapOf(first to Claimed(finalized = true)),
        )

        assertEquals(listOf(false, true), interactor.subscribeClaimsFinalized(listOf(first)).toList())
    }

    private fun givenReadings(vararg readings: Map<AccountId, CoinagePaymentStatus>) {
        every { statusUseCase.subscribeStatuses(any()) } returns flow {
            readings.forEach { statuses ->
                emit(statuses.mapValues { (_, status) -> CoinagePaymentState(mockk(), status) })
            }
            // A storage subscription stays open, so only the interactor can end the watch.
            awaitCancellation()
        }
    }
}
