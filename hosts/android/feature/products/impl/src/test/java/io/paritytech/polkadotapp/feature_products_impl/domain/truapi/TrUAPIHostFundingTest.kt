package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.mockk
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import java.math.BigInteger
import uniffi.truapi.FundingDirection as CoreFundingDirection

class TrUAPIHostFundingTest {
    private val runtime = mockk<TrUAPIHostRuntime>()
    private val funding = TrUAPIHostFunding(
        runtimeProvider = mockk { coEvery { runtime() } returns Result.success(runtime) },
        activityCenter = mockk(),
        sessionPresenter = mockk(),
    )

    @Test
    fun `opening passes the direction and amount to the core`() = runBlocking {
        coEvery { runtime.openFunding(CoreFundingDirection.OUT, "250") } returns "intent"

        assertEquals(Result.success("intent"), funding.openFunding(FundingDirection.OUT, Balance(BigInteger("250"))))
    }

    @Test
    fun `a dismissed overlay opens no session`() = runBlocking {
        coEvery { runtime.openFunding(CoreFundingDirection.IN, null) } returns null

        assertEquals(Result.success(null), funding.openFunding(FundingDirection.IN, null))
    }
}
