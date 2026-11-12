package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import dagger.Lazy
import io.mockk.coEvery
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.CountryDetector
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.RealFundingService
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.FundingChangeKind
import uniffi.truapi.FundingFrameOutcome
import uniffi.truapi.FundingFrameTarget
import uniffi.truapi.FundingPresentOutcome
import uniffi.truapi.ProductId
import uniffi.truapi.FundingDirection as CoreFundingDirection

class AppFundingHostBridgeTest {
    private val ramp = ProductId.parse("ramp.dot").getOrThrow()
    private val countryDetector = mockk<CountryDetector>()
    private val funding = mockk<RealFundingService>(relaxed = true)
    private val bridge = AppFundingHostBridge(Lazy { funding }, countryDetector)

    // The core reads anything but STARTED as the user walking away and discards the session.
    @Test
    fun `the service's answer reaches the core unchanged`() = runBlocking {
        coEvery { funding.present("intent", CoreFundingDirection.OUT, "1000") } returns FundingPresentOutcome.STARTED

        assertEquals(
            FundingPresentOutcome.STARTED,
            bridge.presentFunding(ProductId.parse("market.dot").getOrThrow(), "intent", CoreFundingDirection.OUT, "1000"),
        )
    }

    // The worker reads the onramp's result from the query of the page it returned to.
    @Test
    fun `a provider frame shows the core's target and answers how it closed`() = runBlocking {
        val target = FundingFrameTarget.Page("https://ramp.example/widget", "https://ramp.dot/return")
        val returned = FundingFrameOutcome.Returned("https://ramp.dot/return?status=done")
        coEvery { funding.presentFrame(ramp, "intent", target) } returns returned

        assertEquals(returned, bridge.presentProviderFrame(ramp, "intent", target))
    }

    @Test
    fun `the core is told the country the device detects`() = runBlocking {
        coEvery { countryDetector.detect() } returns "PT"

        assertEquals("PT", bridge.detectedCountry())
    }

    // The core has the new state before it calls; the read happens off its thread.
    @Test
    fun `a change notice goes to the service that reads it back`() {
        bridge.fundingChanged("intent", FundingChangeKind.SESSION)

        verify { funding.changed("intent", FundingChangeKind.SESSION) }
    }

    @Test
    fun `a provider closing its frame reaches the service`() {
        bridge.closeProviderFrame(ramp, "intent")

        verify { funding.closeFrames(ramp, "intent") }
    }
}
