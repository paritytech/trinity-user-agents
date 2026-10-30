package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import dagger.Lazy
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.just
import io.mockk.mockk
import io.mockk.runs
import io.mockk.verify
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlay
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayRequest
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameTarget
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.DeviceCountryDetector
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.onSubscription
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.FundingAmountLimits
import uniffi.truapi.FundingBand
import uniffi.truapi.FundingFrameOutcome
import uniffi.truapi.FundingFrameTarget
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingPresentOutcome
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingStage
import uniffi.truapi.FundingStatus
import uniffi.truapi.HostFundingStatusSubscribeItem
import java.math.BigInteger
import uniffi.truapi.FundingDirection as CoreFundingDirection

class AppFundingHostBridgeTest {
    private val overlay = mockk<FundingOverlay>()
    private val countryDetector = mockk<DeviceCountryDetector>()
    private val bridge = AppFundingHostBridge(Lazy { overlay }, countryDetector)

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
        coEvery {
            overlay.presentProviderFrame(ProductId.fromStoredValue("ramp.dot"), "intent", ProviderFrameTarget.App("/kyc"))
        } returns ProviderFrameOutcome.Closed

        assertEquals(FundingFrameOutcome.Closed, bridge.presentProviderFrame("ramp.dot", "intent", FundingFrameTarget.App("/kyc")))
    }

    // The worker reads the onramp's result from the query of the page it returned to.
    @Test
    fun `an external page hands back the URL it returned to`() = runBlocking {
        val page = ProviderFrameTarget.Page("https://ramp.example/widget", "https://ramp.dot/return")
        val returned = "https://ramp.dot/return?status=done"
        coEvery { overlay.presentProviderFrame(ProductId.fromStoredValue("ramp.dot"), "intent", page) } returns
            ProviderFrameOutcome.Returned(returned)

        assertEquals(
            FundingFrameOutcome.Returned(returned),
            bridge.presentProviderFrame("ramp.dot", "intent", FundingFrameTarget.Page(page.url, page.returnUrl)),
        )
    }

    @Test
    fun `the core is told the country the device detects`() = runBlocking {
        coEvery { countryDetector.detect() } returns "PT"

        assertEquals("PT", bridge.detectedCountry())
    }

    // The CASH card re-reads the session list on this, so a dropped change leaves a stale row.
    @Test
    fun `a session change is announced with its intent`() = runBlocking {
        val changed = withTimeout(1_000) {
            bridge.changedSessions()
                .onSubscription {
                    bridge.fundingSessionChanged("intent", HostFundingStatusSubscribeItem.InProgress(expiresAt = null), null, null)
                }
                .first()
        }

        assertEquals("intent", changed)
    }

    // The funding screens draw what the core last handed over; a session the core keeps only as a summary has none.
    @Test
    fun `a session change keeps the session and progress it carries`() {
        val inProgress = HostFundingStatusSubscribeItem.InProgress(expiresAt = null)
        val session = session()

        bridge.fundingSessionChanged("intent", inProgress, session, progress(FundingStatus.ConvertingToCash))
        val kept = bridge.sessions().value["intent"]
        bridge.fundingSessionChanged("intent", inProgress, null, null)

        assertEquals(session to FundingStatus.ConvertingToCash, kept?.session to kept?.progress?.status)
        assertEquals(emptyMap<String, FundingSessionEntry>(), bridge.sessions().value)
    }

    @Test
    fun `the latest options are kept per session`() {
        bridge.fundingOptionsChanged("a", options(min = "10"))
        bridge.fundingOptionsChanged("b", options(min = "20"))
        bridge.fundingOptionsChanged("a", options(min = "30"))

        assertEquals(mapOf("a" to options(min = "30"), "b" to options(min = "20")), bridge.options().value)
    }

    @Test
    fun `an overlay's options go once it closes`() = runBlocking {
        bridge.fundingOptionsChanged("intent", options(min = "10"))
        bridge.fundingOptionsChanged("other", options(min = "20"))
        coEvery { overlay.present(any()) } returns FundingOverlayOutcome.DISMISSED

        present()

        assertEquals(mapOf("other" to options(min = "20")), bridge.options().value)
    }

    @Test
    fun `the in-progress flag is kept as pushed`() {
        assertEquals(null, bridge.inProgress().value)

        bridge.fundingInProgressChanged(true)

        assertEquals(true, bridge.inProgress().value)
    }

    @Test
    fun `a provider closing its frame reaches the overlay`() {
        every { overlay.closeProviderFrame(any(), any()) } just runs

        bridge.closeProviderFrame("ramp.dot", "intent")

        verify { overlay.closeProviderFrame(ProductId.fromStoredValue("ramp.dot"), "intent") }
    }

    private fun options(min: String) = FundingOptions(
        candidates = emptyList(),
        countries = emptyList(),
        limits = FundingAmountLimits(min = min, max = null),
        networks = emptyList(),
    )

    private fun session() = FundingSession(
        intent = "intent",
        ownerProductId = null,
        direction = CoreFundingDirection.IN,
        amount = "50",
        stage = FundingStage.Open,
        openedAtMs = 0uL,
        deadlineMs = 0uL,
        acknowledged = false,
        providerId = "ramp.dot",
        assignedAtMs = 0uL,
        cancel = null,
        updates = emptyList(),
        started = emptyList(),
        choice = null,
        refund = null,
    )

    private fun progress(status: FundingStatus) = FundingProgress(
        steps = emptyList(),
        band = FundingBand.CONVERTING,
        lastHeardAtMs = null,
        failedAtMs = null,
        transactionId = null,
        reference = null,
        deposit = null,
        mismatch = null,
        retrying = false,
        payout = null,
        refunded = null,
        status = status,
        longerAtMs = null,
        moneyMoved = true,
    )

    private suspend fun present() = bridge.presentFunding(null, "intent", CoreFundingDirection.IN, null)
}
