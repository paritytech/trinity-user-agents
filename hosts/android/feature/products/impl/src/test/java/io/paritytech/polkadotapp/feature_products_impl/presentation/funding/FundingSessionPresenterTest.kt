package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import io.mockk.verify
import io.paritytech.polkadotapp.common.presentation.notification.AppNotification
import io.paritytech.polkadotapp.common.presentation.notification.AppNotifier
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingEvent
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFrameRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.WorkerCrashNotice
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingFixtures.entry
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingFixtures.ramp
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.FundingFrames
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Test
import uniffi.truapi.FundingFrameTarget

class FundingSessionPresenterTest {
    private val service = FakeFundingService()
    private val router = mockk<ProductsRouter>(relaxed = true)
    private val notifier = mockk<AppNotifier>(relaxed = true)
    private val crashNotice = mockk<WorkerCrashNotice>(relaxed = true)
    private val interactor = mockk<FundingFlowInteractor> {
        coEvery { brand(ramp) } returns FundingProviderBrand(ramp, "Ramp", null)
    }
    private val request = FundingFrameRequest("frame-1", "intent", ramp, FundingFrameTarget.App("/kyc"))

    private fun TestScope.presenting(): Pair<FundingSessionPresenter, FundingFrames> {
        val frames = FundingFrames(mockk(), mockk(), notifier, testDispatchers())
        val presenter = FundingSessionPresenter(
            service,
            frames,
            router,
            interactor,
            notifier,
            mockk { every { applicationContext.getString(any(), any()) } answers { "${(it.invocation.args[1] as Array<*>)[0]} wants to show a screen" } },
            crashNotice,
            testDispatchers(),
        )
        backgroundScope.launch { presenter.present() }
        runCurrent()
        return presenter to frames
    }

    @Test
    fun `a new session's sheet opens when the core asks`() = runTest {
        presenting()

        service.eventQueue.emit(FundingEvent.PresentationRequested(entry()))
        runCurrent()

        coVerify { router.openFundingOverlay("intent") }
    }

    @Test
    fun `a provider's screen opens at once over its session's sheet`() = runTest {
        coEvery { router.isFundingOverlayShown("intent") } returns true
        val (_, frames) = presenting()

        service.eventQueue.emit(FundingEvent.FrameRequested(request))
        runCurrent()

        assertNotNull(frames.get("frame-1"))
        coVerify { router.openFundingProviderFrame("intent", "frame-1") }
    }

    @Test
    fun `away from its session, a provider's screen waits for a tap on the notice`() = runTest {
        coEvery { router.isFundingOverlayShown("intent") } returns false
        val notice = slot<AppNotification>()
        every { notifier.notify(capture(notice)) } returns Unit
        service.entry.emit(entry(session = FundingFixtures.entry().session?.copy(providerId = ramp), isPresented = false))
        presenting()

        service.eventQueue.emit(FundingEvent.FrameRequested(request))
        runCurrent()
        coVerify(exactly = 0) { router.openFundingProviderFrame(any(), any()) }
        assertEquals("Ramp wants to show a screen", notice.captured.message)

        (notice.captured as AppNotification.Tappable).onTap()
        runCurrent()
        coVerify { router.openFundingOverlay("intent") }
        coVerify { router.openFundingProviderFrame("intent", "frame-1") }
    }

    @Test
    fun `a provider that keeps crashing is reported with the app's notice`() = runTest {
        presenting()

        service.eventQueue.emit(FundingEvent.ProviderNotWorking(ramp))
        runCurrent()

        verify { crashNotice.onCrashing(ramp) }
    }
}
