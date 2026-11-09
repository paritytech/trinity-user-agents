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
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableHost
import io.paritytech.polkadotapp.feature_products_api.model.Executables
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.model.SemVer
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.DeviceCountryDetector
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.FundingChangeKind
import uniffi.truapi.FundingFrameContext
import uniffi.truapi.FundingFrameOutcome
import uniffi.truapi.FundingFrameTarget
import uniffi.truapi.FundingPresentOutcome
import uniffi.truapi.ProductId
import java.math.BigInteger
import uniffi.truapi.FundingDirection as CoreFundingDirection

class AppFundingHostBridgeTest {
    private val ramp = ProductId.parse("ramp.dot").getOrThrow()
    private val widget = ProductId.parse("widget.dot").getOrThrow()
    private val overlay = mockk<FundingOverlay>()
    private val countryDetector = mockk<DeviceCountryDetector>()
    private val funding = mockk<TrUAPIFundingRuntime>(relaxed = true)
    private val resolveProduct = mockk<ResolveProductUseCase>()
    private val bridge = AppFundingHostBridge(Lazy { overlay }, Lazy { funding }, countryDetector, resolveProduct)
    private val app = FundingFrameContext(awaitsUserTransfer = false)
    private val page = FundingFrameContext(awaitsUserTransfer = true)

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

        bridge.presentFunding(ProductId.parse("market.dot").getOrThrow(), "intent", CoreFundingDirection.OUT, "1000000000000000000000")

        coVerify {
            overlay.present(
                FundingOverlayRequest(
                    productId = ProductId.parse("market.dot").getOrThrow(),
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

    // The frame opens the route the core checked, and the core says whether the user transfers outside it.
    @Test
    fun `a provider frame opens the core's route and answers how it closed`() = runBlocking {
        coEvery {
            overlay.presentProviderFrame(ramp, "intent", ProviderFrameTarget.App("/kyc"), false)
        } returns ProviderFrameOutcome.Closed

        assertEquals(FundingFrameOutcome.Closed, bridge.presentProviderFrame(ramp, "intent", FundingFrameTarget.App("/kyc"), app))
    }

    // The worker reads the onramp's result from the query of the page it returned to.
    @Test
    fun `an external page hands back the URL it returned to`() = runBlocking {
        val target = ProviderFrameTarget.Page("https://ramp.example/widget", "https://ramp.dot/return")
        val returned = "https://ramp.dot/return?status=done"
        coEvery { overlay.presentProviderFrame(ramp, "intent", target, true) } returns
            ProviderFrameOutcome.Returned(returned)

        assertEquals(
            FundingFrameOutcome.Returned(returned),
            bridge.presentProviderFrame(ramp, "intent", FundingFrameTarget.Page(target.url, target.returnUrl), page),
        )
    }

    @Test
    fun `the core is told the country the device detects`() = runBlocking {
        coEvery { countryDetector.detect() } returns "PT"

        assertEquals("PT", bridge.detectedCountry())
    }

    // The core has the new state before it calls; the read happens off its thread.
    @Test
    fun `a change notice goes to the runtime that reads it back`() {
        bridge.fundingChanged("intent", FundingChangeKind.SESSION)
        bridge.fundingChanged(null, FundingChangeKind.IN_PROGRESS)

        verify { funding.onChanged("intent", FundingChangeKind.SESSION) }
        verify { funding.onChanged(null, FundingChangeKind.IN_PROGRESS) }
    }

    @Test
    fun `a provider closing its frame reaches the overlay`() {
        every { overlay.closeProviderFrame(any(), any()) } just runs

        bridge.closeProviderFrame(ramp, "intent")

        verify { overlay.closeProviderFrame(ramp, "intent") }
    }

    @Test
    fun `a provider has an App only when its product declares one`() = runBlocking {
        coEvery { resolveProduct.resolve(ramp) } returns Result.success(resolved(app = true))
        coEvery { resolveProduct.resolve(widget) } returns Result.success(resolved(app = false))

        assertEquals(true, bridge.providerHasApp(ramp))
        assertEquals(false, bridge.providerHasApp(widget))
    }

    // Not knowing must not refuse a provider's screen that would open.
    @Test
    fun `a provider that cannot be resolved keeps its App route`() = runBlocking {
        coEvery { resolveProduct.resolve(any()) } returns Result.failure(IllegalStateException("offline"))

        assertEquals(true, bridge.providerHasApp(ramp))
    }

    private fun resolved(app: Boolean) = ResolvedProduct(
        product = Product(id = ramp, name = "Ramp", icon = null),
        description = null,
        executables = Executables(
            app = if (app) ProductExecutable.App(ExecutableHost("ramp.dot"), SemVer.ZERO) else null,
            widget = null,
            worker = null,
        ),
    )

    private suspend fun present() = bridge.presentFunding(null, "intent", CoreFundingDirection.IN, null)
}
