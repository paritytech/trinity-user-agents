package io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.domain

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.HostFunding
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class SendPaymentInteractorTest {
    private val hostFunding = mockk<HostFunding>()
    private val interactor = RealSendPaymentInteractor(chainAssetProvider = mockk(), hostFunding = hostFunding)

    @Test
    fun `outside the pocket opens outbound funding with no amount`() = runBlocking {
        coEvery { hostFunding.openFunding(FundingDirection.OUT, null) } returns Result.success("intent")

        assertEquals(Result.success(Unit), interactor.openWithdrawal())
    }

    @Test
    fun `a failed open reaches the screen`() = runBlocking {
        val failure = IllegalStateException("runtime unavailable")
        coEvery { hostFunding.openFunding(FundingDirection.OUT, null) } returns Result.failure(failure)

        assertEquals(Result.failure<Unit>(failure), interactor.openWithdrawal())
    }
}
