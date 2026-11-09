package io.paritytech.polkadotapp.design.components.docklet

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DockletAutoHideBlockersTest {
    private val clear = DockletAutoHideBlockers(
        shownAtRest = true,
        searchOpen = false,
        keyboardUp = false,
        tipShown = false,
        panelExpanded = false,
        touchExploration = false,
    )

    @Test
    fun `nothing blocks a shown dock at rest`() {
        assertFalse(clear.blocked)
    }

    @Test
    fun `a moving or hidden dock blocks auto-hide`() {
        assertTrue(clear.copy(shownAtRest = false).blocked)
    }

    @Test
    fun `open search blocks auto-hide`() {
        assertTrue(clear.copy(searchOpen = true).blocked)
    }

    @Test
    fun `a raised keyboard blocks auto-hide`() {
        assertTrue(clear.copy(keyboardUp = true).blocked)
    }

    @Test
    fun `a tip on screen blocks auto-hide`() {
        assertTrue(clear.copy(tipShown = true).blocked)
    }

    @Test
    fun `an expanded panel blocks auto-hide`() {
        assertTrue(clear.copy(panelExpanded = true).blocked)
    }

    @Test
    fun `TalkBack touch exploration blocks auto-hide`() {
        assertTrue(clear.copy(touchExploration = true).blocked)
    }
}
