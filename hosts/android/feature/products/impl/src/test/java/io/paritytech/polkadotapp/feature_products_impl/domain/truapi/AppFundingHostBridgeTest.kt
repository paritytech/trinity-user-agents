package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlay
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayRequest
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameOutcome
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.onSubscription
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.FundingFrameOutcome
import uniffi.truapi.FundingPresentOutcome
import uniffi.truapi.HostFundingStatusSubscribeItem
import java.math.BigInteger
import uniffi.truapi.FundingDirection as CoreFundingDirection

class AppFundingHostBridgeTest {
    private val overlay = mockk<FundingOverlay>()
    private val bridge = AppFundingHostBridge(overlay)

    // The core reads anything but STARTED as the user walking away and discards the session.
    @Test
    fun `the overlay's answer reaches the core unchanged`() = runBlocking {
        coEvery { overlay.present(any()) } returns FundingOverlayOutcome.STARTED
        assertEquals(FundingPresentOutcome.STARTED, present())

        coEvery { overlay.present(any()) } returns FundingOverlayOutcome.DISMISSED
        assertEquals(FundingPresentOutcome.DISMISSED, present())
    }

    @Test
    fun `the overlay is shown the session as the core opened it`() = runBlocking {
        coEvery { overlay.present(any()) } returns FundingOverlayOutcome.DISMISSED

        bridge.presentFunding("market.dot", "intent", CoreFundingDirection.OUT, "1000000000000000000000")

        coVerify {
            overlay.present(
                FundingOverlayRequest(
                    productId = ProductId.fromStoredValue("market.dot"),
                    intent = "intent",
                    direction = FundingDirection.OUT,
                    amount = Balance(BigInteger("1000000000000000000000")),
                )
            )
        }
    }

    @Test
    fun `a session the host opened carries no product or amount`() = runBlocking {
        coEvery { overlay.present(any()) } returns FundingOverlayOutcome.DISMISSED

        bridge.presentFunding(null, "intent", CoreFundingDirection.IN, null)

        coVerify {
            overlay.present(FundingOverlayRequest(productId = null, intent = "intent", direction = FundingDirection.IN, amount = null))
        }
    }

    @Test
    fun `a provider frame answers how it closed`() = runBlocking {
        coEvery { overlay.presentProviderFrame(ProductId.fromStoredValue("ramp.dot"), "intent", "/kyc") } returns
            ProviderFrameOutcome.CLOSED

        assertEquals(FundingFrameOutcome.CLOSED, bridge.presentProviderFrame("ramp.dot", "intent", "/kyc"))
    }

    // The CASH card re-reads the session list on this, so a dropped change leaves a stale pill.
    @Test
    fun `a session change is announced`() = runBlocking {
        withTimeout(1_000) {
            bridge.sessionChanges()
                .onSubscription {
                    bridge.fundingSessionChanged("intent", HostFundingStatusSubscribeItem.InProgress(expiresAt = null))
                }
                .first()
        }
    }

    private suspend fun present() = bridge.presentFunding(null, "intent", CoreFundingDirection.IN, null)
}
