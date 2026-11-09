package io.paritytech.polkadotapp.app.root.presentation.root

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.app.root.domain.main.DockletHideCause
import io.paritytech.polkadotapp.app.root.domain.main.DockletTip
import io.paritytech.polkadotapp.app.root.domain.main.MainInteractor
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class RootNavBarViewModelDockletTest {
    private val interactor = mockk<MainInteractor>(relaxed = true)
    private lateinit var viewModel: RootNavBarViewModel

    @Before
    fun setUp() {
        Dispatchers.setMain(UnconfinedTestDispatcher())
        viewModel = RootNavBarViewModel(
            navigationHolder = mockk(relaxed = true),
            rootRouter = mockk(relaxed = true),
            interactor = interactor,
            productSessionController = mockk(relaxed = true),
            tabBarVisibilityHolder = mockk(relaxed = true),
            tabBarOffsetHolder = mockk(relaxed = true),
            bottomNavHeightProvider = mockk(relaxed = true),
            scanPanelRequests = mockk(relaxed = true),
            dockletInputEvents = mockk(relaxed = true),
        )
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun `the peek plays once per session`() {
        assertTrue(viewModel.takeSessionPeek())
        assertFalse(viewModel.takeSessionPeek())
    }

    @Test
    fun `a hide shows the tip the interactor picks and marks it shown`() {
        every { interactor.dockletTipFor(DockletHideCause.Swipe) } returns DockletTip.Tucked

        viewModel.onDockletHidden(DockletHideCause.Swipe)

        assertEquals(DockletTip.Tucked, viewModel.dockletTip.value)
        verify { interactor.markDockletTipShown(DockletTip.Tucked) }
    }

    @Test
    fun `a hide with no tip left shows none`() {
        every { interactor.dockletTipFor(DockletHideCause.Auto) } returns null

        viewModel.onDockletHidden(DockletHideCause.Auto)

        assertNull(viewModel.dockletTip.value)
        verify(exactly = 0) { interactor.markDockletTipShown(any()) }
    }
}
