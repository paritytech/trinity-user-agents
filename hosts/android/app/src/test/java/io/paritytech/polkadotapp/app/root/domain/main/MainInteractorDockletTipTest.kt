package io.paritytech.polkadotapp.app.root.domain.main

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.app.root.data.storage.OneShotTooltip
import io.paritytech.polkadotapp.app.root.data.storage.TooltipStorage
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class MainInteractorDockletTipTest {
    private val shown = mutableSetOf<OneShotTooltip>()
    private val tooltipStorage = mockk<TooltipStorage> {
        every { wasShown(any()) } answers { firstArg<OneShotTooltip>() in shown }
        every { markShown(any()) } answers { shown += firstArg<OneShotTooltip>() }
    }
    private val interactor = MainInteractor(
        context = mockk(relaxed = true),
        messageSender = mockk(relaxed = true),
        observeTabWarningsUseCase = mockk(relaxed = true),
        tooltipStorage = tooltipStorage,
    )

    @Test
    fun `the first swipe hide shows the tucked tip`() {
        assertEquals(DockletTip.Tucked, interactor.dockletTipFor(DockletHideCause.Swipe))
    }

    @Test
    fun `the tucked tip shows once ever`() {
        interactor.markDockletTipShown(DockletTip.Tucked)

        assertNull(interactor.dockletTipFor(DockletHideCause.Swipe))
        verify { tooltipStorage.markShown(OneShotTooltip.DockletTucked) }
    }

    @Test
    fun `the first auto-hide shows its own tip`() {
        assertEquals(DockletTip.AutoHidden, interactor.dockletTipFor(DockletHideCause.Auto))
    }

    @Test
    fun `after the auto-hide tip neither tip shows again`() {
        interactor.markDockletTipShown(DockletTip.AutoHidden)

        assertNull(interactor.dockletTipFor(DockletHideCause.Auto))
        assertNull(interactor.dockletTipFor(DockletHideCause.Swipe))
    }

    @Test
    fun `the tucked tip leaves the auto-hide tip still to come`() {
        interactor.markDockletTipShown(DockletTip.Tucked)

        assertEquals(DockletTip.AutoHidden, interactor.dockletTipFor(DockletHideCause.Auto))
    }
}
