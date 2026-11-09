package io.paritytech.polkadotapp.design.components.docklet

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

@OptIn(ExperimentalCoroutinesApi::class)
class DockletAutoHideTest {
    private var hides = 0

    @Test
    fun `the outside-touch delay is 0 s and the idle delay is 3 s`() {
        assertEquals(Duration.ZERO, DockletAutoHide.OutsideDelay)
        assertEquals(3.seconds, DockletAutoHide.IdleDelay)
    }

    @Test
    fun `a touch outside the Docklet hides it at once`() = runTest {
        val autoHide = autoHide()

        autoHide.onTouch(DockletTouch.Outside)
        runCurrent()

        assertEquals(1, hides)
    }

    @Test
    fun `a second touch outside does not hide it again`() = runTest {
        val autoHide = autoHide()

        autoHide.onTouch(DockletTouch.Outside)
        autoHide.onTouch(DockletTouch.Outside)
        runCurrent()

        assertEquals(1, hides)
    }

    @Test
    fun `a touch on the Docklet cancels a pending outside hide`() = runTest {
        val autoHide = autoHide()

        autoHide.onTouch(DockletTouch.Outside)
        autoHide.onTouch(DockletTouch.Docklet)
        runCurrent()

        assertEquals(0, hides)
    }

    @Test
    fun `3 s without a touch hides it`() = runTest {
        autoHide()

        advanceTimeBy(2_999L)
        runCurrent()
        assertEquals(0, hides)
        advanceTimeBy(2L)
        runCurrent()
        assertEquals(1, hides)
    }

    @Test
    fun `every touch on the Docklet restarts the idle timer`() = runTest {
        val autoHide = autoHide()

        advanceTimeBy(2_000L)
        autoHide.onTouch(DockletTouch.Docklet)
        advanceTimeBy(2_000L)
        runCurrent()

        assertEquals(0, hides)
    }

    @Test
    fun `a key press restarts the idle timer and cancels a pending hide`() = runTest {
        val autoHide = autoHide()

        advanceTimeBy(2_000L)
        autoHide.onTouch(DockletTouch.Outside)
        autoHide.onKey()
        advanceTimeBy(2_000L)
        runCurrent()

        assertEquals(0, hides)
    }

    @Test
    fun `a wheel scroll counts as a touch outside`() = runTest {
        val autoHide = autoHide()

        autoHide.onWheel()
        runCurrent()

        assertEquals(1, hides)
    }

    @Test
    fun `nothing hides while a blocker is up`() = runTest {
        val autoHide = autoHide()
        autoHide.blocked = true

        autoHide.onTouch(DockletTouch.Outside)
        advanceUntilIdle()

        assertEquals(0, hides)
    }

    @Test
    fun `a blocker going away starts a fresh idle timer`() = runTest {
        val autoHide = autoHide()
        autoHide.blocked = true
        advanceTimeBy(10_000L)

        autoHide.blocked = false
        advanceTimeBy(2_999L)
        runCurrent()
        assertEquals(0, hides)
        advanceTimeBy(2L)
        runCurrent()
        assertEquals(1, hides)
    }

    private fun TestScope.autoHide() = DockletAutoHide(backgroundScope) { hides++ }.also { it.blocked = false }
}
