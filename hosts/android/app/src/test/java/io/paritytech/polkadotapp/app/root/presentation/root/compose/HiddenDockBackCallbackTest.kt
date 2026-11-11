package io.paritytech.polkadotapp.app.root.presentation.root.compose

import androidx.activity.BackEventCompat
import androidx.activity.OnBackPressedDispatcher
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.design.components.docklet.DockletDrawbridge
import io.paritytech.polkadotapp.design.components.docklet.DockletGeometry
import io.paritytech.polkadotapp.design.components.docklet.DockletHideState
import io.paritytech.polkadotapp.design.components.docklet.DockletPoint
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration.Companion.milliseconds

class HiddenDockBackCallbackTest {
    private val bridge = DockletGeometry.dockFrame(402.dp, 874.dp, 34.dp, threeButtonNavigation = false)
        .let { DockletDrawbridge(it, 402.dp, 874.dp) }
    private val hideState = mockk<DockletHideState>(relaxed = true) { every { drawbridge } returns bridge }
    private val dispatcher = mockk<OnBackPressedDispatcher>(relaxed = true)
    private val bandY = bridge.backBand().start + 10f

    @Test
    fun `a right-edge swipe inside the band pulls the dock and settles it on completion`() {
        val callback = callback(restoresOnBack = true)

        callback.handleOnBackStarted(backEvent(x = 400f, y = bandY, edge = BackEventCompat.EDGE_RIGHT))
        callback.handleOnBackProgressed(backEvent(x = 300f, y = bandY, edge = BackEventCompat.EDGE_RIGHT))
        callback.handleOnBackPressed()

        verify { hideState.startRestore(DockletPoint(400f, bandY), Now) }
        verify { hideState.moveRestore(DockletPoint(300f, bandY), Now) }
        verify(exactly = 1) { hideState.cancelDrag() }
        verify(exactly = 0) { dispatcher.onBackPressed() }
        verify(exactly = 0) { hideState.restore() }
    }

    @Test
    fun `a cancelled pull settles the dock once and a later Back passes on`() {
        val callback = callback(restoresOnBack = false)
        callback.handleOnBackStarted(backEvent(x = 400f, y = bandY, edge = BackEventCompat.EDGE_RIGHT))

        callback.handleOnBackCancelled()
        callback.handleOnBackCancelled()
        callback.handleOnBackPressed()

        verify(exactly = 1) { hideState.cancelDrag() }
        verify(exactly = 1) { dispatcher.onBackPressed() }
    }

    @Test
    fun `a left-edge swipe never pulls the dock`() {
        val callback = callback(restoresOnBack = false)

        callback.handleOnBackStarted(backEvent(x = 2f, y = bandY, edge = BackEventCompat.EDGE_LEFT))
        callback.handleOnBackProgressed(backEvent(x = 100f, y = bandY, edge = BackEventCompat.EDGE_LEFT))
        callback.handleOnBackPressed()

        verify(exactly = 0) { hideState.startRestore(any(), any()) }
        verify(exactly = 1) { dispatcher.onBackPressed() }
    }

    @Test
    fun `on Main with nothing open behind it Back restores the dock`() {
        every { dispatcher.hasEnabledCallbacks() } returns false
        val callback = callback(restoresOnBack = true)

        callback.handleOnBackPressed()

        verify(exactly = 1) { hideState.restore() }
        verify(exactly = 0) { dispatcher.onBackPressed() }
        assertTrue(callback.isEnabled)
    }

    @Test
    fun `an open item behind it gets Back while this callback is out of the way`() {
        every { dispatcher.hasEnabledCallbacks() } returns true
        val callback = callback(restoresOnBack = true)
        val enabledDuringDispatch = mutableListOf<Boolean>()
        every { dispatcher.onBackPressed() } answers { enabledDuringDispatch += callback.isEnabled }

        callback.handleOnBackPressed()

        assertEquals(listOf(false), enabledDuringDispatch)
        verify(exactly = 0) { hideState.restore() }
        assertTrue(callback.isEnabled)
    }

    @Test
    fun `a failing dispatch behind it still leaves this callback enabled`() {
        every { dispatcher.hasEnabledCallbacks() } returns true
        every { dispatcher.onBackPressed() } throws IllegalStateException()
        val callback = callback(restoresOnBack = true)

        assertThrows(IllegalStateException::class.java) { callback.handleOnBackPressed() }

        assertTrue(callback.isEnabled)
    }

    @Test
    fun `over a tucked screen Back goes to the screen even with nothing else enabled`() {
        every { dispatcher.hasEnabledCallbacks() } returns false
        val callback = callback(restoresOnBack = false)

        callback.handleOnBackPressed()

        verify(exactly = 1) { dispatcher.onBackPressed() }
        verify(exactly = 0) { hideState.restore() }
    }

    private fun callback(restoresOnBack: Boolean) = HiddenDockBackCallback(
        hideState = hideState,
        density = Density(1f),
        dispatcher = dispatcher,
        restoresOnBack = restoresOnBack,
        uptime = { Now },
    )

    private fun backEvent(x: Float, y: Float, edge: Int) = BackEventCompat(touchX = x, touchY = y, progress = 0.5f, swipeEdge = edge)

    private companion object {
        val Now = 1_000.milliseconds
    }
}
