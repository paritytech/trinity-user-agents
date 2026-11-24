package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import dagger.Lazy
import io.mockk.coEvery
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.CountryDetector
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingIntentId
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.RealFundingProviderFrames
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.RealFundingQuotes
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.RealFundingSessions
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
    private val intent = FundingIntentId("intent")
    private val countryDetector = mockk<CountryDetector>()
    private val sessions = mockk<RealFundingSessions>(relaxed = true)
    private val quotes = mockk<RealFundingQuotes>(relaxed = true)
    private val providerFrames = mockk<RealFundingProviderFrames>(relaxed = true)
    private val bridge = AppFundingHostBridge(Lazy { sessions }, Lazy { quotes }, Lazy { providerFrames }, countryDetector)

    // The core reads anything but STARTED as the user walking away and discards the session.
    @Test
    fun `the sessions' answer reaches the core unchanged`() = runBlocking {
        coEvery { sessions.present(intent, CoreFundingDirection.OUT, 1000.intoBalance()) } returns FundingPresentOutcome.STARTED

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
        coEvery { providerFrames.present(ramp, intent, target) } returns returned

        assertEquals(returned, bridge.presentProviderFrame(ramp, "intent", target))
    }

    @Test
    fun `the core is told the country the device detects`() = runBlocking {
        coEvery { countryDetector.detect() } returns "PT"

        assertEquals("PT", bridge.detectedCountry())
    }

    // The core has the new state before it calls; the read happens off its thread.
    @Test
    fun `each change notice goes to the part that reads it back`() {
        bridge.fundingChanged("intent", FundingChangeKind.SESSION)
        bridge.fundingChanged("intent", FundingChangeKind.OPTIONS)
        bridge.fundingChanged("intent", FundingChangeKind.QUOTES)

        verify { sessions.onSessionChanged(intent) }
        verify { sessions.onOptionsChanged(intent) }
        verify { quotes.onQuotesChanged(intent) }
    }

    @Test
    fun `a provider asking for a screen reaches the provider frames`() {
        bridge.providerFrameRequested(ramp, "intent")

        verify { providerFrames.requested(ramp, intent) }
    }

    @Test
    fun `a provider closing its frame reaches the provider frames`() {
        bridge.closeProviderFrame(ramp, "intent")

        verify { providerFrames.close(ramp, intent) }
    }
}
