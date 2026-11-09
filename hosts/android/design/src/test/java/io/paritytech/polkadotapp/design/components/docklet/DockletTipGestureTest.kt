package io.paritytech.polkadotapp.design.components.docklet

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DockletTipGestureTest {
    @Test
    fun `a fling of the tip past 56 dp brings the dock back`() {
        assertTrue(DockletTipGesture.flungAway(movedDp = 57f))
    }

    @Test
    fun `a 55 dp drag of the tip does not`() {
        assertFalse(DockletTipGesture.flungAway(movedDp = 55f))
    }
}
