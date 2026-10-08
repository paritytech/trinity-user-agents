package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.HostFundingSession
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingStage
import java.math.BigInteger
import uniffi.truapi.FundingDirection as CoreFundingDirection

class TrUAPIHostFundingTest {
    private val runtime = mockk<TrUAPIHostRuntime>()
    private val funding = TrUAPIHostFunding(
        runtimeProvider = mockk { coEvery { runtime() } returns Result.success(runtime) },
        bridge = AppFundingHostBridge(mockk()),
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

    @Test
    fun `sessions read in flight only while open`() = runBlocking {
        every { runtime.fundingSessions() } returns listOf(
            session("open", FundingStage.Open, providerId = "ramp.dot"),
            session("done", FundingStage.Delivered(credited = "5", settledAtMs = 2uL), providerId = null),
        )

        assertEquals(
            listOf(
                HostFundingSession("open", FundingDirection.IN, Balance(BigInteger("5")), ProductId.fromStoredValue("ramp.dot"), inFlight = true),
                HostFundingSession("done", FundingDirection.IN, Balance(BigInteger("5")), providerId = null, inFlight = false),
            ),
            funding.observeSessions().first(),
        )
    }

    private fun session(intent: String, stage: FundingStage, providerId: String?) = FundingSession(
        intent = intent,
        ownerProductId = null,
        direction = CoreFundingDirection.IN,
        amount = "5",
        stage = stage,
        openedAtMs = 1uL,
        deadlineMs = 3uL,
        acknowledged = false,
        providerId = providerId,
        cancelRequested = false,
        updates = emptyList(),
        choice = null,
        saved = null,
    )
}
