package io.paritytech.polkadotapp.feature_chats_impl.presentation.util

import org.junit.Assert.assertEquals
import org.junit.Test

class DockletBandTest {
    @Test
    fun `a shown Docklet keeps its whole band above the system inset`() {
        assertEquals(200, visibleDockletBandPx(barInsetBottom = 300, systemBottom = 100, hiddenFraction = 0f))
    }

    @Test
    fun `a hidden Docklet frees its band`() {
        assertEquals(0, visibleDockletBandPx(barInsetBottom = 300, systemBottom = 100, hiddenFraction = 1f))
    }

    @Test
    fun `a keyboard taller than the Docklet band leaves no band`() {
        assertEquals(0, visibleDockletBandPx(barInsetBottom = 300, systemBottom = 900, hiddenFraction = 0f))
    }
}
